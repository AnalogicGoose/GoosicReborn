using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Threading.Tasks;
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

    /// <summary>The seed of a running radio, and the cursor for more of it.</summary>
    private string? _radioSeed;
    private string? _radioCursor;

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

    public bool HasTracks => Tracks.Count > 0;

    public bool HasQueue => Queue.Count > 0;

    public string QueueSummary => Queue.Count switch
    {
        0 => "Nothing queued",
        1 => "1 track",
        var count => $"{count} tracks",
    } + (_radioSeed is null ? "" : " · Radio");

    /// <summary>The queue entry the page is confirming, if any.</summary>
    internal TrackViewModel? ConfirmedTrack => _confirmedTrack;

    /// <summary>Makes <paramref name="entry"/> the one the queue is on, and the one to confirm.</summary>
    private TrackViewModel Point(TrackViewModel entry)
    {
        if (_current is not null)
        {
            _current.IsCurrent = false;
        }

        _current = entry;
        entry.IsCurrent = true;
        _pendingTrack = entry;
        _advancedAfterEndVideoId = null;
        QueueChanged();
        return entry;
    }

    private void QueueChanged()
    {
        OnPropertyChanged(nameof(HasQueue));
        OnPropertyChanged(nameof(QueueSummary));
    }

    /// <summary>
    /// Replaces the queue with <paramref name="tracks"/> and returns the entry to play.
    /// </summary>
    /// <param name="start">The row chosen, or null to start at the top.</param>
    /// <param name="shuffle">Turns shuffle on for this queue, starting anywhere.</param>
    internal TrackViewModel? StartQueue(IEnumerable<TrackViewModel> tracks, TrackViewModel? start, bool shuffle = false)
    {
        var playable = tracks.Where(track => !string.IsNullOrEmpty(track.VideoId)).ToList();
        if (start is not null && !playable.Contains(start) && !string.IsNullOrEmpty(start.VideoId))
        {
            playable.Insert(0, start);
        }

        if (playable.Count == 0)
        {
            ReportStatus("Nothing here can be played.");
            return null;
        }

        _radioSeed = null;
        _radioCursor = null;
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
        Point(first);
        if (IsShuffled)
        {
            ShuffleUpcoming();
        }

        return first;
    }

    /// <summary>Starts a queue of the rows on the page, from the one chosen.</summary>
    internal TrackViewModel? PlayFromPage(TrackViewModel row) => StartQueue(Tracks, row);

    /// <summary>Starts a queue from the playable cards on the chosen card's shelf.</summary>
    internal TrackViewModel? PlayFromShelf(CardViewModel card)
    {
        var rows = card.Context
            .Where(candidate => !string.IsNullOrEmpty(candidate.VideoId))
            .Select(candidate => new TrackViewModel(candidate))
            .ToList();
        var start = rows.FirstOrDefault(row => row.VideoId == card.VideoId) ?? new TrackViewModel(card);
        return StartQueue(rows, start);
    }

    /// <summary>Moves to an entry already in the queue.</summary>
    internal TrackViewModel? JumpTo(TrackViewModel entry) =>
        Queue.Contains(entry) ? Point(entry) : null;

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

        _unshuffled?.Add(entry);
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

    /// <summary>Clears everything but the track that is playing.</summary>
    internal void ClearUpcoming()
    {
        foreach (var entry in Queue.Where(entry => !ReferenceEquals(entry, _current)).ToList())
        {
            Queue.Remove(entry);
        }

        _unshuffled = IsShuffled ? [] : null;
        _radioSeed = null;
        _radioCursor = null;
        QueueChanged();
    }

    internal void ToggleShuffle()
    {
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
        Repeat = Repeat switch
        {
            RepeatMode.Off => RepeatMode.All,
            RepeatMode.All => RepeatMode.One,
            _ => RepeatMode.Off,
        };
        SaveQueueModes();
    }

    /// <summary>
    /// Keeps the music going after the queue ends, with a radio from the track that just finished.
    /// </summary>
    /// <returns>The first new entry, or null when autoplay is off or nothing came back.</returns>
    internal async Task<TrackViewModel?> AutoplayAfterAsync()
    {
        if (!Autoplay || _current is not { } last)
        {
            return null;
        }

        var seed = await StartRadioAsync(last).ConfigureAwait(true);
        // The radio starts with its seed, which has just played; move on to what follows it.
        return seed is null ? null : Advance(forward: true, natural: false);
    }

    /// <summary>
    /// Chooses the entry to play after a command or a natural end.
    /// </summary>
    /// <param name="forward">Next rather than previous.</param>
    /// <param name="natural">The track ended by itself, so repeat-one applies.</param>
    /// <returns>The entry, or null when the queue has finished.</returns>
    internal TrackViewModel? Advance(bool forward, bool natural)
    {
        if (Queue.Count == 0 || _current is null)
        {
            ReportStatus("Choose a track to begin.");
            return null;
        }

        if (natural && Repeat == RepeatMode.One)
        {
            return Point(_current);
        }

        var index = Queue.IndexOf(_current) + (forward ? 1 : -1);
        if (index >= 0 && index < Queue.Count)
        {
            return Point(Queue[index]);
        }

        // A deliberate Previous at the top, or Next at the bottom, wraps when repeat says so; a
        // natural end at the bottom without repeat finishes instead of starting over.
        if (Repeat == RepeatMode.All || (!natural && _radioSeed is null))
        {
            return Point(Queue[(index + Queue.Count) % Queue.Count]);
        }

        return null;
    }

    /// <summary>Whether running out of queue should fetch more of a radio first.</summary>
    internal bool CanExtendRadio => _radioSeed is not null && _current is not null
        && Queue.IndexOf(_current) == Queue.Count - 1;

    /// <summary>
    /// Replaces the queue with a radio seeded from <paramref name="seed"/>.
    /// </summary>
    /// <returns>The seed's own entry, which is played first.</returns>
    internal async Task<TrackViewModel?> StartRadioAsync(TrackViewModel seed)
    {
        if (string.IsNullOrEmpty(seed.VideoId))
        {
            ReportStatus("A radio needs a track to start from.");
            return null;
        }

        ReportStatus($"Starting a radio from “{seed.Title}”…");
        try
        {
            var answer = await _client
                .RequestAsync("catalog.radio", new JsonObject { ["catalogId"] = seed.VideoId })
                .ConfigureAwait(true);
            var page = answer.Deserialize<CatalogResponsePayload>(ServiceProtocol.Json)?.Page;
            var rows = (page?.Tracks ?? [])
                .Where(item => !string.IsNullOrEmpty(item.VideoId) && item.VideoId != seed.VideoId)
                .Select(item => new TrackViewModel(item))
                .ToList();
            rows.Insert(0, seed);
            var first = StartQueue(rows, seed);
            _radioSeed = seed.VideoId;
            _radioCursor = page?.NextCursor;
            QueueChanged();
            LoadQueueArtwork();
            ReportStatus(page?.Truncated == true ? "The radio was long, so only the first part is queued." : "");
            return first;
        }
        catch (Exception error)
        {
            ReportStatus("Could not start a radio: " + Describe(error));
            return null;
        }
    }

    internal Task<TrackViewModel?> StartRadioAsync(CardViewModel card) =>
        StartRadioAsync(new TrackViewModel(card));

    /// <summary>Appends the next part of the running radio.</summary>
    internal async Task<bool> ExtendRadioAsync()
    {
        if (_radioSeed is null || string.IsNullOrEmpty(_radioCursor))
        {
            return false;
        }

        try
        {
            var payload = new JsonObject { ["catalogId"] = _radioSeed, ["continuation"] = _radioCursor };
            var answer = await _client.RequestAsync("catalog.radio", payload).ConfigureAwait(true);
            var page = answer.Deserialize<CatalogResponsePayload>(ServiceProtocol.Json)?.Page;
            var known = Queue.Select(entry => entry.VideoId).ToHashSet();
            var added = 0;
            foreach (var item in page?.Tracks ?? [])
            {
                if (string.IsNullOrEmpty(item.VideoId) || !known.Add(item.VideoId))
                {
                    continue;
                }

                var entry = new TrackViewModel(item);
                Queue.Add(entry);
                _unshuffled?.Add(entry);
                added++;
            }

            _radioCursor = page?.NextCursor;
            QueueChanged();
            LoadQueueArtwork();
            return added > 0;
        }
        catch (Exception error)
        {
            ReportStatus("Could not continue the radio: " + Describe(error));
            return false;
        }
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
