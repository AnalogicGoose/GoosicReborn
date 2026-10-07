using System;

namespace Goosic.Windows.Presentation;

/// <summary>How one lyric line is drawn.</summary>
/// <param name="Opacity">How strongly the line shows.</param>
/// <param name="Blur">The blur radius, in effective pixels; 0 draws the text itself.</param>
/// <param name="Scale">The line's size about its leading edge.</param>
/// <param name="Lift">How far the line rises, in effective pixels.</param>
public readonly record struct LyricLineLook(double Opacity, double Blur, double Scale, double Lift);

/// <summary>
/// The listening treatment of lyrics: the line being sung is sharp and whole, and the lines around
/// it recede further the further they are from it.
/// </summary>
/// <remarks>
/// These are the macOS shell's numbers, kept here apart from any control so they can be held to
/// its cases. The treatment is for listening only. A line the pointer is on, every line once the
/// listener has scrolled away to read, and all of an unsynced document are drawn sharp, because
/// softened text that someone is trying to read is simply harder to read.
/// </remarks>
public static class LyricsAppearance
{
    /// <param name="index">The line's place in the document.</param>
    /// <param name="active">The line being sung, or -1 before the first one.</param>
    /// <param name="synced">Whether the document carries times at all.</param>
    /// <param name="following">Whether the view is following the song rather than being read.</param>
    /// <param name="pointed">Whether the pointer or keyboard focus is on this line.</param>
    /// <param name="immersive">The full-screen player, as opposed to the side panel.</param>
    /// <param name="plain">Contrast themes and reduced effects: no blur, and a plain dimming.</param>
    public static LyricLineLook Line(int index, int active, bool synced, bool following, bool pointed,
        bool immersive, bool plain)
    {
        if (!synced)
        {
            return new LyricLineLook(0.85, 0, 1, 0);
        }

        var current = index == active;
        if (current)
        {
            return new LyricLineLook(1, 0, 1, -2);
        }

        if (!following || pointed)
        {
            return new LyricLineLook(1, 0, following ? 0.97 : 1, 0);
        }

        if (plain)
        {
            return new LyricLineLook(0.55, 0, 1, 0);
        }

        // Before the first line nothing is nearer than anything else.
        var distance = active < 0 ? 0 : Math.Abs(index - active);
        var opacity = Math.Max(0.16, (immersive ? 0.36 : 0.34) - distance * 0.04);
        var blur = Math.Min(3, (immersive ? 1.1 : 0.8) + distance * 0.35);
        return new LyricLineLook(opacity, blur, 0.97, 0);
    }
}
