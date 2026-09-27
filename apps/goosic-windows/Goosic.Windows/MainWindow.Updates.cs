using System;
using System.Threading;
using System.Threading.Tasks;
using Goosic.Windows.Presentation;
using Goosic.Windows.Service;
using Microsoft.UI.Xaml;

namespace Goosic.Windows;

public sealed partial class MainWindow : Window
{
    // ---- Updates ------------------------------------------------------------------------------

    private AvailableUpdate? _update;
    private bool _updateBusy;

    /// <summary>Fills the Updates card and looks for a newer release once, shortly after launch.</summary>
    /// <remarks>
    /// The launch check waits so it never competes with the first page for the network, and only
    /// runs for a copy Setup installed: the portable ZIP and development builds cannot be updated
    /// in place, so telling them about a release would only offer a button that cannot work.
    /// </remarks>
    private void WireUpdates()
    {
        UpdateVersionText.Text = $"Goosic {AppUpdater.CurrentVersionText}";
        UpdateStatusText.Text = AppUpdater.CanInstall
            ? ""
            : "This copy was not installed by Setup, so it cannot update itself. Updates can still be checked for.";
        if (AppUpdater.CanInstall)
        {
            _ = CheckForUpdatesAsync(announce: true, delay: TimeSpan.FromSeconds(8));
        }
    }

    private async void OnCheckForUpdates(object sender, RoutedEventArgs e) =>
        await CheckForUpdatesAsync(announce: false, delay: TimeSpan.Zero);

    private async Task CheckForUpdatesAsync(bool announce, TimeSpan delay)
    {
        await Task.Delay(delay);
        if (_updateBusy)
        {
            return;
        }

        _updateBusy = true;
        UpdateCheckButton.IsEnabled = false;
        if (!announce)
        {
            UpdateStatusText.Text = "Checking for updates…";
        }

        try
        {
            using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(20));
            _update = await AppUpdater.CheckAsync(timeout.Token);
            ShowUpdate();
            if (announce && _update is { } found)
            {
                Model.ReportStatus($"Goosic {UpdateRules.Format(found.Version)} is available. Install it from Settings.");
            }
        }
        catch (Exception error)
        {
            BridgeLog.Write($"update check failed: {error.GetType().Name}: {error.Message}");
            // A silent launch check that fails says nothing; the listener did not ask.
            if (!announce)
            {
                UpdateStatusText.Text = "Couldn’t reach GitHub to check for updates. Try again later.";
            }
        }
        finally
        {
            _updateBusy = false;
            UpdateCheckButton.IsEnabled = true;
        }
    }

    private void ShowUpdate()
    {
        if (_update is not { } update)
        {
            UpdateStatusText.Text = AppUpdater.CurrentVersion is null
                ? "This is a development build; releases are compared with packaged versions only."
                : "Goosic is up to date.";
            UpdateInstallButton.Visibility = Visibility.Collapsed;
            return;
        }

        var version = UpdateRules.Format(update.Version);
        UpdateStatusText.Text = AppUpdater.CanInstall
            ? $"Goosic {version} is available. Installing closes Goosic and reopens it when Setup finishes."
            : $"Goosic {version} is available on GitHub.";
        UpdateInstallButton.Content = $"Install {version}";
        UpdateInstallButton.Visibility = AppUpdater.CanInstall ? Visibility.Visible : Visibility.Collapsed;
    }

    private async void OnInstallUpdate(object sender, RoutedEventArgs e)
    {
        if (_update is not { } update || _updateBusy || !AppUpdater.CanInstall)
        {
            return;
        }

        _updateBusy = true;
        UpdateInstallButton.IsEnabled = UpdateCheckButton.IsEnabled = false;
        UpdateProgress.Value = 0;
        UpdateProgress.Visibility = Visibility.Visible;
        UpdateStatusText.Text = $"Downloading Goosic {UpdateRules.Format(update.Version)}…";
        try
        {
            var progress = new Progress<double>(fraction => UpdateProgress.Value = fraction);
            var setup = await AppUpdater.DownloadAsync(update, progress, CancellationToken.None);
            UpdateStatusText.Text = "Starting Setup…";
            AppUpdater.StartSetup(setup);
            // Setup replaces these files, so Goosic has to be gone before it copies them.
            Application.Current.Exit();
        }
        catch (Exception error)
        {
            BridgeLog.Write($"update install failed: {error.GetType().Name}: {error.Message}");
            UpdateStatusText.Text = error is System.IO.InvalidDataException
                ? error.Message
                : "The update couldn’t be downloaded. Check your connection and try again.";
            UpdateProgress.Visibility = Visibility.Collapsed;
            UpdateInstallButton.IsEnabled = UpdateCheckButton.IsEnabled = true;
            _updateBusy = false;
        }
    }
}
