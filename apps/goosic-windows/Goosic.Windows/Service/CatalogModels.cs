using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace Goosic.Windows.Service;

/// <summary>
/// The catalog shapes this shell renders.
/// </summary>
/// <remarks>
/// Deliberately a subset. <c>goosic-protocol</c> is the source of truth for the wire types, and
/// <c>SHELL_CONTRACT.md</c> names hand-written mirrors of it as the drift the migration exists
/// to stop -- so these carry only the fields a view actually reads, and unknown fields are
/// ignored rather than asserted. What the shell needs from the protocol beyond this comes from
/// the fixtures, not from widening these records.
/// </remarks>
internal sealed record CatalogItem
{
    [JsonPropertyName("kind")] public string Kind { get; init; } = "";
    [JsonPropertyName("id")] public string Id { get; init; } = "";
    [JsonPropertyName("title")] public string Title { get; init; } = "";
    [JsonPropertyName("subtitle")] public string Subtitle { get; init; } = "";
    [JsonPropertyName("artist")] public string? Artist { get; init; }
    [JsonPropertyName("artistId")] public string? ArtistId { get; init; }
    [JsonPropertyName("album")] public string? Album { get; init; }
    [JsonPropertyName("albumId")] public string? AlbumId { get; init; }
    [JsonPropertyName("duration")] public string? Duration { get; init; }
    [JsonPropertyName("thumbnail")] public string? Thumbnail { get; init; }
    [JsonPropertyName("videoId")] public string? VideoId { get; init; }
    [JsonPropertyName("explicit")] public bool Explicit { get; init; }

    /// <summary>
    /// This occurrence of a track in an account's playlist, from the personal reader only.
    /// </summary>
    /// <remarks>
    /// A playlist may hold the same track twice, so removing one needs this rather than the video id.
    /// </remarks>
    [JsonPropertyName("entryId")] public string? EntryId { get; init; }
}

internal sealed record CatalogShelf
{
    [JsonPropertyName("id")] public string Id { get; init; } = "";
    [JsonPropertyName("title")] public string Title { get; init; } = "";
    [JsonPropertyName("items")] public IReadOnlyList<CatalogItem> Items { get; init; } = [];

    /// <summary><c>list</c> for song rows such as Quick picks; absent, which means cards, otherwise.</summary>
    [JsonPropertyName("layout")] public string? Layout { get; init; }
}

internal sealed record CatalogPage
{
    [JsonPropertyName("id")] public string Id { get; init; } = "";
    [JsonPropertyName("title")] public string Title { get; init; } = "";
    [JsonPropertyName("subtitle")] public string Subtitle { get; init; } = "";
    [JsonPropertyName("shelves")] public IReadOnlyList<CatalogShelf> Shelves { get; init; } = [];
    [JsonPropertyName("tracks")] public IReadOnlyList<CatalogItem> Tracks { get; init; } = [];

    /// <summary>The page's own cover, when it has one, such as a playlist's or Liked Music's.</summary>
    [JsonPropertyName("thumbnail")] public string? Thumbnail { get; init; }

    /// <summary>The opaque cursor for the next page, echoed only to <c>catalog.continue</c>.</summary>
    [JsonPropertyName("nextCursor")] public string? NextCursor { get; init; }

    /// <summary>The playlist behind "Show all" when <see cref="Tracks"/> is only the first few, as on an artist.</summary>
    [JsonPropertyName("allTracksId")] public string? AllTracksId { get; init; }

    /// <summary>
    /// Whether the service had to cut this page short.
    /// </summary>
    /// <remarks>
    /// Carried because a clamped page must say so: presenting a partial list as complete is
    /// one of the invariants, not a detail of presentation.
    /// </remarks>
    [JsonPropertyName("truncated")] public bool Truncated { get; init; }
}

/// <summary>The wrapper the service puts a catalog page inside.</summary>
/// <remarks>
/// The field is <c>catalog</c>, and it is present only on <c>catalog.*</c> responses. Catalog
/// data never carries credentials, which is what makes it safe to hold in a plain record here.
/// </remarks>
internal sealed record CatalogResponsePayload
{
    [JsonPropertyName("catalog")] public CatalogPage? Page { get; init; }
}

/// <summary>The bounded lyric document returned only by <c>lyrics.get</c>.</summary>
internal sealed record LyricsDocument
{
    [JsonPropertyName("source")] public string Source { get; init; } = "";
    [JsonPropertyName("synced")] public bool Synced { get; init; }
    [JsonPropertyName("truncated")] public bool Truncated { get; init; }
    [JsonPropertyName("lines")] public IReadOnlyList<LyricsLine> Lines { get; init; } = [];
}

internal sealed record LyricsLine
{
    [JsonPropertyName("atMs")] public long AtMilliseconds { get; init; }
    [JsonPropertyName("text")] public string Text { get; init; } = "";
}

internal sealed record LyricsResponsePayload
{
    [JsonPropertyName("lyrics")] public LyricsDocument? Document { get; init; }
}

internal sealed record AccountSummary
{
    [JsonPropertyName("id")] public string Id { get; init; } = "";
    [JsonPropertyName("webkitProfileId")] public string WebProfileId { get; init; } = "";
    [JsonPropertyName("channel")] public string? Channel { get; init; }
    [JsonPropertyName("avatarUrl")] public string? AvatarUrl { get; init; }
    [JsonPropertyName("displayName")] public string DisplayName { get; init; } = "";
    [JsonPropertyName("email")] public string? Email { get; init; }
}

internal sealed record AccountsSnapshot
{
    [JsonPropertyName("accounts")] public IReadOnlyList<AccountSummary> Accounts { get; init; } = [];
    [JsonPropertyName("activeAccountId")] public string? ActiveAccountId { get; init; }
}

internal sealed record AccountsResponsePayload
{
    [JsonPropertyName("accounts")] public AccountsSnapshot? Accounts { get; init; }
}
