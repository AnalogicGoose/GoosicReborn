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
    // ---- Queue panel ------------------------------------------------------------------------

    private void OnRemoveFromQueue(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { DataContext: TrackViewModel entry })
        {
            Model.RemoveFromQueue(entry);
        }
    }

    private Microsoft.UI.Dispatching.DispatcherQueueTimer? _undoTimer;

    /// <summary>Clears at once and offers Undo, rather than asking first.</summary>
    private void OnClearQueue(object sender, RoutedEventArgs e)
    {
        var count = Model.UpNext.Count;
        if (!Model.ClearUpcoming())
        {
            return;
        }

        QueueUndoText.Text = count == 1 ? "Removed 1 track" : $"Removed {count} tracks";
        QueueUndoBar.Visibility = Visibility.Visible;
        QueueUndoButton.Focus(FocusState.Programmatic);
        _undoTimer ??= CreateUndoTimer();
        _undoTimer.Stop();
        _undoTimer.Start();
    }

    private Microsoft.UI.Dispatching.DispatcherQueueTimer CreateUndoTimer()
    {
        var timer = DispatcherQueue.CreateTimer();
        timer.Interval = ClearedQueue<TrackViewModel>.UndoWindow;
        timer.IsRepeating = false;
        timer.Tick += (_, _) => HideUndoBar();
        return timer;
    }

    private void OnUndoClearQueue(object sender, RoutedEventArgs e)
    {
        if (!Model.UndoClear())
        {
            ReportUndoUnavailable();
        }

        HideUndoBar();
        QueueItems.Focus(FocusState.Programmatic);
    }

    private void ReportUndoUnavailable() =>
        Model.ReportStatus("The queue can’t be restored after the track changed.");

    private void HideUndoBar()
    {
        _undoTimer?.Stop();
        if (QueueUndoBar.FocusState != FocusState.Unfocused || QueueUndoButton.FocusState != FocusState.Unfocused)
        {
            QueueItems.Focus(FocusState.Programmatic);
        }

        QueueUndoBar.Visibility = Visibility.Collapsed;
    }

    /// <summary>Alt+Up and Alt+Down move the focused Up Next row, so reordering needs no mouse.</summary>
    private void OnQueueItemKeyDown(object sender, KeyRoutedEventArgs e)
    {
        var alt = (Microsoft.UI.Input.InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Menu)
            & global::Windows.UI.Core.CoreVirtualKeyStates.Down) != 0;
        if (!alt || e.Key is not (VirtualKey.Up or VirtualKey.Down)
            || FocusManager.GetFocusedElement(RootGrid.XamlRoot) is not ListViewItem { Content: TrackViewModel entry })
        {
            return;
        }

        e.Handled = true;
        if (Model.MoveUpNext(entry, e.Key == VirtualKey.Up ? -1 : 1)
            && QueueItems.ContainerFromItem(entry) is ListViewItem moved)
        {
            moved.Focus(FocusState.Keyboard);
        }
    }

    // ---- Side panel -------------------------------------------------------------------------

    /// <summary>Keeps the line being sung in the upper part of the lyrics panel.</summary>
    private void FollowLyricOnScreen(int index)
    {
        if (FullPlayer.Visibility == Visibility.Visible
            && FullPlayerLyricsItems.ContainerFromIndex(index) is FrameworkElement fullLine)
        {
            fullLine.StartBringIntoView(new BringIntoViewOptions { VerticalAlignmentRatio = 0.36, AnimationDesired = true });
        }

        if (LyricsPanel.Visibility != Visibility.Visible)
        {
            return;
        }

        if (LyricsItems.ContainerFromIndex(index) is FrameworkElement line)
        {
            line.StartBringIntoView(new BringIntoViewOptions
            {
                VerticalAlignmentRatio = 0.35,
                AnimationDesired = true,
            });
        }
    }

    private readonly SidePanelState _sidePanel = new();

    private async void OnToggleLyrics(object sender, RoutedEventArgs e) =>
        await ToggleSidePanelAsync(SidePanelContent.Lyrics);

    private async void OnToggleQueue(object sender, RoutedEventArgs e) =>
        await ToggleSidePanelAsync(SidePanelContent.Queue);

    private void OnCloseSidePanel(object sender, RoutedEventArgs e) => CloseSidePanel();

    private async Task ToggleSidePanelAsync(SidePanelContent content)
    {
        var available = content == SidePanelContent.Lyrics ? Model.HasPlayback : Model.HasQueue;
        var showing = _sidePanel.Toggle(content, available);
        ApplySidePanel();
        if (showing == SidePanelContent.Lyrics)
        {
            await Model.LoadLyricsAsync();
            if (_sidePanel.Content == SidePanelContent.Lyrics)
            {
                ApplySidePanel();
            }
        }
    }

    private void CloseSidePanel()
    {
        var focus = _sidePanel.Close();
        ApplySidePanel();
        (focus == SidePanelContent.Lyrics ? LyricsButton : QueueButton).Focus(FocusState.Programmatic);
    }

    private async void OnRetryLyrics(object sender, RoutedEventArgs e)
    {
        if (sender is Button button)
        {
            button.IsEnabled = false;
            await Model.LoadLyricsAsync();
            button.IsEnabled = true;
        }

        ApplySidePanel();
    }

    /// <summary>Mirrors <see cref="_sidePanel"/> onto the toggles and the panel's contents.</summary>
    /// <remarks>What each panel shows inside is bound to the model; this only picks the panel.</remarks>
    private void ApplySidePanel()
    {
        var lyrics = _sidePanel.Content == SidePanelContent.Lyrics;
        var queue = _sidePanel.Content == SidePanelContent.Queue;
        LyricsButton.IsChecked = lyrics;
        QueueButton.IsChecked = queue;
        LyricsPanel.Visibility = lyrics ? Visibility.Visible : Visibility.Collapsed;
        QueuePanel.Visibility = queue ? Visibility.Visible : Visibility.Collapsed;
        if (!queue)
        {
            QueueUndoBar.Visibility = Visibility.Collapsed;
        }

        SidePanel.Visibility = _sidePanel.IsOpen ? Visibility.Visible : Visibility.Collapsed;
        ApplyInsets();
    }
}
