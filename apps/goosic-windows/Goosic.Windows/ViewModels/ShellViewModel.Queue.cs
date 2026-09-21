using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Threading.Tasks;
using Goosic.Windows.Presentation;
using Goosic.Windows.Service;

namespace Goosic.Windows.ViewModels;

/// <summary>What happens when the queue runs out, or a track ends.</summary>
public enum RepeatMode
{
    Off,
    All,
    One,
}

/// <summary>
/// The queue: what plays next, shuffle, repeat, radio, and the pages that continue.
/// </summary>
/// <remarks>
/// Everything here decides only *what to ask for*. Whether a track may play is still Rust's
/// answer, reached through the playback host's claim; a queue that picked a track Rust refused
/// shows the refusal rather than pretending it moved on.
/// </remarks>
public sealed partial class ShellViewModel
{
    private static readonly Random Shuffler = new();
    private TrackViewModel? _current;
    private bool _shuffle;
    private RepeatMode _repeat = RepeatMode.Off;
    private string? _nextCursor;
    private bool _loadingMore;

    /// <summary>Upcoming entries in the order they were queued, kept while shuffle is on.</summary>
    private List<TrackViewModel>? _unshuffled;

    /// <summary>The station whose recommendations follow the queue, when one is running.</summary>
    private RadioStation? _station;

    /// <summary>Bumped whenever the queue is replaced, so a late page for the old one is dropped.</summary>
    private int _stationRevision;

    /// <summary>A station page on its way; a second request joins it instead of repeating it.</summary>
    private Task<int>? _stationLoad;

    /// <summary>Songs played this session, newest last, kept out of recommendations.</summary>
    private readonly List<string> _recentlyPlayed = [];
    private const int RecentlyPlayedLimit = 200;

    /// <summary>Raised when the track the page confirmed changes, with that track.</summary>
    internal event Action<TrackViewModel>? NowPlayingChanged;

    public bool IsShuffled
    {
        get => _shuffle;
        private set
        {
            if (Set(ref _shuffle, value))
            {
                OnPropertyChanged(nameof(ShuffleLabel));
            }
        }
    }

    public string ShuffleLabel => IsShuffled ? "Shuffle is on" : "Shuffle";

    public RepeatMode Repeat
    {
        get => _repeat;
        private set
        {
            if (Set(ref _repeat, value))
            {
                OnPropertyChanged(nameof(RepeatGlyph));
                OnPropertyChanged(nameof(RepeatLabel));
                OnPropertyChanged(nameof(IsRepeating));
            }
        }
    }

    /// <summary>Segoe Fluent Icons: RepeatOne for one track, RepeatAll otherwise.</summary>
    public string RepeatGlyph => Repeat == RepeatMode.One ? "" : "";

    public string RepeatLabel => Repeat switch
    {
        RepeatMode.All => "Repeat all",
        RepeatMode.One => "Repeat one",
        _ => "Repeat is off",
    };

    public bool IsRepeating => Repeat != RepeatMode.Off;

    /// <summary>The cursor for more of the page on screen, when the service offered one.</summary>
    internal string? NextCursor
    {
        get => _nextCursor;
        private set
        {
            if (Set(ref _nextCursor, value))
            {
                OnPropertyChanged(nameof(HasMore));
            }
        }
    }

    public bool HasMore => !string.IsNullOrEmpty(_nextCursor);

    /// <summary>The playlist holding every song when the page shows only its top few, as an artist does.</summary>
    internal string? AllTracksId
    {
        get => _allTracksId;
        private set
        {
            if (Set(ref _allTracksId, string.IsNullOrEmpty(value) ? null : value))
            {
                OnPropertyChanged(nameof(HasAllTracks));
            }
        }
    }

    private string? _allTracksId;

    public bool HasAllTracks => _allTracksId is not null;

    public bool HasTracks => Tracks.Count > 0;

    public bool HasQueue => Queue.Count > 0;

    public string QueueSummary => Queue.Count switch
    {
        0 => "Nothing queued",
        1 => "1 track",
        var count => $"{count} tracks",
    } + (_station is null ? "" : " · Radio");

    /// <summary>The queue entry the page is confirming, if any.</summary>
    internal TrackViewModel? ConfirmedTrack => _confirmedTrack;

    /// <summary>Makes <paramref name="entry"/> the one the queue is on, and the one to confirm.</summary>
    private TrackViewModel Point(TrackViewModel entry)
    {
        if (!ReferenceEquals(entry, _current))
        {
            // The last song's position would otherwise stand until the new one reports, and
            // Previous would read it as "past the first seconds" and restart the new song.
            PlaybackPosition = 0;
            PlaybackDuration = 0;
            _pendingSeek = null;
        }

        if (_current is not null)
        {
            _current.IsCurrent = false;
        }

        _current = entry;
        entry.IsCurrent = true;
        _pendingTrack = entry;
        OnPropertyChanged(nameof(HasPlayback));
        OnPropertyChanged(nameof(CanSkip));
        // A new play has to be heard playing before its end counts; see MarkNaturalEnd.
        _endArmed = false;
        _endHandled = false;
        _listenerPaused = false;
        _pendingAlias = null;
        RememberPlayed(entry);
        QueueChanged();
        TopUpStation();
        return entry;
    }

    private void RememberPlayed(TrackViewModel entry)
    {
        if (string.IsNullOrEmpty(entry.VideoId))
        {
            return;
        }

        _recentlyPlayed.Remove(entry.VideoId);
        _recentlyPlayed.Add(entry.VideoId);
        if (_recentlyPlayed.Count > RecentlyPlayedLimit)
        {
            _recentlyPlayed.RemoveAt(0);
        }
    }

    private void QueueChanged()
    {
        OnPropertyChanged(nameof(HasQueue));
        OnPropertyChanged(nameof(QueueSummary));
        OnPropertyChanged(nameof(NowPlayingEntry));
        OnPropertyChanged(nameof(HasNowPlayingEntry));
        OnPropertyChanged(nameof(HasUpNext));
        OnPropertyChanged(nameof(HasNoUpNext));
        OnPropertyChanged(nameof(UpNextSummary));
        SyncUpNext();
    }

    /// <summary>The entry the queue is on, shown above Up Next.</summary>
    public TrackViewModel? NowPlayingEntry => _current is not null && Queue.Contains(_current) ? _current : null;

    public bool HasNowPlayingEntry => NowPlayingEntry is not null;

    /// <summary>What plays after the current entry, in order. Reordering it reorders the queue.</summary>
    public ObservableCollection<TrackViewModel> UpNext { get; } = [];

    public bool HasUpNext => QueueLayout.UpNext(Queue, NowPlayingEntry).Count > 0;

    /// <summary>Nothing follows, and nothing is on its way either.</summary>
    public bool HasNoUpNext => HasQueue && !HasUpNext && _stationLoad is null;

    public string UpNextSummary => QueueLayout.UpNext(Queue, NowPlayingEntry).Count switch
    {
        0 when _stationLoad is not null => "Finding songs like this…",
        0 => "Nothing after this",
        1 => "1 track",
        var count => $"{count} tracks",
    } + (_station is null ? "" : " · Radio");

    private bool _syncingUpNext;

    /// <summary>Makes <see cref="UpNext"/> match the queue, touching it only where it differs.</summary>
    private void SyncUpNext()
    {
        var wanted = QueueLayout.UpNext(Queue, NowPlayingEntry);
        if (wanted.SequenceEqual(UpNext))
        {
            return;
        }

        _syncingUpNext = true;
        try
        {
            UpNext.Clear();
            foreach (var entry in wanted)
            {
                UpNext.Add(entry);
            }
        }
        finally
        {
            _syncingUpNext = false;
        }
    }

    /// <summary>
    /// A drag in Up Next arrives as a removal then an insertion. The insertion completes the move,
    /// and is when the queue takes the new order.
    /// </summary>
    private void OnUpNextChanged(object? sender, System.Collections.Specialized.NotifyCollectionChangedEventArgs e)
    {
        if (_syncingUpNext || e.Action != System.Collections.Specialized.NotifyCollectionChangedAction.Add)
        {
            return;
        }

        ApplyUpNextOrder();
    }

    /// <summary>Gives the queue Up Next's order, or resynchronises Up Next if the two disagree.</summary>
    private bool ApplyUpNextOrder()
    {
        var reordered = QueueLayout.Reordered(Queue, NowPlayingEntry, UpNext);
        if (reordered is null)
        {
            SyncUpNext();
            return false;
        }

        _syncingUpNext = true;
        try
        {
            for (var target = 0; target < reordered.Count; target++)
            {
                var from = Queue.IndexOf(reordered[target]);
                if (from != target)
                {
                    Queue.Move(from, target);
                }
            }
        }
        finally
        {
            _syncingUpNext = false;
        }

        QueueChanged();
        return true;
    }

    /// <summary>Moves an Up Next entry by <paramref name="delta"/> places, for keyboard reordering.</summary>
    internal bool MoveUpNext(TrackViewModel entry, int delta)
    {
        var index = UpNext.IndexOf(entry);
        if (QueueLayout.MoveTarget(index, UpNext.Count, delta) is not { } target)
        {
            return false;
        }

        _syncingUpNext = true;
        try
        {
            UpNext.Move(index, target);
        }
        finally
        {
            _syncingUpNext = false;
        }

        return ApplyUpNextOrder();
    }

    /// <summary>
    /// Replaces the queue with <paramref name="tracks"/> and returns the entry to play.
    /// </summary>
    /// <param name="start">The row chosen, or null to start at the top.</param>
    /// <param name="shuffle">Turns shuffle on for this queue, starting anywhere.</param>
    internal TrackViewModel? StartQueue(IEnumerable<TrackViewModel> tracks, TrackViewModel? start, bool shuffle = false)
    {
        if (!CanChangeTrack())
        {
            return null;
        }

        var playable = tracks.Where(track => !string.IsNullOrEmpty(track.VideoId) && !IsRefused(track.VideoId)).ToList();
        if (start is not null && !playable.Contains(start) && !string.IsNullOrEmpty(start.VideoId))
        {
            playable.Insert(0, start);
        }

        if (playable.Count == 0)
        {
            ReportStatus("Nothing here can be played.");
            return null;
        }

        _station = null;
        _stationRevision++;
        _unshuffled = null;
        Queue.Clear();
        TrackViewModel? first = null;
        foreach (var track in playable)
        {
            var entry = track.Clone();
            Queue.Add(entry);
            if (ReferenceEquals(track, start))
            {
                first = entry;
            }
        }

        if (shuffle)
        {
            first ??= Queue[Shuffler.Next(Queue.Count)];
            IsShuffled = true;
        }

        first ??= Queue[0];
        if (IsShuffled)
        {
            // The chosen song leads and the rest are shuffled after it: a song placed before it
            // would be one Previous reaches without it ever having played.
            var order = PlaybackOrder.ShuffledStartingWith(Queue.ToList(), first, Shuffler);
            _unshuffled = Queue.Where(entry => !ReferenceEquals(entry, first)).ToList();
            Queue.Clear();
            foreach (var entry in order)
            {
                Queue.Add(entry);
            }
        }

        Point(first);

        return first;
    }

    /// <summary>
    /// Plays a row chosen on the page: an album or playlist in its order, anything else as a
    /// station that starts with the row.
    /// </summary>
    internal TrackViewModel? PlayFromPage(TrackViewModel row) =>
        PlaybackOrder.LaunchFor(PageKind) == LaunchKind.Ordered ? StartQueue(Tracks, row) : StartStation(row);

    /// <summary>
    /// Plays a shelf card as a station. A shelf is a set of suggestions, not an order anyone chose;
    /// queuing its neighbours is what turned "Listen again" into the queue.
    /// </summary>
    internal TrackViewModel? PlayFromShelf(CardViewModel card) => StartStation(new TrackViewModel(card));

    /// <summary>Moves to an entry already in the queue.</summary>
    internal TrackViewModel? JumpTo(TrackViewModel entry) =>
        Queue.Contains(entry) && CanChangeTrack() ? Point(entry) : null;

    internal bool IsQueueEntry(TrackViewModel track) => Queue.Contains(track);

    /// <summary>Puts a track straight after the current one, or at the end.</summary>
    internal void Enqueue(TrackViewModel track, bool next)
    {
        if (string.IsNullOrEmpty(track.VideoId))
        {
            ReportStatus("That row does not carry a playable track.");
            return;
        }

        var entry = track.Clone();
        if (next && _current is not null)
        {
            Queue.Insert(Queue.IndexOf(_current) + 1, entry);
        }
        else
        {
            Queue.Add(entry);
        }

        // Kept in the same place in the unshuffled order, so turning shuffle off leaves it next.
        if (next)
        {
            _unshuffled?.Insert(0, entry);
        }
        else
        {
            _unshuffled?.Add(entry);
        }

        QueueChanged();
        ReportStatus(next ? $"“{track.Title}” plays next." : $"“{track.Title}” was added to the queue.");
    }

    internal void Enqueue(CardViewModel card, bool next) => Enqueue(new TrackViewModel(card), next);

    /// <summary>Takes an entry out of the queue. The one playing stays until something replaces it.</summary>
    internal void RemoveFromQueue(TrackViewModel entry)
    {
        if (ReferenceEquals(entry, _current))
        {
            ReportStatus("The track that is playing stays in the queue.");
            return;
        }

        Queue.Remove(entry);
        _unshuffled?.Remove(entry);
        QueueChanged();
    }

    private ClearedQueue<TrackViewModel>? _lastCleared;
    private (List<TrackViewModel>? Unshuffled, RadioStation? Station) _clearedState;

    /// <summary>Clears everything after the track that is playing, keeping it for Undo.</summary>
    /// <remarks>Tracks already played stay, so Previous still reaches them.</remarks>
    internal bool ClearUpcoming()
    {
        var removed = QueueLayout.UpNext(Queue, NowPlayingEntry);
        if (removed.Count == 0)
        {
            return false;
        }

        _lastCleared = new ClearedQueue<TrackViewModel>(NowPlayingEntry, removed, DateTimeOffset.Now);
        _clearedState = (_unshuffled?.ToList(), _station);
        foreach (var entry in removed)
        {
            Queue.Remove(entry);
        }

        _unshuffled = IsShuffled ? _unshuffled?.Where(Queue.Contains).ToList() ?? [] : null;
        _station = null;
        _stationRevision++;
        QueueChanged();
        return true;
    }

    internal bool CanUndoClear => _lastCleared?.CanUndo(NowPlayingEntry, DateTimeOffset.Now) == true;

    /// <summary>Puts back what the last Clear removed, after the track that is playing.</summary>
    internal bool UndoClear()
    {
        if (_lastCleared is not { } cleared || !cleared.CanUndo(NowPlayingEntry, DateTimeOffset.Now))
        {
            return false;
        }

        foreach (var entry in cleared.ToRestore(Queue))
        {
            Queue.Add(entry);
        }

        (_unshuffled, _station) = _clearedState;
        _lastCleared = null;
        QueueChanged();
        return true;
    }

    internal void ToggleShuffle()
    {
        if (!HasPlayback)
        {
            return;
        }

        if (IsShuffled)
        {
            // Put the upcoming entries back in the order they were queued.
            if (_unshuffled is not null && _current is not null)
            {
                var index = Queue.IndexOf(_current);
                var upcoming = _unshuffled.Where(Queue.Contains).ToList();
                foreach (var entry in upcoming)
                {
                    Queue.Remove(entry);
                }

                index = Queue.IndexOf(_current);
                foreach (var entry in upcoming)
                {
                    Queue.Insert(++index, entry);
                }
            }

            _unshuffled = null;
            IsShuffled = false;
            SaveQueueModes();
            return;
        }

        IsShuffled = true;
        ShuffleUpcoming();
        SaveQueueModes();
    }

    /// <summary>Shuffles only what is after the current entry; what already played stays put.</summary>
    private void ShuffleUpcoming()
    {
        var index = _current is null ? -1 : Queue.IndexOf(_current);
        var upcoming = Queue.Skip(index + 1).ToList();
        _unshuffled = [.. upcoming];
        foreach (var entry in upcoming)
        {
            Queue.Remove(entry);
        }

        foreach (var entry in upcoming.OrderBy(_ => Shuffler.Next()))
        {
            Queue.Add(entry);
        }
    }

    internal void CycleRepeat()
    {
        if (!HasPlayback)
        {
            return;
        }

        Repeat = Repeat switch
        {
            RepeatMode.Off => RepeatMode.All,
            RepeatMode.All => RepeatMode.One,
            _ => RepeatMode.Off,
        };
        SaveQueueModes();
    }

    /// <summary>What a move decided: the entry to play, and whether it is the same one again.</summary>
    internal readonly record struct MoveResult(TrackViewModel? Entry, bool Restart);

    private static QueueRepeat ToQueueRepeat(RepeatMode mode) => mode switch
    {
        RepeatMode.All => QueueRepeat.All,
        RepeatMode.One => QueueRepeat.One,
        _ => QueueRepeat.Off,
    };

    /// <summary>
    /// Moves for Next, Previous or a natural end, continuing with recommendations when the queue
    /// runs out going forward.
    /// </summary>
    /// <param name="natural">The track ended by itself, rather than being skipped.</param>
    internal async Task<MoveResult> MoveAsync(bool forward, bool natural)
    {
        if (_current is not { } current || Queue.Count == 0)
        {
            ReportStatus("Choose a track to begin.");
            return default;
        }

        // A natural end is the page's doing and never happens inside an advertisement.
        if (!natural && !CanChangeTrack())
        {
            return default;
        }

        var decision = PlaybackOrder.Move(Queue.Count, Queue.IndexOf(current), forward, natural, ToQueueRepeat(Repeat));
        switch (decision.Kind)
        {
            case MoveKind.Play:
                return new(Point(Queue[decision.Index]), false);
            case MoveKind.Restart:
                return new(Point(current), true);
        }

        // Out of songs going forward. A running station continues when skipped into, and on a
        // natural end when autoplay is on; without one, autoplay starts one from this song.
        var mayContinue = _station is not null ? Autoplay || !natural : Autoplay;
        if (!mayContinue || current.VideoId is not { Length: > 0 } seed)
        {
            ReportStatus("The queue has finished.");
            return default;
        }

        if (_station is null || !_station.UsableWith(_activeAccount?.Id))
        {
            _station = new RadioStation(seed, AccountForRadio());
        }

        var revision = _stationRevision;
        var before = Queue.Count;
        await ExtendStationAsync().ConfigureAwait(true);
        if (revision != _stationRevision || !ReferenceEquals(current, _current))
        {
            // Something else was chosen while the recommendations loaded; that choice stands.
            return default;
        }

        if (Queue.Count > before)
        {
            return new(Point(Queue[before]), false);
        }

        ReportStatus("The queue has finished.");
        return default;
    }

    /// <summary>
    /// Plays <paramref name="seed"/> now and fills the queue after it with recommendations, as
    /// YouTube Music's own radio does. The song does not wait for them.
    /// </summary>
    internal TrackViewModel? StartStation(TrackViewModel seed)
    {
        if (string.IsNullOrEmpty(seed.VideoId))
        {
            ReportStatus("A radio needs a track to start from.");
            return null;
        }

        var first = StartQueue([seed], seed);
        if (first is null)
        {
            return null;
        }

        _station = new RadioStation(seed.VideoId, AccountForRadio());
        QueueChanged();
        _ = ExtendStationAsync();
        return first;
    }

    internal TrackViewModel? StartStation(CardViewModel card) => StartStation(new TrackViewModel(card));

    /// <summary>
    /// The account whose YouTube Music recommendations the radio uses: the signed-in one, whose
    /// "Up next" is personal, or none for the anonymous catalog's.
    /// </summary>
    private string? AccountForRadio() => IsSignedIn && Personal is not null ? _activeAccount?.Id : null;

    /// <summary>Keeps a few recommendations ahead of the song playing, so Next never waits.</summary>
    private void TopUpStation()
    {
        if (_station is { CanLoadMore: true } && _current is not null && _stationLoad is null
            && Queue.Count - Queue.IndexOf(_current) - 1 < 3)
        {
            _ = ExtendStationAsync();
        }
    }

    /// <summary>Appends the station's next page, or joins the one already on its way.</summary>
    /// <returns>How many songs were added.</returns>
    private async Task<int> ExtendStationAsync()
    {
        if (_stationLoad is { } running)
        {
            return await running.ConfigureAwait(true);
        }

        if (_station is not { CanLoadMore: true } station)
        {
            return 0;
        }

        var load = LoadStationPageAsync(station, _stationRevision);
        _stationLoad = load;
        QueueChanged();
        try
        {
            return await load.ConfigureAwait(true);
        }
        finally
        {
            if (ReferenceEquals(_stationLoad, load))
            {
                _stationLoad = null;
                QueueChanged();
            }
        }
    }

    private async Task<int> LoadStationPageAsync(RadioStation station, int revision)
    {
        CatalogPage? page;
        try
        {
            page = await FetchRadioAsync(station).ConfigureAwait(true);
        }
        catch (Exception error) when (station.IsPersonal && !station.Loaded)
        {
            // The account's reader could not answer; the anonymous catalog still can.
            BridgeLog.Write($"personal radio failed, using the catalog's: {error.GetType().Name}");
            if (revision != _stationRevision)
            {
                return 0;
            }

            station = new RadioStation(station.Seed, AccountId: null);
            _station = station;
            try
            {
                page = await FetchRadioAsync(station).ConfigureAwait(true);
            }
            catch (Exception fallback)
            {
                ReportRadioFailure(fallback, revision);
                return 0;
            }
        }
        catch (Exception error)
        {
            ReportRadioFailure(error, revision);
            return 0;
        }

        if (revision != _stationRevision || !station.UsableWith(_activeAccount?.Id) || page is null)
        {
            return 0;
        }

        var fresh = PlaybackOrder.FreshRecommendations(page.Tracks, item => item.VideoId,
            Queue.Select(entry => entry.VideoId).Concat(_recentlyPlayed).Concat(_refusedVideos));
        foreach (var item in fresh)
        {
            var entry = new TrackViewModel(item);
            Queue.Add(entry);
            _unshuffled?.Add(entry);
        }

        _station = station.After(page.NextCursor);
        QueueChanged();
        LoadQueueArtwork();
        BridgeLog.Write($"radio added {fresh.Count} of {page.Tracks.Count} ({(station.IsPersonal ? "account" : "catalog")})");
        return fresh.Count;
    }

    private void ReportRadioFailure(Exception error, int revision)
    {
        if (revision == _stationRevision)
        {
            ReportStatus("Could not load recommendations: " + Describe(error));
        }
    }

    /// <summary>
    /// A page of recommendations: the account's own "Up next" when signed in, the anonymous
    /// catalog's otherwise. The account's is read inside its profile, where its cookies live.
    /// </summary>
    private async Task<CatalogPage?> FetchRadioAsync(RadioStation station)
    {
        if (station.IsPersonal)
        {
            if (Personal is null)
            {
                throw new InvalidOperationException("The account's reader is not available.");
            }

            return await Personal.RadioAsync(station.Seed, station.Continuation).ConfigureAwait(true);
        }

        var payload = new JsonObject { ["catalogId"] = station.Seed };
        if (station.Continuation is { } continuation)
        {
            payload["continuation"] = continuation;
        }

        var answer = await _client.RequestAsync("catalog.radio", payload).ConfigureAwait(true);
        return answer.Deserialize<CatalogResponsePayload>(ServiceProtocol.Json)?.Page;
    }

    /// <summary>Fetches an album, playlist or artist and queues its tracks.</summary>
    internal async Task<TrackViewModel?> PlayEntityAsync(string kind, string id, bool shuffle)
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
            ReportStatus("That item cannot be played.");
            return null;
        }

        try
        {
            var answer = await _client.RequestAsync(command, new JsonObject { ["catalogId"] = id })
                .ConfigureAwait(true);
            var page = answer.Deserialize<CatalogResponsePayload>(ServiceProtocol.Json)?.Page;
            var rows = (page?.Tracks ?? []).Select(item => new TrackViewModel(item)).ToList();
            if (rows.Count == 0 && page is not null)
            {
                // An artist page has no ordered list; its songs shelf is the closest real one.
                rows = page.Shelves
                    .SelectMany(shelf => shelf.Items)
                    .Where(item => !string.IsNullOrEmpty(item.VideoId))
                    .Select(item => new TrackViewModel(item))
                    .ToList();
            }

            IsShuffled = false;
            var first = StartQueue(rows, null, shuffle);
            LoadQueueArtwork();
            return first;
        }
        catch (Exception error)
        {
            ReportStatus("Could not play that: " + Describe(error));
            return null;
        }
    }

    /// <summary>Plays the page on screen from the top, or shuffled.</summary>
    internal TrackViewModel? PlayPage(bool shuffle)
    {
        IsShuffled = false;
        return StartQueue(Tracks, null, shuffle);
    }

    private void LoadQueueArtwork()
    {
        foreach (var entry in Queue.Where(entry => entry.Artwork is null))
        {
            _ = entry.LoadArtworkAsync(_artwork);
        }
    }

    /// <summary>The cover of <paramref name="track"/> on disk, for the system media overlay.</summary>
    internal Task<string?> ArtworkFileAsync(TrackViewModel track) => _artwork.LocalFileAsync(track.Thumbnail);

    /// <summary>Appends the next part of the page on screen.</summary>
    internal async Task LoadMoreAsync()
    {
        if (_loadingMore || string.IsNullOrEmpty(_nextCursor))
        {
            return;
        }

        _loadingMore = true;
        var cursor = _nextCursor;
        try
        {
            if (await TryLoadMorePersonalAsync(cursor!).ConfigureAwait(true))
            {
                return;
            }

            var answer = await _client
                .RequestAsync("catalog.continue", new JsonObject { ["continuation"] = cursor })
                .ConfigureAwait(true);
            var page = answer.Deserialize<CatalogResponsePayload>(ServiceProtocol.Json)?.Page;
            if (page is null || _nextCursor != cursor)
            {
                // Another page was opened while this was loading; its rows do not belong there.
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

            NextCursor = page.NextCursor;
            OnPropertyChanged(nameof(HasTracks));
            if (page.Truncated)
            {
                ReportStatus("This page was long, so only the first part is shown.");
            }
        }
        catch (Exception error)
        {
            ReportStatus("Could not load more: " + Describe(error));
        }
        finally
        {
            _loadingMore = false;
        }
    }

    /// <summary>The public YouTube Music address of a track.</summary>
    internal static string LinkFor(TrackViewModel track) =>
        "https://music.youtube.com/watch?v=" + Uri.EscapeDataString(track.VideoId ?? "");

    /// <summary>The public YouTube Music address of a card.</summary>
    internal static string LinkFor(CardViewModel card) => card.Kind switch
    {
        _ when !string.IsNullOrEmpty(card.VideoId) =>
            "https://music.youtube.com/watch?v=" + Uri.EscapeDataString(card.VideoId),
        "playlist" => "https://music.youtube.com/playlist?list="
            + Uri.EscapeDataString(card.Id.StartsWith("VL", StringComparison.Ordinal) ? card.Id[2..] : card.Id),
        "artist" => "https://music.youtube.com/channel/" + Uri.EscapeDataString(card.Id),
        _ => "https://music.youtube.com/browse/" + Uri.EscapeDataString(card.Id),
    };

    private void AnnounceConfirmed(TrackViewModel track)
    {
        OnPropertyChanged(nameof(IsNowPlayingLiked));
        OnPropertyChanged(nameof(IsNowPlayingDisliked));
        NowPlayingChanged?.Invoke(track);
    }
}
