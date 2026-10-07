using Goosic.Windows.Service;
using Goosic.Windows.Views;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Goosic.Windows;

/// <summary>
/// The two lyrics views, in the side panel and in the full-screen player, and the page's fade
/// under the title bar, which is drawn the same way as theirs.
/// </summary>
public sealed partial class MainWindow
{
    private readonly global::Windows.UI.ViewManagement.AccessibilitySettings _accessibility = new();
    private LyricsEffects? _fullLyrics;
    private LyricsEffects? _panelLyrics;

    /// <summary>
    /// Whether to leave out what is drawn through another surface: in a contrast theme, with
    /// Windows' transparency effects off, and in Efficiency mode, which exists to skip exactly
    /// this kind of work.
    /// </summary>
    private bool PlainEffects =>
        _accessibility.HighContrast || !_uiSettings.AdvancedEffectsEnabled || ShellPreferences.EfficiencyMode;

    private void WireLyrics()
    {
        _fullLyrics = new LyricsEffects(FullPlayerLyricsScroller, FullPlayerLyricsResume, immersive: true,
            () => AnimationsEnabled, () => PlainEffects);
        // In the panel the sung line takes the accent, as on macOS; over a cover it stays white.
        Application.Current.Resources.TryGetValue("GoosicAccentBrush", out var accent);
        _panelLyrics = new LyricsEffects(LyricsScroller, LyricsResume, immersive: false,
            () => AnimationsEnabled, () => PlainEffects, accent as Microsoft.UI.Xaml.Media.Brush);
        _fullLyrics.Resumed += () => FollowLyricOnScreen(Model.CurrentLyricIndex);
        _panelLyrics.Resumed += () => FollowLyricOnScreen(Model.CurrentLyricIndex);
        if (PlainEffects)
        {
            return;
        }

        // The page thins out under the title bar and the lyrics at both ends; where that cannot
        // be done the page keeps its plain gradient and the lyrics simply end at their edges.
        if (FadeMask.Attach(ContentMaskHost, ContentScroller, ContentMask))
        {
            TitleFade.Visibility = Visibility.Collapsed;
        }

        FadeMask.Attach(FullPlayerLyricsMaskHost, FullPlayerLyricsScroller, FullPlayerLyricsMask);
        FadeMask.Attach(LyricsMaskHost, LyricsScroller, LyricsMask);
    }

    private void OnFullLyricLineLoaded(object sender, RoutedEventArgs e) => _fullLyrics?.Add((TextBlock)sender);

    private void OnFullLyricLineUnloaded(object sender, RoutedEventArgs e) => _fullLyrics?.Remove((TextBlock)sender);

    private void OnPanelLyricLineLoaded(object sender, RoutedEventArgs e) => _panelLyrics?.Add((TextBlock)sender);

    private void OnPanelLyricLineUnloaded(object sender, RoutedEventArgs e) => _panelLyrics?.Remove((TextBlock)sender);

    private void OnResumeFullLyrics(object sender, RoutedEventArgs e) => _fullLyrics?.SetFollowing(true);

    private void OnResumePanelLyrics(object sender, RoutedEventArgs e) => _panelLyrics?.SetFollowing(true);

    /// <summary>
    /// Redraws both views for the line the song has reached, and keeps that line in the upper
    /// part of whichever view is following it.
    /// </summary>
    /// <remarks>
    /// A view the listener has scrolled is left where they put it. Pulling it back on every line
    /// would make reading ahead impossible, which is what scrolling it was for.
    /// </remarks>
    private void FollowLyricOnScreen(int index)
    {
        _fullLyrics?.Refresh();
        _panelLyrics?.Refresh();
        if (index < 0)
        {
            return;
        }

        if (FullPlayer.Visibility == Visibility.Visible && _fullLyrics?.Following == true
            && FullPlayerLyricsItems.ContainerFromIndex(index) is FrameworkElement fullLine)
        {
            fullLine.StartBringIntoView(new BringIntoViewOptions { VerticalAlignmentRatio = 0.36, AnimationDesired = true });
        }

        if (LyricsPanel.Visibility == Visibility.Visible && _panelLyrics?.Following == true
            && LyricsItems.ContainerFromIndex(index) is FrameworkElement line)
        {
            line.StartBringIntoView(new BringIntoViewOptions { VerticalAlignmentRatio = 0.36, AnimationDesired = true });
        }
    }

    /// <summary>A new song is followed from its start, however the last one was left.</summary>
    private void FollowLyricsAgain()
    {
        _fullLyrics?.SetFollowing(true);
        _panelLyrics?.SetFollowing(true);
    }
}
