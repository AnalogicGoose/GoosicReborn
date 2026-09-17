using System;
using System.Collections.Generic;
using System.Linq;

namespace Goosic.Windows.Presentation;

public enum QueueRepeat { Off, All, One }

/// <summary>What the queue should do when asked to move.</summary>
public enum MoveKind
{
    /// <summary>Play the entry at <see cref="MoveDecision.Index"/>.</summary>
    Play,
    /// <summary>Start the current entry again from the beginning.</summary>
    Restart,
    /// <summary>The queue has run out going forward; recommendations may continue it.</summary>
    NeedMore,
}

public readonly record struct MoveDecision(MoveKind Kind, int Index = -1);

/// <summary>How a chosen song starts playing.</summary>
public enum LaunchKind
{
    /// <summary>The song, then YouTube Music's recommendations after it.</summary>
    Station,
    /// <summary>The list the song was chosen from, in its order.</summary>
    Ordered,
}

/// <summary>
/// The rules for which song plays next, kept apart from the queue's controls so every case can
/// be tested. They follow the macOS shell: an ordered list is played in order, anything else
/// starts a station, and recommendations extend a queue rather than replace it.
/// </summary>
public static class PlaybackOrder
{
    /// <summary>
    /// Where Next, Previous or a natural end goes.
    /// </summary>
    /// <param name="index">The current entry's position, or -1 when it is no longer in the queue.</param>
    /// <param name="natural">The track ended by itself, which is the only time repeat-one applies.</param>
    public static MoveDecision Move(int count, int index, bool forward, bool natural, QueueRepeat repeat)
    {
        if (count == 0 || index < 0 || index >= count)
        {
            // A current entry the queue no longer holds has no neighbours; guessing one from a
            // stale position is how Previous used to land on an unrelated song.
            return new(forward ? MoveKind.NeedMore : MoveKind.Restart);
        }

        if (natural && repeat == QueueRepeat.One)
        {
            return new(MoveKind.Restart);
        }

        var target = index + (forward ? 1 : -1);
        if (target >= 0 && target < count)
        {
            return new(MoveKind.Play, target);
        }

        if (repeat == QueueRepeat.All)
        {
            return new(MoveKind.Play, forward ? 0 : count - 1);
        }

        // Previous on the first song starts it again, as every player does; it never wraps to
        // the end of the queue. Next on the last song asks for more instead of starting over.
        return new(forward ? MoveKind.NeedMore : MoveKind.Restart);
    }

    /// <summary>
    /// Recommendations worth adding: none already queued or recently played, and no repeats
    /// within the batch. A user's own queue may hold a song twice; a radio may not, because a
    /// repeated recommendation reads as a broken "up next".
    /// </summary>
    public static List<T> FreshRecommendations<T>(IEnumerable<T> candidates, Func<T, string?> videoId,
        IEnumerable<string?> exclude)
    {
        var seen = new HashSet<string>(exclude.Where(id => !string.IsNullOrEmpty(id))!, StringComparer.Ordinal);
        return candidates
            .Where(candidate => videoId(candidate) is { Length: > 0 } id && seen.Add(id))
            .ToList();
    }

    /// <summary>
    /// A shuffled queue that starts with <paramref name="first"/>: nothing is placed before the
    /// song that plays first, so Previous never reaches a song that was never heard.
    /// </summary>
    public static List<T> ShuffledStartingWith<T>(IReadOnlyList<T> items, T first, Random random) where T : class
    {
        var rest = items.Where(item => !ReferenceEquals(item, first)).OrderBy(_ => random.Next()).ToList();
        rest.Insert(0, first);
        return rest;
    }

    /// <summary>
    /// Whether choosing a song on a page plays that page in order or starts a station from it.
    /// Albums, playlists and liked songs are lists someone means to hear in order; a shelf card or
    /// a search result is a song chosen on its own.
    /// </summary>
    public static LaunchKind LaunchFor(DetailKind page) =>
        page is DetailKind.Album or DetailKind.Playlist ? LaunchKind.Ordered : LaunchKind.Station;

    /// <summary>
    /// Whether a track's natural end should be acted on: only for the entry being played, only
    /// once, and only after that entry has actually played — a late "ended" from the song before
    /// must not skip the one that replaced it.
    /// </summary>
    public static bool AcceptsEnd(string? endedVideoId, string? currentVideoId, bool armed) =>
        armed && !string.IsNullOrEmpty(endedVideoId) && endedVideoId == currentVideoId;
}

/// <summary>Where recommendations come from, so a continuation is only used with its issuer.</summary>
/// <param name="Seed">The video the station started from.</param>
/// <param name="AccountId">The account whose reader issued the cursor, or null for the anonymous catalog.</param>
/// <param name="Continuation">The cursor for the next page, once one page has loaded.</param>
/// <param name="Loaded">Whether the first page has loaded; after that, no cursor means no more.</param>
public sealed record RadioStation(string Seed, string? AccountId, string? Continuation = null, bool Loaded = false)
{
    public bool IsPersonal => AccountId is not null;

    public bool CanLoadMore => !Loaded || !string.IsNullOrEmpty(Continuation);

    /// <summary>
    /// Whether this station may still be used with the account now active. An account's cursor
    /// means nothing to another reader; the anonymous catalog's works for anyone.
    /// </summary>
    public bool UsableWith(string? activeAccountId) => !IsPersonal || AccountId == activeAccountId;

    /// <summary>The station after a page arrived with <paramref name="nextCursor"/>.</summary>
    /// <remarks>A cursor that repeats the one just used would loop forever, so it ends the station.</remarks>
    public RadioStation After(string? nextCursor) =>
        this with { Loaded = true, Continuation = nextCursor == Continuation ? null : nextCursor };
}
