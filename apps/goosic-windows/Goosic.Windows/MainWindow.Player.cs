using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
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

    /// <summary>How far the position line draws in at each end to make room for a time.</summary>
    private const double ProgressTimeInset = 40;

    private Microsoft.UI.Dispatching.DispatcherQueueTimer? _progressHoverTimer;
    private bool _progressHovered;
    private bool _progressTimesShown;

    /// <summary>
    /// Pointing at the position line, and only at it, shows the times either side of it.
    /// </summary>
    /// <remarks>
    /// The pause before it reacts is the macOS player's: long enough that a pointer crossing the
    /// pill on its way elsewhere changes nothing, and uneven so that the line settling under a
    /// pointer resting on its edge cannot start a leave-and-enter loop.
    /// </remarks>
    private void OnProgressPointerEntered(object sender, PointerRoutedEventArgs e) => ScheduleProgressHover(true);

    private void OnProgressPointerExited(object sender, PointerRoutedEventArgs e) => ScheduleProgressHover(false);

    private void ScheduleProgressHover(bool hovered)
    {
        if (_progressHoverTimer is null)
        {
            _progressHoverTimer = DispatcherQueue.CreateTimer();
            _progressHoverTimer.IsRepeating = false;
            _progressHoverTimer.Tick += (_, _) => UpdateProgressExpansion();
        }

        _progressHovered = hovered;
        _progressHoverTimer.Stop();
        _progressHoverTimer.Interval = TimeSpan.FromMilliseconds(hovered ? 240 : 140);
        _progressHoverTimer.Start();
    }

    /// <summary>
    /// Draws the position line as a hairline, or shortened between its two times and thicker.
    /// </summary>
    /// <remarks>
    /// Only the line's own scale and the labels' opacity change, so nothing is measured again and
    /// the pill keeps its size. The length is left alone during a drag: the pointer is being read
    /// against the line, and a line that moved under it would move the song with it.
    /// </remarks>
    private void UpdateProgressExpansion()
    {
        if (!_seeking)
        {
            _progressTimesShown = _progressHovered && Model.HasPlayback;
        }

        var width = PillLine.ActualWidth;
        var length = _progressTimesShown && width > 0
            ? Math.Max(0.2, (width - 2 * ProgressTimeInset) / width)
            : 1;
        var thick = _progressTimesShown || _seeking;
        PillLine.CenterPoint = new System.Numerics.Vector3((float)(width / 2), (float)(PillLine.ActualHeight / 2), 0);
        PillLine.Scale = new System.Numerics.Vector3((float)length, thick ? 4f / 3f : 1f, 1);
        PillElapsed.Opacity = PillRemaining.Opacity = _progressTimesShown ? 1 : 0;
    }

    /// <summary>Pointing at the cover shows that clicking it opens the full-screen player.</summary>
    private void OnCoverPointerEntered(object sender, PointerRoutedEventArgs e)
    {
        FadePlayerElement(PillCoverHover, 1);
        SpringPlayerScale(PillCoverButton, 1.06f);
    }

    private void OnCoverPointerExited(object sender, PointerRoutedEventArgs e)
    {
        FadePlayerElement(PillCoverHover, 0);
        SpringPlayerScale(PillCoverButton, 1);
    }

    /// <summary>The pill's "more" menu: what the row menu offers, for the track that is playing.</summary>
    private void OnNowPlayingMore(object sender, RoutedEventArgs e)
    {
        if (Model.ConfirmedTrack is not { } track)
        {
            Model.ReportStatus("Nothing is playing.");
            return;
        }

        BuildNowPlayingMenu(track)
            .ShowAt(PillMoreButton, new FlyoutShowOptions { Placement = FlyoutPlacementMode.TopEdgeAlignedRight });
    }

    private async void OnAccount(object sender, RoutedEventArgs e)
    {
        await Model.RefreshAccountsAsync();
    }

    /// <summary>Loads the next part of a long page as the reader nears its end, as YouTube Music does.</summary>
    private async void OnContentViewChanged(object? sender, ScrollViewerViewChangedEventArgs e) =>
        await LoadMoreIfNearEndAsync();

    /// <summary>
    /// Asks for more when the end of the page is in view — including when the page is too short
    /// to scroll at all, which no scroll event would ever report. There is no button to press.
    /// </summary>
    private async Task LoadMoreIfNearEndAsync()
    {
        if (Model.HasMore && ContentScroller.VerticalOffset > ContentScroller.ScrollableHeight - 900)
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

        // Arrow → its StackPanel → the header Grid → the shelf, which holds a card carousel and a
        // row carousel; only the one matching the shelf's layout is visible.
        if (VisualTreeHelper.GetParent(button) is FrameworkElement arrows
            && VisualTreeHelper.GetParent(arrows) is FrameworkElement header
            && VisualTreeHelper.GetParent(header) is Panel shelf
            && shelf.Children.OfType<ScrollViewer>().FirstOrDefault(child => child.Visibility == Visibility.Visible)
                is { } carousel)
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
            if (((Button)sender).DataContext is CardViewModel card)
            {
                Model.RememberEntityThumbnail(card.Kind, card.Id, card.Thumbnail);
            }

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

        // While a playlist is being edited, a row is something to choose, not something to play.
        if (Model.IsEditingPlaylist && button.DataContext is TrackViewModel { } editing && !Model.IsQueueEntry(editing))
        {
            editing.IsSelected = !editing.IsSelected;
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
            Model.ReportDetail("There is no service to claim playback from.");
            return;
        }

        await _playback.PlayAsync(videoId);
    }

    private async void OnPlayPage(object sender, RoutedEventArgs e)
    {
        // The list already playing is paused and resumed from here rather than restarted.
        if (Model.IsPageQueued)
        {
            await TogglePauseAsync();
            return;
        }

        await PlayEntryAsync(Model.PlayPage(shuffle: false));
    }

    private async void OnShufflePage(object sender, RoutedEventArgs e) => await PlayEntryAsync(Model.PlayPage(shuffle: true));

    private void OnShuffle(object sender, RoutedEventArgs e) => Model.ToggleShuffle();

    private void OnRepeat(object sender, RoutedEventArgs e) => Model.CycleRepeat();

    private async void OnPrevious(object sender, RoutedEventArgs e)
    {
        NudgePlayerControl(sender as UIElement, -1);
        await PreviousAsync();
    }

    /// <summary>Restarts the track after its first few seconds, as every player does; otherwise goes back.</summary>
    private async Task PreviousAsync()
    {
        if (!Model.HasPlayback || Model.NowPlayingEntry is null || !Model.CanChangeTrack()) return;
        if (_playback is not null && Model.IsSeekable && Model.PlaybackPosition > 3)
        {
            await SeekToAsync(0);
            return;
        }

        await AdvanceAsync(forward: false, natural: false);
    }

    private async void OnNext(object sender, RoutedEventArgs e)
    {
        NudgePlayerControl(sender as UIElement, 1);
        await AdvanceAsync(forward: true, natural: false);
    }

    private async Task AdvanceAsync(bool forward, bool natural)
    {
        if (_advancingQueue) return;
        _advancingQueue = true;
        try
        {
            await AdvanceCoreAsync(forward, natural);
        }
        finally
        {
            _advancingQueue = false;
        }
    }

    private bool _advancingQueue;

    private async Task AdvanceCoreAsync(bool forward, bool natural)
    {
        // The sleep timer's "end of this song": the song has ended, so nothing follows it.
        if (natural && TakeSleepAtEndOfSong())
        {
            Model.ReportStatus("Sleep timer: stopped at the end of the song.");
            return;
        }

        var move = await Model.MoveAsync(forward, natural);
        if (move.Entry is null)
        {
            return;
        }

        // Previous on the first song starts it again. A natural end under repeat-one has to load
        // the song afresh, because the page has already finished it.
        if (move.Restart && !natural && _playback is not null)
        {
            await SeekToAsync(0);
            return;
        }

        await PlayEntryAsync(move.Entry);
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
            Model.ReportDetail("There is no service to claim playback from.");
            return;
        }

        Model.NoteListenerToggle();
        await _playback.TogglePauseAsync();
    }

    // ---- Transport gestures -----------------------------------------------------------------

    private double _scrubPosition;

    /// <summary>Keeps the position line in step with what the page confirmed.</summary>
    private void WireSeekGestures()
    {
        // Slider's own pointer handling must not consume the wheel before the volume action.
        VolumeSlider.AddHandler(UIElement.PointerWheelChangedEvent, new PointerEventHandler(OnVolumeWheel), true);
        FullPlayerVolume.AddHandler(UIElement.PointerWheelChangedEvent, new PointerEventHandler(OnVolumeWheel), true);
        // The speaker takes the wheel too, so the volume can be turned without opening anything.
        VolumeButton.AddHandler(UIElement.PointerWheelChangedEvent, new PointerEventHandler(OnVolumeWheel), true);
        Model.PropertyChanged += (_, args) =>
        {
            if (args.PropertyName is nameof(ShellViewModel.PlaybackPosition) or nameof(ShellViewModel.PlaybackDuration))
            {
                DrawProgress(_seeking ? _scrubPosition : Model.PlaybackPosition);
            }
            else if (args.PropertyName == nameof(ShellViewModel.IsSeekable))
            {
                // Still drawn while it cannot be moved, as on macOS, only fainter.
                PlaybackProgress.Opacity = Model.IsSeekable ? 1 : 0.45;
            }
        };
        PlaybackProgress.Opacity = Model.IsSeekable ? 1 : 0.45;
    }

    private void DrawProgress(double position)
    {
        var width = PillLine.ActualWidth;
        var duration = Model.PlaybackDuration;
        PillFill.Width = duration > 0 && width > 0 ? Math.Clamp(position / duration, 0, 1) * width : 0;
    }

    /// <summary>Reads the pointer against the line as it is drawn, which is shorter between the times.</summary>
    private double PositionAt(PointerRoutedEventArgs e)
    {
        var inset = _progressTimesShown ? ProgressTimeInset : 0;
        var x = e.GetCurrentPoint(PlaybackProgress).Position.X - inset;
        var width = Math.Max(1, PlaybackProgress.ActualWidth - 2 * inset);
        return Math.Clamp(x / width, 0, 1) * Model.PlaybackDuration;
    }

    private void OnProgressSizeChanged(object sender, SizeChangedEventArgs e)
    {
        DrawProgress(_seeking ? _scrubPosition : Model.PlaybackPosition);
        UpdateProgressExpansion();
    }

    private void OnProgressPressed(object sender, PointerRoutedEventArgs e)
    {
        if (!Model.IsSeekable)
        {
            return;
        }

        _seeking = true;
        Model.IsScrubbing = true;
        PlaybackProgress.CapturePointer(e.Pointer);
        UpdateProgressExpansion();
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
        UpdateProgressExpansion();
        // An advertisement can start while the thumb is held; the seek is dropped then.
        if (_playback is not null && Model.IsSeekable)
        {
            await SeekToAsync(_scrubPosition);
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
        if (_playback is null || Math.Abs(e.NewValue - Model.VolumePercent) < 0.5 || !Model.CanAdjustSound())
        {
            return;
        }

        var gain = Presentation.VolumeTaper.ToGain(e.NewValue / 100.0);
        await _playback.SetVolumeAsync(gain);
        Model.RememberVolume(gain, _playback.PreferredMuted);
    }

    private bool _volumeOpen;

    private void OnToggleVolume(object sender, RoutedEventArgs e) => SetVolumeOpen(!_volumeOpen);

    /// <summary>
    /// Opens the volume capsule around the speaker button, or closes it again.
    /// </summary>
    /// <remarks>
    /// The speaker is one button that stays where it is, so keyboard focus stays on it through
    /// both changes. That matters beyond tidiness: a focused control that disappears hands focus
    /// to the first control in the window, which is the search box, and the box answers focus by
    /// opening its recent searches. The slider is the one thing here that does go away, so focus
    /// is taken off it before it does.
    /// </remarks>
    private void SetVolumeOpen(bool open)
    {
        if (open == _volumeOpen)
        {
            return;
        }

        _volumeOpen = open;
        // The capsule covers these; they keep their place but stop being targets of any kind.
        foreach (var covered in new Control[] { PillLikeButton, LyricsButton, QueueButton })
        {
            covered.IsHitTestVisible = !open;
            covered.IsTabStop = !open;
            Microsoft.UI.Xaml.Automation.AutomationProperties.SetAccessibilityView(covered,
                open ? Microsoft.UI.Xaml.Automation.Peers.AccessibilityView.Raw
                     : Microsoft.UI.Xaml.Automation.Peers.AccessibilityView.Content);
        }

        var label = open ? "Hide volume controls" : "Show volume controls";
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(VolumeButton, label);
        ToolTipService.SetToolTip(VolumeButton, label);

        if (open)
        {
            PlayerVolumeCapsule.Visibility = Visibility.Visible;
        }
        else if (VolumeSlider.FocusState != FocusState.Unfocused)
        {
            VolumeButton.Focus(FocusState.Programmatic);
        }

        var storyboard = open ? VolumeOpenStoryboard : VolumeCloseStoryboard;
        storyboard.Begin();
        if (!AnimationsEnabled)
        {
            storyboard.SkipToFill();
        }
    }

    /// <summary>The capsule leaves the tree once it has faded, unless it was reopened meanwhile.</summary>
    private void OnVolumeCloseCompleted(object? sender, object e)
    {
        if (!_volumeOpen)
        {
            PlayerVolumeCapsule.Visibility = Visibility.Collapsed;
        }
    }

    /// <summary>
    /// A wheel over a volume control changes its slider, consuming the gesture once. Over the
    /// speaker button it changes the pill's slider, whether or not the capsule is showing it.
    /// </summary>
    private void OnVolumeWheel(object sender, PointerRoutedEventArgs e)
    {
        if (e.Handled) return;
        var slider = sender as Slider ?? VolumeSlider;
        if (!Model.CanAdjustSound()) return;
        var point = e.GetCurrentPoint((UIElement)sender).Properties;
        if (point.IsHorizontalMouseWheel || point.MouseWheelDelta == 0) return;
        slider.Value = Math.Clamp(slider.Value + point.MouseWheelDelta / 120.0 * 5, slider.Minimum, slider.Maximum);
        e.Handled = true;
    }

    private void OnDismissToast(object sender, RoutedEventArgs e) => Model.DismissToast();

    /// <summary>Every seek goes through here, so the position line shows it before the player confirms it.</summary>
    private async Task SeekToAsync(double target)
    {
        if (_playback is null)
        {
            return;
        }

        Model.BeginSeek(target);
        await _playback.SeekAsync(target);
    }

    private async void OnToggleMuted(object sender, RoutedEventArgs e)
    {
        if (_playback is not null && Model.CanAdjustSound())
        {
            await _playback.ToggleMutedAsync();
            Model.RememberVolume(Model.Volume, _playback.PreferredMuted);
        }
    }
}
