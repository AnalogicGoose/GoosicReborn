using System;

namespace Goosic.Windows.Presentation;

/// <summary>What the full-screen player shows side by side, or instead of each other.</summary>
public enum FullPlayerMode
{
    /// <summary>Cover and controls beside the lyrics.</summary>
    Split,
    /// <summary>Cover and controls alone: lyrics hidden, or no room for them.</summary>
    Cover,
    /// <summary>Lyrics fill the player; the controls stay, the cover steps aside.</summary>
    Lyrics,
}

/// <summary>How the full-screen player is arranged, decided without touching a control.</summary>
public static class FullPlayerLayout
{
    /// <summary>Below this width the cover and the lyrics are each too cramped to share a row.</summary>
    public const double SplitFrom = 900;

    public const double SeekStepSeconds = 5;

    /// <param name="width">The window's width in effective pixels.</param>
    /// <param name="lyricsRequested">Whether the lyrics toggle is on.</param>
    public static FullPlayerMode Mode(double width, bool lyricsRequested) => (width >= SplitFrom, lyricsRequested) switch
    {
        (_, false) => FullPlayerMode.Cover,
        (true, true) => FullPlayerMode.Split,
        (false, true) => FullPlayerMode.Lyrics,
    };

    /// <summary>The player's outer padding: generous on a large window, tight on a small one.</summary>
    /// <remarks>
    /// A single column starts below the buttons floating in the top right corner; side by side,
    /// only the lyrics column reaches that corner and it keeps its own margin instead.
    /// </remarks>
    public static (double Horizontal, double Top, double Bottom) Padding(double width) => width switch
    {
        < 600 => (20, CornerButtonsClearance, 24),
        < SplitFrom => (36, CornerButtonsClearance, 40),
        _ => (72, 56, 72),
    };

    /// <summary>How far down content starts to clear the corner buttons.</summary>
    public const double CornerButtonsClearance = 112;

    /// <summary>The lyrics column's own top margin, for the split layout where it shares the corner.</summary>
    public static double LyricsTopMargin(FullPlayerMode mode) => mode == FullPlayerMode.Split ? 64 : 0;

    /// <summary>
    /// Whether the blurred cover sits behind the player. With transparency effects turned off in
    /// Windows, the player keeps its moving colours on black and skips the blur.
    /// </summary>
    public static bool ShowsBlur(bool transparencyEffectsEnabled, bool hasArtwork) =>
        transparencyEffectsEnabled && hasArtwork;

    /// <summary>Where a key on the position slider seeks to, or null when the key is not a seek key.</summary>
    /// <remarks>
    /// Arrow keys move five seconds, Page keys a tenth of the track, Home and End to either end —
    /// the steps a slider is expected to take, applied as a seek because the slider alone only
    /// moves its thumb.
    /// </remarks>
    public static double? KeySeek(SeekKey key, double position, double duration)
    {
        if (duration <= 0)
        {
            return null;
        }

        var target = key switch
        {
            SeekKey.Back => position - SeekStepSeconds,
            SeekKey.Forward => position + SeekStepSeconds,
            SeekKey.PageBack => position - duration / 10,
            SeekKey.PageForward => position + duration / 10,
            SeekKey.Start => 0,
            SeekKey.End => duration,
            _ => double.NaN,
        };
        return double.IsNaN(target) ? null : Math.Clamp(target, 0, duration);
    }
}

public enum SeekKey { None, Back, Forward, PageBack, PageForward, Start, End }
