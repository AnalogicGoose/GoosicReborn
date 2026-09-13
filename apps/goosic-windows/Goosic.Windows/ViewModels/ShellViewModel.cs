using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Linq;
using System.Runtime.CompilerServices;
using System.Text.Json;
using System.IO;
using System.Text.Json.Nodes;
using System.Threading.Tasks;
using Goosic.Windows.Service;
using Microsoft.UI.Xaml.Media.Imaging;

namespace Goosic.Windows.ViewModels;

/// <summary>One entry in the rail and the named sidebar.</summary>
/// <remarks>
/// The glyphs are Segoe Fluent Icons code points, so they follow the system font rather than
/// shipping an icon set the shell would then have to theme itself.
/// </remarks>
public sealed record RouteEntry(string Route, string Title, string Glyph)
{
    public static IReadOnlyList<RouteEntry> All { get; } =
    [
        new("home", "Home", ""),
        new("explore", "Explore", ""),
        new("charts", "Charts", ""),
        new("moodsAndGenres", "Moods & genres", ""),
        new("newReleases", "New releases", ""),
        new("library", "Library", ""),
        new("downloads", "Downloads", ""),
    ];
}

public sealed class CardViewModel : INotifyPropertyChanged
{
    internal CardViewModel(CatalogItem item)
    {
        Title = item.Title;
        Subtitle = string.IsNullOrWhiteSpace(item.Subtitle) ? item.Artist ?? "" : item.Subtitle;
        Id = item.Id;
        Kind = item.Kind;
        VideoId = item.VideoId;
        Thumbnail = item.Thumbnail;
    }

    public string Title { get; }
    public string Subtitle { get; }
    internal string Id { get; }
    internal string Kind { get; }

    /// <summary>
    /// What activating the card means, as one string the view can hand back.
    /// </summary>
    /// <remarks>
    /// A bare video id plays; anything else is an entity to open, encoded as kind, id and title
    /// joined by a unit separator -- a character no title or id carries, so the three parts come
    /// back apart exactly as they went in.
    /// </remarks>
    public string ActivationTag => VideoId is { Length: > 0 } video
        ? video
        : string.Join(ShellViewModel.KeySeparator, Kind, Id, Title);
    internal string? VideoId { get; }
    internal string? Thumbnail { get; }
    internal IReadOnlyList<CardViewModel> Context { get; set; } = [];

    /// <summary>The id for the view to hand back, or empty when the card is not a track.</summary>
    public string VideoIdOrEmpty => VideoId ?? "";

    public event PropertyChangedEventHandler? PropertyChanged;

    /// <summary>
    /// The artwork, once it has been fetched.
    /// </summary>
    /// <remarks>
    /// Left null until <see cref="ArtworkLoader"/> has the bytes on disk. Binding the control to
    /// the remote URL instead would let a catalog response point the shell at an arbitrary host,
    /// which the allow-list exists to prevent -- so the view only ever sees a local file.
    /// </remarks>
    public BitmapImage? Artwork
    {
        get => _artwork;
        private set
        {
            _artwork = value;
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Artwork)));
        }
    }

    private BitmapImage? _artwork;

    /// <summary>Asks the loader for this card's cover and shows it if one arrives.</summary>
    internal async Task LoadArtworkAsync(ArtworkLoader loader)
    {
        var file = await loader.LocalFileAsync(Thumbnail).ConfigureAwait(true);
        if (file is null)
        {
            return;
        }

        var image = new BitmapImage();
        using var stream = File.OpenRead(file);
        await image.SetSourceAsync(stream.AsRandomAccessStream());
        Artwork = image;
    }
}

/// <summary>One track row: a search result, or an ordered list on a detail page.</summary>
public sealed class TrackViewModel : INotifyPropertyChanged
{
    internal TrackViewModel(CatalogItem item)
    {
        Title = item.Title;
        Subtitle = string.IsNullOrWhiteSpace(item.Subtitle) ? item.Artist ?? "" : item.Subtitle;
        Duration = item.Duration ?? "";
        VideoId = item.VideoId;
        Thumbnail = item.Thumbnail;
        Explicit = item.Explicit;
    }

    /// <summary>Promotes a playable shelf card without inventing metadata it did not carry.</summary>
    internal TrackViewModel(CardViewModel card)
    {
        Title = card.Title;
        Subtitle = card.Subtitle;
        Duration = "";
        VideoId = card.VideoId;
        Thumbnail = card.Thumbnail;
        Artwork = card.Artwork;
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public string Title { get; }
    public string Subtitle { get; }
    public string Duration { get; }
    public bool Explicit { get; }
    internal string? VideoId { get; }
    internal string? Thumbnail { get; }

    /// <summary>The id for the view to hand back, or empty when the row cannot be played.</summary>
    public string VideoIdOrEmpty => VideoId ?? "";

    public BitmapImage? Artwork
    {
        get => _artwork;
        private set
        {
            _artwork = value;
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Artwork)));
        }
    }

    private BitmapImage? _artwork;

    internal async Task LoadArtworkAsync(ArtworkLoader loader)
    {
        var file = await loader.LocalFileAsync(Thumbnail).ConfigureAwait(true);
        if (file is null)
        {
            return;
        }

        var image = new BitmapImage();
        using var stream = File.OpenRead(file);
        await image.SetSourceAsync(stream.AsRandomAccessStream());
        Artwork = image;
    }
}

public sealed class ShelfViewModel
{
    internal ShelfViewModel(CatalogShelf shelf)
    {
        Title = shelf.Title;
        Items = new ObservableCollection<CardViewModel>(shelf.Items.Select(item => new CardViewModel(item)));
        foreach (var item in Items)
        {
            item.Context = Items;
        }
    }

    public string Title { get; }
    public ObservableCollection<CardViewModel> Items { get; }
}

/// <summary>One line shown in the lyrics panel, never synthesized by the shell.</summary>
/// <summary>One line of lyrics, and whether the music is on it.</summary>
public sealed class LyricLineViewModel : INotifyPropertyChanged
{
    private bool _isCurrent;

    internal LyricLineViewModel(string text, long atMilliseconds)
    {
        Text = text;
        AtMilliseconds = atMilliseconds;
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public string Text { get; }
    public long AtMilliseconds { get; }

    public bool IsCurrent
    {
        get => _isCurrent;
        internal set
        {
            if (_isCurrent == value)
            {
                return;
            }

            _isCurrent = value;
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(IsCurrent)));
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Emphasis)));
        }
    }

    /// <summary>The current line at full strength, the rest receding, as the reference does.</summary>
    public double Emphasis => IsCurrent ? 1.0 : 0.45;
}
public sealed record AccountViewModel(string DisplayName, string Detail);

/// <summary>What the window is showing, and how it asks the service to change it.</summary>
public sealed class ShellViewModel : INotifyPropertyChanged
{
    private readonly GoosicServiceClient _client;
    private readonly ArtworkLoader _artwork = new();
    private string _pageTitle = "Home";
    private string _pageSubtitle = "Live from YouTube Music, browsed as a guest";
    private string _status = "";
    private string _accountInitials = "G";
    /// <summary>Separates the parts of a page key; it appears in no title, id or query.</summary>
    internal const string KeySeparator = "\u001f";

    /// <summary>
    /// What is on screen, as enough to put it back.
    /// </summary>
    /// <remarks>
    /// History used to hold bare route names, so going back to a search asked
    /// <c>catalog.browse</c> for a surface called "search". A key says which kind of page it was
    /// and carries what reloading it needs.
    /// </remarks>
    private string _currentRoute = "route" + KeySeparator + "home";
    private readonly Stack<string> _routeHistory = new();

    internal ShellViewModel(GoosicServiceClient client)
    {
        _client = client;
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public IReadOnlyList<RouteEntry> Routes => RouteEntry.All;
    public ObservableCollection<ShelfViewModel> Shelves { get; } = [];
    public ObservableCollection<TrackViewModel> Tracks { get; } = [];
    /// <summary>The real catalog rows surrounding the user's current selection.</summary>
    public ObservableCollection<TrackViewModel> Queue { get; } = [];
    public ObservableCollection<LyricLineViewModel> Lyrics { get; } = [];
    public ObservableCollection<AccountViewModel> Accounts { get; } = [];
    private string _accountStatus = "Checking account…";
    private int _queueIndex = -1;
    private TrackViewModel? _pendingTrack;
    private TrackViewModel? _confirmedTrack;
    private string? _advancedAfterEndVideoId;
    private string _lyricsStatus = "Nothing playing.";
    private double _playbackPosition;
    private double _playbackDuration;
    private double _volume = 1;
    private bool _isMuted;

    /// <summary>The filters `catalog.search` accepts, in the order they are offered.</summary>
    public IReadOnlyList<string> SearchFilters { get; } = ["all", "songs", "albums", "artists", "playlists"];

    public string PageTitle { get => _pageTitle; private set => Set(ref _pageTitle, value); }
    public string PageSubtitle { get => _pageSubtitle; private set => Set(ref _pageSubtitle, value); }
    public string AccountInitials { get => _accountInitials; private set => Set(ref _accountInitials, value); }

    public string Status
    {
        get => _status;
        private set
        {
            if (Set(ref _status, value))
            {
                OnPropertyChanged(nameof(HasStatus));
            }
        }
    }

    public bool HasStatus => !string.IsNullOrEmpty(_status);

    private string _nowPlayingTitle = "Nothing playing";
    private string _nowPlayingSubtitle = "Choose a track to begin";
    private BitmapImage? _nowPlayingArtwork;

    public string NowPlayingTitle { get => _nowPlayingTitle; private set => Set(ref _nowPlayingTitle, value); }
    public string NowPlayingSubtitle { get => _nowPlayingSubtitle; private set => Set(ref _nowPlayingSubtitle, value); }
    public BitmapImage? NowPlayingArtwork { get => _nowPlayingArtwork; private set => Set(ref _nowPlayingArtwork, value); }
    public string LyricsStatus { get => _lyricsStatus; private set => Set(ref _lyricsStatus, value); }
    public string AccountStatus { get => _accountStatus; private set => Set(ref _accountStatus, value); }
    public double PlaybackPosition { get => _playbackPosition; private set => Set(ref _playbackPosition, value); }
    public double PlaybackDuration { get => _playbackDuration; private set => Set(ref _playbackDuration, value); }
    public bool IsSeekable => PlaybackDuration > 0;
    public double Volume
    {
        get => _volume;
        private set
        {
            if (Set(ref _volume, value))
            {
                OnPropertyChanged(nameof(VolumePercent));
                OnPropertyChanged(nameof(VolumeGlyph));
            }
        }
    }

    public bool IsMuted
    {
        get => _isMuted;
        private set
        {
            if (Set(ref _isMuted, value))
            {
                OnPropertyChanged(nameof(VolumeGlyph));
                OnPropertyChanged(nameof(MuteLabel));
            }
        }
    }

    /// <summary>The confirmed volume on the 0-100 scale the slider shows.</summary>
    public double VolumePercent => Math.Round(Volume * 100);

    /// <summary>Segoe Fluent Icons: muted, low, medium or high.</summary>
    public string VolumeGlyph => IsMuted || Volume <= 0 ? "\uE74F"
        : Volume < 0.34 ? "\uE993"
        : Volume < 0.67 ? "\uE994"
        : "\uE995";

    public string MuteLabel => IsMuted ? "Unmute" : "Mute";

    /// <summary>
    /// Set while the listener drags the position slider.
    /// </summary>
    /// <remarks>
    /// Samples arrive twice a second. Moving the slider under the listener's pointer to follow
    /// them would pull the thumb back to where the music is while they are choosing where it
    /// should go.
    /// </remarks>
    internal bool IsScrubbing { get; set; }

    /// <summary>
    /// Shows what the page confirmed.
    /// </summary>
    /// <remarks>
    /// Driven by accepted samples rather than by the request, so the bar says what is happening
    /// instead of what was asked for. An advertisement is named, because it is a marker to be
    /// reported and never something to hide or skip past.
    /// </remarks>
    /// <returns>Whether this is the first confirmed natural end for the current queue item.</returns>
    private bool _isPlaying;
    private bool _lyricsSynced;
    private int _currentLyric = -1;

    /// <summary>
    /// Raised when the page confirms a different track from the one it was confirming.
    /// </summary>
    /// <remarks>
    /// Confirmation, not selection, is what makes lyrics stale: a skip that Rust refuses, or a
    /// load that has not reached the page yet, must leave the lyrics of what is still playing.
    /// </remarks>
    internal event Action? ConfirmedTrackChanged;

    /// <summary>Raised with the index of the lyric line the music has reached.</summary>
    internal event Action<int>? CurrentLyricChanged;

    /// <summary>
    /// Whether the page last confirmed that it is playing.
    /// </summary>
    /// <remarks>
    /// Taken from an accepted sample, never from the click. The transport button offers the
    /// opposite of this, so it must not flip until the page has actually changed state -- a
    /// button that flipped on request would show "pause" over a player that refused to start.
    /// </remarks>
    public bool IsPlaying
    {
        get => _isPlaying;
        private set
        {
            if (Set(ref _isPlaying, value))
            {
                OnPropertyChanged(nameof(PlayPauseGlyph));
                OnPropertyChanged(nameof(PlayPauseLabel));
            }
        }
    }

    /// <summary>Segoe Fluent Icons: Pause while playing, Play otherwise.</summary>
    public string PlayPauseGlyph => IsPlaying ? "\uE769" : "\uE768";

    public string PlayPauseLabel => IsPlaying ? "Pause" : "Play";

    /// <summary>
    /// Moves the highlight to the line the confirmed position has reached.
    /// </summary>
    /// <remarks>
    /// Only for synced lyrics. Plain lyrics carry no times, and highlighting one of them would
    /// claim a position the document never stated.
    /// </remarks>
    private void FollowLyrics(double seconds)
    {
        if (!_lyricsSynced || Lyrics.Count == 0)
        {
            return;
        }

        var at = (long)(seconds * 1000);
        var index = -1;
        for (var i = 0; i < Lyrics.Count; i++)
        {
            if (Lyrics[i].AtMilliseconds > at)
            {
                break;
            }

            index = i;
        }

        if (index == _currentLyric)
        {
            return;
        }

        if (_currentLyric >= 0 && _currentLyric < Lyrics.Count)
        {
            Lyrics[_currentLyric].IsCurrent = false;
        }

        _currentLyric = index;
        if (index >= 0)
        {
            Lyrics[index].IsCurrent = true;
            CurrentLyricChanged?.Invoke(index);
        }
    }

    internal bool ReportPlayback(BridgeEvent sample)
    {
        var position = TimeSpan.FromSeconds(Math.Max(0, sample.CurrentTime));
        var total = TimeSpan.FromSeconds(Math.Max(0, sample.Duration));
        if (!IsScrubbing)
        {
            PlaybackPosition = Math.Max(0, sample.CurrentTime);
        }

        PlaybackDuration = Math.Max(0, sample.Duration);
        Volume = Math.Clamp(sample.Volume, 0, 1);
        IsMuted = sample.Muted;
        IsPlaying = sample.State == "playing";
        OnPropertyChanged(nameof(IsSeekable));
        if (!sample.IsAdvertisement && _pendingTrack is { } pending && pending.VideoId == sample.VideoId)
        {
            var changed = !ReferenceEquals(_confirmedTrack, pending);
            _confirmedTrack = pending;
            if (changed)
            {
                Lyrics.Clear();
                _lyricsSynced = false;
                _currentLyric = -1;
                ConfirmedTrackChanged?.Invoke();
            }

            FollowLyrics(sample.CurrentTime);
            NowPlayingArtwork = pending.Artwork;
            NowPlayingTitle = pending.Title;
            NowPlayingSubtitle = pending.Subtitle + " · " + Clock(position)
                + (total > TimeSpan.Zero ? " / " + Clock(total) : "");
            return MarkNaturalEnd(sample);
        }

        // A sample for a track the model did not ask for: an advertisement, or the moments before
        // the requested page takes over. Name the advertisement; otherwise say it is loading,
        // never the player's raw state word.
        if (sample.IsAdvertisement)
        {
            NowPlayingTitle = "Advertisement";
            NowPlayingSubtitle = total > TimeSpan.Zero ? Clock(position) + " / " + Clock(total) : Clock(position);
        }
        else if (_pendingTrack is { } waiting)
        {
            NowPlayingTitle = waiting.Title;
            NowPlayingSubtitle = "Loading\u2026";
        }

        return MarkNaturalEnd(sample);
    }

    private bool MarkNaturalEnd(BridgeEvent sample)
    {
        if (sample.IsAdvertisement || sample.State != "ended" || _advancedAfterEndVideoId == sample.VideoId)
        {
            return false;
        }

        _advancedAfterEndVideoId = sample.VideoId;
        return true;
    }

    /// <summary>Starts a fresh queue from the real track rows currently on screen.</summary>
    internal void SelectTrack(TrackViewModel track)
    {
        Queue.Clear();
        foreach (var candidate in Tracks.Where(candidate => !string.IsNullOrEmpty(candidate.VideoId)))
        {
            Queue.Add(candidate);
        }

        if (Queue.Count == 0)
        {
            Queue.Add(track);
        }

        _queueIndex = Math.Max(0, Queue.IndexOf(track));
        _pendingTrack = track;
        _advancedAfterEndVideoId = null;
    }

    /// <summary>Builds a queue from the playable cards in the selected card's real shelf.</summary>
    internal void SelectCard(CardViewModel card)
    {
        Queue.Clear();
        foreach (var candidate in card.Context.Where(candidate => !string.IsNullOrEmpty(candidate.VideoId)))
        {
            Queue.Add(new TrackViewModel(candidate));
        }

        if (Queue.Count == 0)
        {
            Queue.Add(new TrackViewModel(card));
        }

        _queueIndex = Math.Max(0, Queue.ToList().FindIndex(item => item.VideoId == card.VideoId));
        _pendingTrack = Queue[_queueIndex];
        _advancedAfterEndVideoId = null;
    }

    /// <summary>Chooses an adjacent real queue entry, wrapping for a deliberate command.</summary>
    internal TrackViewModel? AdjacentTrack(bool forward, bool wrap = true)
    {
        if (Queue.Count == 0)
        {
            ReportStatus("Choose a track to begin.");
            return null;
        }

        var nextIndex = _queueIndex + (forward ? 1 : -1);
        if (!wrap && (nextIndex < 0 || nextIndex >= Queue.Count))
        {
            ReportStatus("The queue has finished.");
            return null;
        }

        _queueIndex = (nextIndex + Queue.Count) % Queue.Count;
        var track = Queue[_queueIndex];
        _pendingTrack = track;
        _advancedAfterEndVideoId = null;
        return track;
    }

    /// <summary>Loads lyrics for the track the official renderer actually confirmed.</summary>
    internal async Task LoadLyricsAsync()
    {
        var track = _confirmedTrack;
        if (track is null)
        {
            Lyrics.Clear();
            LyricsStatus = "Nothing playing.";
            return;
        }

        Lyrics.Clear();
        _lyricsSynced = false;
        _currentLyric = -1;
        LyricsStatus = $"Looking up lyrics for {track.Title}…";
        try
        {
            var query = new JsonObject { ["title"] = track.Title, ["artist"] = track.Subtitle };
            var answer = await _client.RequestAsync("lyrics.get", new JsonObject { ["lyrics"] = query })
                .ConfigureAwait(true);
            var document = answer.Deserialize<LyricsResponsePayload>(ServiceProtocol.Json)?.Document;
            if (document is null || document.Lines.Count == 0)
            {
                LyricsStatus = "No lyrics were found for this track.";
                return;
            }

            foreach (var line in document.Lines)
            {
                Lyrics.Add(new LyricLineViewModel(line.Text, line.AtMilliseconds));
            }

            _lyricsSynced = document.Synced;
            LyricsStatus = document.Synced ? $"Synced lyrics from {document.Source}." : $"Lyrics from {document.Source}.";
            if (document.Truncated)
            {
                LyricsStatus += " Only the first part is shown.";
            }
        }
        catch (ServiceRefusedException refused) when (refused.Code == "lyricsNotFound")
        {
            LyricsStatus = "No lyrics were found for this track.";
        }
        catch (Exception error)
        {
            LyricsStatus = "Could not load lyrics: " + Describe(error);
        }
    }

    /// <summary>A position as minutes and seconds, counting minutes past the hour.</summary>
    private static string Clock(TimeSpan value) =>
        $"{(int)value.TotalMinutes:D2}:{value.Seconds:D2}";

    /// <summary>Says something on screen that did not come from the service.</summary>
    internal void ReportStatus(string message) => Status = message;

    /// <summary>Greets the service, then loads the opening screen.</summary>
    internal async Task StartAsync()
    {
        try
        {
            await _client.RequestAsync("hello").ConfigureAwait(true);
            Status = "";
        }
        catch (Exception error)
        {
            Status = Describe(error);
            return;
        }

        await LoadRouteAsync("home").ConfigureAwait(true);
        await RefreshAccountsAsync().ConfigureAwait(true);
    }

    /// <summary>Reads metadata persisted by Rust; credentials never appear in this response.</summary>
    internal async Task RefreshAccountsAsync()
    {
        AccountStatus = "Checking account…";
        try
        {
            var response = await _client.RequestAsync("accounts.get").ConfigureAwait(true);
            var snapshot = response.Deserialize<AccountsResponsePayload>(ServiceProtocol.Json)?.Accounts;
            Accounts.Clear();
            if (snapshot is null || snapshot.Accounts.Count == 0)
            {
                AccountStatus = "Signed out";
                AccountInitials = "G";
                return;
            }

            foreach (var account in snapshot.Accounts)
            {
                Accounts.Add(new AccountViewModel(account.DisplayName, account.Email ?? "YouTube Music account"));
            }

            var active = snapshot.Accounts.FirstOrDefault(account => account.Id == snapshot.ActiveAccountId)
                ?? snapshot.Accounts[0];
            AccountInitials = string.IsNullOrWhiteSpace(active.DisplayName) ? "G" : active.DisplayName[..1].ToUpperInvariant();
            AccountStatus = active.DisplayName;
        }
        catch (Exception error)
        {
            AccountStatus = "Account unavailable: " + Describe(error);
        }
    }

    /// <summary>Loads one browse surface.</summary>
    internal async Task LoadRouteAsync(string route, bool rememberCurrentRoute = true)
    {
        Remember("route" + KeySeparator + route, rememberCurrentRoute);
        var entry = RouteEntry.All.FirstOrDefault(candidate => candidate.Route == route);
        PageTitle = entry?.Title ?? route;
        Shelves.Clear();
        Tracks.Clear();
        Status = $"Loading {PageTitle.ToLowerInvariant()}…";

        try
        {
            // `catalog.browse` reads the surface from `catalogId`, and uses `query` as the title
            // it echoes back on the page.
            var payload = new JsonObject
            {
                ["catalogId"] = route,
                ["query"] = PageTitle,
            };
            var answer = await _client.RequestAsync("catalog.browse", payload).ConfigureAwait(true);
            var page = answer.Deserialize<CatalogResponsePayload>(ServiceProtocol.Json)?.Page;
            if (page is null)
            {
                Status = "The service answered without a page.";
                return;
            }

            PageSubtitle = string.IsNullOrWhiteSpace(page.Subtitle)
                ? "Live from YouTube Music, browsed as a guest"
                : page.Subtitle;

            foreach (var track in page.Tracks)
            {
                var row = new TrackViewModel(track);
                Tracks.Add(row);
                _ = row.LoadArtworkAsync(_artwork);
            }

            foreach (var shelf in page.Shelves)
            {
                var model = new ShelfViewModel(shelf);
                Shelves.Add(model);
                foreach (var card in model.Items)
                {
                    // Not awaited: a page should render immediately and fill in as covers
                    // arrive, rather than waiting on the slowest thumbnail.
                    _ = card.LoadArtworkAsync(_artwork);
                }
            }

            // A clamped page says so rather than presenting a partial list as complete.
            Status = page.Truncated
                ? "This page was long, so only the first part is shown."
                : "";
        }
        catch (Exception error)
        {
            Status = Describe(error);
        }
    }

    /// <summary>Returns to the prior native catalog route without involving the WebView.</summary>
    internal async Task GoBackAsync()
    {
        if (_routeHistory.Count == 0)
        {
            ReportStatus("There is no earlier page.");
            return;
        }

        var parts = _routeHistory.Pop().Split(KeySeparator);
        switch (parts[0])
        {
            case "search" when parts.Length == 3:
                await SearchAsync(parts[1], parts[2], remember: false).ConfigureAwait(true);
                break;
            case "entity" when parts.Length == 4:
                await OpenEntityAsync(parts[1], parts[2], parts[3], remember: false).ConfigureAwait(true);
                break;
            default:
                await LoadRouteAsync(parts.Length > 1 ? parts[1] : "home", rememberCurrentRoute: false)
                    .ConfigureAwait(true);
                break;
        }
    }

    /// <summary>Records the page being left, then makes <paramref name="key"/> current.</summary>
    private void Remember(string key, bool remember)
    {
        if (remember && !string.Equals(key, _currentRoute, StringComparison.Ordinal))
        {
            _routeHistory.Push(_currentRoute);
        }

        _currentRoute = key;
    }

    /// <summary>Opens an album, playlist or artist.</summary>
    /// <remarks>
    /// Each kind has its own command. A kind this build does not know opens nothing rather than
    /// being guessed at: the protocol decodes unfamiliar kinds precisely so a newer service's row
    /// can be shown inert instead of failing the page.
    /// </remarks>
    internal async Task OpenEntityAsync(string kind, string id, string title, bool remember = true)
    {
        var command = kind switch
        {
            "album" => "catalog.album",
            "playlist" => "catalog.playlist",
            "artist" => "catalog.artist",
            _ => null,
        };
        if (command is null || id.Length == 0)
        {
            Status = "That item cannot be opened.";
            return;
        }

        Remember(string.Join(KeySeparator, "entity", kind, id, title), remember);
        PageTitle = title;
        PageSubtitle = kind switch { "album" => "Album", "playlist" => "Playlist", _ => "Artist" };
        Shelves.Clear();
        Tracks.Clear();
        Status = $"Loading {title}…";

        try
        {
            var answer = await _client.RequestAsync(command, new JsonObject { ["catalogId"] = id })
                .ConfigureAwait(true);
            var page = answer.Deserialize<CatalogResponsePayload>(ServiceProtocol.Json)?.Page;
            if (page is null)
            {
                Status = "The service answered without a page.";
                return;
            }

            if (!string.IsNullOrWhiteSpace(page.Title))
            {
                PageTitle = page.Title;
            }

            if (!string.IsNullOrWhiteSpace(page.Subtitle))
            {
                PageSubtitle = page.Subtitle;
            }

            foreach (var track in page.Tracks)
            {
                var row = new TrackViewModel(track);
                Tracks.Add(row);
                _ = row.LoadArtworkAsync(_artwork);
            }

            foreach (var shelf in page.Shelves)
            {
                var model = new ShelfViewModel(shelf);
                Shelves.Add(model);
                foreach (var card in model.Items)
                {
                    _ = card.LoadArtworkAsync(_artwork);
                }
            }

            Status = Tracks.Count == 0 && Shelves.Count == 0
                ? "This page came back empty."
                : page.Truncated ? "This page was long, so only the first part is shown." : "";
        }
        catch (Exception error)
        {
            Status = Describe(error);
        }
    }

    /// <summary>Searches the catalog, and shows what came back as rows.</summary>
    internal async Task SearchAsync(string query, string filter, bool remember = true)
    {
        var trimmed = query.Trim();
        if (trimmed.Length == 0)
        {
            Status = "Type something to search the catalog.";
            return;
        }

        Remember(string.Join(KeySeparator, "search", trimmed, filter), remember);
        PageTitle = $"Results for “{trimmed}”";
        PageSubtitle = filter == "all" ? "Everything matching" : $"Matching {filter}";
        Shelves.Clear();
        Tracks.Clear();
        Status = "Searching…";

        try
        {
            var payload = new JsonObject { ["query"] = trimmed, ["filter"] = filter };
            var answer = await _client.RequestAsync("catalog.search", payload).ConfigureAwait(true);
            var page = answer.Deserialize<CatalogResponsePayload>(ServiceProtocol.Json)?.Page;
            if (page is null)
            {
                Status = "The service answered without a page.";
                return;
            }

            foreach (var track in page.Tracks)
            {
                var row = new TrackViewModel(track);
                Tracks.Add(row);
                _ = row.LoadArtworkAsync(_artwork);
            }

            foreach (var shelf in page.Shelves)
            {
                var model = new ShelfViewModel(shelf);
                Shelves.Add(model);
                foreach (var card in model.Items)
                {
                    _ = card.LoadArtworkAsync(_artwork);
                }
            }

            Status = Tracks.Count == 0 && Shelves.Count == 0
                ? $"Nothing matched “{trimmed}”."
                : page.Truncated ? "This page was long, so only the first part is shown." : "";
        }
        catch (Exception error)
        {
            Status = Describe(error);
        }
    }

    /// <summary>Turns a transport or refusal into something worth reading on screen.</summary>
    private static string Describe(Exception error) => error switch
    {
        ServiceRefusedException refused => refused.Message,
        ServiceUnavailableException unavailable => unavailable.Message,
        _ => error.Message,
    };

    private bool Set<T>(ref T field, T value, [CallerMemberName] string? name = null)
    {
        if (EqualityComparer<T>.Default.Equals(field, value))
        {
            return false;
        }

        field = value;
        OnPropertyChanged(name);
        return true;
    }

    private void OnPropertyChanged(string? name) =>
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
}
