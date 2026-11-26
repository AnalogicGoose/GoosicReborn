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
/// The queue as this window shows it: what plays next, shuffle, repeat, radio, and the pages
/// that continue.
/// </summary>
/// <remarks>
/// What the queue does is decided by <c>QueueEngine</c> in <c>goosic-shell-support</c>, the same
/// engine the Mac uses, through <see cref="SharedQueue"/>. This file sends it the listener's
/// commands, keeps the row behind each entry, fetches the radio pages it asks for, and draws the
/// snapshot it answers with. Whether a track may play is still Rust's answer, reached through the
/// playback host's claim; a queue that picked a track Rust refused shows the refusal rather than
/// pretending it moved on.
/// </remarks>
public sealed partial class ShellViewModel
{
    private readonly SharedQueue _engine = new();

    /// <summary>The row behind each engine entry. Entries are the engine's, so a song may be queued twice.</summary>
    private readonly Dictionary<ulong, TrackViewModel> _entries = new();
    private readonly Dictionary<TrackViewModel, ulong> _entryIds = new(ReferenceEqualityComparer.Instance);

    private QueueSnapshot? _snapshot;
    private TrackViewModel? _current;
    private bool _shuffle;
    private RepeatMode _repeat = RepeatMode.Off;
    private string? _nextCursor;
    private bool _loadingMore;
    private bool _radioAccountSent;
    private string? _radioAccount;

    /// <summary>The radio page on its way, resolving to the entry it started, if it started one.</summary>
    private Task<TrackViewModel?>? _radioLoad;

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
    public string RepeatGlyph => Repeat == RepeatMode.One ? "" : "";

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
                OnPropertyChanged(nameof(PageMeta));
            }
        }
    }

    public bool HasMore => !string.IsNullOrEmpty(_nextCursor);

    /// <summary>
    /// Whether the next part of the page lengthens a list that scrolls sideways, as a search's
    /// songs do, rather than making the page taller. Such a page has to ask for more from the end
    /// of that list: the page itself never grows, so its own end is always in view.
    /// </summary>
    internal bool MoreGoesSideways => PageKind == DetailKind.Search && Shelves.Count > 0 && Shelves[Shelves.Count - 1].IsList;

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
    } + (_snapshot?.Radio == true ? " · Radio" : "");

    /// <summary>The queue entry the page is confirming, if any.</summary>
    internal TrackViewModel? ConfirmedTrack => _confirmedTrack;

    /// <summary>Whether the sleep timer stops the music when the playing song ends.</summary>
    internal bool SleepsAtEndOfSong => _snapshot?.SleepAtEnd == true;

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
    public bool HasNoUpNext => HasQueue && !HasUpNext && _snapshot?.RadioLoading != true;

    /// <summary>Where the queue came from and how much is left, as "From Liked Music · 99 songs".</summary>
    /// <remarks>Spotify and Apple Music both name the source; a bare count says nothing about it.</remarks>
    public string UpNextSummary => _snapshot?.UpNextText ?? "";

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

        var order = new JsonArray();
        foreach (var entry in reordered)
        {
            if (_entryIds.TryGetValue(entry, out var id))
            {
                order.Add(id);
            }
        }

        Dispatch(Command("reorder", ("entries", order)));
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

    private string? _queueRoute;
    private string _queueSource = "";

    /// <summary>What the queue was started from, for "Playing from" above it.</summary>
    public string QueueSource => _queueSource;

    public bool HasQueueSource => _queueSource.Length > 0;

    /// <summary>Whether the queue is this page's list, so its Play button controls it instead.</summary>
    public bool IsPageQueued => _queueRoute is not null && _queueRoute == _currentRoute;

    /// <summary>A page whose list is already playing offers Pause, as Apple Music and Spotify do.</summary>
    public string PagePlayLabel => IsPageQueued && IsPlaying ? "Pause" : "Play";

    public string PagePlayGlyph => IsPageQueued && IsPlaying ? "" : "";

    private void PagePlaybackChanged()
    {
        OnPropertyChanged(nameof(IsPageQueued));
        OnPropertyChanged(nameof(PagePlayLabel));
        OnPropertyChanged(nameof(PagePlayGlyph));
    }

    // ---- Talking to the engine -------------------------------------------------------------

    /// <summary>What one command asked this window to do.</summary>
    /// <param name="Played">The entry to load and play, when the engine moved to one.</param>
    /// <param name="SeekToStart">Start the playing song again instead of loading anything.</param>
    /// <param name="StopForSleep">The sleep timer's end of this song was reached: play nothing else.</param>
    internal readonly record struct MoveResult(TrackViewModel? Played, bool SeekToStart, bool StopForSleep);

    private static JsonObject Command(string type, params (string Name, JsonNode? Value)[] fields)
    {
        var command = new JsonObject { ["type"] = type };
        foreach (var (name, value) in fields)
        {
            command[name] = value;
        }

        return command;
    }

    private static JsonObject TrackJson(TrackViewModel track) => SharedQueue.Track(track.VideoId ?? "", track.Title);

    private static RepeatMode ParseRepeat(string repeat) => repeat switch
    {
        "all" => RepeatMode.All,
        "one" => RepeatMode.One,
        _ => RepeatMode.Off,
    };

    private static string RepeatName(RepeatMode repeat) => repeat switch
    {
        RepeatMode.All => "all",
        RepeatMode.One => "one",
        _ => "off",
    };

    /// <summary>Seconds on a clock that never jumps, for the Undo window after Clear.</summary>
    private static double QueueClock() => Environment.TickCount64 / 1000.0;

    /// <summary>
    /// Sends one command, keeps the rows it carried for the entries the engine made of them, draws
    /// the answer, and carries out what it asks.
    /// </summary>
    /// <param name="carried">The rows the command's tracks came from, in the same order.</param>
    private MoveResult Dispatch(JsonObject command, IReadOnlyList<TrackViewModel>? carried = null)
    {
        SyncRadioAccount();
        if (Send(command) is not { } outcome)
        {
            return default;
        }

        foreach (var added in outcome.Added)
        {
            if (carried is not null && added.Input >= 0 && added.Input < carried.Count)
            {
                var entry = carried[added.Input].Clone();
                _entries[added.Entry] = entry;
                _entryIds[entry] = added.Entry;
            }
        }

        var previous = _current;
        ApplySnapshot(outcome.Snapshot);

        var result = default(MoveResult);
        foreach (var effect in outcome.Effects)
        {
            switch (effect)
            {
                case PlayEffect play when _entries.TryGetValue(play.Entry, out var entry):
                    Begin(entry, changed: !ReferenceEquals(entry, previous));
                    previous = entry;
                    result = result with { Played = entry };
                    break;
                case SeekToStartEffect:
                    result = result with { SeekToStart = true };
                    break;
                case StopForSleepEffect:
                    result = result with { StopForSleep = true };
                    break;
                case FetchRadioEffect fetch:
                    _radioLoad = LoadRadioAsync(fetch);
                    break;
                case SavePreferencesEffect:
                    SaveQueueModes();
                    break;
                case NoticeEffect notice:
                    ReportStatus(notice.Text);
                    break;
            }
        }

        return result;
    }

    /// <summary>Sends a command, or says the queue could not be changed when the engine is missing.</summary>
    private QueueOutcome? Send(JsonObject command)
    {
        try
        {
            return _engine.Handle(command);
        }
        catch (Exception error) when (error is InvalidOperationException or DllNotFoundException or EntryPointNotFoundException)
        {
            BridgeLog.Write("queue engine: " + error.Message);
            ReportDetail("The queue could not be changed.");
            return null;
        }
    }

    /// <summary>Tells the engine whose recommendations a new radio uses, when that changed.</summary>
    private void SyncRadioAccount()
    {
        var account = AccountForRadio();
        if (_radioAccountSent && account == _radioAccount)
        {
            return;
        }

        if (Send(Command("setRadioAccount", ("accountId", account))) is not null)
        {
            _radioAccountSent = true;
            _radioAccount = account;
        }
    }

    /// <summary>Restores the saved queue preferences; restoring them is not a change to save.</summary>
    private void ConfigureQueue(bool shuffle, RepeatMode repeat, bool autoplay)
    {
        Dispatch(Command("configure", ("shuffle", shuffle), ("repeat", RepeatName(repeat)), ("autoplay", autoplay)));
    }

    /// <summary>Redraws the queue from the engine's snapshot, touching the list only where it differs.</summary>
    private void ApplySnapshot(QueueSnapshot snapshot)
    {
        _snapshot = snapshot;
        if (snapshot.UndoDeadline is null)
        {
            // While Undo is offered the cleared rows are kept, so putting them back restores them
            // as they were; afterwards an entry the engine no longer has is forgotten.
            var live = snapshot.Entries.ToHashSet();
            foreach (var gone in _entries.Keys.Where(id => !live.Contains(id)).ToList())
            {
                _entryIds.Remove(_entries[gone]);
                _entries.Remove(gone);
            }
        }

        var wanted = snapshot.Entries
            .Select(id => _entries.GetValueOrDefault(id))
            .OfType<TrackViewModel>()
            .ToList();
        _syncingUpNext = true;
        try
        {
            for (var target = 0; target < wanted.Count; target++)
            {
                if (target < Queue.Count && ReferenceEquals(Queue[target], wanted[target]))
                {
                    continue;
                }

                var from = Queue.IndexOf(wanted[target]);
                if (from >= 0)
                {
                    Queue.Move(from, target);
                }
                else
                {
                    Queue.Insert(target, wanted[target]);
                }
            }

            while (Queue.Count > wanted.Count)
            {
                Queue.RemoveAt(Queue.Count - 1);
            }
        }
        finally
        {
            _syncingUpNext = false;
        }

        var current = snapshot.Current is int index && index < snapshot.Entries.Count
            ? _entries.GetValueOrDefault(snapshot.Entries[index])
            : null;
        if (!ReferenceEquals(current, _current))
        {
            if (_current is not null)
            {
                _current.IsCurrent = false;
            }

            _current = current;
            if (current is not null)
            {
                current.IsCurrent = true;
            }
        }

        IsShuffled = snapshot.Shuffle;
        Repeat = ParseRepeat(snapshot.Repeat);
        var source = snapshot.SourceTitle ?? "";
        if (source != _queueSource || snapshot.SourceKey != _queueRoute)
        {
            _queueRoute = snapshot.SourceKey;
            _queueSource = source;
            OnPropertyChanged(nameof(QueueSource));
            OnPropertyChanged(nameof(HasQueueSource));
            PagePlaybackChanged();
        }

        QueueChanged();
    }

    /// <summary>Gets ready to confirm <paramref name="entry"/>, which the engine moved to.</summary>
    /// <param name="changed">It is a different entry from the one before, not the same one again.</param>
    private void Begin(TrackViewModel entry, bool changed)
    {
        if (changed)
        {
            // The last song's position would otherwise stand until the new one reports, and
            // Previous would read it as "past the first seconds" and restart the new song.
            PlaybackPosition = 0;
            PlaybackDuration = 0;
            _pendingSeek = null;
        }

        _pendingTrack = entry;
        OnPropertyChanged(nameof(HasPlayback));
        OnPropertyChanged(nameof(CanSkip));
        // A new play has to be heard playing before its end counts; see MarkNaturalEnd.
        PlaybackRules.EndGuardBegin(ref _endGuard);
        _listenerPaused = false;
        _pendingAlias = null;
    }

    // ---- Starting and moving ---------------------------------------------------------------

    /// <summary>Replaces the queue with <paramref name="rows"/> and returns the entry to play.</summary>
    /// <param name="start">The row chosen, or null to start at the top (or anywhere, shuffled).</param>
    /// <param name="kind">"ordered" to play the list, "station" to play the row and its recommendations.</param>
    /// <param name="shuffle">What a Play or Shuffle button asks for; null leaves the preference.</param>
    private TrackViewModel? Launch(
        IEnumerable<TrackViewModel> rows, TrackViewModel? start, string kind, string? sourceTitle, string? sourceKey,
        bool? shuffle = null)
    {
        if (!CanChangeTrack())
        {
            return null;
        }

        var list = rows.ToList();
        var index = start is null ? -1 : list.IndexOf(start);
        if (start is not null && index < 0)
        {
            list.Insert(0, start);
            index = 0;
        }

        var tracks = new JsonArray();
        foreach (var row in list)
        {
            tracks.Add(TrackJson(row));
        }

        var command = Command("launch",
            ("tracks", tracks),
            ("start", index < 0 ? null : (JsonNode)index),
            ("kind", kind),
            ("sourceTitle", sourceTitle),
            ("sourceKey", sourceKey),
            ("shuffle", shuffle));
        var played = Dispatch(command, list).Played;
        LoadQueueArtwork();
        return played;
    }

    /// <summary>
    /// Plays a row chosen on the page: an album or playlist in its order, anything else as a
    /// station that starts with the row. The rule is the engine's.
    /// </summary>
    internal TrackViewModel? PlayFromPage(TrackViewModel row) =>
        Launch(VisibleTracks, row, SharedQueue.LaunchKind(LaunchSurface), PageTitle, _currentRoute);

    /// <summary>Where a row on the page on screen was chosen, in the engine's words.</summary>
    private string LaunchSurface => PageKind switch
    {
        DetailKind.Album => "album",
        DetailKind.Playlist => "playlist",
        DetailKind.Artist => "artist",
        DetailKind.Search => "search",
        _ => "shelf",
    };

    /// <summary>
    /// Plays a shelf card as a station. A shelf is a set of suggestions, not an order anyone chose;
    /// queuing its neighbours is what turned "Listen again" into the queue.
    /// </summary>
    internal TrackViewModel? PlayFromShelf(CardViewModel card) => StartStation(new TrackViewModel(card));

    /// <summary>Moves to an entry already in the queue.</summary>
    internal TrackViewModel? JumpTo(TrackViewModel entry) =>
        CanChangeTrack() && _entryIds.TryGetValue(entry, out var id) && Queue.Contains(entry)
            ? Dispatch(Command("jump", ("entry", id))).Played
            : null;

    internal bool IsQueueEntry(TrackViewModel track) => Queue.Contains(track);

    /// <summary>Puts a track straight after the current one, or at the end.</summary>
    /// <returns>The entry to play, when nothing was playing and the track starts the queue.</returns>
    internal TrackViewModel? Enqueue(TrackViewModel track, bool next)
    {
        var played = Dispatch(Command(next ? "playNext" : "addToQueue", ("track", TrackJson(track))), [track]).Played;
        LoadQueueArtwork();
        return played;
    }

    internal TrackViewModel? Enqueue(CardViewModel card, bool next) => Enqueue(new TrackViewModel(card), next);

    /// <summary>Takes an entry out of the queue. The one playing stays until something replaces it.</summary>
    internal void RemoveFromQueue(TrackViewModel entry)
    {
        if (_entryIds.TryGetValue(entry, out var id))
        {
            Dispatch(Command("remove", ("entry", id)));
        }
    }

    /// <summary>Clears everything after the track that is playing, keeping it for Undo.</summary>
    /// <remarks>Tracks already played stay, so Previous still reaches them.</remarks>
    internal bool ClearUpcoming()
    {
        if (_snapshot?.CanClear != true)
        {
            return false;
        }

        Dispatch(Command("clearUpcoming", ("now", QueueClock())));
        return _snapshot?.UndoDeadline is not null;
    }

    /// <summary>How long Undo stays offered after a Clear.</summary>
    internal TimeSpan UndoTimeLeft =>
        _snapshot?.UndoDeadline is { } deadline ? TimeSpan.FromSeconds(Math.Max(0, deadline - QueueClock())) : TimeSpan.Zero;

    internal bool CanUndoClear => _snapshot?.UndoDeadline is { } deadline && deadline > QueueClock();

    /// <summary>Puts back what the last Clear removed, after the track that is playing.</summary>
    internal bool UndoClear()
    {
        if (!CanUndoClear)
        {
            return false;
        }

        var before = Queue.Count;
        Dispatch(Command("undoClear", ("now", QueueClock())));
        return Queue.Count > before;
    }

    internal void ToggleShuffle()
    {
        if (HasPlayback)
        {
            Dispatch(Command("toggleShuffle"));
        }
    }

    internal void CycleRepeat()
    {
        if (HasPlayback)
        {
            Dispatch(Command("cycleRepeat"));
        }
    }

    /// <summary>Asks the sleep timer to stop at the end of the playing song, or not to.</summary>
    internal void SetSleepAtEndOfSong(bool enabled)
    {
        Dispatch(Command("setSleepAtEnd", ("enabled", enabled)));
    }

    /// <summary>
    /// Next, Previous or a natural end. When the queue runs out going forward the engine may
    /// continue with recommendations, and this waits for them.
    /// </summary>
    /// <param name="natural">The track ended by itself, rather than being skipped.</param>
    internal async Task<MoveResult> MoveAsync(bool forward, bool natural)
    {
        // A natural end is the page's doing and never happens inside an advertisement.
        if (!natural && !CanChangeTrack())
        {
            return default;
        }

        var command = natural
            ? Command("ended")
            : forward
                ? Command("next")
                : Command("previous", ("position", IsSeekable ? PlaybackPosition : 0.0));
        var result = Dispatch(command);
        if (result.Played is not null || result.SeekToStart || result.StopForSleep)
        {
            return result;
        }

        // Nothing to play yet: the engine is waiting for a radio page and plays from it once it
        // arrives. A personal radio that fails is retried once from the catalog, which is a
        // second load to wait for.
        while (_snapshot?.RadioLoading == true && _radioLoad is { } load)
        {
            if (await load.ConfigureAwait(true) is { } played)
            {
                return new MoveResult(played, false, false);
            }

            if (ReferenceEquals(load, _radioLoad))
            {
                break;
            }
        }

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
            ReportDetail("A radio needs a track to start from.");
            return null;
        }

        if (!CanChangeTrack())
        {
            return null;
        }

        var played = Dispatch(Command("startRadio", ("track", TrackJson(seed))), [seed]).Played;
        LoadQueueArtwork();
        return played;
    }

    internal TrackViewModel? StartStation(CardViewModel card) => StartStation(new TrackViewModel(card));

    /// <summary>
    /// The account whose YouTube Music recommendations the radio uses: the signed-in one, whose
    /// "Up next" is personal, or none for the anonymous catalog's.
    /// </summary>
    private string? AccountForRadio() => IsSignedIn && Personal is not null ? _activeAccount?.Id : null;

    /// <summary>Fetches the radio page the engine asked for and gives it the answer.</summary>
    /// <returns>The entry the page started, when the engine was waiting on it to move on.</returns>
    private async Task<TrackViewModel?> LoadRadioAsync(FetchRadioEffect fetch)
    {
        // Answered on a later turn of the dispatcher, never inside the command that asked.
        await Task.Yield();
        CatalogPage? page;
        try
        {
            page = await FetchRadioAsync(fetch).ConfigureAwait(true);
        }
        catch (Exception error)
        {
            BridgeLog.Write($"radio failed ({(fetch.AccountId is null ? "catalog" : "account")}): {error.GetType().Name}: {error.Message}");
            return Dispatch(Command("radioFailed", ("request", fetch.Request))).Played;
        }

        var rows = Listed(page?.Tracks ?? []).Select(item => new TrackViewModel(item)).ToList();
        var tracks = new JsonArray();
        foreach (var row in rows)
        {
            tracks.Add(TrackJson(row));
        }

        var played = Dispatch(
            Command("radioLoaded", ("request", fetch.Request), ("tracks", tracks), ("nextCursor", page?.NextCursor)),
            rows).Played;
        LoadQueueArtwork();
        BridgeLog.Write($"radio answered {rows.Count} ({(fetch.AccountId is null ? "catalog" : "account")})");
        return played;
    }

    /// <summary>
    /// A page of recommendations: the account's own "Up next" when signed in, the anonymous
    /// catalog's otherwise. The account's is read inside its profile, where its cookies live.
    /// </summary>
    private async Task<CatalogPage?> FetchRadioAsync(FetchRadioEffect fetch)
    {
        if (fetch.AccountId is not null)
        {
            if (Personal is null || fetch.AccountId != _activeAccount?.Id)
            {
                throw new InvalidOperationException("The account's reader is not available.");
            }

            return await Personal.RadioAsync(fetch.SeedVideoId, fetch.Continuation).ConfigureAwait(true);
        }

        var payload = new JsonObject { ["catalogId"] = fetch.SeedVideoId };
        if (fetch.Continuation is { } continuation)
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
            ReportDetail("That item cannot be played.");
            return null;
        }

        try
        {
            var answer = await _client.RequestAsync(command, new JsonObject { ["catalogId"] = id })
                .ConfigureAwait(true);
            var page = answer.Deserialize<CatalogResponsePayload>(ServiceProtocol.Json)?.Page;
            var rows = Listed(page?.Tracks ?? []).Select(item => new TrackViewModel(item)).ToList();
            if (rows.Count == 0 && page is not null)
            {
                // An artist page has no ordered list; its songs shelf is the closest real one.
                rows = page.Shelves
                    .SelectMany(shelf => Listed(shelf.Items))
                    .Where(item => !string.IsNullOrEmpty(item.VideoId))
                    .Select(item => new TrackViewModel(item))
                    .ToList();
            }

            return Launch(rows, null, "ordered", page?.Title, null, shuffle);
        }
        catch (Exception error)
        {
            ReportStatus("Could not play that: " + Describe(error));
            return null;
        }
    }

    /// <summary>Plays the page on screen from the top, or shuffled.</summary>
    internal TrackViewModel? PlayPage(bool shuffle) => Launch(VisibleTracks, null, "ordered", PageTitle, _currentRoute, shuffle);

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
        OnPropertyChanged(nameof(IsLoadingMore));
        var cursor = _nextCursor;
        var contentRevision = _pageContentRevision;
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
            if (page is null || _nextCursor != cursor || contentRevision != _pageContentRevision)
            {
                // Another page was opened while this was loading; its rows do not belong there.
                return;
            }

            foreach (var track in Listed(page.Tracks))
            {
                var row = new TrackViewModel(track);
                Tracks.Add(row);
                _ = row.LoadArtworkAsync(_artwork);
            }

            foreach (var shelf in ArtistContentShelves(ListedShelves(page.Shelves)))
            {
                if (Shelves.Count > 0 && Shelves[Shelves.Count - 1] is var shown
                    && ShelfColumns.Continues(shown.Id, shown.Heading, shelf.Id, shelf.Title))
                {
                    var cards = ShelfColumns.Unseen(shown.Items.Select(card => card.Id), shelf.Items, item => item.Id)
                        .Select(item => new CardViewModel(item))
                        .ToList();
                    shown.Extend(cards);
                    foreach (var card in cards)
                    {
                        _ = card.LoadArtworkAsync(_artwork);
                    }

                    continue;
                }

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
                ReportDetail("This page was long, so only the first part is shown.");
            }
        }
        catch (Exception error)
        {
            // Loading more happens by itself as the page is scrolled, so a failure is not the
            // listener's to deal with. The feed ends where it is instead of reporting an error and
            // asking again on every scroll: YouTube Music now answers past the last page with an
            // empty frame, which reached here as a failure several times a second.
            BridgeLog.Write($"load more ended the feed: {error.GetType().Name}: {error.Message}");
            if (_nextCursor == cursor)
            {
                NextCursor = null;
            }
        }
        finally
        {
            _loadingMore = false;
            OnPropertyChanged(nameof(IsLoadingMore));
            // Rows that just arrived join a sorted list in their sorted place.
            if (_trackSort != "custom")
            {
                ReorderTracks();
            }
        }
    }

    /// <summary>Whether the next part of the page is on its way, for the spinner at the bottom.</summary>
    public bool IsLoadingMore => _loadingMore;

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
        NowPlayingRatingChanged();
        NowPlayingChanged?.Invoke(track);
    }
}
