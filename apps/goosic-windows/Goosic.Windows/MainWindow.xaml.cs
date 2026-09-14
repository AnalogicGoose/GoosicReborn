using System;
using System.Collections.Generic;
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
    private PersonalCatalogHost? _personal;
    private readonly Views.MeshBackground _fullPlayerMesh = new();
    private readonly Views.MeshBackground _backdropMesh = new();
    private string? _paletteFor;
    private bool _fullPlayerSeeking;
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
        WireFullPlayer();
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

        if (_client is not null)
        {
            // Both web surfaces live in one invisible host panel, and are rebuilt there whenever the
            // active account changes, because a WebView2 cannot move between profiles.
            _playback = new OfficialPlaybackHost(WebHost, _client);
            _personal = new PersonalCatalogHost(WebHost);
            Model.Playback = _playback;
            Model.Personal = _personal;
            _playback.Status += message => Model.ReportStatus(message);
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
                if (FullPlayer.Visibility == Visibility.Visible && LyricsButton.IsChecked != true)
                {
                    FullPlayerLyricsScroller.ChangeView(null, 0, null, disableAnimation: true);
                    await Model.LoadLyricsAsync();
                }

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

    /// <summary>
    /// A sidebar item: a route, or a library section written as <c>library:browseId</c>.
    /// </summary>
    private async void OnNavigate(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { Tag: string tag })
        {
            return;
        }

        HighlightNavigation(tag);
        if (tag.StartsWith("library:", StringComparison.Ordinal))
        {
            await Model.OpenLibrarySectionAsync(tag["library:".Length..]);
        }
        else
        {
            await Model.LoadRouteAsync(tag);
        }

        ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
        UpdateBackButton();
    }

    /// <summary>Marks the sidebar item for the page on screen, as the macOS sidebar selects its row.</summary>
    private void HighlightNavigation(string? tag)
    {
        var selected = (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["GoosicSidebarSelectedBrush"];
        foreach (var child in SidebarItems.Children)
        {
            if (child is Button { Tag: string itemTag } item)
            {
                item.Background = itemTag == tag ? selected : new Microsoft.UI.Xaml.Media.SolidColorBrush(Microsoft.UI.Colors.Transparent);
            }
        }
    }

    private void UpdateBackButton() =>
        BackButton.Visibility = Model.CanGoBack ? Visibility.Visible : Visibility.Collapsed;

    /// <summary>Hides or shows the sidebar; the content takes the whole width while it is hidden.</summary>
    private void OnToggleSidebar(object sender, RoutedEventArgs e) => ToggleSidebar();

    private void ToggleSidebar()
    {
        Sidebar.Visibility = Sidebar.Visibility == Visibility.Visible ? Visibility.Collapsed : Visibility.Visible;
        ApplyInsets();
    }

    private const double PanelGap = 8;
    private const double ContentGutter = 26;
    private readonly List<ScrollViewer> _carousels = [];

    private double LeftInset => Sidebar.Visibility == Visibility.Visible ? PanelGap + Sidebar.Width : 0;

    private double RightInset => SidePanel.Visibility == Visibility.Visible ? PanelGap + SidePanel.Width : 0;

    /// <summary>
    /// Keeps the content clear of the floating panels without clipping it at their edge.
    /// </summary>
    /// <remarks>
    /// The page is padded so its first card and its headings start beside the sidebar, while each
    /// row of cards is pulled back out to the window's edges and padded in again: its first card
    /// lines up with the page, and scrolling it slides the cards under the glass rather than
    /// cutting them off at a margin.
    /// </remarks>
    private void ApplyInsets()
    {
        var left = LeftInset + ContentGutter;
        var right = RightInset + ContentGutter;
        ContentStack.Padding = new Thickness(left, 56, right, 128);
        PlayerPill.Margin = new Thickness(LeftInset + 16, 0, RightInset + 16, 18);
        BackButton.Margin = new Thickness(56, 7, 0, 0);
        foreach (var carousel in _carousels)
        {
            FitCarousel(carousel, left, right);
        }
    }

    private static void FitCarousel(ScrollViewer carousel, double left, double right)
    {
        carousel.Margin = new Thickness(-left, 0, -right, 0);
        // Padded through a border: an ItemsControl's own Padding never reaches its panel.
        if (carousel.Content is Border row)
        {
            row.Padding = new Thickness(left, 0, right, 0);
        }
    }

    private void OnCarouselLoaded(object sender, RoutedEventArgs e)
    {
        if (sender is ScrollViewer carousel && !_carousels.Contains(carousel))
        {
            _carousels.Add(carousel);
            FitCarousel(carousel, LeftInset + ContentGutter, RightInset + ContentGutter);
        }
    }

    private void OnCarouselUnloaded(object sender, RoutedEventArgs e)
    {
        if (sender is ScrollViewer carousel)
        {
            _carousels.Remove(carousel);
        }
    }

    /// <summary>
    /// Scrolls the page, not the row, when the wheel turns over a row of cards.
    /// </summary>
    /// <remarks>
    /// A scroller that can only move sideways turns a vertical wheel into sideways movement, so
    /// scrolling down the page stalled on every shelf and slid its cards instead. A plain wheel is
    /// handed to the page here, before the row sees it; a horizontal wheel or Shift+wheel still
    /// moves the row, and so do its arrows, touch and the touchpad.
    /// </remarks>
    private void OnCarouselWheel(object sender, PointerRoutedEventArgs e)
    {
        var point = e.GetCurrentPoint(ContentScroller);
        var shift = (e.KeyModifiers & VirtualKeyModifiers.Shift) != 0;
        if (point.Properties.IsHorizontalMouseWheel || shift)
        {
            return;
        }

        e.Handled = true;
        ContentScroller.ChangeView(null, ContentScroller.VerticalOffset - point.Properties.MouseWheelDelta, null);
    }

    private void OpenSearch()
    {
        if (Sidebar.Visibility != Visibility.Visible)
        {
            ToggleSidebar();
        }

        SidebarSearch.Focus(FocusState.Programmatic);
    }

    private async void OnSearchSubmitted(AutoSuggestBox sender, AutoSuggestBoxQuerySubmittedEventArgs args)
    {
        HighlightNavigation(null);
        await Model.SearchAsync(args.QueryText, "all");
        ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
        UpdateBackButton();
    }

    private async void OnBack(object sender, RoutedEventArgs e) => await GoBackAsync();

    private async Task GoBackAsync()
    {
        await Model.GoBackAsync();
        HighlightNavigation(Model.CurrentRouteName);
        ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
        UpdateBackButton();
    }

    private async Task OpenAsync(string kind, string id, string title)
    {
        HighlightNavigation(null);
        await Model.OpenEntityAsync(kind, id, title);
        ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
        UpdateBackButton();
    }

    // ---- The player pill --------------------------------------------------------------------

    /// <summary>Pointing at the pill shows the times either side of the position line.</summary>
    private void OnPillPointerEntered(object sender, PointerRoutedEventArgs e)
    {
        PillElapsed.Visibility = Visibility.Visible;
        PillRemaining.Visibility = Visibility.Visible;
    }

    private void OnPillPointerExited(object sender, PointerRoutedEventArgs e)
    {
        PillElapsed.Visibility = Visibility.Collapsed;
        PillRemaining.Visibility = Visibility.Collapsed;
    }

    /// <summary>Pointing at the cover shows that clicking it opens the full-screen player.</summary>
    private void OnCoverPointerEntered(object sender, PointerRoutedEventArgs e) => PillCoverHover.Opacity = 1;

    private void OnCoverPointerExited(object sender, PointerRoutedEventArgs e) => PillCoverHover.Opacity = 0;

    /// <summary>The pill's "more" menu: what the row menu offers, for the track that is playing.</summary>
    private void OnNowPlayingMore(object sender, RoutedEventArgs e)
    {
        if (Model.ConfirmedTrack is not { } track || BuildMenu(track) is not { } menu)
        {
            Model.ReportStatus("Nothing is playing.");
            return;
        }

        menu.Items.Insert(0, new MenuFlyoutSeparator());
        var full = new MenuFlyoutItem { Text = "Full-screen player", Icon = new FontIcon { Glyph = "\uE740" } };
        full.Click += (_, _) => SetFullPlayerOpen(true);
        menu.Items.Insert(0, full);
        menu.ShowAt(PillMoreButton, new FlyoutShowOptions { Placement = FlyoutPlacementMode.TopEdgeAlignedRight });
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
            entry = await Model.AutoplayAfterAsync();
            if (entry is null)
            {
                Model.ReportStatus("The queue has finished.");
            }
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
                Add(menu, "Play", "\uE768", async () => await PlayEntryAsync(Model.PlayFromPage(track)));
                Add(menu, "Start radio", "\uEC05", async () => await PlayEntryAsync(await Model.StartRadioAsync(track)));
                menu.Items.Add(new MenuFlyoutSeparator());
                Add(menu, "Play next", "\uE7AC", () => Model.Enqueue(track, next: true));
                Add(menu, "Add to queue", "\uE710", () => Model.Enqueue(track, next: false));
                AddAccountTrackItems(menu, track.VideoId, track.Title);
                if (Model.IsOwnedPlaylistPage && !string.IsNullOrEmpty(track.EntryId))
                {
                    Add(menu, "Remove from playlist", "\uE74D", async () => await Model.RemoveFromPagePlaylistAsync(track));
                }

                menu.Items.Add(new MenuFlyoutSeparator());
                AddNavigation(menu, track.ArtistId, track.AlbumId, track.Subtitle, track.Title);
                Add(menu, "Copy link", "", () => CopyLink(ShellViewModel.LinkFor(track)));
                break;

            case CardViewModel card when !string.IsNullOrEmpty(card.VideoId):
                Add(menu, "Play", "", async () => await PlayEntryAsync(Model.PlayFromShelf(card)));
                Add(menu, "Start radio", "", async () => await PlayEntryAsync(await Model.StartRadioAsync(card)));
                menu.Items.Add(new MenuFlyoutSeparator());
                Add(menu, "Play next", "\uE7AC", () => Model.Enqueue(card, next: true));
                Add(menu, "Add to queue", "\uE710", () => Model.Enqueue(card, next: false));
                AddAccountTrackItems(menu, card.VideoId, card.Title);
                menu.Items.Add(new MenuFlyoutSeparator());
                AddNavigation(menu, card.ArtistId, card.AlbumId, card.Subtitle, card.Title);
                Add(menu, "Copy link", "", () => CopyLink(ShellViewModel.LinkFor(card)));
                break;

            case CardViewModel card when card.Kind is "album" or "playlist" or "artist":
                Add(menu, "Play", "", async () => await PlayEntryAsync(await Model.PlayEntityAsync(card.Kind, card.Id, shuffle: false)));
                Add(menu, "Shuffle", "", async () => await PlayEntryAsync(await Model.PlayEntityAsync(card.Kind, card.Id, shuffle: true)));
                menu.Items.Add(new MenuFlyoutSeparator());
                Add(menu, card.Kind == "artist" ? "Go to artist" : card.Kind == "album" ? "Go to album" : "Open playlist",
                    "\uE8A7", async () => await OpenAsync(card.Kind, card.Id, card.Title));
                if (Model.IsSignedIn && card.Kind == "playlist")
                {
                    if (Model.OwnsPlaylist(card.Id))
                    {
                        Add(menu, "Delete playlist…", "\uE74D", async () => await ConfirmDeleteAsync(card.Id, card.Title));
                    }
                    else
                    {
                        Add(menu, "Save to library", "\uE8F4", async () => await Model.SavePlaylistAsync(card.Id, card.Title, saved: true));
                        Add(menu, "Remove from library", "\uE8F5", async () => await Model.SavePlaylistAsync(card.Id, card.Title, saved: false));
                    }
                }
                else if (Model.IsSignedIn && card.Kind == "artist")
                {
                    Add(menu, "Subscribe", "\uE8FA", async () => await Model.FollowArtistAsync(card.Id, card.Title, follow: true));
                    Add(menu, "Unsubscribe", "\uE8F8", async () => await Model.FollowArtistAsync(card.Id, card.Title, follow: false));
                }
                Add(menu, "Copy link", "", () => CopyLink(ShellViewModel.LinkFor(card)));
                break;

            default:
                return null;
        }

        return menu;
    }

    /// <summary>Like, dislike and save to playlist, offered only while an account is active.</summary>
    private void AddAccountTrackItems(MenuFlyout menu, string? videoId, string title)
    {
        if (!Model.IsSignedIn || string.IsNullOrEmpty(videoId))
        {
            return;
        }

        menu.Items.Add(new MenuFlyoutSeparator());
        var rating = Model.RatingOf(videoId);
        Add(menu, rating == "LIKE" ? "Remove from liked songs" : "Like", "\uE8E1",
            async () => await Model.RateAsync(videoId, title, rating == "LIKE" ? "INDIFFERENT" : "LIKE"));
        Add(menu, rating == "DISLIKE" ? "Remove dislike" : "Dislike", "\uE8E0",
            async () => await Model.RateAsync(videoId, title, rating == "DISLIKE" ? "INDIFFERENT" : "DISLIKE"));

        var save = new MenuFlyoutSubItem { Text = "Save to playlist", Icon = new FontIcon { Glyph = "\uE8F4" } };
        var create = new MenuFlyoutItem { Text = "New playlist…", Icon = new FontIcon { Glyph = "\uE710" } };
        create.Click += async (_, _) => await NewPlaylistAsync(videoId);
        save.Items.Add(create);
        if (Model.UserPlaylists.Count > 0)
        {
            save.Items.Add(new MenuFlyoutSeparator());
        }

        foreach (var playlist in Model.UserPlaylists)
        {
            var item = new MenuFlyoutItem { Text = playlist.Title };
            item.Click += async (_, _) => await Model.AddToPlaylistAsync(playlist, videoId, title);
            save.Items.Add(item);
        }

        menu.Items.Add(save);
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

    // ---- Account ----------------------------------------------------------------------------

    private async void OnSignIn(object sender, RoutedEventArgs e)
    {
        AccountFlyout.Hide();
        await Model.SignInAsync();
    }

    private async void OnSignOut(object sender, RoutedEventArgs e)
    {
        AccountFlyout.Hide();
        if (await ConfirmAsync($"Sign out of {Model.AccountName}?",
                "Goosic forgets this account and clears its sign-in from this computer.", "Sign out"))
        {
            await Model.SignOutAsync();
        }
    }

    private async void OnUseAccount(object sender, RoutedEventArgs e)
    {
        AccountFlyout.Hide();
        if (sender is Button { Tag: string id })
        {
            await Model.SwitchAccountAsync(id);
        }
    }

    private async void OnUseGuest(object sender, RoutedEventArgs e)
    {
        AccountFlyout.Hide();
        await Model.SwitchAccountAsync(null);
    }

    private async void OnAccountRoute(object sender, RoutedEventArgs e)
    {
        AccountFlyout.Hide();
        if (sender is Button { Tag: string route })
        {
            await Model.LoadRouteAsync(route);
            ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
        }
    }

    private async void OnLibrarySection(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: LibrarySection section })
        {
            await Model.ShowLibrarySectionAsync(section);
        }
    }

    private async void OnSidebarPlaylist(object sender, RoutedEventArgs e)
    {
        if (sender is Button { Tag: string tag } && tag.Split(ShellViewModel.KeySeparator) is { Length: 3 } parts)
        {
            await OpenAsync(parts[0], parts[1], parts[2]);
        }
    }

    private async void OnNewPlaylist(object sender, RoutedEventArgs e)
    {
        await NewPlaylistAsync(null);
    }

    private async Task NewPlaylistAsync(string? videoId)
    {
        var answer = await PromptAsync("New playlist", "Title", "", "Create", privacy: true);
        if (answer is not { } chosen)
        {
            return;
        }

        var id = await Model.CreatePlaylistAsync(chosen.Text, chosen.Privacy, videoId);
        if (id is not null && videoId is null)
        {
            await OpenAsync("playlist", id, chosen.Text);
        }
    }

    private async void OnSavePagePlaylist(object sender, RoutedEventArgs e)
    {
        if (Model.PagePlaylistId is { } id)
        {
            await Model.SavePlaylistAsync(id, Model.PageTitle, saved: true);
        }
    }

    private async void OnFollowPageArtist(object sender, RoutedEventArgs e)
    {
        if (Model.PageArtistId is { } id)
        {
            await Model.FollowArtistAsync(id, Model.PageTitle, follow: true);
        }
    }

    private async void OnRenamePagePlaylist(object sender, RoutedEventArgs e)
    {
        if (await PromptAsync("Rename playlist", "Title", Model.PageTitle, "Rename", privacy: false) is { } answer)
        {
            await Model.RenamePagePlaylistAsync(answer.Text);
        }
    }

    private async void OnPagePlaylistPrivacy(object sender, RoutedEventArgs e)
    {
        if (sender is MenuFlyoutItem { Tag: string privacy })
        {
            await Model.SetPagePlaylistPrivacyAsync(privacy);
        }
    }

    private async void OnDeletePagePlaylist(object sender, RoutedEventArgs e)
    {
        if (Model.PagePlaylistId is { } id)
        {
            await ConfirmDeleteAsync(id, Model.PageTitle);
        }
    }

    private async Task ConfirmDeleteAsync(string playlistId, string title)
    {
        if (await ConfirmAsync($"Delete “{title}”?",
                "The playlist is deleted from YouTube Music for good. This cannot be undone.", "Delete"))
        {
            await Model.DeletePlaylistAsync(playlistId, title);
        }
    }

    private async void OnLikeNowPlaying(object sender, RoutedEventArgs e) =>
        await Model.ToggleNowPlayingRatingAsync("LIKE");

    private async void OnDislikeNowPlaying(object sender, RoutedEventArgs e) =>
        await Model.ToggleNowPlayingRatingAsync("DISLIKE");

    private async Task<bool> ConfirmAsync(string title, string message, string action)
    {
        var dialog = new ContentDialog
        {
            XamlRoot = RootGrid.XamlRoot,
            Title = title,
            Content = new TextBlock { Text = message, TextWrapping = TextWrapping.Wrap },
            PrimaryButtonText = action,
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Close,
        };
        return await dialog.ShowAsync() == ContentDialogResult.Primary;
    }

    private async Task<(string Text, string Privacy)?> PromptAsync(
        string title, string placeholder, string initial, string action, bool privacy)
    {
        var field = new TextBox { PlaceholderText = placeholder, Text = initial, MaxLength = 150 };
        var visibility = new ComboBox
        {
            Header = "Who can see this",
            ItemsSource = new[] { "Private", "Unlisted", "Public" },
            SelectedIndex = 0,
            HorizontalAlignment = HorizontalAlignment.Stretch,
        };
        var panel = new StackPanel { Spacing = 12, MinWidth = 320, Children = { field } };
        if (privacy)
        {
            panel.Children.Add(visibility);
        }

        var dialog = new ContentDialog
        {
            XamlRoot = RootGrid.XamlRoot,
            Title = title,
            Content = panel,
            PrimaryButtonText = action,
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
        };
        field.Loaded += (_, _) => field.Focus(FocusState.Programmatic);
        if (await dialog.ShowAsync() != ContentDialogResult.Primary || field.Text.Trim().Length == 0)
        {
            return null;
        }

        return (field.Text.Trim(), (visibility.SelectedItem as string ?? "Private").ToUpperInvariant());
    }

    // ---- Full-screen player -----------------------------------------------------------------

    /// <summary>
    /// Builds the parts of the full-screen player that live in code: its background, and its seek
    /// gestures, which the slider hides from markup handlers exactly as the transport's does.
    /// </summary>
    private void WireFullPlayer()
    {
        FullPlayerBackdrop.Children.Add(_fullPlayerMesh);
        ArtworkBackdrop.Children.Insert(0, _backdropMesh);
        FullPlayerProgress.AddHandler(UIElement.PointerPressedEvent,
            new PointerEventHandler((_, _) => { _fullPlayerSeeking = true; Model.IsScrubbing = true; }), handledEventsToo: true);
        PointerEventHandler done = async (_, _) =>
        {
            if (!_fullPlayerSeeking)
            {
                return;
            }

            _fullPlayerSeeking = false;
            Model.IsScrubbing = false;
            if (_playback is not null)
            {
                await _playback.SeekAsync(FullPlayerProgress.Value);
            }
        };
        FullPlayerProgress.AddHandler(UIElement.PointerReleasedEvent, done, handledEventsToo: true);
        FullPlayerProgress.AddHandler(UIElement.PointerCaptureLostEvent, done, handledEventsToo: true);

        Model.ArtworkChanged += async thumbnail => await ShowArtworkAsync(thumbnail);
        Model.PropertyChanged += (_, args) =>
        {
            if (args.PropertyName == nameof(ShellViewModel.ArtworkBackground))
            {
                UpdateBackdrop();
            }
        };
    }

    /// <summary>Loads the cover at full size and samples its colours for both backgrounds.</summary>
    private async Task ShowArtworkAsync(string? thumbnail)
    {
        _paletteFor = thumbnail;
        var file = await Model.LargeArtworkFileAsync(thumbnail);
        if (_paletteFor != thumbnail)
        {
            return;
        }

        if (file is null)
        {
            FullPlayerCover.Source = null;
            _fullPlayerMesh.SetPalette(null);
            _backdropMesh.SetPalette(null);
            UpdateBackdrop();
            return;
        }

        var image = new Microsoft.UI.Xaml.Media.Imaging.BitmapImage();
        using (var stream = File.OpenRead(file))
        {
            await image.SetSourceAsync(stream.AsRandomAccessStream());
        }

        var palette = await Views.MeshBackground.PaletteAsync(file);
        if (_paletteFor != thumbnail)
        {
            return;
        }

        FullPlayerCover.Source = image;
        _fullPlayerMesh.SetPalette(palette);
        _backdropMesh.SetPalette(palette);
        UpdateBackdrop();
    }

    private void UpdateBackdrop() =>
        ArtworkBackdrop.Visibility = Model.ArtworkBackground && _paletteFor is not null
            ? Visibility.Visible
            : Visibility.Collapsed;

    private void OnOpenFullPlayer(object sender, RoutedEventArgs e) => SetFullPlayerOpen(true);

    private void OnCloseFullPlayer(object sender, RoutedEventArgs e) => SetFullPlayerOpen(false);

    private void OnNowPlayingDoubleTapped(object sender, DoubleTappedRoutedEventArgs e) => SetFullPlayerOpen(true);

    /// <summary>
    /// Opens or closes the full-screen player, taking the window to full screen with it.
    /// </summary>
    /// <remarks>
    /// While it is open the drag region moves to an empty strip at its top, because the transport
    /// bar that is normally the title bar is underneath it and its drag would swallow clicks on
    /// the player's own controls.
    /// </remarks>
    private async void SetFullPlayerOpen(bool open)
    {
        var visible = FullPlayer.Visibility == Visibility.Visible;
        if (open == visible)
        {
            return;
        }

        FullPlayer.Visibility = open ? Visibility.Visible : Visibility.Collapsed;
        SetTitleBar(open ? FullPlayerDragStrip : TitleBarStrip);
        AppWindow.SetPresenter(open
            ? Microsoft.UI.Windowing.AppWindowPresenterKind.FullScreen
            : Microsoft.UI.Windowing.AppWindowPresenterKind.Default);
        if (open)
        {
            FullPlayerCoverButton.Focus(FocusState.Programmatic);
            if (Model.Lyrics.Count == 0)
            {
                await Model.LoadLyricsAsync();
            }
        }
    }

    private void OnToggleFullPlayerLyrics(object sender, RoutedEventArgs e)
    {
        var shown = FullPlayerLyricsToggle.IsChecked == true;
        FullPlayerLyrics.Visibility = shown ? Visibility.Visible : Visibility.Collapsed;
        FullPlayerLyricsColumn.Width = shown ? new GridLength(1.3, GridUnitType.Star) : new GridLength(0);
    }

    /// <summary>Seeks to a synced line. Plain lyrics carry no times, so their lines do nothing.</summary>
    private async void OnLyricLineClicked(object sender, RoutedEventArgs e)
    {
        if (sender is Button { Tag: long at } && at > 0 && _playback is not null && Model.IsSeekable)
        {
            await _playback.SeekAsync(at / 1000.0);
        }
    }

    private async void OnAutoplayToggled(object sender, RoutedEventArgs e)
    {
        if (sender is ToggleSwitch toggle && toggle.IsOn != Model.Autoplay)
        {
            await Model.SetAutoplayAsync(toggle.IsOn);
        }
    }

    private void OnShuffleToggled(object sender, RoutedEventArgs e)
    {
        if (sender is ToggleSwitch toggle && toggle.IsOn != Model.IsShuffled)
        {
            Model.ToggleShuffle();
        }
    }

    private async void OnArtworkBackgroundToggled(object sender, RoutedEventArgs e)
    {
        if (sender is ToggleSwitch toggle && toggle.IsOn != Model.ArtworkBackground)
        {
            await Model.SetArtworkBackgroundAsync(toggle.IsOn);
        }
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
        Accelerator(VirtualKey.F11, VirtualKeyModifiers.None, () => SetFullPlayerOpen(FullPlayer.Visibility != Visibility.Visible));
        Accelerator(VirtualKey.F, VirtualKeyModifiers.Control | VirtualKeyModifiers.Shift,
            () => SetFullPlayerOpen(FullPlayer.Visibility != Visibility.Visible));
        Accelerator(VirtualKey.Escape, VirtualKeyModifiers.None, () =>
        {
            if (FullPlayer.Visibility == Visibility.Visible)
            {
                SetFullPlayerOpen(false);
                return;
            }

        });
        Accelerator(VirtualKey.B, VirtualKeyModifiers.Control, ToggleSidebar);
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

    private double _scrubPosition;

    /// <summary>Keeps the position line in step with what the page confirmed.</summary>
    private void WireSeekGestures()
    {
        Model.PropertyChanged += (_, args) =>
        {
            if (args.PropertyName is nameof(ShellViewModel.PlaybackPosition) or nameof(ShellViewModel.PlaybackDuration))
            {
                DrawProgress(_seeking ? _scrubPosition : Model.PlaybackPosition);
            }
        };
    }

    private void DrawProgress(double position)
    {
        var width = PlaybackProgress.ActualWidth;
        var duration = Model.PlaybackDuration;
        PillFill.Width = duration > 0 && width > 0 ? Math.Clamp(position / duration, 0, 1) * width : 0;
    }

    private double PositionAt(PointerRoutedEventArgs e)
    {
        var x = e.GetCurrentPoint(PlaybackProgress).Position.X;
        var width = Math.Max(1, PlaybackProgress.ActualWidth);
        return Math.Clamp(x / width, 0, 1) * Model.PlaybackDuration;
    }

    private void OnProgressSizeChanged(object sender, SizeChangedEventArgs e) =>
        DrawProgress(_seeking ? _scrubPosition : Model.PlaybackPosition);

    private void OnProgressPressed(object sender, PointerRoutedEventArgs e)
    {
        if (!Model.IsSeekable)
        {
            return;
        }

        _seeking = true;
        Model.IsScrubbing = true;
        PlaybackProgress.CapturePointer(e.Pointer);
        PillTrack.Height = PillFill.Height = 5;
        _scrubPosition = PositionAt(e);
        DrawProgress(_scrubPosition);
        e.Handled = true;
    }

    private void OnProgressMoved(object sender, PointerRoutedEventArgs e)
    {
        if (_seeking)
        {
            _scrubPosition = PositionAt(e);
            DrawProgress(_scrubPosition);
        }
    }

    private async void OnProgressReleased(object sender, PointerRoutedEventArgs e)
    {
        PlaybackProgress.ReleasePointerCapture(e.Pointer);
        await FinishSeekAsync();
    }

    private async void OnProgressCaptureLost(object sender, PointerRoutedEventArgs e) => await FinishSeekAsync();

    private async Task FinishSeekAsync()
    {
        if (!_seeking)
        {
            return;
        }

        _seeking = false;
        Model.IsScrubbing = false;
        PillTrack.Height = PillFill.Height = 3;
        if (_playback is not null)
        {
            await _playback.SeekAsync(_scrubPosition);
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
        ApplyInsets();
    }
}
