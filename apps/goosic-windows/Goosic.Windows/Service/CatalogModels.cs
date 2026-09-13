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
    [JsonPropertyName("duration")] public string? Duration { get; init; }
    [JsonPropertyName("thumbnail")] public string? Thumbnail { get; init; }
    [JsonPropertyName("videoId")] public string? VideoId { get; init; }
    [JsonPropertyName("explicit")] public bool Explicit { get; init; }
}

internal sealed record CatalogShelf
{
    [JsonPropertyName("id")] public string Id { get; init; } = "";
    [JsonPropertyName("title")] public string Title { get; init; } = "";
    [JsonPropertyName("items")] public IReadOnlyList<CatalogItem> Items { get; init; } = [];
}

internal sealed record CatalogPage
{
    [JsonPropertyName("id")] public string Id { get; init; } = "";
    [JsonPropertyName("title")] public string Title { get; init; } = "";
    [JsonPropertyName("subtitle")] public string Subtitle { get; init; } = "";
    [JsonPropertyName("shelves")] public IReadOnlyList<CatalogShelf> Shelves { get; init; } = [];
    [JsonPropertyName("tracks")] public IReadOnlyList<CatalogItem> Tracks { get; init; } = [];

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
