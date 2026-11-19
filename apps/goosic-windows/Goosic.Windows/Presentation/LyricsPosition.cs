using System;

namespace Goosic.Windows.Presentation;

/// <summary>Which position the lyrics follow for a reported playback sample.</summary>
/// <remarks>
/// While a seek is held the shown position is the requested target, and confirming it on every
/// sample made the follower's clock snap back to it repeatedly. The target is confirmed once when
/// the seek begins, so held samples are skipped. While the thumb is dragged the shown position is
/// not updated, but the audio plays on, so the lyrics follow what the player reports.
/// </remarks>
public static class LyricsPosition
{
    /// <summary>The position to confirm, or null to skip this sample.</summary>
    public static double? Choose(double sampleSeconds, PendingSeek? pending, bool scrubbing, DateTimeOffset now)
    {
        var reported = Math.Max(0, sampleSeconds);
        if (scrubbing)
        {
            return reported;
        }

        var (_, settled) = SeekSettle.Show(reported, pending, now);
        return settled ? reported : null;
    }
}
