using System;
using System.IO;
using Goosic.Windows.Presentation;
using Goosic.Windows.Service;
using Goosic.Windows.ViewModels;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Graphics;

namespace Goosic.Windows;

public sealed partial class MainWindow : Window
{
    /// <summary>Opens the folder holding the log, so it can be attached to a bug report.</summary>
    private void OnOpenLogFolder(object sender, RoutedEventArgs e)
    {
        var folder = System.IO.Path.GetDirectoryName(BridgeLog.Location)!;
        System.IO.Directory.CreateDirectory(folder);
        System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo(folder) { UseShellExecute = true });
    }

    // ---- How the program behaves on Windows ---------------------------------------------------

    private TrayIcon? _tray;
    private NowPlayingNotifier? _notifier;
    private DiscordPresence? _discord;
    private bool _quitting;
    private bool _windowActive = true;
    private bool _backgroundLaunchHandled;

    private static string IconPath => Path.Combine(AppContext.BaseDirectory, "Assets", "Goosic.ico");

    /// <summary>
    /// Applies the Windows-only preferences: icon, window placement, tray, sign-in launch,
    /// notifications, Discord, and Efficiency mode.
    /// </summary>
    private void WireShellSettings()
    {
        if (File.Exists(IconPath))
        {
            AppWindow.SetIcon(IconPath);
        }

        RestorePlacement();
        EfficiencyMode.Apply(ShellPreferences.EfficiencyMode);
        ApplyTray();
        ApplyNotifications();
        ApplyDiscord();

        AppWindow.Closing += OnAppWindowClosing;
        Activated += (_, e) =>
        {
            _windowActive = e.WindowActivationState != WindowActivationState.Deactivated;
            HandleBackgroundLaunch();
        };
        Closed += (_, _) =>
        {
            _tray?.Dispose();
            _notifier?.Dispose();
            _discord?.Dispose();
        };

        Model.ConfirmedTrackChanged += OnConfirmedTrackForShell;
        Model.PropertyChanged += (_, e) =>
        {
            if (e.PropertyName is nameof(ShellViewModel.IsPlaying) or nameof(ShellViewModel.PlaybackDuration))
            {
                UpdateDiscord();
            }
        };
        Model.OpeningRouteChosen += HighlightNavigation;
        WireSettingsControls();
    }

    // ---- Closing, the tray, and starting at sign-in ----

    /// <summary>With Close to tray on, the close button hides the window and the music keeps playing.</summary>
    private void OnAppWindowClosing(AppWindow sender, AppWindowClosingEventArgs args)
    {
        SavePlacement();
        if (ShellPreferences.CloseToTray && !_quitting && _tray is not null)
        {
            args.Cancel = true;
            sender.Hide();
        }
    }

    private void ApplyTray()
    {
        if (ShellPreferences.CloseToTray && _tray is null)
        {
            try
            {
                _tray = new TrayIcon(Environment.ProcessPath);
                _tray.Opened += ShowFromTray;
                _tray.PlayPauseRequested += () => _ = TogglePauseAsync();
                _tray.NextRequested += () => _ = AdvanceAsync(forward: true, natural: false);
                _tray.QuitRequested += Quit;
                UpdateTrayTooltip();
            }
            catch (Exception error)
            {
                BridgeLog.Write($"tray icon unavailable: {error.Message}");
            }
        }
        else if (!ShellPreferences.CloseToTray && _tray is not null)
        {
            _tray.Dispose();
            _tray = null;
            AppWindow.Show();
        }
    }

    private void ShowFromTray()
    {
        AppWindow.Show();
        if (AppWindow.Presenter is OverlappedPresenter { State: OverlappedPresenterState.Minimized } presenter)
        {
            presenter.Restore();
        }

        Activate();
    }

    /// <summary>Quits for real, from the tray menu; closing the window only hides it then.</summary>
    private void Quit()
    {
        _quitting = true;
        Close();
    }

    private void UpdateTrayTooltip()
    {
        if (_tray is null)
        {
            return;
        }

        var track = Model.ConfirmedTrack;
        _tray.SetTooltip(track is null ? "Goosic" : $"{track.Title} — {track.Subtitle}");
    }

    /// <summary>
    /// A launch at sign-in stays out of the way: in the tray when Close to tray is on, otherwise
    /// minimized. It runs once, on the first activation, when the window exists to hide.
    /// </summary>
    private void HandleBackgroundLaunch()
    {
        if (_backgroundLaunchHandled)
        {
            return;
        }

        _backgroundLaunchHandled = true;
        if (!StartupRegistration.LaunchedInBackground)
        {
            return;
        }

        if (_tray is not null)
        {
            AppWindow.Hide();
        }
        else if (AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.Minimize();
        }
    }

    // ---- Window placement ----

    private void RestorePlacement()
    {
        if (!ShellPreferences.RememberWindow || ShellPreferences.Placement is not { } placement)
        {
            return;
        }

        var bounds = new RectInt32(placement.X, placement.Y, placement.Width, placement.Height);
        // A monitor that has since been unplugged would leave the window off every screen; the
        // nearest display's work area keeps it reachable.
        var area = DisplayArea.GetFromRect(bounds, DisplayAreaFallback.Nearest).WorkArea;
        var width = Math.Min(Math.Max(placement.Width, 640), area.Width);
        var height = Math.Min(Math.Max(placement.Height, 480), area.Height);
        var x = Math.Clamp(placement.X, area.X, area.X + area.Width - width);
        var y = Math.Clamp(placement.Y, area.Y, area.Y + area.Height - height);
        AppWindow.MoveAndResize(new RectInt32(x, y, width, height));
        if (placement.Maximized && AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.Maximize();
        }

        if (placement.SidebarHidden && Sidebar.Visibility == Visibility.Visible)
        {
            ToggleSidebar();
        }
    }

    private void SavePlacement()
    {
        if (!ShellPreferences.RememberWindow || AppWindow.Presenter is not OverlappedPresenter presenter
            || presenter.State == OverlappedPresenterState.Minimized)
        {
            return;
        }

        var maximized = presenter.State == OverlappedPresenterState.Maximized;
        // A maximized window's own size is the screen's; keep the size it restores to instead.
        var previous = ShellPreferences.Placement;
        var (position, size) = maximized && previous is not null
            ? (new PointInt32(previous.X, previous.Y), new SizeInt32(previous.Width, previous.Height))
            : (AppWindow.Position, AppWindow.Size);
        ShellPreferences.Placement = new WindowPlacement(position.X, position.Y, size.Width, size.Height,
            maximized, Sidebar.Visibility != Visibility.Visible);
    }

    // ---- Now playing: notification, Discord, tray ----

    private async void OnConfirmedTrackForShell()
    {
        UpdateTrayTooltip();
        UpdateDiscord();
        if (_notifier is null || _windowActive || Model.ConfirmedTrack is not { } track)
        {
            return;
        }

        var artwork = await Model.LargeArtworkFileAsync(Model.NowPlayingThumbnail);
        if (ReferenceEquals(Model.ConfirmedTrack, track))
        {
            _notifier.Show(track.Title, track.Subtitle, artwork);
        }
    }

    private void ApplyNotifications()
    {
        if (ShellPreferences.NowPlayingNotifications && _notifier is null)
        {
            _notifier = new NowPlayingNotifier();
            _notifier.Clicked += () => DispatcherQueue.TryEnqueue(ShowFromTray);
        }
        else if (!ShellPreferences.NowPlayingNotifications && _notifier is not null)
        {
            _notifier.Dispose();
            _notifier = null;
        }
    }

    private void ApplyDiscord()
    {
        if (ShellPreferences.DiscordStatus && DiscordPresence.Available && _discord is null)
        {
            _discord = new DiscordPresence();
            UpdateDiscord();
        }
        else if (!ShellPreferences.DiscordStatus && _discord is not null)
        {
            // Closing the pipe is what clears the status on Discord's side.
            _discord.Dispose();
            _discord = null;
        }
    }

    private void UpdateDiscord()
    {
        if (_discord is null)
        {
            return;
        }

        if (Model.ConfirmedTrack is not { } track || !Model.IsPlaying)
        {
            _discord.Update(null);
            return;
        }

        var started = DateTimeOffset.UtcNow - TimeSpan.FromSeconds(Math.Max(0, Model.PlaybackPosition));
        DateTimeOffset? ends = Model.PlaybackDuration > 0 ? started + TimeSpan.FromSeconds(Model.PlaybackDuration) : null;
        _discord.Update(new DiscordActivity(track.Title, track.Artist ?? track.Subtitle, track.Album,
            Model.NowPlayingThumbnail, started, ends));
    }

    // ---- The Settings page ----

    /// <summary>Fills the Windows switches and the start-page choice, which are not bound to the model.</summary>
    private void WireSettingsControls()
    {
        foreach (var (_, label) in StartRoute.Choices)
        {
            StartPageChoice.Items.Add(label);
        }

        StartPageChoice.SelectedIndex = StartRoute.IndexOf(Model.StartPage);
        ShortcutList.ItemsSource = ShortcutEntry.All;
        Model.PropertyChanged += (_, e) =>
        {
            if (e.PropertyName == nameof(ShellViewModel.StartPage))
            {
                StartPageChoice.SelectedIndex = StartRoute.IndexOf(Model.StartPage);
            }
        };

        CloseToTrayToggle.IsOn = ShellPreferences.CloseToTray;
        LaunchAtStartupToggle.IsOn = StartupRegistration.Available && StartupRegistration.Enabled;
        LaunchAtStartupToggle.IsEnabled = StartupRegistration.Available;
        NotificationsToggle.IsOn = ShellPreferences.NowPlayingNotifications;
        DiscordToggle.IsOn = ShellPreferences.DiscordStatus && DiscordPresence.Available;
        DiscordToggle.IsEnabled = DiscordPresence.Available;
        RememberWindowToggle.IsOn = ShellPreferences.RememberWindow;
        EfficiencyToggle.IsOn = ShellPreferences.EfficiencyMode;
        if (!StartupRegistration.Available)
        {
            LaunchAtStartupRow.Description = "Available in copies installed by Setup";
        }

        if (!DiscordPresence.Available)
        {
            DiscordRow.Description = "Not set up in this build yet";
        }
    }

    private async void OnStartPageChanged(object sender, SelectionChangedEventArgs e)
    {
        var index = StartPageChoice.SelectedIndex;
        if (index >= 0 && index < StartRoute.Choices.Count && StartRoute.Choices[index].Value != Model.StartPage)
        {
            await Model.SetStartPageAsync(StartRoute.Choices[index].Value);
        }
    }

    private async void OnHideExplicitToggled(object sender, RoutedEventArgs e)
    {
        if (sender is ToggleSwitch toggle && toggle.IsOn != Model.HideExplicit)
        {
            await Model.SetHideExplicitAsync(toggle.IsOn);
        }
    }

    private async void OnReduceMotionToggled(object sender, RoutedEventArgs e)
    {
        if (sender is ToggleSwitch toggle && toggle.IsOn != Model.ReduceMotion)
        {
            await Model.SetReduceMotionAsync(toggle.IsOn);
        }
    }

    private void OnCloseToTrayToggled(object sender, RoutedEventArgs e)
    {
        if (CloseToTrayToggle.IsOn != ShellPreferences.CloseToTray)
        {
            ShellPreferences.CloseToTray = CloseToTrayToggle.IsOn;
            ApplyTray();
        }
    }

    private void OnLaunchAtStartupToggled(object sender, RoutedEventArgs e)
    {
        if (!StartupRegistration.Available || LaunchAtStartupToggle.IsOn == StartupRegistration.Enabled)
        {
            return;
        }

        try
        {
            StartupRegistration.Enabled = LaunchAtStartupToggle.IsOn;
        }
        catch (Exception error) when (error is UnauthorizedAccessException or System.Security.SecurityException or IOException)
        {
            BridgeLog.Write($"startup registration failed: {error.Message}");
            Model.ReportStatus("Windows didn’t allow Goosic to change its startup setting.");
            LaunchAtStartupToggle.IsOn = StartupRegistration.Enabled;
        }
    }

    private void OnNotificationsToggled(object sender, RoutedEventArgs e)
    {
        if (NotificationsToggle.IsOn != ShellPreferences.NowPlayingNotifications)
        {
            ShellPreferences.NowPlayingNotifications = NotificationsToggle.IsOn;
            ApplyNotifications();
        }
    }

    private void OnDiscordToggled(object sender, RoutedEventArgs e)
    {
        if (DiscordToggle.IsOn != ShellPreferences.DiscordStatus)
        {
            ShellPreferences.DiscordStatus = DiscordToggle.IsOn;
            ApplyDiscord();
        }
    }

    private void OnRememberWindowToggled(object sender, RoutedEventArgs e)
    {
        if (RememberWindowToggle.IsOn != ShellPreferences.RememberWindow)
        {
            ShellPreferences.RememberWindow = RememberWindowToggle.IsOn;
            if (!RememberWindowToggle.IsOn)
            {
                ShellPreferences.Placement = null;
            }
        }
    }

    private void OnEfficiencyToggled(object sender, RoutedEventArgs e)
    {
        if (EfficiencyToggle.IsOn != ShellPreferences.EfficiencyMode)
        {
            ShellPreferences.EfficiencyMode = EfficiencyToggle.IsOn;
            EfficiencyMode.Apply(EfficiencyToggle.IsOn);
            ApplyMotion();
        }
    }
}
