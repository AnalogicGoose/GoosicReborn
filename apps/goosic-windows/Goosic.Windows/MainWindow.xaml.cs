using System;
using System.IO;
using Goosic.Windows.Service;
using Goosic.Windows.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Goosic.Windows;

public sealed partial class MainWindow : Window
{
    private readonly GoosicServiceClient? _client;

    public MainWindow()
    {
        // The model is built before the markup, because `x:Bind` resolves while
        // `InitializeComponent` runs. Assigning it afterwards leaves every binding to read a
        // null model during parsing, which surfaces as a crash inside Microsoft.UI.Xaml rather
        // than as anything naming this file.
        try
        {
            _client = GoosicServiceClient.Start(LocateService());
            Model = new ShellViewModel(_client);
        }
        catch (ServiceUnavailableException error)
        {
            // Without the service there is no catalog and no playback, so the window opens and
            // says why rather than presenting an empty screen that looks like an empty catalog.
            _client = null;
            Model = new ShellViewModel(GoosicServiceClient.Unavailable(error.Message));
        }

        InitializeComponent();
        Title = "Goosic";

        // The transport strip is the caption area, so the window has to be told which element
        // the user may drag the window by -- otherwise the whole row swallows the gesture.
        ExtendsContentIntoTitleBar = true;
        SetTitleBar(TransportBar);

        var rules = CheckShellSupport();
        if (rules.Length > 0)
        {
            Model.ReportStatus(rules);
        }

        _ = Model.StartAsync();
    }

    public ShellViewModel Model { get; }

    /// <summary>
    /// Where the service binary is.
    /// </summary>
    /// <remarks>
    /// A packaged build ships it beside the executable. <c>GOOSIC_SERVICE_PATH</c> overrides
    /// that for development, where the shell is run out of its build directory and the service
    /// is in the Cargo target directory. Both are full paths: resolving a bare name through
    /// <c>PATH</c> would let an unrelated program answer for the playback authority.
    /// </remarks>
    private static string LocateService()
    {
        var configured = Environment.GetEnvironmentVariable("GOOSIC_SERVICE_PATH");
        if (!string.IsNullOrWhiteSpace(configured))
        {
            return configured;
        }

        var beside = Path.Combine(AppContext.BaseDirectory, "goosic-service.exe");
        return beside;
    }

    private async void OnRailRoute(object sender, RoutedEventArgs e)
    {
        if (sender is Button { Tag: string route })
        {
            await Model.LoadRouteAsync(route);
        }
    }

    private async void OnSidebarRoute(object sender, RoutedEventArgs e)
    {
        SidebarOverlay.Visibility = Visibility.Collapsed;
        if (sender is Button { Tag: string route })
        {
            await Model.LoadRouteAsync(route);
        }
    }

    private void OnToggleSidebar(object sender, RoutedEventArgs e) =>
        SidebarOverlay.Visibility = SidebarOverlay.Visibility == Visibility.Visible
            ? Visibility.Collapsed
            : Visibility.Visible;

    private void OnDismissSidebar(object sender, Microsoft.UI.Xaml.Input.TappedRoutedEventArgs e) =>
        SidebarOverlay.Visibility = Visibility.Collapsed;

    private void OnSearchRoute(object sender, RoutedEventArgs e)
    {
        SidebarOverlay.Visibility = Visibility.Visible;
        SidebarSearch.Focus(FocusState.Programmatic);
    }

    private async void OnSearchSubmitted(AutoSuggestBox sender, AutoSuggestBoxQuerySubmittedEventArgs args)
    {
        SidebarOverlay.Visibility = Visibility.Collapsed;
        await Model.SearchAsync(args.QueryText, "all");
    }

    /// <summary>
    /// Plays a row, once there is somewhere to play it.
    /// </summary>
    /// <remarks>
    /// The lease is claimed before anything renders, because Rust decides whether a transition is
    /// allowed and a renderer that started first would have escaped that. Until the WebView2 host
    /// exists there is nothing to claim it for, so this says so rather than appearing to work.
    /// </remarks>
    private void OnPlayTrack(object sender, RoutedEventArgs e)
    {
        if (sender is Button { Tag: string videoId } && videoId.Length > 0)
        {
            Model.ReportStatus(ShellSupport.IsValidVideoId(videoId)
                ? "Playback is not implemented on Windows yet."
                : "That row does not carry a playable track.");
        }
    }

    private void OnBack(object sender, RoutedEventArgs e)
    {
    }

    private void OnAccount(object sender, RoutedEventArgs e)
    {
    }

    /// <summary>
    /// Proves the rules library is reachable and answering, at the point the window opens.
    /// </summary>
    /// <remarks>
    /// A P/Invoke that cannot find its library fails at the first call rather than at load, which
    /// would otherwise be somewhere deep in playback. Asking it something with a known answer
    /// here turns that into a status line naming the library.
    /// </remarks>
    private string CheckShellSupport()
    {
        try
        {
            var host = ShellSupport.AllowedHost;
            var valid = ShellSupport.IsValidVideoId("dQw4w9WgXcQ");
            return valid && host == "music.youtube.com"
                ? ""
                : $"the rules library answered unexpectedly: host={host}, idCheck={valid}";
        }
        catch (DllNotFoundException)
        {
            return "goosic_shell_support_ffi.dll is missing, so playback rules cannot be applied.";
        }
        catch (EntryPointNotFoundException error)
        {
            return $"the rules library is out of step with this shell: {error.Message}";
        }
    }

    private void OnShuffle(object sender, RoutedEventArgs e)
    {
    }

    private void OnPrevious(object sender, RoutedEventArgs e)
    {
    }

    private void OnPlayPause(object sender, RoutedEventArgs e)
    {
    }

    private void OnNext(object sender, RoutedEventArgs e)
    {
    }

    private void OnRepeat(object sender, RoutedEventArgs e)
    {
    }

    /// <summary>Lyrics and Playing Next share the right edge, so opening one closes the other.</summary>
    private void OnToggleLyrics(object sender, RoutedEventArgs e)
    {
        QueueButton.IsChecked = false;
        ShowSidePanel(LyricsButton.IsChecked == true, "Nothing playing.");
    }

    private void OnToggleQueue(object sender, RoutedEventArgs e)
    {
        LyricsButton.IsChecked = false;
        ShowSidePanel(QueueButton.IsChecked == true, "Playing Next is empty.");
    }

    private void ShowSidePanel(bool visible, string message)
    {
        SidePanelMessage.Text = message;
        SidePanel.Visibility = visible ? Visibility.Visible : Visibility.Collapsed;
    }
}
