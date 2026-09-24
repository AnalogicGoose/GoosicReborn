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
    private readonly GoosicServiceClient? _client;
    private OfficialPlaybackHost? _playback;
    private SystemMediaControls? _media;
    private PersonalCatalogHost? _personal;
    private readonly Views.MeshBackground _fullPlayerMesh = new();
    private readonly Views.ArtworkBackdropRenderer _backdropRenderer = new();
    private string? _paletteFor;
    private bool _fullPlayerSeeking;
    private bool _seeking;
    private WindowWidthClass _widthClass = WindowWidthClass.Wide;

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
            BridgeLog.Write($"startup: service unavailable: {error.Message}");
            _client = null;
            Model = new ShellViewModel(GoosicServiceClient.Unavailable(error.Message));
        }

        InitializeComponent();
        // For checking a theme without changing Windows: GOOSIC_THEME=Light or Dark.
        if (Enum.TryParse<ElementTheme>(Environment.GetEnvironmentVariable("GOOSIC_THEME"), ignoreCase: true, out var theme))
        {
            RootGrid.RequestedTheme = theme;
        }
        RootGrid.SizeChanged += OnRootSizeChanged;
        Model.PropertyChanged += OnPageLoadingChanged;
        WireSeekGestures();
        // A page that grows (or arrives) shorter than the window never scrolls, so it asks here too.
        ContentStack.SizeChanged += async (_, _) => await LoadMoreIfNearEndAsync();
        WireKeyboard();
        WireFullPlayer();
        WireMotion();
        WireUpdates();
        WireShellSettings();
        HighlightNavigation("home");
        Title = "Goosic";

        // Content runs under the caption area, as it does on macOS, and an empty strip along the
        // top is handed to the system as the drag region -- the sidebar toggle sits left of it so
        // it stays clickable. Mica is the window's material; the sidebar and the player pill are
        // acrylic over it and over the cover's colours.
        ExtendsContentIntoTitleBar = true;
        AppWindow.TitleBar.PreferredHeightOption = Microsoft.UI.Windowing.TitleBarHeightOption.Tall;
        SetTitleBar(TitleBarStrip);
        SystemBackdrop = new Microsoft.UI.Xaml.Media.MicaBackdrop();
        RootGrid.Loaded += (_, _) =>
        {
            ApplyMinimumWindowSize();
            RootGrid.XamlRoot.Changed += (_, _) => ApplyMinimumWindowSize();
        };

        if (_client is not null)
        {
            // Both web surfaces live in one invisible host panel, and are rebuilt there whenever the
            // active account changes, because a WebView2 cannot move between profiles.
            _playback = new OfficialPlaybackHost(WebHost, _client);
            _personal = new PersonalCatalogHost(WebHost);
            Model.Playback = _playback;
            Model.Personal = _personal;
            _playback.Status += message => Model.ReportStatus(message);
            _playback.IsSubstitute = Model.AcceptSubstitute;
            _playback.PageRefused += Model.ReportRefused;
            _playback.PageMovedOn += videoId =>
            {
                // YouTube Music started a track of its own when the requested one finished; that is
                // the requested track's natural end, and the queue decides what plays next.
                if (Model.ConfirmEndedByPage(videoId))
                {
                    _ = AdvanceAsync(forward: true, natural: true);
                }
            };
            Model.CurrentLyricChanged += FollowLyricOnScreen;
            Model.ConfirmedTrackChanged += async () =>
            {
                // Only fetched while the panel is open: lyrics are a third-party lookup, and a
                // listener who never opens the panel should not send one per track.
                if (FullPlayer.Visibility == Visibility.Visible && _sidePanel.Content != SidePanelContent.Lyrics)
                {
                    FullPlayerLyricsScroller.ChangeView(null, 0, null, disableAnimation: true);
                    await Model.LoadLyricsAsync();
                }

                if (_sidePanel.Content == SidePanelContent.Lyrics)
                {
                    LyricsScroller.ChangeView(null, 0, null, disableAnimation: true);
                    await Model.LoadLyricsAsync();
                    ApplySidePanel();
                }
            };
            _playback.Sampled += sample =>
            {
                _media?.ReportSample(sample);
                if (Model.ReportPlayback(sample))
                {
                    _ = AdvanceAsync(forward: true, natural: true);
                }
            };
            WireSystemMediaControls();
        }

        var rules = CheckShellSupport();
        if (rules.Length > 0)
        {
            Model.ReportStatus(rules);
        }

        Closed += (_, _) => _media?.Dispose();
        BridgeLog.Write($"startup: window ready, service {(_client is null ? "unavailable" : "started")}");
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
        if (File.Exists(beside))
        {
            return beside;
        }

        // Run out of a build directory inside the repository: use the Cargo build beside it, so
        // launching the executable directly still reaches the service (and its accounts).
        for (var directory = new DirectoryInfo(AppContext.BaseDirectory); directory is not null; directory = directory.Parent)
        {
            var built = Path.Combine(directory.FullName, "target", "debug", "goosic-service.exe");
            if (File.Exists(built))
            {
                return built;
            }
        }

        return beside;
    }

    // ---- Shell support ----------------------------------------------------------------------

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
}
