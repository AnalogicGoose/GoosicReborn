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

    private async void OnClearQueue(object sender, RoutedEventArgs e)
    {
        if (await ConfirmAsync("Clear Playing Next?", "Remove every upcoming track from the queue?", "Clear"))
        {
            Model.ClearUpcoming();
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

        if (LyricsItems.Visibility != Visibility.Visible)
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
    private void ApplySidePanel()
    {
        var lyrics = _sidePanel.Content == SidePanelContent.Lyrics;
        var queue = _sidePanel.Content == SidePanelContent.Queue;
        LyricsButton.IsChecked = lyrics;
        QueueButton.IsChecked = queue;
        LyricsItems.Visibility = lyrics ? Visibility.Visible : Visibility.Collapsed;
        LyricsEmptyState.Visibility = lyrics && Model.Lyrics.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
        QueuePanel.Visibility = queue ? Visibility.Visible : Visibility.Collapsed;
        SidePanelMessage.Text = lyrics ? Model.LyricsStatus : "";
        SidePanel.Visibility = _sidePanel.IsOpen ? Visibility.Visible : Visibility.Collapsed;
        ApplyInsets();
    }
}
