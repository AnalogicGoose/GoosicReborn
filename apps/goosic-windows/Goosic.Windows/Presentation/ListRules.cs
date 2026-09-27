using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;

namespace Goosic.Windows.Presentation;

/// <summary>What a list row offers the sort and find rules: its words, its length, its arrival.</summary>
public interface ISortableTrack
{
    string Title { get; }
    string Subtitle { get; }
    string? Artist { get; }
    string? Album { get; }
    string Duration { get; }

    /// <summary>The order the row arrived in, which is the playlist's own order.</summary>
    long Ordinal { get; }
}

/// <summary>
/// Sorting and finding in a playlist, as Spotify and Apple Music offer them.
/// </summary>
/// <remarks>
/// Both are views over rows already loaded: nothing here changes a playlist. "custom" is the
/// playlist's own order, which is the order the rows arrived in.
/// </remarks>
public static class TrackOrder
{
    public static readonly IReadOnlyList<(string Label, string Key)> Choices =
    [
        ("Custom order", "custom"),
        ("Title", "title"),
        ("Artist", "artist"),
        ("Album", "album"),
        ("Duration", "duration"),
    ];

    public static string Label(string key) =>
        Choices.FirstOrDefault(choice => choice.Key == key).Label ?? "Custom order";

    /// <summary>The rows in the order <paramref name="key"/> asks for; ties keep the playlist's order.</summary>
    public static IReadOnlyList<T> Sort<T>(IEnumerable<T> rows, string key)
        where T : ISortableTrack
    {
        var comparer = StringComparer.Create(CultureInfo.CurrentCulture, ignoreCase: true);
        IEnumerable<T> ordered = key switch
        {
            "title" => rows.OrderBy(row => row.Title, comparer),
            "artist" => rows.OrderBy(row => row.Artist ?? row.Subtitle, comparer),
            "album" => rows.OrderBy(row => row.Album ?? "", comparer),
            // A row whose length is unknown goes last rather than first.
            "duration" => rows.OrderBy(row => DetailLayout.ParseDuration(row.Duration) ?? TimeSpan.MaxValue),
            _ => rows.OrderBy(row => row.Ordinal),
        };
        return (ordered as IOrderedEnumerable<T>)?.ThenBy(row => row.Ordinal).ToList() ?? ordered.ToList();
    }

    /// <summary>Whether a row matches what was typed in the find box: title, artist line or album.</summary>
    public static bool Matches(ISortableTrack row, string typed)
    {
        var text = typed.Trim();
        return text.Length == 0
            || row.Title.Contains(text, StringComparison.CurrentCultureIgnoreCase)
            || row.Subtitle.Contains(text, StringComparison.CurrentCultureIgnoreCase)
            || (row.Album?.Contains(text, StringComparison.CurrentCultureIgnoreCase) ?? false);
    }
}

/// <summary>The search box's memory of what was searched.</summary>
public static class RecentSearches
{
    public const int Limit = 8;

    /// <summary><paramref name="query"/> first, without a second copy of it, at most <see cref="Limit"/>.</summary>
    public static IReadOnlyList<string> Remember(IEnumerable<string> earlier, string query)
    {
        var trimmed = query.Trim();
        if (trimmed.Length == 0)
        {
            return earlier.Take(Limit).ToList();
        }

        return new[] { trimmed }
            .Concat(earlier.Where(item => !string.Equals(item, trimmed, StringComparison.CurrentCultureIgnoreCase)))
            .Take(Limit)
            .ToList();
    }

    /// <summary>
    /// What to offer while <paramref name="typed"/> is in the box: every recent search when it is
    /// empty, those containing it otherwise, and never the text already there.
    /// </summary>
    public static IReadOnlyList<string> Suggest(IEnumerable<string> recent, string typed)
    {
        var text = typed.Trim();
        return recent
            .Where(item => !string.Equals(item, text, StringComparison.CurrentCultureIgnoreCase))
            .Where(item => text.Length == 0 || item.Contains(text, StringComparison.CurrentCultureIgnoreCase))
            .ToList();
    }
}

/// <summary>Words the player shows about the queue and the sleep timer.</summary>
public static class PlayerText
{
    /// <summary>"From Liked Music · 99 songs": where the queue came from, then what is left.</summary>
    public static string UpNext(string source, int remaining, bool findingMore) =>
        (source.Length > 0 ? $"From {source} · " : "")
        + remaining switch
        {
            0 when findingMore => "Finding songs like this…",
            0 => "Nothing after this",
            1 => "1 song",
            _ => $"{remaining} songs",
        };

    /// <summary>The sleep timer's menu label: off, at the end of the song, or the minutes left.</summary>
    public static string SleepTimer(TimeSpan? left, bool endOfSong)
    {
        if (endOfSong)
        {
            return "Sleep timer: end of song";
        }

        if (left is not { } remaining)
        {
            return "Sleep timer";
        }

        // Rounded up, so the last half-minute reads "1 min left" rather than "0".
        return $"Sleep timer: {Math.Max(1, (int)Math.Ceiling(remaining.TotalMinutes))} min left";
    }
}

/// <summary>Rules about which page an id opens and what colour a mood is.</summary>
public static class CatalogRules
{
    /// <summary>Liked Music's playlist ids, which open the Liked Music page wherever they appear.</summary>
    public static bool IsLikedMusic(string kind, string id) => kind == "playlist" && id is "LM" or "VLLM";

    /// <summary>The library's artists, which are the artists of its songs.</summary>
    public const string LibraryArtists = "FEmusic_library_corpus_track_artists";

    /// <summary>The channels the account subscribes to.</summary>
    public const string LibrarySubscriptions = "FEmusic_library_corpus_artists";

    /// <summary>A category's <c>#RRGGBB</c>, or <c>null</c> for anything else.</summary>
    public static (byte R, byte G, byte B)? ParseColor(string? hex)
    {
        if (hex is not { Length: 7 } || hex[0] != '#'
            || !uint.TryParse(hex.AsSpan(1), NumberStyles.HexNumber, CultureInfo.InvariantCulture, out var rgb))
        {
            return null;
        }

        return ((byte)(rgb >> 16), (byte)(rgb >> 8), (byte)rgb);
    }
}
