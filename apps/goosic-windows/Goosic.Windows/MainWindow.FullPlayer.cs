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
        FullPlayerBackdrop.Children.Add(_fullPlayerMesh);
        WireFullPlayerBlur();
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
        _uiSettings.AdvancedEffectsEnabledChanged += (_, _) =>
            DispatcherQueue.TryEnqueue(UpdateFullPlayerBlur);
        Model.PropertyChanged += (_, args) =>
        {
            if (args.PropertyName == nameof(ShellViewModel.ArtworkBackground))
            {
                UpdateBackdrop();
            }
        };
    }

    /// <summary>
    /// Blurs everything drawn beneath <c>FullPlayerBlurHost</c>: the moving colours and the cover.
    /// </summary>
    /// <remarks>
    /// The mesh is drawn as soft-edged blocks, and without a blur over it they read as blocks, not
    /// as the wash of colour the macOS player shows. A backdrop blur keeps the mesh moving while it
    /// is blurred, which a pre-rendered image could not.
    /// </remarks>
    private void WireFullPlayerBlur()
    {
        try
        {
            var compositor = Microsoft.UI.Xaml.Hosting.ElementCompositionPreview
                .GetElementVisual(FullPlayerBlurHost).Compositor;
            var blur = new Microsoft.Graphics.Canvas.Effects.GaussianBlurEffect
            {
                Name = "Blur",
                BlurAmount = 60f,
                BorderMode = Microsoft.Graphics.Canvas.Effects.EffectBorderMode.Hard,
                Optimization = Microsoft.Graphics.Canvas.Effects.EffectOptimization.Balanced,
                Source = new Microsoft.UI.Composition.CompositionEffectSourceParameter("Backdrop"),
            };
            var brush = compositor.CreateEffectFactory(blur).CreateBrush();
            brush.SetSourceParameter("Backdrop", compositor.CreateBackdropBrush());
            var visual = compositor.CreateSpriteVisual();
            visual.Brush = brush;
            visual.RelativeSizeAdjustment = System.Numerics.Vector2.One;
            Microsoft.UI.Xaml.Hosting.ElementCompositionPreview.SetElementChildVisual(FullPlayerBlurHost, visual);
        }
        catch (Exception error)
        {
            // Without the blur the player still works; it only looks as it did before.
            BridgeLog.Write($"full-screen blur unavailable: {error.GetType().Name}");
        }
    }

    /// <summary>Loads the cover at full size and samples its colours for both backgrounds.</summary>
    private async Task ShowArtworkAsync(string? thumbnail)
    {
        _paletteFor = thumbnail;
        var file = await Model.LargeArtworkFileAsync(thumbnail);
        if (_paletteFor != thumbnail)
        {
            return;
        }

        if (file is null)
        {
            FullPlayerCover.Source = null;
            _fullPlayerMesh.SetPalette(null);
            ArtworkBackdropImage.Source = null;
            FullPlayerBlur.Source = null;
            UpdateBackdrop();
            UpdateFullPlayerBlur();
            return;
        }

        var image = new Microsoft.UI.Xaml.Media.Imaging.BitmapImage();
        using (var stream = File.OpenRead(file))
        {
            await image.SetSourceAsync(stream.AsRandomAccessStream());
        }

        var paletteTask = Views.MeshBackground.PaletteAsync(file);
        var backdropTask = _backdropRenderer.RenderAsync(file);
        await Task.WhenAll(paletteTask, backdropTask);
        if (_paletteFor != thumbnail)
        {
            return;
        }

        FullPlayerCover.Source = image;
        _fullPlayerMesh.SetPalette(paletteTask.Result);
        ArtworkBackdropImage.Source = backdropTask.Result;
        FullPlayerBlur.Source = backdropTask.Result;
        UpdateBackdrop();
        UpdateFullPlayerBlur();
    }

    private readonly global::Windows.UI.ViewManagement.UISettings _uiSettings = new();

    private void UpdateFullPlayerBlur() =>
        FullPlayerBlur.Visibility = FullPlayerLayout.ShowsBlur(_uiSettings.AdvancedEffectsEnabled, FullPlayerBlur.Source is not null)
            ? Visibility.Visible
            : Visibility.Collapsed;

    /// <summary>Arranges cover, controls and lyrics for the window's width and the lyrics toggle.</summary>
    private void ApplyFullPlayerLayout()
    {
        var width = RootGrid.ActualWidth;
        if (width <= 0)
        {
            return;
        }

        var mode = FullPlayerLayout.Mode(width, FullPlayerLyricsToggle.IsChecked == true);
        var (horizontal, top, bottom) = FullPlayerLayout.Padding(width);
        FullPlayerLayoutGrid.Padding = new Thickness(horizontal, top, horizontal, bottom);
        FullPlayerLayoutGrid.ColumnSpacing = mode == FullPlayerMode.Split ? 72 : 0;

        var lyricsOnly = mode == FullPlayerMode.Lyrics;
        FullPlayerLyrics.Visibility = mode == FullPlayerMode.Cover ? Visibility.Collapsed : Visibility.Visible;
        FullPlayerCoverButton.Visibility = lyricsOnly ? Visibility.Collapsed : Visibility.Visible;
        FullPlayerLyricsColumn.Width = mode == FullPlayerMode.Split
            ? new GridLength(1.3, GridUnitType.Star)
            : new GridLength(0);
        FullPlayerCoverColumn.MaxWidth = mode == FullPlayerMode.Split ? 560 : double.PositiveInfinity;

        // Lyrics alone take the top row; the controls sit under them rather than beside.
        Grid.SetColumn(FullPlayerLyrics, lyricsOnly ? 0 : 1);
        Grid.SetRow(FullPlayerLyrics, 0);
        Grid.SetRowSpan(FullPlayerLyrics, lyricsOnly ? 1 : 2);
        Grid.SetRow(FullPlayerControls, lyricsOnly ? 1 : 0);
        Grid.SetRowSpan(FullPlayerControls, lyricsOnly ? 1 : 2);
        FullPlayerControls.VerticalAlignment = lyricsOnly ? VerticalAlignment.Bottom : VerticalAlignment.Center;
        // The volume, full-screen, lyrics and close buttons float over the top right corner.
        FullPlayerLyrics.Margin = new Thickness(0, FullPlayerLayout.LyricsTopMargin(mode), 0, 0);
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
        SetTitleBar(open ? FullPlayerDragStrip : TitleBarStrip);
        if (!open)
        {
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
    private async void OnLyricLineClicked(object sender, RoutedEventArgs e)
    {
        if (sender is Button { Tag: long at } && at > 0 && _playback is not null && Model.IsSeekable)
        {
            await SeekToAsync(at / 1000.0);
        }
    }

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
