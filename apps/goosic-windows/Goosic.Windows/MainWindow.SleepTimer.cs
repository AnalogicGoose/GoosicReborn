using System;
using System.Threading.Tasks;
using Goosic.Windows.Presentation;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Graphics;

namespace Goosic.Windows;

public sealed partial class MainWindow : Window
{
    // ---- Sleep timer ------------------------------------------------------------------------

    private DispatcherTimer? _sleepTimer;
    private DateTimeOffset _sleepAt;
    private bool _sleepAtEndOfSong;

    /// <summary>
    /// Stop after a while, or when this song ends, as Apple Music and Spotify offer. It pauses
    /// rather than quitting, so the queue is where it was in the morning.
    /// </summary>
    private MenuFlyoutSubItem BuildSleepTimerMenu()
    {
        var menu = new MenuFlyoutSubItem
        {
            Text = SleepTimerLabel(),
            Icon = new FontIcon { Glyph = "" },
        };
        foreach (var minutes in new[] { 15, 30, 45, 60 })
        {
            var item = new MenuFlyoutItem { Text = $"{minutes} minutes" };
            item.Click += (_, _) => StartSleepTimer(TimeSpan.FromMinutes(minutes));
            menu.Items.Add(item);
        }

        var end = new MenuFlyoutItem { Text = "End of this song" };
        end.Click += (_, _) =>
        {
            CancelSleepTimer();
            _sleepAtEndOfSong = true;
            Model.ReportStatus("Goosic will stop when this song ends.");
        };
        menu.Items.Add(end);

        if (_sleepTimer is not null || _sleepAtEndOfSong)
        {
            menu.Items.Add(new MenuFlyoutSeparator());
            var off = new MenuFlyoutItem { Text = "Turn off sleep timer" };
            off.Click += (_, _) =>
            {
                CancelSleepTimer();
                Model.ReportStatus("Sleep timer off.");
            };
            menu.Items.Add(off);
        }

        return menu;
    }

    private string SleepTimerLabel() =>
        PlayerText.SleepTimer(_sleepTimer is null ? null : _sleepAt - DateTimeOffset.Now, _sleepAtEndOfSong);

    private void StartSleepTimer(TimeSpan after)
    {
        CancelSleepTimer();
        _sleepAt = DateTimeOffset.Now + after;
        _sleepTimer = new DispatcherTimer { Interval = after };
        _sleepTimer.Tick += async (_, _) =>
        {
            CancelSleepTimer();
            await PauseForSleepAsync();
        };
        _sleepTimer.Start();
        Model.ReportStatus($"Goosic will stop in {(int)after.TotalMinutes} minutes.");
    }

    private void CancelSleepTimer()
    {
        _sleepTimer?.Stop();
        _sleepTimer = null;
        _sleepAtEndOfSong = false;
    }

    /// <summary>Whether a song that just ended should be the last, and clears the request if so.</summary>
    private bool TakeSleepAtEndOfSong()
    {
        if (!_sleepAtEndOfSong)
        {
            return false;
        }

        _sleepAtEndOfSong = false;
        return true;
    }

    private async Task PauseForSleepAsync()
    {
        if (Model.IsPlaying)
        {
            await TogglePauseAsync();
        }
    }

    // ---- Mini player ------------------------------------------------------------------------

    private void OnToggleMiniPlayer(object sender, RoutedEventArgs e) => SetMiniPlayer(!_miniPlayer);

    private bool _miniPlayer;
    private bool _lyricsBeforeMiniPlayer = true;
    private RectInt32? _beforeMiniPlayer;

    /// <summary>
    /// A small always-on-top window with the cover and the controls, as Apple Music's mini
    /// player and Spotify's miniplayer are. It is the full-screen player in a compact overlay
    /// window, so there is one player layout rather than two to keep in step.
    /// </summary>
    private void SetMiniPlayer(bool on)
    {
        if (on == _miniPlayer)
        {
            return;
        }

        _miniPlayer = on;
        // The mini player is the cover and the controls; lyrics and filling the screen are what
        // the full player is for, so they step aside and come back as they were.
        if (on)
        {
            _lyricsBeforeMiniPlayer = FullPlayerLyricsToggle.IsChecked == true;
            FullPlayerLyricsToggle.IsChecked = false;
        }
        else
        {
            FullPlayerLyricsToggle.IsChecked = _lyricsBeforeMiniPlayer;
        }

        var inMini = on ? Visibility.Collapsed : Visibility.Visible;
        FullPlayerLyricsToggle.Visibility = inMini;
        FullPlayerWindowToggle.Visibility = inMini;
        if (on)
        {
            _beforeMiniPlayer = new RectInt32(AppWindow.Position.X, AppWindow.Position.Y, AppWindow.Size.Width, AppWindow.Size.Height);
            SetFullPlayerOpen(true);
            AppWindow.SetPresenter(AppWindowPresenterKind.CompactOverlay);
            var scale = RootGrid.XamlRoot?.RasterizationScale ?? 1.0;
            AppWindow.Resize(new SizeInt32((int)(360 * scale), (int)(560 * scale)));
        }
        else
        {
            AppWindow.SetPresenter(AppWindowPresenterKind.Overlapped);
            ApplyMinimumWindowSize();
            if (_beforeMiniPlayer is { } bounds)
            {
                AppWindow.MoveAndResize(bounds);
            }
        }

        MiniPlayerToggle.IsChecked = on;
        ApplyFullPlayerLayout();
    }
}
