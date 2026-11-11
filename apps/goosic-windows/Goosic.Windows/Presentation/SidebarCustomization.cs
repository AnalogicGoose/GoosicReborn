using System;
using System.Collections.Generic;
using System.Linq;

namespace Goosic.Windows.Presentation;

/// <summary>
/// Something the listener pinned to the top of the sidebar: a playlist, album, artist or song.
/// </summary>
/// <remarks>
/// It carries only what drawing and opening it need, ids and a title, never anything an account
/// keeps private. A song is identified by its video id, which is also its <see cref="Id"/>.
/// </remarks>
public sealed record SidebarPin(
    string Kind, string Id, string Title, string Subtitle = "", string? Thumbnail = null)
{
    public bool IsSong => Kind is "song" or "video";

    public string Glyph => Kind switch
    {
        "album" => "\uE93C",
        "artist" => "\uE77B",
        "playlist" => "\uE90B",
        _ => "\uE8D6",
    };
}

/// <summary>What the listener chose to show in the sidebar, and what they pinned there.</summary>
/// <remarks>
/// Search, Home and Settings are always present and are never offered for hiding, so the listener
/// can never hide the way back to the setting that brings an item back. The rail mirrors the
/// expanded list, so one rule decides both.
/// </remarks>
public static class SidebarCustomization
{
    /// <summary>Every destination that can be hidden, in sidebar order, with its heading.</summary>
    public static IReadOnlyList<(string Tag, string Title, string Section)> Hideable { get; } =
    [
        ("explore", "Explore", "Discover"),
        ("charts", "Charts", "Discover"),
        ("moodsAndGenres", "Moods & genres", "Discover"),
        ("newReleases", "New releases", "Discover"),
        ("liked", "Liked Music", "Library"),
        ("library:FEmusic_library_corpus_track_artists", "Artists", "Library"),
        ("library:FEmusic_liked_albums", "Albums", "Library"),
        ("history", "History", "Library"),
        ("library:FEmusic_liked_playlists", "All Playlists", "Playlists"),
    ];

    /// <summary>The most pins kept; past it the sidebar would be all pins and no navigation.</summary>
    public const int MaximumPins = 30;

    public static bool IsHideable(string tag) => Hideable.Any(item => item.Tag == tag);

    /// <summary>Whether a destination appears; anything not offered for hiding always does.</summary>
    public static bool IsShown(string tag, IReadOnlyCollection<string> hidden) =>
        !IsHideable(tag) || !hidden.Contains(tag);

    /// <summary>Whether a section heading still has anything under it.</summary>
    public static bool SectionHasItems(string section, IReadOnlyCollection<string> hidden) =>
        Hideable.Where(item => item.Section == section).Any(item => !hidden.Contains(item.Tag));

    /// <summary>Keeps only names this version knows, once each, so a stale store cannot hide Home.</summary>
    public static IReadOnlyList<string> Sanitize(IEnumerable<string>? hidden) =>
        (hidden ?? []).Where(IsHideable).Distinct().ToArray();

    public static IReadOnlyList<string> SetShown(IReadOnlyList<string> hidden, string tag, bool shown)
    {
        if (!IsHideable(tag))
        {
            return hidden;
        }

        var rest = hidden.Where(item => item != tag);
        return (shown ? rest : rest.Append(tag)).ToArray();
    }

    public static bool IsPinned(IReadOnlyList<SidebarPin> pins, string kind, string id) =>
        pins.Any(pin => SameItem(pin, kind, id));

    /// <summary>Adds a pin at the end, or leaves the list alone if it is already there or full.</summary>
    public static IReadOnlyList<SidebarPin> Pin(IReadOnlyList<SidebarPin> pins, SidebarPin pin) =>
        string.IsNullOrEmpty(pin.Id) || IsPinned(pins, pin.Kind, pin.Id) || pins.Count >= MaximumPins
            ? pins
            : pins.Append(pin).ToArray();

    public static IReadOnlyList<SidebarPin> Unpin(IReadOnlyList<SidebarPin> pins, string kind, string id) =>
        pins.Where(pin => !SameItem(pin, kind, id)).ToArray();

    /// <summary>Moves a pin by <paramref name="offset"/> places, stopping at either end.</summary>
    public static IReadOnlyList<SidebarPin> Move(IReadOnlyList<SidebarPin> pins, int index, int offset)
    {
        if (index < 0 || index >= pins.Count)
        {
            return pins;
        }

        var target = Math.Clamp(index + offset, 0, pins.Count - 1);
        var list = pins.ToList();
        var pin = list[index];
        list.RemoveAt(index);
        list.Insert(target, pin);
        return list;
    }

    /// <summary>Drops anything unusable from a stored list: no id, an unknown kind, a duplicate.</summary>
    public static IReadOnlyList<SidebarPin> Sanitize(IEnumerable<SidebarPin?>? pins)
    {
        var result = new List<SidebarPin>();
        foreach (var pin in pins ?? [])
        {
            if (pin is { Id.Length: > 0, Kind: "album" or "artist" or "playlist" or "song" or "video" }
                && !IsPinned(result, pin.Kind, pin.Id) && result.Count < MaximumPins)
            {
                result.Add(pin);
            }
        }

        return result;
    }

    // A song and its music video share a video id, and pinning one should not pin it twice.
    private static bool SameItem(SidebarPin pin, string kind, string id) =>
        pin.Id == id && (pin.Kind == kind || pin.IsSong && kind is "song" or "video");
}
