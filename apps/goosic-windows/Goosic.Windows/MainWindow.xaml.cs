using System;
using System.IO;
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
    private readonly GoosicServiceClient? _client;
    private OfficialPlaybackHost? _playback;
    private SystemMediaControls? _media;
    private bool _seeking;

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
        WireSeekGestures();
        WireKeyboard();
        Title = "Goosic";

        // The transport strip is the caption area, so the window has to be told which element
        // the user may drag the window by -- otherwise the whole row swallows the gesture.
        ExtendsContentIntoTitleBar = true;
        SetTitleBar(TransportBar);

        if (_client is not null)
        {
            _playback = new OfficialPlaybackHost(PlaybackView, _client);
            _playback.Status += message => Model.ReportStatus(message);
            Model.CurrentLyricChanged += FollowLyricOnScreen;
            Model.ConfirmedTrackChanged += async () =>
            {
                // Only fetched while the panel is open: lyrics are a third-party lookup, and a
                // listener who never opens the panel should not send one per track.
                if (LyricsButton.IsChecked == true)
                {
                    LyricsScroller.ChangeView(null, 0, null, disableAnimation: true);
                    await Model.LoadLyricsAsync();
                    SidePanelMessage.Text = Model.LyricsStatus;
                    SidePanelMessage.Visibility = Model.Lyrics.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
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

    // ---- Navigation -------------------------------------------------------------------------

    private async void OnRailRoute(object sender, RoutedEventArgs e)
    {
        if (sender is Button { Tag: string route })
        {
            await Model.LoadRouteAsync(route);
            ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
        }
    }

    private async void OnSidebarRoute(object sender, RoutedEventArgs e)
    {
        SidebarOverlay.Visibility = Visibility.Collapsed;
        if (sender is Button { Tag: string route })
        {
            await Model.LoadRouteAsync(route);
            ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
        }
    }

    private void OnToggleSidebar(object sender, RoutedEventArgs e) =>
        SidebarOverlay.Visibility = SidebarOverlay.Visibility == Visibility.Visible
            ? Visibility.Collapsed
            : Visibility.Visible;

    private void OnDismissSidebar(object sender, TappedRoutedEventArgs e) =>
        SidebarOverlay.Visibility = Visibility.Collapsed;

    private void OnSearchRoute(object sender, RoutedEventArgs e) => OpenSearch();

    private void OpenSearch()
    {
        SidebarOverlay.Visibility = Visibility.Visible;
        SidebarSearch.Focus(FocusState.Programmatic);
    }

    private async void OnSearchSubmitted(AutoSuggestBox sender, AutoSuggestBoxQuerySubmittedEventArgs args)
    {
        SidebarOverlay.Visibility = Visibility.Collapsed;
        await Model.SearchAsync(args.QueryText, "all");
        ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
    }

    private async void OnBack(object sender, RoutedEventArgs e) => await GoBackAsync();

    private async Task GoBackAsync()
    {
        await Model.GoBackAsync();
        ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
    }

    private async Task OpenAsync(string kind, string id, string title)
    {
        await Model.OpenEntityAsync(kind, id, title);
        ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
    }

    private async void OnAccount(object sender, RoutedEventArgs e)
    {
        await Model.RefreshAccountsAsync();
    }

    private async void OnLoadMore(object sender, RoutedEventArgs e) => await Model.LoadMoreAsync();

    /// <summary>Pages a shelf sideways by most of its visible width, like the carousel arrows.</summary>
    private void OnShelfScroll(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { Tag: string direction } button)
        {
            return;
        }

        // Arrow → its StackPanel → the header Grid → the shelf, whose next child is the carousel.
        if (VisualTreeHelper.GetParent(button) is FrameworkElement arrows
            && VisualTreeHelper.GetParent(arrows) is FrameworkElement header
            && VisualTreeHelper.GetParent(header) is Panel shelf
            && shelf.Children.Count > 1
            && shelf.Children[1] is ScrollViewer carousel)
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
            Model.ReportStatus("There is no service to claim playback from.");
            return;
        }

        await _playback.PlayAsync(videoId);
    }

    private async void OnPlayPage(object sender, RoutedEventArgs e) => await PlayEntryAsync(Model.PlayPage(shuffle: false));

    private async void OnShufflePage(object sender, RoutedEventArgs e) => await PlayEntryAsync(Model.PlayPage(shuffle: true));

    private void OnShuffle(object sender, RoutedEventArgs e) => Model.ToggleShuffle();

    private void OnRepeat(object sender, RoutedEventArgs e) => Model.CycleRepeat();

    private async void OnPrevious(object sender, RoutedEventArgs e) => await PreviousAsync();

    /// <summary>Restarts the track after its first few seconds, as every player does; otherwise goes back.</summary>
    private async Task PreviousAsync()
    {
        if (_playback is not null && Model.PlaybackPosition > 3)
        {
            await _playback.SeekAsync(0);
            return;
        }

        await AdvanceAsync(forward: false, natural: false);
    }

    private async void OnNext(object sender, RoutedEventArgs e) => await AdvanceAsync(forward: true, natural: false);

    private async Task AdvanceAsync(bool forward, bool natural)
    {
        if (forward && Model.CanExtendRadio && Model.Repeat != RepeatMode.One)
        {
            await Model.ExtendRadioAsync();
        }

        var entry = Model.Advance(forward, natural);
        if (entry is null && natural)
        {
            Model.ReportStatus("The queue has finished.");
        }

        await PlayEntryAsync(entry);
    }

    private async void OnPlayPause(object sender, RoutedEventArgs e) => await TogglePauseAsync();

    private async Task TogglePauseAsync()
    {
        if (_playback is null)
        {
            Model.ReportStatus("There is no service to claim playback from.");
            return;
        }

        await _playback.TogglePauseAsync();
    }

    // ---- Queue panel ------------------------------------------------------------------------

    private void OnRemoveFromQueue(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { DataContext: TrackViewModel entry })
        {
            Model.RemoveFromQueue(entry);
        }
    }

    private void OnClearQueue(object sender, RoutedEventArgs e) => Model.ClearUpcoming();

    // ---- Context menus ----------------------------------------------------------------------

    private void OnItemMore(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement element && BuildMenu(element.DataContext) is { } menu)
        {
            menu.ShowAt(element, new FlyoutShowOptions { Placement = FlyoutPlacementMode.BottomEdgeAlignedRight });
        }
    }

    private void OnItemContextRequested(UIElement sender, ContextRequestedEventArgs args)
    {
        if (sender is not FrameworkElement element || BuildMenu(element.DataContext) is not { } menu)
        {
            return;
        }

        args.Handled = true;
        var options = new FlyoutShowOptions();
        if (args.TryGetPosition(element, out var point))
        {
            options.Position = point;
        }

        menu.ShowAt(element, options);
    }

    /// <summary>
    /// The menu for a row or a card, offering only what that item can do.
    /// </summary>
    /// <remarks>
    /// Library, likes and playlists belong to a signed-in account and are left out until the
    /// account profile exists, rather than shown as items that would fail.
    /// </remarks>
    private MenuFlyout? BuildMenu(object? item)
    {
        var menu = new MenuFlyout();
        switch (item)
        {
            case TrackViewModel track when Model.IsQueueEntry(track):
                Add(menu, "Play", "", async () => await PlayEntryAsync(Model.JumpTo(track)));
                Add(menu, "Start radio", "", async () => await PlayEntryAsync(await Model.StartRadioAsync(track)));
                menu.Items.Add(new MenuFlyoutSeparator());
                AddNavigation(menu, track.ArtistId, track.AlbumId, track.Subtitle, track.Title);
                Add(menu, "Copy link", "", () => CopyLink(ShellViewModel.LinkFor(track)));
                menu.Items.Add(new MenuFlyoutSeparator());
                Add(menu, "Remove from queue", "", () => Model.RemoveFromQueue(track));
                break;

            case TrackViewModel track when !string.IsNullOrEmpty(track.VideoId):
                Add(menu, "Play", "", async () => await PlayEntryAsync(Model.PlayFromPage(track)));
                Add(menu, "Start radio", "", async () => await PlayEntryAsync(await Model.StartRadioAsync(track)));
                menu.Items.Add(new MenuFlyoutSeparator());
                Add(menu, "Play next", "", () => Model.Enqueue(track, next: true));
                Add(menu, "Add to queue", "", () => Model.Enqueue(track, next: false));
                menu.Items.Add(new MenuFlyoutSeparator());
                AddNavigation(menu, track.ArtistId, track.AlbumId, track.Subtitle, track.Title);
                Add(menu, "Copy link", "", () => CopyLink(ShellViewModel.LinkFor(track)));
                break;

            case CardViewModel card when !string.IsNullOrEmpty(card.VideoId):
                Add(menu, "Play", "", async () => await PlayEntryAsync(Model.PlayFromShelf(card)));
                Add(menu, "Start radio", "", async () => await PlayEntryAsync(await Model.StartRadioAsync(card)));
                menu.Items.Add(new MenuFlyoutSeparator());
                Add(menu, "Play next", "", () => Model.Enqueue(card, next: true));
                Add(menu, "Add to queue", "", () => Model.Enqueue(card, next: false));
                menu.Items.Add(new MenuFlyoutSeparator());
                AddNavigation(menu, card.ArtistId, card.AlbumId, card.Subtitle, card.Title);
                Add(menu, "Copy link", "", () => CopyLink(ShellViewModel.LinkFor(card)));
                break;

            case CardViewModel card when card.Kind is "album" or "playlist" or "artist":
                Add(menu, "Play", "", async () => await PlayEntryAsync(await Model.PlayEntityAsync(card.Kind, card.Id, shuffle: false)));
                Add(menu, "Shuffle", "", async () => await PlayEntryAsync(await Model.PlayEntityAsync(card.Kind, card.Id, shuffle: true)));
                menu.Items.Add(new MenuFlyoutSeparator());
                Add(menu, card.Kind == "artist" ? "Go to artist" : card.Kind == "album" ? "Go to album" : "Open playlist",
                    "", async () => await OpenAsync(card.Kind, card.Id, card.Title));
                Add(menu, "Copy link", "", () => CopyLink(ShellViewModel.LinkFor(card)));
                break;

            default:
                return null;
        }

        return menu;
    }

    private void AddNavigation(MenuFlyout menu, string? artistId, string? albumId, string artist, string title)
    {
        if (!string.IsNullOrEmpty(artistId))
        {
            Add(menu, "Go to artist", "", async () => await OpenAsync("artist", artistId, artist));
        }

        if (!string.IsNullOrEmpty(albumId))
        {
            Add(menu, "Go to album", "", async () => await OpenAsync("album", albumId, title));
        }
    }

    private static void Add(MenuFlyout menu, string text, string glyph, Action action)
    {
        var item = new MenuFlyoutItem { Text = text, Icon = new FontIcon { Glyph = glyph } };
        item.Click += (_, _) => action();
        menu.Items.Add(item);
    }

    private void CopyLink(string link)
    {
        var package = new DataPackage();
        package.SetText(link);
        Clipboard.SetContent(package);
        Model.ReportStatus("Link copied.");
    }

    // ---- Keyboard ---------------------------------------------------------------------------

    /// <summary>
    /// The shortcuts a music app is expected to have.
    /// </summary>
    /// <remarks>
    /// Space is handled on preview rather than as an accelerator, so it stays a space inside the
    /// search box and still activates a focused button there; everywhere else it toggles playback.
    /// </remarks>
    private void WireKeyboard()
    {
        RootGrid.KeyboardAcceleratorPlacementMode = KeyboardAcceleratorPlacementMode.Hidden;
        Accelerator(VirtualKey.Right, VirtualKeyModifiers.Control, () => _ = AdvanceAsync(forward: true, natural: false));
        Accelerator(VirtualKey.Left, VirtualKeyModifiers.Control, () => _ = PreviousAsync());
        Accelerator(VirtualKey.Up, VirtualKeyModifiers.Control, () => _ = NudgeVolumeAsync(0.05));
        Accelerator(VirtualKey.Down, VirtualKeyModifiers.Control, () => _ = NudgeVolumeAsync(-0.05));
        Accelerator(VirtualKey.M, VirtualKeyModifiers.Control, () => _ = _playback?.ToggleMutedAsync());
        Accelerator(VirtualKey.S, VirtualKeyModifiers.Control, Model.ToggleShuffle);
        Accelerator(VirtualKey.R, VirtualKeyModifiers.Control, Model.CycleRepeat);
        Accelerator(VirtualKey.F, VirtualKeyModifiers.Control, OpenSearch);
        Accelerator(VirtualKey.L, VirtualKeyModifiers.Control, () =>
        {
            LyricsButton.IsChecked = LyricsButton.IsChecked != true;
            OnToggleLyrics(LyricsButton, new RoutedEventArgs());
        });
        Accelerator(VirtualKey.Q, VirtualKeyModifiers.Control, () =>
        {
            QueueButton.IsChecked = QueueButton.IsChecked != true;
            OnToggleQueue(QueueButton, new RoutedEventArgs());
        });
        Accelerator(VirtualKey.Left, VirtualKeyModifiers.Menu, () => _ = GoBackAsync());
        Accelerator(VirtualKey.Escape, VirtualKeyModifiers.None, () => SidebarOverlay.Visibility = Visibility.Collapsed);
        RootGrid.PreviewKeyDown += OnPreviewKeyDown;
    }

    private void Accelerator(VirtualKey key, VirtualKeyModifiers modifiers, Action action)
    {
        var accelerator = new KeyboardAccelerator { Key = key, Modifiers = modifiers };
        accelerator.Invoked += (_, args) =>
        {
            args.Handled = true;
            action();
        };
        RootGrid.KeyboardAccelerators.Add(accelerator);
    }

    private void OnPreviewKeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key != VirtualKey.Space)
        {
            return;
        }

        var focused = FocusManager.GetFocusedElement(RootGrid.XamlRoot);
        if (focused is TextBox or AutoSuggestBox or ButtonBase or Slider)
        {
            return;
        }

        e.Handled = true;
        _ = TogglePauseAsync();
    }

    private async Task NudgeVolumeAsync(double delta)
    {
        if (_playback is not null)
        {
            await _playback.SetVolumeAsync(Math.Clamp(Model.Volume + delta, 0, 1));
        }
    }

    // ---- System media controls --------------------------------------------------------------

    private void WireSystemMediaControls()
    {
        try
        {
            _media = new SystemMediaControls(DispatcherQueue);
        }
        catch (Exception error)
        {
            // The overlay is a convenience; a machine without it still plays.
            Model.ReportStatus("Windows media controls are unavailable: " + error.Message);
            return;
        }

        _media.ButtonPressed += button =>
        {
            switch (button)
            {
                case SystemMediaTransportControlsButton.Play:
                case SystemMediaTransportControlsButton.Pause:
                    _ = TogglePauseAsync();
                    break;
                case SystemMediaTransportControlsButton.Next:
                    _ = AdvanceAsync(forward: true, natural: false);
                    break;
                case SystemMediaTransportControlsButton.Previous:
                    _ = PreviousAsync();
                    break;
            }
        };
        _media.SeekRequested += seconds => _ = _playback?.SeekAsync(seconds);
        Model.NowPlayingChanged += async track =>
        {
            var artwork = await Model.ArtworkFileAsync(track);
            if (_media is not null && track.VideoId is { } videoId)
            {
                await _media.ShowTrackAsync(videoId, track.Title, track.Subtitle, artwork);
            }
        };
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

    // ---- Transport gestures -----------------------------------------------------------------

    /// <summary>
    /// Wires the position slider's pointer events, including those the slider itself handles.
    /// </summary>
    /// <remarks>
    /// A Slider marks pointer presses and releases on its thumb and track as handled, so handlers
    /// attached in markup never run -- which is why seeking did nothing. Registering with
    /// handledEventsToo is how a page observes a gesture a control has already consumed.
    /// </remarks>
    private void WireSeekGestures()
    {
        PlaybackProgress.AddHandler(UIElement.PointerPressedEvent,
            new PointerEventHandler(OnSeekStarted), handledEventsToo: true);
        PlaybackProgress.AddHandler(UIElement.PointerReleasedEvent,
            new PointerEventHandler(OnSeekCompleted), handledEventsToo: true);
        PlaybackProgress.AddHandler(UIElement.PointerCaptureLostEvent,
            new PointerEventHandler(OnSeekCompleted), handledEventsToo: true);
    }

    private void OnSeekStarted(object sender, PointerRoutedEventArgs e)
    {
        _seeking = true;
        Model.IsScrubbing = true;
    }

    private async void OnSeekCompleted(object sender, PointerRoutedEventArgs e)
    {
        if (!_seeking)
        {
            return;
        }

        _seeking = false;
        Model.IsScrubbing = false;
        if (_playback is not null)
        {
            await _playback.SeekAsync(PlaybackProgress.Value);
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
        if (_playback is null || Math.Abs(e.NewValue - Model.VolumePercent) < 0.5)
        {
            return;
        }

        await _playback.SetVolumeAsync(e.NewValue / 100.0);
    }

    private async void OnToggleMuted(object sender, RoutedEventArgs e)
    {
        if (_playback is not null)
        {
            await _playback.ToggleMutedAsync();
        }
    }

    // ---- Side panel -------------------------------------------------------------------------

    /// <summary>Keeps the line being sung in the upper part of the lyrics panel.</summary>
    private void FollowLyricOnScreen(int index)
    {
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

    /// <summary>Lyrics and Playing Next share the right edge, so opening one closes the other.</summary>
    private async void OnToggleLyrics(object sender, RoutedEventArgs e)
    {
        QueueButton.IsChecked = false;
        QueuePanel.Visibility = Visibility.Collapsed;
        var visible = LyricsButton.IsChecked == true;
        LyricsItems.Visibility = visible ? Visibility.Visible : Visibility.Collapsed;
        SidePanelMessage.Visibility = visible ? Visibility.Visible : Visibility.Collapsed;
        ShowSidePanel(visible, Model.LyricsStatus);
        if (!visible)
        {
            return;
        }

        await Model.LoadLyricsAsync();
        SidePanelMessage.Text = Model.LyricsStatus;
        SidePanelMessage.Visibility = Model.Lyrics.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
    }

    private void OnToggleQueue(object sender, RoutedEventArgs e)
    {
        LyricsButton.IsChecked = false;
        LyricsItems.Visibility = Visibility.Collapsed;
        SidePanelMessage.Visibility = Visibility.Collapsed;
        var visible = QueueButton.IsChecked == true;
        QueuePanel.Visibility = visible ? Visibility.Visible : Visibility.Collapsed;
        ShowSidePanel(visible, "");
    }

    private void ShowSidePanel(bool visible, string message)
    {
        SidePanelMessage.Text = message;
        SidePanel.Visibility = visible ? Visibility.Visible : Visibility.Collapsed;
    }
}
