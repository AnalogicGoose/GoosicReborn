using System;
using System.Collections.Generic;
using System.IO;
using System.Threading.Tasks;
using Goosic.Windows.Presentation;
using Goosic.Windows.Service;
using Goosic.Windows.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Windows.ApplicationModel.DataTransfer;
using Windows.Media;
using Windows.System;

namespace Goosic.Windows;

public sealed partial class MainWindow : Window
{
    // ---- Full-screen player -----------------------------------------------------------------

    /// <summary>
    /// Builds the parts of the full-screen player that live in code: its background, and its seek
    /// gestures, which the slider hides from markup handlers exactly as the transport's does.
    /// </summary>
    private void WireFullPlayer()
    {
        FullPlayerBackdrop.Children.Add(_fullPlayerArtworkBackdrop);
        FullPlayerCoverFrame.SizeChanged += (_, _) => UpdateCoverScale(animate: false);
        UpdateCoverScale(animate: false);
        FullPlayerProgress.AddHandler(UIElement.PointerPressedEvent,
            new PointerEventHandler((_, _) => { _fullPlayerSeeking = true; Model.IsScrubbing = true; }), handledEventsToo: true);
        PointerEventHandler done = async (_, _) =>
        {
            if (!_fullPlayerSeeking)
            {
                return;
            }

            _fullPlayerSeeking = false;
            Model.IsScrubbing = false;
            if (_playback is not null && Model.IsSeekable)
            {
                await SeekToAsync(FullPlayerProgress.Value);
            }
        };
        FullPlayerProgress.AddHandler(UIElement.PointerReleasedEvent, done, handledEventsToo: true);
        FullPlayerProgress.AddHandler(UIElement.PointerCaptureLostEvent, done, handledEventsToo: true);

        Model.ArtworkChanged += async thumbnail => await ShowArtworkAsync(thumbnail);
        RootGrid.SizeChanged += (_, _) => ApplyFullPlayerLayout();
        Activated += (_, _) => DispatcherQueue.TryEnqueue(UpdateFullPlayerBackdrop);
        AppWindow.Changed += (_, args) =>
        {
            if (args.DidVisibilityChange || args.DidSizeChange || args.DidPresenterChange)
            {
                DispatcherQueue.TryEnqueue(UpdateFullPlayerBackdrop);
            }
        };
        Model.PropertyChanged += (_, args) =>
        {
            if (args.PropertyName == nameof(ShellViewModel.IsPlaying))
            {
                UpdateCoverScale(animate: true);
            }
            if (args.PropertyName == nameof(ShellViewModel.ArtworkBackground))
            {
                UpdateBackdrop();
            }
        };
    }

    /// <summary>Lets the cover settle smaller while paused without moving the controls.</summary>
    private void UpdateCoverScale(bool animate)
    {
        var visual = Microsoft.UI.Xaml.Hosting.ElementCompositionPreview.GetElementVisual(FullPlayerCoverFrame);
        visual.CenterPoint = new System.Numerics.Vector3((float)(FullPlayerCoverFrame.ActualWidth / 2),
            (float)(FullPlayerCoverFrame.ActualHeight / 2), 0);
        var scale = Model.IsPlaying ? 1f : (float)LyricsMotion.PausedCoverScale;
        var target = new System.Numerics.Vector3(scale, scale, 1);
        if (!animate || !AnimationsEnabled)
        {
            visual.StopAnimation("Scale");
            visual.Scale = target;
            return;
        }

        // macOS's .snappy(duration: 0.5, extraBounce: 0.1), on SwiftUI's own curve.
        visual.StartAnimation("Scale", Views.SwiftUISpring.Vector3(visual.Compositor, LyricsMotion.CoverResponse,
            LyricsMotion.CoverDamping, target));
    }

    /// <summary>Loads the playing cover and supplies its cached file to the immersive backdrop.</summary>
    private async Task ShowArtworkAsync(string? thumbnail)
    {
        var request = ++_artworkRequest;
        _fullPlayerArtworkFile = null;
        UpdateFullPlayerBackdrop();
        var file = await Model.LargeArtworkFileAsync(thumbnail);
        if (_artworkRequest != request)
        {
            return;
        }

        if (file is null)
        {
            ApplyCoverAspect(1);
            FullPlayerCover.Source = null;
            ArtworkBackdropImage.Source = null;
            UpdateBackdrop();
            return;
        }

        var image = new Microsoft.UI.Xaml.Media.Imaging.BitmapImage();
        using (var stream = File.OpenRead(file))
        {
            await image.SetSourceAsync(stream.AsRandomAccessStream());
        }

        var backdropTask = _backdropRenderer.RenderAsync(file);
        var aspect = await CoverAspectAsync(file);
        if (_artworkRequest != request)
        {
            return;
        }

        ApplyCoverAspect(aspect);
        FullPlayerCover.Source = image;
        FadePlayerArtwork(FullPlayerCover);
        _fullPlayerArtworkFile = file;
        UpdateFullPlayerBackdrop();

        var browseBackdrop = await backdropTask;
        if (_artworkRequest == request)
        {
            ArtworkBackdropImage.Source = browseBackdrop;
            UpdateBackdrop();
        }
    }

    private void UpdateFullPlayerBackdrop()
    {
        var visible = FullPlayer.Visibility == Visibility.Visible && AppWindow.IsVisible
            && AppWindow.Presenter is not Microsoft.UI.Windowing.OverlappedPresenter
                { State: Microsoft.UI.Windowing.OverlappedPresenterState.Minimized };
        _fullPlayerArtworkBackdrop.Update(visible ? _fullPlayerArtworkFile : null, AnimationsEnabled);
    }

    private readonly global::Windows.UI.ViewManagement.UISettings _uiSettings = new();

    /// <summary>Reads the picture's shape from its header, turned as the file says to turn it.</summary>
    private static async Task<double> CoverAspectAsync(string file)
    {
        try
        {
            using var stream = File.OpenRead(file);
            var decoder = await global::Windows.Graphics.Imaging.BitmapDecoder.CreateAsync(stream.AsRandomAccessStream());
            return CoverShape.Aspect(decoder.OrientedPixelWidth, decoder.OrientedPixelHeight);
        }
        catch (Exception error)
        {
            // An unreadable header leaves the cover square, as it always was.
            BridgeLog.Write($"cover shape unavailable: {error.GetType().Name}");
            return 1;
        }
    }

    /// <summary>The playing cover's width over its height, from the full-size picture.</summary>
    private double _coverAspect = 1;

    /// <summary>The player bar's cover height; its width follows the cover's shape.</summary>
    private const double PillCoverHeight = 34;

    /// <summary>
    /// Gives the cover in the player bar and in the full player the shape of its picture: square
    /// for a song, wide for a music video, instead of cropping every picture to a square.
    /// </summary>
    private void ApplyCoverAspect(double aspect)
    {
        _coverAspect = aspect;
        var (width, height) = CoverShape.Fit(aspect, 520, 520);
        FullPlayerCoverCanvas.Width = width;
        FullPlayerCoverCanvas.Height = height;
        PillCoverButton.Width = Math.Round(PillCoverHeight * aspect);
        ApplyFullPlayerLayout();
    }

    /// <summary>Arranges cover, controls and lyrics for the window's width and the lyrics toggle.</summary>
    private void ApplyFullPlayerLayout()
    {
        var width = RootGrid.ActualWidth;
        if (width <= 0)
        {
            return;
        }

        var mode = FullPlayerLayout.Mode(width, FullPlayerLyricsToggle.IsChecked == true);
        var split = mode == FullPlayerMode.Split;
        var (horizontal, top, bottom) = FullPlayerLayout.Padding(width);
        var lyricsOnly = mode == FullPlayerMode.Lyrics;
        FullPlayerLyrics.Visibility = mode == FullPlayerMode.Cover ? Visibility.Collapsed : Visibility.Visible;
        _fullLyrics?.SetVisible(FullPlayer.Visibility == Visibility.Visible && mode != FullPlayerMode.Cover);
        // The cover shrinks with the window's height too, so the controls under it stay on screen.
        var cover = FullPlayerLayout.CoverSize(RootGrid.ActualHeight, top, bottom);

        // Side by side, the two columns are sized for what they hold and centred as one group.
        var (inset, gutter, player, lyrics) = FullPlayerLayout.Split(width, cover);
        if (split)
        {
            horizontal = inset;
            cover = Math.Min(cover, player);
        }

        FullPlayerLayoutGrid.Padding = new Thickness(horizontal, top, horizontal, bottom);
        FullPlayerLayoutGrid.ColumnSpacing = split ? gutter : 0;
        FullPlayerLayoutGrid.HorizontalAlignment = split ? HorizontalAlignment.Center : HorizontalAlignment.Stretch;
        FullPlayerCoverColumn.Width = split ? new GridLength(player) : new GridLength(1, GridUnitType.Star);
        FullPlayerCoverColumn.MaxWidth = double.PositiveInfinity;
        FullPlayerLyricsColumn.Width = new GridLength(split ? lyrics : 0);
        FullPlayerControls.MaxWidth = split ? player : 520;
        // The cover fits the square it used to fill: a wide one is as wide and shorter, a tall
        // one as tall and narrower, and the controls below move to meet it.
        FullPlayerCoverButton.MaxWidth = cover > 0
            ? CoverShape.Fit(_coverAspect, cover, cover).Width
            : double.PositiveInfinity;
        FullPlayerCoverButton.HorizontalAlignment = HorizontalAlignment.Left;
        FullPlayerCoverButton.Visibility = lyricsOnly || cover == 0 ? Visibility.Collapsed : Visibility.Visible;

        // Lyrics alone take the top row; the controls sit under them rather than beside.
        Grid.SetColumn(FullPlayerLyrics, lyricsOnly ? 0 : 1);
        Grid.SetRow(FullPlayerLyrics, 0);
        Grid.SetRowSpan(FullPlayerLyrics, lyricsOnly ? 1 : 2);
        Grid.SetRow(FullPlayerControls, lyricsOnly ? 1 : 0);
        Grid.SetRowSpan(FullPlayerControls, lyricsOnly ? 1 : 2);
        FullPlayerControls.VerticalAlignment = lyricsOnly ? VerticalAlignment.Bottom : VerticalAlignment.Center;
        // Beside the cover the lyrics take the window's full height, as Music's do: they pass
        // under the top buttons and fade out toward the bottom edge rather than stopping in a box
        // above it. The negative margin undoes the layout's own top and bottom padding. Alone they
        // take the row they are given, above the controls.
        FullPlayerLyrics.VerticalAlignment = VerticalAlignment.Stretch;
        FullPlayerLyrics.Margin = split ? new Thickness(0, -top, 0, -bottom) : new Thickness(0);
    }

    private void UpdateBackdrop() =>
        ArtworkBackdrop.Visibility = Model.ArtworkBackground && ArtworkBackdropImage.Source is not null
            ? Visibility.Visible
            : Visibility.Collapsed;

    private void OnOpenFullPlayer(object sender, RoutedEventArgs e)
    {
        if (Model.HasPlayback)
        {
            SetFullPlayerOpen(true);
        }
    }

    private void OnCloseFullPlayer(object sender, RoutedEventArgs e) => SetFullPlayerOpen(false);

    /// <summary>The full player's own song menu, beside Like, as the macOS full player has.</summary>
    private void OnFullPlayerMore(object sender, RoutedEventArgs e)
    {
        if (Model.ConfirmedTrack is not { } track)
        {
            Model.ReportStatus("Nothing is playing.");
            return;
        }

        BuildNowPlayingMenu(track, inFullPlayer: true)
            .ShowAt(FullPlayerMoreButton, new FlyoutShowOptions { Placement = FlyoutPlacementMode.BottomEdgeAlignedRight });
    }

    private void OnNowPlayingDoubleTapped(object sender, DoubleTappedRoutedEventArgs e)
    {
        if (Model.HasPlayback)
        {
            SetFullPlayerOpen(true);
        }
    }

    /// <summary>
    /// Opens or closes the full-screen player, taking the window to full screen with it.
    /// </summary>
    /// <remarks>
    /// While it is open the drag region moves to an empty strip at its top, because the transport
    /// bar that is normally the title bar is underneath it and its drag would swallow clicks on
    /// the player's own controls.
    /// </remarks>
    private async void SetFullPlayerOpen(bool open)
    {
        var visible = FullPlayer.Visibility == Visibility.Visible;
        if (open == visible)
        {
            return;
        }

        // The player fills the window; taking the window itself to full screen is a separate
        // choice, made with its own button or F11, and closing the player always undoes it.
        if (open)
        {
            _focusBeforeFullPlayer = FocusManager.GetFocusedElement(RootGrid.XamlRoot) as Control;
            ApplyFullPlayerLayout();
        }

        FullPlayer.Visibility = open ? Visibility.Visible : Visibility.Collapsed;
        UpdateFullPlayerBackdrop();
        _fullLyrics?.SetVisible(open && FullPlayerLyrics.Visibility == Visibility.Visible);
        _panelLyrics?.SetVisible(!open && _sidePanel.Content == SidePanelContent.Lyrics);
        SetTitleBar(open ? FullPlayerDragStrip : TitleBarStrip);
        if (!open)
        {
            SetMiniPlayer(false);
            SetWindowFullScreen(false);
            RestoreFocusAfterFullPlayer();
            return;
        }

        // Play/Pause is what someone who just opened a player most likely wants next, and
        // unlike the cover it is visible in every layout.
        FullPlayerPlayButton.Focus(FocusState.Programmatic);
        if (Model.Lyrics.Count == 0)
        {
            await Model.LoadLyricsAsync();
        }
    }

    private Control? _focusBeforeFullPlayer;

    /// <summary>
    /// Hands focus back to what had it before the player opened, or to the control that opens the
    /// player again when that element has since left the page.
    /// </summary>
    private void RestoreFocusAfterFullPlayer()
    {
        var previous = _focusBeforeFullPlayer;
        _focusBeforeFullPlayer = null;
        if (previous is { XamlRoot: not null, IsEnabled: true, Visibility: Visibility.Visible }
            && previous.Focus(FocusState.Programmatic))
        {
            return;
        }

        PillCoverButton.Focus(FocusState.Programmatic);
    }

    /// <summary>Makes the position slider's keys seek, since the slider alone only moves its thumb.</summary>
    private async void OnFullPlayerProgressKeyDown(object sender, KeyRoutedEventArgs e)
    {
        var key = e.Key switch
        {
            VirtualKey.Left or VirtualKey.Down => SeekKey.Back,
            VirtualKey.Right or VirtualKey.Up => SeekKey.Forward,
            VirtualKey.PageDown => SeekKey.PageBack,
            VirtualKey.PageUp => SeekKey.PageForward,
            VirtualKey.Home => SeekKey.Start,
            VirtualKey.End => SeekKey.End,
            _ => SeekKey.None,
        };
        if (key == SeekKey.None)
        {
            return;
        }

        e.Handled = true;
        if (_playback is not null && Model.IsSeekable
            && FullPlayerLayout.KeySeek(key, Model.PlaybackPosition, Model.PlaybackDuration) is { } target)
        {
            await SeekToAsync(target);
        }
    }

    private void OnToggleWindowFullScreen(object sender, RoutedEventArgs e) =>
        SetWindowFullScreen(FullPlayerWindowToggle.IsChecked == true);

    private void SetWindowFullScreen(bool fullScreen)
    {
        FullPlayerWindowToggle.IsChecked = fullScreen;
        FullPlayerWindowToggle.Content = fullScreen ? "\uE73F" : "\uE740";
        AppWindow.SetPresenter(fullScreen
            ? Microsoft.UI.Windowing.AppWindowPresenterKind.FullScreen
            : Microsoft.UI.Windowing.AppWindowPresenterKind.Default);
    }

    private void OnToggleFullPlayerLyrics(object sender, RoutedEventArgs e)
    {
        ApplyFullPlayerLayout();
    }

    /// <summary>Seeks to a synced line. Plain lyrics carry no times, so their lines do nothing.</summary>
    private async void OnAutoplayToggled(object sender, RoutedEventArgs e)
    {
        if (sender is ToggleSwitch toggle && toggle.IsOn != Model.Autoplay)
        {
            await Model.SetAutoplayAsync(toggle.IsOn);
        }
    }

    private void OnShuffleToggled(object sender, RoutedEventArgs e)
    {
        if (sender is ToggleSwitch toggle && toggle.IsOn != Model.IsShuffled)
        {
            Model.ToggleShuffle();
        }
    }

    private async void OnArtworkBackgroundToggled(object sender, RoutedEventArgs e)
    {
        if (sender is ToggleSwitch toggle && toggle.IsOn != Model.ArtworkBackground)
        {
            await Model.SetArtworkBackgroundAsync(toggle.IsOn);
        }
    }
}
