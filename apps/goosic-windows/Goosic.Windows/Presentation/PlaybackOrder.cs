using System;

namespace Goosic.Windows.Presentation;

/// <summary>
/// The rules for when the page's track has ended, kept apart from the player so every case can
/// be tested. Which song plays next is not here: that is <c>QueueEngine</c> in
/// <c>goosic-shell-support</c>, reached through <c>SharedQueue</c>.
/// </summary>
public static class PlaybackOrder
{
    /// <summary>
    /// Whether a track's natural end should be acted on: only for the entry being played, only
    /// once, and only after that entry has actually played — a late "ended" from the song before
    /// must not skip the one that replaced it.
    /// </summary>
    public static bool AcceptsEnd(string? endedVideoId, string? currentVideoId, bool armed) =>
        armed && !string.IsNullOrEmpty(endedVideoId) && endedVideoId == currentVideoId;

    /// <summary>How close to its length a stopped track has to be to count as finished.</summary>
    public const double EndTolerance = 1.5;

    /// <summary>
    /// Whether a report says the track has finished.
    /// </summary>
    /// <remarks>
    /// With Automix off, YouTube Music's page does not report "ended": it stops a fraction of a
    /// second short of the length and reports "paused". A pause there is the end, unless the
    /// listener paused it themselves.
    /// </remarks>
    public static bool IsFinished(string state, double position, double duration, bool listenerPaused) =>
        state == "ended"
        || (state == "paused" && !listenerPaused && duration > 0 && position >= duration - EndTolerance);

    /// <summary>
    /// Whether the page's title names the song that was asked for.
    /// </summary>
    /// <remarks>
    /// YouTube Music swaps a song for its other version -- the audio track for the music video,
    /// or back -- before it starts. That is the same song under another id, not the page moving
    /// on, and treating it as an end skipped the song. The titles differ only by decoration such
    /// as "(Official Video)", so both are reduced to their words before comparing.
    /// </remarks>
    public static bool IsSameSong(string? requestedTitle, string? pageTitle)
    {
        var requested = SongWords(requestedTitle);
        var page = SongWords(pageTitle);
        // Whole words only, so "Down" is not found inside "Downtown".
        return requested.Length > 0 && page.Length > 0
            && ($" {page} ".Contains($" {requested} ", StringComparison.Ordinal)
                || $" {requested} ".Contains($" {page} ", StringComparison.Ordinal));
    }

    private static string SongWords(string? title)
    {
        if (string.IsNullOrWhiteSpace(title))
        {
            return "";
        }

        // Drop bracketed decoration: (Official Video), [Lyrics], (feat. …) and the like.
        var text = System.Text.RegularExpressions.Regex.Replace(title, @"[\(\[][^\)\]]*[\)\]]", " ");
        var words = new System.Text.StringBuilder();
        foreach (var ch in text.ToLowerInvariant())
        {
            // An apostrophe joins a word rather than splitting it: "don't" and "dont" are one word.
            if (ch is '\'' or '’')
            {
                continue;
            }

            words.Append(char.IsLetterOrDigit(ch) ? ch : ' ');
        }

        return string.Join(' ', words.ToString().Split(' ', StringSplitOptions.RemoveEmptyEntries));
    }
}
