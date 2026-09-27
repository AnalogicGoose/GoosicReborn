using System;

namespace Goosic.Windows.Presentation;

/// <summary>A seek that has been asked for and not yet confirmed by the player.</summary>
public readonly record struct PendingSeek(double Target, DateTimeOffset RequestedAt);

/// <summary>
/// What the position line shows while a seek is on its way.
/// </summary>
/// <remarks>
/// The player keeps reporting the old position for a moment after a seek, and showing it made the
/// thumb jump back before jumping forward. The requested position is shown until a report lands
/// near it or a second has passed. The same window and tolerance as <c>SEEK_SETTLE_WINDOW</c> and
/// <c>SEEK_LANDED_TOLERANCE_SECONDS</c> in <c>goosic-shell-support</c>, which macOS applies.
/// </remarks>
public static class SeekSettle
{
    public static readonly TimeSpan Window = TimeSpan.FromSeconds(1);

    public const double LandedTolerance = 1.5;

    /// <summary>The position to show, and whether the pending seek is finished with.</summary>
    public static (double Shown, bool Settled) Show(double confirmed, PendingSeek? pending, DateTimeOffset now)
    {
        if (pending is not { } seek)
        {
            return (confirmed, true);
        }

        var landed = Math.Abs(confirmed - seek.Target) < LandedTolerance;
        return landed || now - seek.RequestedAt >= Window ? (confirmed, true) : (seek.Target, false);
    }
}
