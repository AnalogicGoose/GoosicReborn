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

    /// <summary>
    /// Height the title, details, seek bar and transport need below the cover, with their spacing.
    /// </summary>
    public const double ControlsHeight = 250;

    /// <summary>The cover's full size, when there is room for it.</summary>
    public const double CoverMax = 520;

    /// <summary>Below this the cover is a smudge; the player shows just the controls instead.</summary>
    public const double CoverMin = 96;

    /// <summary>
    /// The largest cover that leaves the controls on screen, or 0 when there is no room for one.
    /// </summary>
    /// <remarks>
    /// The cover was sized by the column's width alone, so a wide, short window grew it until the
    /// seek bar and transport were pushed out of the bottom of the window.
    /// </remarks>
    public static double CoverSize(double height, double top, double bottom)
    {
        var room = height - top - bottom - ControlsHeight;
        return room < CoverMin ? 0 : Math.Min(room, CoverMax);
    }

    /// <summary>How far down content starts to clear the corner buttons.</summary>
    public const double CornerButtonsClearance = 112;

    /// <summary>The narrowest the controls under the cover stay usable at.</summary>
    public const double PlayerColumnMin = 300;

    /// <summary>Beyond this a line of lyrics is too long to read at a glance.</summary>
    public const double LyricsMaxWidth = 768;

    /// <summary>
    /// The widths of the side-by-side layout: the margin at either side, the gap between the two
    /// columns, and the columns themselves.
    /// </summary>
    /// <remarks>
    /// The macOS player's proportions. The two columns are sized for what they hold and then
    /// placed in the middle of the window as one group, so a wide window gains margin on both
    /// sides. Giving the lyrics everything the cover did not use instead leaves the cover pinned
    /// to the left edge and the whole player looking as if it had slid that way.
    /// </remarks>
    /// <param name="width">The window's width in effective pixels.</param>
    /// <param name="cover">The cover's size, or 0 when there is no room for one.</param>
    public static (double Inset, double Gutter, double Player, double Lyrics) Split(double width, double cover)
    {
        var gutter = Math.Clamp(width * 0.07, 32, 128);
        var inset = Math.Clamp(width * 0.09, 40, 176);
        var room = Math.Max(0, width - 2 * inset - gutter);
        // The cover's column never takes more than its share from the lyrics on a narrow window.
        var player = Math.Min(Math.Max(cover, PlayerColumnMin), Math.Max(PlayerColumnMin, room * 0.44));
        player = Math.Min(player, room);
        return (inset, gutter, player, Math.Min(room - player, LyricsMaxWidth));
    }

    /// <summary>
    /// How tall the lyrics may be beside the cover: most of the window, so that they fade out
    /// short of its top and bottom edges instead of running into the buttons in its corners.
    /// </summary>
    public static double LyricsMaxHeight(double height, FullPlayerMode mode) =>
        mode == FullPlayerMode.Split ? Math.Min(height * 0.7, 832) : double.PositiveInfinity;

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
