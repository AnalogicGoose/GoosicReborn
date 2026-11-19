using System;
namespace Goosic.Windows.Presentation;

/// <summary>What the lyrics lookup is asked for a track.</summary>
public sealed record LyricsLookup(string Title, string Artist, string Album, int? DurationSeconds)
{
    /// <summary>
    /// Builds the question from the catalog's own fields rather than from what the row displays.
    /// </summary>
    /// <remarks>
    /// The artist used to be the row's subtitle, which on a home shelf reads "Song • The Weeknd"
    /// or "Ecoficient • 356K views". LRCLIB's exact match then missed, and its search matched
    /// whatever record happened to carry that text, so some tracks had no lyrics or the wrong
    /// ones. A subtitle is used only when it is nothing but a name.
    /// </remarks>
    public static LyricsLookup For(string title, string? artist, string subtitle, string? album, string? duration, double? playedSeconds = null)
    {
        var name = !string.IsNullOrWhiteSpace(artist) ? artist.Trim()
            : IsBareName(subtitle) ? subtitle.Trim()
            : "";
        return new(title.Trim(), name, album?.Trim() ?? "", Length(playedSeconds) ?? Seconds(duration));
    }

    /// <summary>The recording's real length, whole seconds, or null when it is unknown.</summary>
    /// <remarks>YouTube Music may play another cut than the catalog row lists, and a length that
    /// differs by an intro matches the wrong lyrics. Callers pass nothing for advertisements.</remarks>
    public static int? Length(double? played) =>
        played is > 0 and < 86_400 ? (int)Math.Round(played.Value, MidpointRounding.AwayFromZero) : null;

    /// <summary>A subtitle with no separator is an artist line; one with a separator is a description.</summary>
    private static bool IsBareName(string subtitle) =>
        !string.IsNullOrWhiteSpace(subtitle) && subtitle.IndexOfAny(['•', '·']) < 0;

    /// <summary>Parses "m:ss" or "h:mm:ss"; anything else is no duration rather than a wrong one.</summary>
    public static int? Seconds(string? duration)
    {
        if (string.IsNullOrWhiteSpace(duration))
        {
            return null;
        }

        var parts = duration.Trim().Split(':');
        if (parts.Length is < 2 or > 3)
        {
            return null;
        }

        var total = 0;
        foreach (var part in parts)
        {
            if (!int.TryParse(part, System.Globalization.NumberStyles.None,
                    System.Globalization.CultureInfo.InvariantCulture, out var value))
            {
                return null;
            }

            total = total * 60 + value;
        }

        return total;
    }
}
