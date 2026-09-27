using System;
using System.Collections.Generic;
using System.Globalization;

namespace Goosic.Windows.Presentation;

/// <summary>Which kind of page holds the track rows, as far as their layout is concerned.</summary>
public enum DetailKind { Browse, Search, Album, Playlist, Artist }

/// <summary>How each kind of page lays out its hero and its rows.</summary>
/// <remarks>
/// An album's rows share one cover, so repeating it on every row is noise and the number carries
/// the order instead. A search result has no order worth numbering. An artist's portrait is round,
/// as it is everywhere else in YouTube Music.
/// </remarks>
public static class DetailLayout
{
    public static DetailKind FromEntity(string kind) => kind switch
    {
        "album" => DetailKind.Album,
        "playlist" => DetailKind.Playlist,
        "artist" => DetailKind.Artist,
        _ => DetailKind.Browse,
    };

    public static bool ShowsNumbers(DetailKind kind) =>
        kind is DetailKind.Album or DetailKind.Playlist or DetailKind.Artist;

    public static bool ShowsRowArtwork(DetailKind kind) => kind != DetailKind.Album;

    public static bool HasRoundArtwork(DetailKind kind) => kind == DetailKind.Artist;

    /// <summary>The hero's corner radius for a 148-pixel cover.</summary>
    public static double HeroCornerRadius(DetailKind kind) => HasRoundArtwork(kind) ? 74 : 10;

    /// <summary>
    /// The line under a detail page's title: what the page said about itself, then how many
    /// songs it holds and how long they run.
    /// </summary>
    /// <param name="durations">Each row's duration as the catalog wrote it, such as "3:45".</param>
    /// <param name="truncated">
    /// Whether the page was cut short. A clamped page never presents its count as the whole.
    /// </param>
    public static string Summary(string subtitle, IReadOnlyList<string> durations, bool truncated)
    {
        var parts = new List<string>();
        if (!string.IsNullOrWhiteSpace(subtitle))
        {
            parts.Add(subtitle.Trim());
        }

        if (durations.Count > 0)
        {
            var count = durations.Count == 1 ? "1 song" : $"{durations.Count} songs";
            parts.Add(truncated ? $"{durations.Count}+ songs" : count);

            // A total is only true when every row had a length and the list is complete.
            var total = TimeSpan.Zero;
            var known = !truncated;
            foreach (var duration in durations)
            {
                if (ParseDuration(duration) is { } length)
                {
                    total += length;
                }
                else
                {
                    known = false;
                }
            }

            if (known && total > TimeSpan.Zero)
            {
                parts.Add(FormatTotal(total));
            }
        }

        return string.Join(" · ", parts);
    }

    /// <summary>Reads "m:ss" or "h:mm:ss"; anything else is unknown.</summary>
    public static TimeSpan? ParseDuration(string? text)
    {
        if (string.IsNullOrWhiteSpace(text))
        {
            return null;
        }

        var pieces = text.Trim().Split(':');
        if (pieces.Length is < 2 or > 3)
        {
            return null;
        }

        var seconds = 0;
        foreach (var piece in pieces)
        {
            if (!int.TryParse(piece, NumberStyles.None, CultureInfo.InvariantCulture, out var value))
            {
                return null;
            }

            seconds = checked(seconds * 60 + value);
        }

        return TimeSpan.FromSeconds(seconds);
    }

    public static string FormatTotal(TimeSpan total)
    {
        var minutes = (int)Math.Round(total.TotalMinutes, MidpointRounding.AwayFromZero);
        if (minutes < 60)
        {
            return minutes <= 1 ? "1 min" : $"{minutes} min";
        }

        var hours = minutes / 60;
        var rest = minutes % 60;
        var hourText = hours == 1 ? "1 hr" : $"{hours} hr";
        return rest == 0 ? hourText : $"{hourText} {rest} min";
    }
}
