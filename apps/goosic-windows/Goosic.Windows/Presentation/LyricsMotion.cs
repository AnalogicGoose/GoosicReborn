using System;

namespace Goosic.Windows.Presentation;

/// <summary>
/// How the lyric column moves on a line change, measured from a recording of Music on November 16,
/// 2026 and shared with the macOS shell (<c>docs/LYRICS_TIMING.md</c>).
/// </summary>
/// <remarks>
/// Rows at or above the new line start together and each row below starts 50 ms after the one
/// above, up to ten rows. Each row moves on its own critically damped spring with a response of
/// 0.4 seconds: a quarter of the way in 50 ms, half in 100 ms, 98% in 340 ms, without overshoot.
/// Scrolling the column as one block, on any curve, cannot produce that, and read as the lyrics
/// stopping and starting.
/// </remarks>
public static class LyricsMotion
{
    /// <summary>Where the row being followed sits in the viewport, as a fraction of its height.</summary>
    public const double Anchor = 0.36;

    /// <summary>The spring's period (SwiftUI's response); with a damping ratio of 1 it never overshoots.</summary>
    public static readonly TimeSpan RowSpringPeriod = TimeSpan.FromSeconds(0.4);

    /// <summary>How long each row below the leader waits after the one above.</summary>
    public static readonly TimeSpan RowDelay = TimeSpan.FromMilliseconds(50);

    /// <summary>Rows further below than this all start with the tenth.</summary>
    public const int MaxDelayedRows = 10;

    /// <summary>The scale of the full player's cover while playback is paused.</summary>
    public const double PausedCoverScale = 0.74;

    /// <summary>How long a row waits before giving back its offset.</summary>
    /// <param name="order">The row's place in the column.</param>
    /// <param name="leader">The new current line; rows at or above it move first.</param>
    public static TimeSpan Delay(int order, int leader) =>
        RowDelay * Math.Clamp(order - leader, 0, MaxDelayedRows);

    /// <summary>
    /// The share of the distance a row has covered <paramref name="seconds"/> after it starts: a
    /// critically damped spring, 1 - (1 + ωt)e^(-ωt) with ω = 2π / period.
    /// </summary>
    public static double Covered(double seconds)
    {
        if (seconds <= 0)
        {
            return 0;
        }

        var omega = 2 * Math.PI / RowSpringPeriod.TotalSeconds;
        return 1 - (1 + omega * seconds) * Math.Exp(-omega * seconds);
    }

    /// <summary>
    /// The padding above the first row and below the last, so that any row, the first and the last
    /// included, can reach the anchor. The per-row motion relies on the column scrolling by exactly
    /// the distance between two rows, which a clamped scroll would not.
    /// </summary>
    public static (double Top, double Bottom) Padding(double viewport, bool immersive)
    {
        var least = immersive ? 160 : 120;
        return (Math.Max(least, viewport * Anchor), Math.Max(least, viewport * (1 - Anchor)));
    }

    /// <summary>
    /// The vertical offset that puts a row at the anchor: the point 36% down the row meets the point
    /// 36% down the viewport. Clamped to what the scroller can reach.
    /// </summary>
    public static double Target(double rowTop, double rowHeight, double viewport, double scrollable) =>
        Math.Clamp(rowTop + rowHeight * Anchor - viewport * Anchor, 0, Math.Max(0, scrollable));
}
