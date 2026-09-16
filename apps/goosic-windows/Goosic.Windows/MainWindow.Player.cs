using System;
using System.Collections.Generic;
using System.IO;
using System.Threading.Tasks;
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
    // ---- The player pill --------------------------------------------------------------------

    /// <summary>Pointing at the pill shows the times either side of the position line.</summary>
    private void OnPillPointerEntered(object sender, PointerRoutedEventArgs e)
    {
        if (!Model.HasPlayback)
        {
            return;
        }

        PillElapsed.Visibility = Visibility.Visible;
        PillRemaining.Visibility = Visibility.Visible;
    }

    private void OnPillPointerExited(object sender, PointerRoutedEventArgs e)
    {
        PillElapsed.Visibility = Visibility.Collapsed;
        PillRemaining.Visibility = Visibility.Collapsed;
    }

    /// <summary>Pointing at the cover shows that clicking it opens the full-screen player.</summary>
    private void OnCoverPointerEntered(object sender, PointerRoutedEventArgs e) => PillCoverHover.Opacity = 1;

    private void OnCoverPointerExited(object sender, PointerRoutedEventArgs e) => PillCoverHover.Opacity = 0;

    /// <summary>The pill's "more" menu: what the row menu offers, for the track that is playing.</summary>
    private void OnNowPlayingMore(object sender, RoutedEventArgs e)
    {
        if (Model.ConfirmedTrack is not { } track || BuildMenu(track) is not { } menu)
        {
            Model.ReportStatus("Nothing is playing.");
            return;
        }

        menu.Items.Insert(0, new MenuFlyoutSeparator());
        var full = new MenuFlyoutItem { Text = "Full-screen player", Icon = new FontIcon { Glyph = "\uE740" } };
        full.Click += (_, _) => SetFullPlayerOpen(true);
        menu.Items.Insert(0, full);
        menu.ShowAt(PillMoreButton, new FlyoutShowOptions { Placement = FlyoutPlacementMode.TopEdgeAlignedRight });
    }

    private async void OnAccount(object sender, RoutedEventArgs e)
    {
        await Model.RefreshAccountsAsync();
    }

    private async void OnLoadMore(object sender, RoutedEventArgs e) => await Model.LoadMoreAsync();

    /// <summary>Loads the next part of a long page as the reader nears its end, as YouTube Music does.</summary>
    private async void OnContentViewChanged(object? sender, ScrollViewerViewChangedEventArgs e)
    {
        if (Model.HasMore && ContentScroller.ScrollableHeight > 0
            && ContentScroller.VerticalOffset > ContentScroller.ScrollableHeight - 900)
        {
            await Model.LoadMoreAsync();
        }
    }

    private async void OnSearchFilter(object sender, RoutedEventArgs e)
    {
        if (sender is Button { Tag: string filter })
        {
            await Model.RefilterSearchAsync(filter);
            ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
        }
    }

    private async void OnQueueItemClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is TrackViewModel entry)
        {
            await PlayEntryAsync(Model.JumpTo(entry));
        }
    }

    /// <summary>Pages a shelf sideways by most of its visible width, like the carousel arrows.</summary>
    private void OnShelfScroll(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { Tag: string direction } button)
        {
            return;
        }

        // Arrow → its StackPanel → the header Grid → the shelf, whose next child is the carousel.
        if (VisualTreeHelper.GetParent(button) is FrameworkElement arrows
            && VisualTreeHelper.GetParent(arrows) is FrameworkElement header
            && VisualTreeHelper.GetParent(header) is Panel shelf
            && shelf.Children.Count > 1
            && shelf.Children[1] is ScrollViewer carousel)
        {
            var step = Math.Max(200, carousel.ViewportWidth * 0.8) * (direction == "-1" ? -1 : 1);
            carousel.ChangeView(Math.Max(0, carousel.HorizontalOffset + step), null, null);
        }
    }

    // ---- Playing ----------------------------------------------------------------------------

    /// <summary>A card either plays its track or opens the album, playlist or artist it names.</summary>
    private async void OnCardActivated(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { Tag: string tag } || tag.Length == 0)
        {
            return;
        }

        var parts = tag.Split(ShellViewModel.KeySeparator);
        if (parts.Length == 3)
        {
            await OpenAsync(parts[0], parts[1], parts[2]);
            return;
        }

        OnPlayTrack(sender, e);
    }

    /// <summary>
    /// Plays a row, once there is somewhere to play it.
    /// </summary>
    /// <remarks>
    /// A row in Playing Next moves the queue; a row anywhere else starts a new queue from the
    /// rows around it. Either way the lease is claimed before anything renders, because Rust
    /// decides whether a transition is allowed and a renderer that started first would have
    /// escaped that.
    /// </remarks>
    private async void OnPlayTrack(object sender, RoutedEventArgs e)
    {
        if (sender is not Button button)
        {
            return;
        }

        TrackViewModel? entry = button.DataContext switch
        {
            TrackViewModel track when Model.IsQueueEntry(track) => Model.JumpTo(track),
            TrackViewModel track => Model.PlayFromPage(track),
            CardViewModel card => Model.PlayFromShelf(card),
            _ => null,
        };
        await PlayEntryAsync(entry);
    }

    private async Task PlayEntryAsync(TrackViewModel? entry)
    {
        if (entry?.VideoId is not { Length: > 0 } videoId)
        {
            return;
        }

        if (_playback is null)
        {
            Model.ReportStatus("There is no service to claim playback from.");
            return;
        }

        await _playback.PlayAsync(videoId);
    }

    private async void OnPlayPage(object sender, RoutedEventArgs e) => await PlayEntryAsync(Model.PlayPage(shuffle: false));

    private async void OnShufflePage(object sender, RoutedEventArgs e) => await PlayEntryAsync(Model.PlayPage(shuffle: true));

    private void OnShuffle(object sender, RoutedEventArgs e) => Model.ToggleShuffle();

    private void OnRepeat(object sender, RoutedEventArgs e) => Model.CycleRepeat();

    private async void OnPrevious(object sender, RoutedEventArgs e) => await PreviousAsync();

    /// <summary>Restarts the track after its first few seconds, as every player does; otherwise goes back.</summary>
    private async Task PreviousAsync()
    {
        if (_playback is not null && Model.PlaybackPosition > 3)
        {
            await _playback.SeekAsync(0);
            return;
        }

        await AdvanceAsync(forward: false, natural: false);
    }

    private async void OnNext(object sender, RoutedEventArgs e) => await AdvanceAsync(forward: true, natural: false);

    private async Task AdvanceAsync(bool forward, bool natural)
    {
        if (forward && Model.CanExtendRadio && Model.Repeat != RepeatMode.One)
        {
            await Model.ExtendRadioAsync();
        }

        var entry = Model.Advance(forward, natural);
        if (entry is null && natural)
        {
            entry = await Model.AutoplayAfterAsync();
            if (entry is null)
            {
                Model.ReportStatus("The queue has finished.");
            }
        }

        await PlayEntryAsync(entry);
    }

    private async void OnPlayPause(object sender, RoutedEventArgs e) => await TogglePauseAsync();

    private async Task TogglePauseAsync()
    {
        if (!Model.HasPlayback)
        {
            return;
        }

        if (_playback is null)
        {
            Model.ReportStatus("There is no service to claim playback from.");
            return;
        }

        await _playback.TogglePauseAsync();
    }

    // ---- Transport gestures -----------------------------------------------------------------

    private double _scrubPosition;

    /// <summary>Keeps the position line in step with what the page confirmed.</summary>
    private void WireSeekGestures()
    {
        Model.PropertyChanged += (_, args) =>
        {
            if (args.PropertyName is nameof(ShellViewModel.PlaybackPosition) or nameof(ShellViewModel.PlaybackDuration))
            {
                DrawProgress(_seeking ? _scrubPosition : Model.PlaybackPosition);
            }
        };
    }

    private void DrawProgress(double position)
    {
        var width = PlaybackProgress.ActualWidth;
        var duration = Model.PlaybackDuration;
        PillFill.Width = duration > 0 && width > 0 ? Math.Clamp(position / duration, 0, 1) * width : 0;
    }

    private double PositionAt(PointerRoutedEventArgs e)
    {
        var x = e.GetCurrentPoint(PlaybackProgress).Position.X;
        var width = Math.Max(1, PlaybackProgress.ActualWidth);
        return Math.Clamp(x / width, 0, 1) * Model.PlaybackDuration;
    }

    private void OnProgressSizeChanged(object sender, SizeChangedEventArgs e) =>
        DrawProgress(_seeking ? _scrubPosition : Model.PlaybackPosition);

    private void OnProgressPressed(object sender, PointerRoutedEventArgs e)
    {
        if (!Model.IsSeekable)
        {
            return;
        }

        _seeking = true;
        Model.IsScrubbing = true;
        PlaybackProgress.CapturePointer(e.Pointer);
        PillTrack.Height = PillFill.Height = 5;
        _scrubPosition = PositionAt(e);
        DrawProgress(_scrubPosition);
        e.Handled = true;
    }

    private void OnProgressMoved(object sender, PointerRoutedEventArgs e)
    {
        if (_seeking)
        {
            _scrubPosition = PositionAt(e);
            DrawProgress(_scrubPosition);
        }
    }

    private async void OnProgressReleased(object sender, PointerRoutedEventArgs e)
    {
        PlaybackProgress.ReleasePointerCapture(e.Pointer);
        await FinishSeekAsync();
    }

    private async void OnProgressCaptureLost(object sender, PointerRoutedEventArgs e) => await FinishSeekAsync();

    private async Task FinishSeekAsync()
    {
        if (!_seeking)
        {
            return;
        }

        _seeking = false;
        Model.IsScrubbing = false;
        PillTrack.Height = PillFill.Height = 3;
        if (_playback is not null)
        {
            await _playback.SeekAsync(_scrubPosition);
        }
    }

    /// <summary>
    /// Sends a volume the listener chose, and ignores the ones the page reported.
    /// </summary>
    /// <remarks>
    /// The slider is bound to the confirmed volume, so every sample also raises ValueChanged. A
    /// value equal to what the page last confirmed is that echo, not a choice, and sending it
    /// back would fight a listener mid-drag with their own previous position.
    /// </remarks>
    private async void OnVolumeChanged(object sender, RangeBaseValueChangedEventArgs e)
    {
        if (_playback is null || Math.Abs(e.NewValue - Model.VolumePercent) < 0.5)
        {
            return;
        }

        await _playback.SetVolumeAsync(e.NewValue / 100.0);
        Model.RememberVolume(e.NewValue / 100.0, _playback.PreferredMuted);
    }

    private async void OnToggleMuted(object sender, RoutedEventArgs e)
    {
        if (_playback is not null)
        {
            await _playback.ToggleMutedAsync();
            Model.RememberVolume(Model.Volume, _playback.PreferredMuted);
        }
    }
}
