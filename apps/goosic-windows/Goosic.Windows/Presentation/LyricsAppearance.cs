using System;

namespace Goosic.Windows.Presentation;

/// <summary>How one lyric line is drawn.</summary>
/// <param name="Opacity">How strongly the line shows.</param>
/// <param name="Blur">The blur radius, in effective pixels; 0 draws the text itself.</param>
public readonly record struct LyricLineLook(double Opacity, double Blur);

/// <summary>
/// The listening treatment of lyrics: the line being sung is sharp and whole, what is about to be
/// sung stays readable, and what has been sung falls back further.
/// </summary>
/// <remarks>
/// These are the macOS shell's numbers, kept here apart from any control so they can be held to
/// its cases (<c>docs/LYRICS_TIMING.md</c>). The treatment is for listening only. A line the
/// pointer is on, every line once the listener has scrolled away to read, and all of an unsynced
/// document are drawn sharp, because softened text that someone is trying to read is simply harder
/// to read. A line keeps its size and place when it becomes current: an earlier lift and context
/// scale read as a zoom on every line change, which Music does not have.
/// </remarks>
public static class LyricsAppearance
{
    /// <param name="index">The line's place in the document.</param>
    /// <param name="current">The line drawn as current, or -1 before the first one.</param>
    /// <param name="resting">The current line has reached its end and the next has not begun.</param>
    /// <param name="synced">Whether the document carries times at all.</param>
    /// <param name="following">Whether the view is following the song rather than being read.</param>
    /// <param name="pointed">Whether the pointer or keyboard focus is on this line.</param>
    /// <param name="plain">Contrast themes and reduced effects: no blur, and a plain dimming.</param>
    public static LyricLineLook Line(int index, int current, bool resting, bool synced, bool following, bool pointed,
        bool plain)
    {
        if (!synced)
        {
            return new LyricLineLook(0.85, 0);
        }

        if (Lit(index, current, resting) || !following || pointed)
        {
            return new LyricLineLook(1, 0);
        }

        if (plain)
        {
            return new LyricLineLook(0.55, 0);
        }

        // Before the first line every line is still to come.
        var offset = index - current;
        var distance = Math.Abs(offset);
        // Nothing is being sung: the introduction, or a line that has reached its end.
        var idle = resting || current < 0;
        if (offset > 0 || (resting && offset == 0))
        {
            // What is about to be sung stays readable, and sharp while nothing is lit.
            return idle
                ? new LyricLineLook(0.62, 0)
                : new LyricLineLook(Math.Max(0.3, 0.6 - (distance - 1) * 0.07), Math.Min(3, distance * 0.5));
        }

        return new LyricLineLook(Math.Max(0.16, 0.34 - (distance - 1) * 0.06), Math.Min(3, 0.8 + distance * 0.6));
    }

    /// <summary>Whether this is the line being sung: current, and not at rest.</summary>
    public static bool Lit(int index, int current, bool resting) => index == current && !resting;
}
