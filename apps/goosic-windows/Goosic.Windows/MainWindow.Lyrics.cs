using System;
using Goosic.Windows.Service;
using Goosic.Windows.ViewModels;
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
        _fullLyrics = new LyricsEffects(FullPlayerLyricsScroller, FullPlayerLyricsContent, FullPlayerLyricsItems,
            FullPlayerLyricsIntroduction, FullPlayerLyricsResume, immersive: true, Model.LyricFollower,
            () => AnimationsEnabled, () => PlainEffects, () => CanSeekLyrics);
        // In the panel the sung line takes the accent, as on macOS; over a cover it stays white.
        Application.Current.Resources.TryGetValue("GoosicAccentBrush", out var accent);
        _panelLyrics = new LyricsEffects(LyricsScroller, LyricsContent, LyricsItems, LyricsIntroduction,
            LyricsResume, immersive: false, Model.LyricFollower,
            () => AnimationsEnabled, () => PlainEffects, () => CanSeekLyrics, accent as Microsoft.UI.Xaml.Media.Brush);
        _fullLyrics.Resumed += () => FollowLyricOnScreen(Model.CurrentLyricIndex);
        _panelLyrics.Resumed += () => FollowLyricOnScreen(Model.CurrentLyricIndex);
        Model.LyricClockChanged += () =>
        {
            _fullLyrics.ClockChanged();
            _panelLyrics.ClockChanged();
        };
        Closed += (_, _) =>
        {
            _fullLyrics.SetVisible(false);
            _panelLyrics.SetVisible(false);
        };
        Model.LyricsLoaded += () =>
        {
            FollowLyricsAgain();
            DispatcherQueue.TryEnqueue(() => FollowLyricOnScreen(Model.CurrentLyricIndex));
        };
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

    /// <summary>
    /// Seeks to a synced line the listener chose, from either view, as on macOS. Choosing a line is
    /// choosing to listen from it, so that view follows the song again. Unsynced lines carry no
    /// time and are not buttons in effect: a click on one does nothing.
    /// </summary>
    private async void OnLyricLineClicked(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: LyricLineViewModel { Synced: true } line } button
            || _playback is null || !CanSeekLyrics)
        {
            return;
        }

        (IsInFullPlayer(button) ? _fullLyrics : _panelLyrics)?.SetFollowing(true);
        await SeekToAsync(line.AtMilliseconds / 1000.0);
    }

    /// <summary>Whether a lyric line may seek right now: the macOS shell's <c>canSeek</c>.</summary>
    private bool CanSeekLyrics => Model.IsSeekable && _playback is not null;

    private bool IsInFullPlayer(DependencyObject element)
    {
        for (var node = element; node is not null; node = Microsoft.UI.Xaml.Media.VisualTreeHelper.GetParent(node))
        {
            if (ReferenceEquals(node, FullPlayerLyricsScroller))
            {
                return true;
            }
        }

        return false;
    }

    /// <summary>
    /// Lets the listener choose a timed-lyrics JSON file for the song playing now, for this
    /// listening session, as the macOS context menu does. The file is read here and checked by the
    /// model; nothing about it reaches the service or changes playback.
    /// </summary>
    private async void OnImportTimedLyrics(object sender, RoutedEventArgs e)
    {
        if (Model.ConfirmedTrack is not { } track)
        {
            Model.ReportStatus("Nothing is playing.");
            return;
        }

        var picker = new global::Windows.Storage.Pickers.FileOpenPicker();
        WinRT.Interop.InitializeWithWindow.Initialize(picker, WinRT.Interop.WindowNative.GetWindowHandle(this));
        picker.FileTypeFilter.Add(".json");
        picker.SuggestedStartLocation = global::Windows.Storage.Pickers.PickerLocationId.DocumentsLibrary;
        var file = await picker.PickSingleFileAsync();
        if (file is null)
        {
            return;
        }

        try
        {
            var size = (await file.GetBasicPropertiesAsync()).Size;
            if (size == 0 || size > ShellViewModel.MaxImportedLyricsBytes)
            {
                Model.ReportStatus("Choose a lyrics file smaller than 2 MB.");
                return;
            }

            var json = await global::Windows.Storage.FileIO.ReadTextAsync(file);
            var document = System.Text.Json.JsonSerializer.Deserialize<LyricsDocument>(json, ServiceProtocol.Json);
            if (document is null)
            {
                throw new System.Text.Json.JsonException("empty document");
            }

            Model.InstallImportedLyrics(document, track);
        }
        catch (Exception error) when (error is System.Text.Json.JsonException or System.IO.IOException
            or UnauthorizedAccessException or ArgumentException or NotSupportedException)
        {
            BridgeLog.Write($"lyrics import failed: {error.GetType().Name}");
            Model.ReportStatus("Could not read that lyrics file. Check its JSON format and try again.");
        }
    }

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
        if (FullPlayer.Visibility == Visibility.Visible)
        {
            _fullLyrics?.Follow(true);
        }

        if (LyricsPanel.Visibility == Visibility.Visible)
        {
            _panelLyrics?.Follow(true);
        }
    }

    /// <summary>A new song is followed from its start, however the last one was left.</summary>
    private void FollowLyricsAgain()
    {
        _fullLyrics?.Reset();
        _panelLyrics?.Reset();
        _fullLyrics?.SetFollowing(true);
        _panelLyrics?.SetFollowing(true);
    }
}
