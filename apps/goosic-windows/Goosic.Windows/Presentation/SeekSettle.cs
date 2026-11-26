using System;

namespace Goosic.Windows.Presentation;

/// <summary>A seek that has been asked for and not yet confirmed by the player.</summary>
public readonly record struct PendingSeek(double Target, DateTimeOffset RequestedAt);

/// <summary>
/// What the position line shows while a seek is on its way.
/// </summary>
/// <remarks>
/// The player keeps reporting the old position for a moment after a seek, and showing it made the
/// thumb jump back before jumping forward. The rule is <c>settle_seek</c> in
/// <c>goosic-shell-support</c>, which the Mac asks too; the elapsed time is measured here.
/// </remarks>
public static class SeekSettle
{
    /// <summary>The position to show, and whether the pending seek is finished with.</summary>
    public static (double Shown, bool Settled) Show(double confirmed, PendingSeek? pending, DateTimeOffset now) =>
        pending is { } seek
            ? PlaybackRules.SettleSeek(confirmed, seek.Target, now - seek.RequestedAt)
            : (confirmed, true);
}
