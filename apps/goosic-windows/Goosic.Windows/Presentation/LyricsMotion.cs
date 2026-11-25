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
    public static double Covered(double seconds) => Spring(seconds, RowSpringPeriod.TotalSeconds);

    /// <summary>
    /// A SwiftUI spring: the share of the distance covered <paramref name="seconds"/> after it starts,
    /// for a spring with this <paramref name="response"/> and damping ratio, from rest.
    /// </summary>
    /// <remarks>
    /// SwiftUI's response is the undamped period, so ω = 2π / response; <c>.smooth(duration: d)</c> is
    /// response d with a damping ratio of 1, and a bounce b is a damping ratio of 1 − b. The
    /// compositor's own spring animations take a "period" that is not this quantity, and given the
    /// same numbers they ran several times slower than macOS, so Windows draws this curve itself.
    /// </remarks>
    public static double Spring(double seconds, double response, double damping = 1)
    {
        if (seconds <= 0 || response <= 0)
        {
            return seconds <= 0 ? 0 : 1;
        }

        var omega = 2 * Math.PI / response;
        if (damping >= 1)
        {
            return 1 - (1 + omega * seconds) * Math.Exp(-omega * seconds);
        }

        var decay = damping * omega;
        var ringing = omega * Math.Sqrt(1 - damping * damping);
        return 1 - Math.Exp(-decay * seconds)
            * (Math.Cos(ringing * seconds) + decay / ringing * Math.Sin(ringing * seconds));
    }

    /// <summary>
    /// How long a spring is drawn for: until it is within a thousandth of its target and stays there.
    /// </summary>
    public static double SettlingTime(double response, double damping = 1)
    {
        var omega = 2 * Math.PI / response;
        // e^(-ζωt) envelope below 1e-3, with a margin for the critically damped polynomial factor.
        return Math.Min(4 * response, 9.5 / (Math.Min(1, damping) * omega));
    }

    /// <summary>The full player's cover on pause and play: macOS's <c>.snappy(duration: 0.5, extraBounce: 0.1)</c>.</summary>
    public const double CoverResponse = 0.5;

    /// <summary>Snappy's own bounce of 0.15 plus the extra 0.1, as a damping ratio.</summary>
    public const double CoverDamping = 0.75;

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
