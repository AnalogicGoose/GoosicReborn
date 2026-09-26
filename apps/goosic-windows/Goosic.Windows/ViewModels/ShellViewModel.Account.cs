using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Linq;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Threading.Tasks;
using Goosic.Windows.Presentation;
using Goosic.Windows.Service;

namespace Goosic.Windows.ViewModels;

/// <summary>One stored account, as the account menu lists it.</summary>
public sealed class AccountViewModel : INotifyPropertyChanged
{
    internal AccountViewModel(AccountSummary summary, bool active)
    {
        Id = summary.Id;
        WebProfileId = summary.WebProfileId;
        DisplayName = string.IsNullOrWhiteSpace(summary.DisplayName) ? "YouTube Music account" : summary.DisplayName;
        Detail = summary.Email ?? summary.Channel ?? "YouTube Music account";
        AvatarUrl = summary.AvatarUrl;
        IsActive = active;
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    internal string Id { get; }
    internal string WebProfileId { get; }
    public string DisplayName { get; }
    public string Detail { get; }
    private string? AvatarUrl { get; }
    public bool IsActive { get; }
    public string Initial => DisplayName[..1].ToUpperInvariant();
    public string ActiveMarker => IsActive ? "" : "";

    /// <summary>The account's id, for the view to hand back.</summary>
    public string Tag => Id;

    public Microsoft.UI.Xaml.Media.Imaging.BitmapImage? Artwork { get; private set; }

    internal async Task LoadArtworkAsync(ArtworkLoader loader)
    {
        if (await loader.LocalFileAsync(AvatarUrl).ConfigureAwait(true) is not { } file)
        {
            return;
        }

        var image = new Microsoft.UI.Xaml.Media.Imaging.BitmapImage();
        using var stream = System.IO.File.OpenRead(file);
        await image.SetSourceAsync(System.IO.WindowsRuntimeStreamExtensions.AsRandomAccessStream(stream));
        Artwork = image;
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Artwork)));
    }
}

/// <summary>A playlist the signed-in account owns and may edit.</summary>
public sealed class PlaylistSummaryViewModel : INotifyPropertyChanged
{
    private Microsoft.UI.Xaml.Media.Imaging.BitmapImage? _artwork;

    internal PlaylistSummaryViewModel(string id, string title, string subtitle, string? thumbnail)
    {
        Id = id;
        Title = title;
        Subtitle = subtitle;
        Thumbnail = thumbnail;
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    internal string Id { get; }
    public string Title { get; }
    public string Subtitle { get; }
    internal string? Thumbnail { get; }

    /// <summary>What activating the row opens.</summary>
    public string ActivationTag => string.Join(ShellViewModel.KeySeparator, "playlist", Id, Title);

    public Microsoft.UI.Xaml.Media.Imaging.BitmapImage? Artwork
    {
        get => _artwork;
        private set
        {
            _artwork = value;
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Artwork)));
        }
    }

    internal async Task LoadArtworkAsync(ArtworkLoader loader)
    {
        if (await loader.LocalFileAsync(Thumbnail).ConfigureAwait(true) is not { } file)
        {
            return;
        }

        var image = new Microsoft.UI.Xaml.Media.Imaging.BitmapImage();
        using var stream = System.IO.File.OpenRead(file);
        await image.SetSourceAsync(System.IO.WindowsRuntimeStreamExtensions.AsRandomAccessStream(stream));
        Artwork = image;
    }
}

/// <summary>One search filter chip.</summary>
public sealed record SearchFilterChoice(string Title, string Filter);

/// <summary>The sections of the library, as the chips above it name them.</summary>
public sealed record LibrarySection(string Title, string BrowseId, string Shape);

/// <summary>
/// Accounts, and everything that belongs to one: the library, likes, playlists.
/// </summary>
/// <remarks>
/// <para>
/// Credentials never pass through here. An account crosses the protocol as metadata -- a name, an
/// email, the id of its web profile -- and Rust stores it; the cookies stay in the WebView2
/// profile the sign-in window wrote them into. Account-scoped reads and changes run in that
/// profile through <see cref="PersonalCatalogHost"/>, never through the service, whose catalog
/// reads are anonymous by construction.
/// </para>
/// <para>
/// Changing account is a playback transition, so it follows the lease: the renderer is silenced
/// and the lease given back before Rust is asked, and Rust refuses the change if anything still
/// owns playback.
/// </para>
/// </remarks>
public sealed partial class ShellViewModel
{
    private AccountViewModel? _activeAccount;
    private bool _accountBusy;

    /// <summary>A sign-in, sign-out or switch is under way; the account controls wait for it.</summary>
    public bool IsAccountBusy
    {
        get => _accountBusy;
        private set
        {
            if (Set(ref _accountBusy, value))
            {
                OnPropertyChanged(nameof(IsAccountIdle));
                OnPropertyChanged(nameof(ConnectionLabel));
            }
        }
    }

    public bool IsAccountIdle => !_accountBusy;
    private (string BrowseId, string Title, string Shape)? _personalSource;
    private string? _pagePlaylistId;
    private string? _pageArtistId;
    private string _librarySection = "FEmusic_liked_playlists";
    private readonly Dictionary<string, string> _ratings = new();

    internal PersonalCatalogHost? Personal { get; set; }
    internal OfficialPlaybackHost? Playback { get; set; }

    public ObservableCollection<PlaylistSummaryViewModel> UserPlaylists { get; } = [];

    public IReadOnlyList<LibrarySection> LibrarySections { get; } =
    [
        new("Playlists", "FEmusic_liked_playlists", "shelves"),
        new("Songs", "VLLM", "tracks"),
        new("Albums", "FEmusic_liked_albums", "shelves"),
        new("Artists", "FEmusic_library_corpus_artists", "shelves"),
        new("Subscriptions", "FEmusic_library_corpus_track_artists", "shelves"),
    ];

    public bool IsSignedIn => _activeAccount is not null;

    public bool IsGuest => _activeAccount is null;

    public string AccountName => _activeAccount?.DisplayName ?? "Guest";
    public Microsoft.UI.Xaml.Media.Imaging.BitmapImage? AccountArtwork => _activeAccount?.Artwork;

    public string AccountDetail => _activeAccount?.Detail ?? "Sign in to see your library, likes and playlists";

    /// <summary>Whether the page on screen is a library section, so its chips show.</summary>
    public bool IsLibraryPage { get => _isLibraryPage; private set => Set(ref _isLibraryPage, value); }
    private bool _isLibraryPage;

    /// <summary>The playlist on screen, when it is one this account owns.</summary>
    public bool IsOwnedPlaylistPage => _pagePlaylistId is { } id && UserPlaylists.Any(playlist => playlist.Id == id);

    /// <summary>A playlist on screen the account does not own, which it can save to its library.</summary>
    public bool CanSavePagePlaylist => IsSignedIn && _pagePlaylistId is not null && !IsOwnedPlaylistPage;

    public bool CanFollowPageArtist => IsSignedIn && _pageArtistId is not null;

    public bool HasPageActions => IsOwnedPlaylistPage || CanSavePagePlaylist || CanFollowPageArtist;

    /// <summary>The rating chosen for the confirmed track in this session, for the transport's buttons.</summary>
    public bool IsNowPlayingLiked => _confirmedTrack?.VideoId is { } id && _ratings.GetValueOrDefault(id) == "LIKE";

    public bool IsNowPlayingDisliked => _confirmedTrack?.VideoId is { } id && _ratings.GetValueOrDefault(id) == "DISLIKE";

    internal string? PagePlaylistId => _pagePlaylistId;

    internal string? PageArtistId => _pageArtistId;

    private void AccountChanged()
    {
        foreach (var name in new[]
        {
            nameof(IsSignedIn), nameof(IsGuest), nameof(AccountName), nameof(AccountDetail), nameof(ConnectionLabel),
            nameof(IsOwnedPlaylistPage), nameof(CanSavePagePlaylist), nameof(CanFollowPageArtist),
            nameof(HasPageActions),
        })
        {
            OnPropertyChanged(name);
        }
    }

    private void PageActionsChanged()
    {
        OnPropertyChanged(nameof(IsOwnedPlaylistPage));
        OnPropertyChanged(nameof(CanSavePagePlaylist));
        OnPropertyChanged(nameof(CanFollowPageArtist));
        OnPropertyChanged(nameof(HasPageActions));
        OnPropertyChanged(nameof(IsPageArtistSubscribed));
        OnPropertyChanged(nameof(SubscribeLabel));
        OnPropertyChanged(nameof(SubscribeGlyph));
    }

    private void ForgetPersonalPage()
    {
        // Leaving the page ends editing; its rows are about to be replaced anyway.
        EndPlaylistEdit();
        _personalSource = null;
        _pagePlaylistId = null;
        _pageArtistId = null;
        IsLibraryPage = false;
        IsSettingsPage = false;
        IsSearchPage = false;
        PageActionsChanged();
    }

    // ---- Accounts -------------------------------------------------------------------------

    /// <summary>Reads metadata persisted by Rust; credentials never appear in this response.</summary>
    internal async Task RefreshAccountsAsync()
    {
        AccountStatus = "Checking account…";
        try
        {
            var response = await _client.RequestAsync("accounts.get").ConfigureAwait(true);
            ApplySnapshot(response);
        }
        catch (Exception error)
        {
            BridgeLog.Write($"accounts.get failed: {error}");
            AccountStatus = "Account unavailable: " + Describe(error);
        }
    }

    /// <summary>Shows a snapshot and binds the web surfaces to the account it names as active.</summary>
    private void ApplySnapshot(JsonNode? response)
    {
        var snapshot = response.Deserialize<AccountsResponsePayload>(ServiceProtocol.Json)?.Accounts;
        if (snapshot is null)
        {
            BridgeLog.Write($"account snapshot unreadable: {response?.ToJsonString()}");
            return;
        }

        Accounts.Clear();
        foreach (var account in snapshot.Accounts)
        {
            var model = new AccountViewModel(account, account.Id == snapshot.ActiveAccountId);
            model.PropertyChanged += (_, args) =>
            {
                if (args.PropertyName == nameof(AccountViewModel.Artwork) && model.IsActive)
                {
                    OnPropertyChanged(nameof(AccountArtwork));
                }
            };
            Accounts.Add(model);
            _ = model.LoadArtworkAsync(_artwork);
        }

        var active = Accounts.FirstOrDefault(account => account.IsActive);
        var changed = active?.Id != _activeAccount?.Id;
        _activeAccount = active;
        AccountInitials = active?.Initial ?? "G";
        OnPropertyChanged(nameof(AccountArtwork));
        AccountStatus = active?.DisplayName ?? "Browsing as a guest";
        Personal?.Bind(active?.WebProfileId);
        Playback?.BindProfile(active?.WebProfileId);
        AccountChanged();
        if (changed)
        {
            UserPlaylists.Clear();
            _ratings.Clear();
            if (active is not null)
            {
                _ = LoadUserPlaylistsAsync();
            }
        }
    }

    /// <summary>
    /// Silences the renderer and gives the lease back, returning the generation to transition from.
    /// </summary>
    private async Task<ulong> QuiesceAsync()
    {
        Playback?.Stop();
        IsPlaying = false;
        var snapshot = await _client.RequestAsync("state.get").ConfigureAwait(true);
        var owner = snapshot?["state"]?["owner"]?.GetValue<string>() ?? "none";
        var generation = snapshot?["state"]?["generation"]?.GetValue<ulong>() ?? 0;
        if (owner != "none")
        {
            var released = await _client.RequestAsync("playback.release", new JsonObject
            {
                ["owner"] = owner,
                ["generation"] = generation,
            }).ConfigureAwait(true);
            generation = released?["state"]?["generation"]?.GetValue<ulong>() ?? generation + 1;
        }

        return generation;
    }

    /// <summary>
    /// Opens Google's sign-in, and keeps the account only once Rust has stored and activated it.
    /// </summary>
    internal async Task SignInAsync()
    {
        if (_accountBusy)
        {
            return;
        }

        IsAccountBusy = true;
        var window = new AccountLoginWindow();
        try
        {
            ReportStatus("Sign in to YouTube Music in the window that opened.");
            var result = await window.RunAsync().ConfigureAwait(true);
            if (result is null)
            {
                ReportStatus("Sign-in was cancelled.");
                await window.DiscardAsync().ConfigureAwait(true);
                return;
            }

            var account = new JsonObject
            {
                ["id"] = result.AccountId,
                ["webkitProfileId"] = result.ProfileId,
                ["displayName"] = result.Summary.DisplayName,
            };
            if (result.Summary.Email is { } email) account["email"] = email;
            if (result.Summary.Channel is { } channel) account["channel"] = channel;
            if (result.Summary.AvatarUrl is { } avatar) account["avatarUrl"] = avatar;

            try
            {
                await _client.RequestAsync("accounts.upsert", new JsonObject { ["account"] = account })
                    .ConfigureAwait(true);
                var generation = await QuiesceAsync().ConfigureAwait(true);
                var activated = await _client.RequestAsync("accounts.activate", new JsonObject
                {
                    ["accountId"] = result.AccountId,
                    ["generation"] = generation,
                }).ConfigureAwait(true);

                // Kept only now: stored, activated, and about to be bound.
                window.CommitPromotion();
                ApplySnapshot(activated);
                ReportStatus($"Signed in as {AccountName}.");
            }
            catch (Exception error)
            {
                ReportStatus("Could not finish signing in: " + Describe(error));
                await window.DiscardAsync().ConfigureAwait(true);
                // The metadata may have been stored before activation failed; an account without
                // its profile is not one to offer.
                try
                {
                    await _client.RequestAsync("accounts.remove", new JsonObject { ["accountId"] = result.AccountId })
                        .ConfigureAwait(true);
                }
                catch (Exception)
                {
                    // Nothing was stored, or it is active and will be reconciled on the next read.
                }

                await RefreshAccountsAsync().ConfigureAwait(true);
                return;
            }

            await LoadRouteAsync("home").ConfigureAwait(true);
        }
        finally
        {
            window.Close();
            IsAccountBusy = false;
        }
    }

    /// <summary>Makes a stored account active, or browses as a guest when <paramref name="accountId"/> is null.</summary>
    internal async Task SwitchAccountAsync(string? accountId)
    {
        if (_accountBusy || accountId == _activeAccount?.Id)
        {
            return;
        }

        IsAccountBusy = true;
        var switchFailed = false;
        try
        {
            // The page belongs to the account being left; show it loading rather than stale.
            Shelves.Clear();
            Tracks.Clear();
            PageState = PageState.Loading;
            var generation = await QuiesceAsync().ConfigureAwait(true);
            var payload = new JsonObject { ["generation"] = generation };
            if (accountId is not null)
            {
                payload["accountId"] = accountId;
            }

            var response = await _client
                .RequestAsync(accountId is null ? "account.change" : "accounts.activate", payload)
                .ConfigureAwait(true);
            if (response?["accounts"] is not null)
            {
                ApplySnapshot(response);
            }
            else
            {
                await RefreshAccountsAsync().ConfigureAwait(true);
            }

            ReportStatus(accountId is null ? "Browsing as a guest." : $"Switched to {AccountName}.");
            await LoadRouteAsync("home").ConfigureAwait(true);
        }
        catch (Exception error)
        {
            ReportStatus("Could not switch account: " + Describe(error));
            switchFailed = true;
        }
        finally
        {
            IsAccountBusy = false;
        }

        // The page was cleared for the switch; put back what the unchanged account shows.
        if (switchFailed)
        {
            await RetryPageAsync().ConfigureAwait(true);
        }
    }

    /// <summary>
    /// Signs the active account out: its profile is cleared, then Rust forgets the account.
    /// </summary>
    internal async Task SignOutAsync()
    {
        if (_accountBusy || _activeAccount is not { } account)
        {
            return;
        }

        IsAccountBusy = true;
        try
        {
            if (Personal is not null)
            {
                await Personal.ClearProfileAsync().ConfigureAwait(true);
            }

            var generation = await QuiesceAsync().ConfigureAwait(true);
            var response = await _client.RequestAsync("accounts.remove", new JsonObject
            {
                ["accountId"] = account.Id,
                ["generation"] = generation,
            }).ConfigureAwait(true);
            if (response?["accounts"] is not null)
            {
                ApplySnapshot(response);
            }
            else
            {
                await RefreshAccountsAsync().ConfigureAwait(true);
            }

            ReportStatus($"Signed out of {account.DisplayName}.");
            await LoadRouteAsync("home").ConfigureAwait(true);
        }
        catch (Exception error)
        {
            ReportStatus("Could not sign out: " + Describe(error));
        }
        finally
        {
            IsAccountBusy = false;
        }
    }

    // ---- Personal pages -------------------------------------------------------------------

    /// <summary>Loads Home, the library, liked music or history from the account, when that is what was asked for.</summary>
    private async Task<bool> TryLoadPersonalRouteAsync(string route)
    {
        (string BrowseId, string Title, string Shape)? source = route switch
        {
            "home" when IsSignedIn => ("FEmusic_home", "Home", "shelves"),
            "library" => (_librarySection, "Library", LibrarySections.FirstOrDefault(s => s.BrowseId == _librarySection)?.Shape ?? "shelves"),
            "liked" => ("VLLM", "Liked Music", "tracks"),
            "history" => ("FEmusic_history", "History", "auto"),
            _ => null,
        };
        if (source is null)
        {
            return false;
        }

        if (!IsSignedIn || Personal is null)
        {
            PageSubtitle = "Sign in to see this";
            PageState = PageState.SignInRequired;
            return true;
        }

        IsLibraryPage = route == "library";
        PageSubtitle = AccountName;
        await LoadPersonalAsync(source.Value).ConfigureAwait(true);
        return true;
    }

    /// <summary>Shows another library section in place.</summary>
    internal async Task ShowLibrarySectionAsync(LibrarySection section)
    {
        _librarySection = section.BrowseId;
        await LoadRouteAsync("library", rememberCurrentRoute: false).ConfigureAwait(true);
    }

    /// <summary>
    /// Opens a playlist through the account, so private playlists open and their rows can be edited.
    /// </summary>
    private async Task<bool> TryOpenPersonalEntityAsync(string kind, string id, string title)
    {
        if (kind == "artist")
        {
            _pageArtistId = id;
            PageActionsChanged();
        }

        if (kind != "playlist")
        {
            return false;
        }

        _pagePlaylistId = id.StartsWith("VL", StringComparison.Ordinal) ? id[2..] : id;
        PageActionsChanged();
        if (!IsSignedIn || Personal is null)
        {
            return false;
        }

        await LoadPersonalAsync(("VL" + _pagePlaylistId, title, "tracks")).ConfigureAwait(true);
        return true;
    }

    private async Task LoadPersonalAsync((string BrowseId, string Title, string Shape) source)
    {
        _personalSource = source;
        try
        {
            var page = await Personal!.BrowseAsync(source.BrowseId, source.Title, null, source.Shape)
                .ConfigureAwait(true);
            if (_personalSource != source)
            {
                return;
            }

            if (!string.IsNullOrWhiteSpace(page.Title) && source.Shape == "tracks")
            {
                PageTitle = page.Title;
            }

            if (!string.IsNullOrWhiteSpace(page.Subtitle))
            {
                PageSubtitle = page.Subtitle;
            }

            Fill(page);
            Status = "";
            PageState = Tracks.Count == 0 && Shelves.Count == 0
                ? PageState.Empty(PageSubject.Library, source.Title)
                : PageState.Content;
        }
        catch (Exception error)
        {
            if (_personalSource == source)
            {
                PageState = FailureState(error, source.Title);
            }
            else
            {
                Describe(error);
            }
        }
    }

    private void Fill(CatalogPage page)
    {
        foreach (var track in Listed(page.Tracks))
        {
            var row = new TrackViewModel(track);
            Tracks.Add(row);
            _ = row.LoadArtworkAsync(_artwork);
            if (ShowPageHeader && PageArtwork is null)
            {
                _ = SetPageArtworkAsync(row, _pageArtworkVersion);
            }
        }

        foreach (var shelf in ListedShelves(page.Shelves))
        {
            var model = new ShelfViewModel(shelf);
            Shelves.Add(model);
            foreach (var card in model.Items)
            {
                _ = card.LoadArtworkAsync(_artwork);
            }
        }

        NextCursor = page.NextCursor;
        AllTracksId = page.AllTracksId;
        SetPageTruncated(page.Truncated);
    }

    /// <summary>Continues a page the account's reader issued, which only that reader understands.</summary>
    private async Task<bool> TryLoadMorePersonalAsync(string cursor)
    {
        if (_personalSource is not { } source || Personal is null)
        {
            return false;
        }

        var page = await Personal.BrowseAsync(source.BrowseId, source.Title, cursor, source.Shape).ConfigureAwait(true);
        if (_personalSource == source && _nextCursor == cursor)
        {
            Fill(page);
        }

        return true;
    }

    // ---- Changes to the account -----------------------------------------------------------

    internal async Task LoadUserPlaylistsAsync()
    {
        if (Personal is null || !IsSignedIn)
        {
            return;
        }

        try
        {
            var answer = await Personal.MutateAsync("listUserPlaylists", new JsonObject()).ConfigureAwait(true);
            UserPlaylists.Clear();
            foreach (var item in answer?["value"]?.AsArray() ?? [])
            {
                if (item?["id"]?.GetValue<string>() is { Length: > 0 } id)
                {
                    var playlist = new PlaylistSummaryViewModel(
                        id,
                        item["title"]?.GetValue<string>() ?? "",
                        item["subtitle"]?.GetValue<string>() ?? "",
                        item["thumbnail"]?.GetValue<string>());
                    UserPlaylists.Add(playlist);
                    _ = playlist.LoadArtworkAsync(_artwork);
                }
            }

            PageActionsChanged();
        }
        catch (Exception error)
        {
            BridgeLog.Write($"playlists could not be listed: {error.Message}");
        }
    }

    /// <summary>Runs one named change, reporting the outcome rather than assuming it.</summary>
    private async Task<JsonNode?> ChangeAsync(string operation, JsonObject arguments, string done)
    {
        if (Personal is null || !IsSignedIn)
        {
            ReportStatus("Sign in to change your library.");
            return null;
        }

        try
        {
            var answer = await Personal.MutateAsync(operation, arguments).ConfigureAwait(true);
            ReportStatus(done);
            return answer ?? new JsonObject();
        }
        catch (Exception error)
        {
            ReportStatus("YouTube Music refused that: " + Describe(error));
            return null;
        }
    }

    /// <summary>Likes, dislikes, or clears the rating of a track.</summary>
    internal async Task RateAsync(string? videoId, string title, string rating)
    {
        if (string.IsNullOrEmpty(videoId))
        {
            return;
        }

        var done = rating switch
        {
            "LIKE" => $"Added “{title}” to your liked music.",
            "DISLIKE" => $"You won't be recommended “{title}” as often.",
            _ => $"Removed your rating from “{title}”.",
        };
        if (await ChangeAsync("rateTrack", new JsonObject { ["videoId"] = videoId, ["status"] = rating }, done)
                .ConfigureAwait(true) is not null)
        {
            _ratings[videoId] = rating;
            OnPropertyChanged(nameof(IsNowPlayingLiked));
            OnPropertyChanged(nameof(IsNowPlayingDisliked));
        }
    }

    /// <summary>The rating chosen for a track in this session, if any.</summary>
    internal string RatingOf(string? videoId) =>
        videoId is not null && _ratings.TryGetValue(videoId, out var rating) ? rating : "INDIFFERENT";

    /// <summary>Toggles the like on the confirmed track, for the transport's button.</summary>
    internal Task ToggleNowPlayingRatingAsync(string rating)
    {
        if (_confirmedTrack is not { } track)
        {
            ReportStatus("Nothing is playing.");
            return Task.CompletedTask;
        }

        return RateAsync(track.VideoId, track.Title, RatingOf(track.VideoId) == rating ? "INDIFFERENT" : rating);
    }

    internal Task AddToPlaylistAsync(PlaylistSummaryViewModel playlist, string? videoId, string title) =>
        string.IsNullOrEmpty(videoId)
            ? Task.CompletedTask
            : ChangeAsync("addToPlaylist", new JsonObject { ["playlistId"] = playlist.Id, ["videoId"] = videoId },
                $"Saved “{title}” to {playlist.Title}.");

    /// <summary>Removes this exact occurrence of a track from the playlist on screen.</summary>
    internal async Task RemoveFromPagePlaylistAsync(TrackViewModel track)
    {
        if (_pagePlaylistId is not { } playlist || string.IsNullOrEmpty(track.EntryId) || string.IsNullOrEmpty(track.VideoId))
        {
            ReportStatus("That row cannot be removed from this playlist.");
            return;
        }

        var answer = await ChangeAsync("removeFromPlaylist", new JsonObject
        {
            ["playlistId"] = playlist,
            ["videoId"] = track.VideoId,
            ["setVideoId"] = track.EntryId,
        }, $"Removed “{track.Title}” from this playlist.").ConfigureAwait(true);
        if (answer is not null)
        {
            Tracks.Remove(track);
        }
    }

    /// <summary>Creates a playlist, optionally holding a first track, and returns its id.</summary>
    internal async Task<string?> CreatePlaylistAsync(string title, string privacy, string? videoId)
    {
        var arguments = new JsonObject { ["title"] = title, ["privacy"] = privacy };
        if (!string.IsNullOrEmpty(videoId))
        {
            arguments["videoIds"] = new JsonArray(videoId);
        }

        var answer = await ChangeAsync("createPlaylist", arguments, $"Created “{title}”.").ConfigureAwait(true);
        await LoadUserPlaylistsAsync().ConfigureAwait(true);
        return answer?["playlistId"]?.GetValue<string>();
    }

    internal async Task RenamePagePlaylistAsync(string title)
    {
        if (_pagePlaylistId is not { } playlist)
        {
            return;
        }

        if (await ChangeAsync("renamePlaylist", new JsonObject { ["playlistId"] = playlist, ["title"] = title },
                $"Renamed to “{title}”.").ConfigureAwait(true) is not null)
        {
            PageTitle = title;
            await LoadUserPlaylistsAsync().ConfigureAwait(true);
        }
    }

    internal Task SetPagePlaylistPrivacyAsync(string privacy) =>
        _pagePlaylistId is { } playlist
            ? ChangeAsync("setPlaylistPrivacy", new JsonObject { ["playlistId"] = playlist, ["privacy"] = privacy },
                $"This playlist is now {privacy.ToLowerInvariant()}.")
            : Task.CompletedTask;

    /// <summary>Deletes a playlist the account owns. There is no undo upstream; the view confirms first.</summary>
    internal async Task DeletePlaylistAsync(string playlistId, string title)
    {
        var bare = playlistId.StartsWith("VL", StringComparison.Ordinal) ? playlistId[2..] : playlistId;
        if (await ChangeAsync("deletePlaylist", new JsonObject { ["playlistId"] = bare }, $"Deleted “{title}”.")
                .ConfigureAwait(true) is null)
        {
            return;
        }

        await LoadUserPlaylistsAsync().ConfigureAwait(true);
        if (_pagePlaylistId == bare)
        {
            await LoadRouteAsync("library", rememberCurrentRoute: false).ConfigureAwait(true);
        }
    }

    /// <summary>Saves a playlist to the library, or removes it.</summary>
    internal Task SavePlaylistAsync(string playlistId, string title, bool saved) =>
        ChangeAsync("ratePlaylist", new JsonObject
        {
            ["playlistId"] = playlistId.StartsWith("VL", StringComparison.Ordinal) ? playlistId[2..] : playlistId,
            ["saved"] = saved,
        }, saved ? $"Saved “{title}” to your library." : $"Removed “{title}” from your library.");

    /// <summary>Channels this session subscribed to or left, by channel id.</summary>
    private readonly Dictionary<string, bool> _subscriptions = [];
    private bool _subscriptionBusy;

    public bool IsPageArtistSubscribed =>
        _pageArtistId is { } id && _subscriptions.GetValueOrDefault(id);

    public bool IsSubscriptionIdle => !_subscriptionBusy;

    public string SubscribeLabel => IsPageArtistSubscribed ? "Subscribed" : "Subscribe";

    /// <summary>Segoe Fluent Icons: a check once subscribed, a plus-person before.</summary>
    public string SubscribeGlyph => IsPageArtistSubscribed ? "" : "";

    /// <summary>Subscribes to an artist's channel, or unsubscribes, and shows the result on the button.</summary>
    internal async Task FollowArtistAsync(string channelId, string name, bool follow)
    {
        if (_subscriptionBusy)
        {
            return;
        }

        SetSubscriptionBusy(true);
        try
        {
            var answer = await ChangeAsync("setSubscription",
                new JsonObject { ["channelId"] = channelId, ["subscribed"] = follow },
                follow ? $"Subscribed to {name}." : $"Unsubscribed from {name}.").ConfigureAwait(true);
            if (answer is not null)
            {
                _subscriptions[channelId] = follow;
            }
            else
            {
                BridgeLog.Write($"subscription change for a channel did not complete (follow={follow})");
            }
        }
        finally
        {
            SetSubscriptionBusy(false);
        }
    }

    private void SetSubscriptionBusy(bool busy)
    {
        _subscriptionBusy = busy;
        OnPropertyChanged(nameof(IsSubscriptionIdle));
        OnPropertyChanged(nameof(IsPageArtistSubscribed));
        OnPropertyChanged(nameof(SubscribeLabel));
        OnPropertyChanged(nameof(SubscribeGlyph));
    }

    internal bool OwnsPlaylist(string playlistId)
    {
        var bare = playlistId.StartsWith("VL", StringComparison.Ordinal) ? playlistId[2..] : playlistId;
        return UserPlaylists.Any(playlist => playlist.Id == bare);
    }
}
