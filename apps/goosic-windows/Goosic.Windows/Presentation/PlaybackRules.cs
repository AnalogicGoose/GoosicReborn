using System;
using System.Runtime.InteropServices;

namespace Goosic.Windows.Presentation;

/// <summary>
/// The playback rules asked on every sample, drag and frame, answered by <c>goosic-shell-support</c>.
/// </summary>
/// <remarks>
/// The Mac asks the same functions, so the volume curve, the time text, seek settling, when a
/// track has finished and whether a swapped-in video is the same song cannot drift between the
/// two again. They cross as plain numbers rather than JSON because a player reports NaN for an
/// unknown length, which JSON cannot carry; each rule already reads NaN as "not known".
/// </remarks>
public static class PlaybackRules
{
    private const string Library = "goosic_shell_support_ffi";

    [DllImport(Library, EntryPoint = "goosic_volume_gain")]
    private static extern double VolumeGainRaw(double position);

    [DllImport(Library, EntryPoint = "goosic_volume_position")]
    private static extern double VolumePositionRaw(double gain);

    [DllImport(Library, EntryPoint = "goosic_time_text")]
    private static extern IntPtr TimeTextRaw(double seconds);

    [DllImport(Library, EntryPoint = "goosic_track_has_finished")]
    [return: MarshalAs(UnmanagedType.I1)]
    private static extern bool HasFinishedRaw(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string state,
        double position,
        double duration,
        [MarshalAs(UnmanagedType.I1)] bool listenerPaused);

    [DllImport(Library, EntryPoint = "goosic_is_same_song")]
    [return: MarshalAs(UnmanagedType.I1)]
    private static extern bool IsSameSongRaw(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string requested,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string page);

    [DllImport(Library, EntryPoint = "goosic_seek_settle")]
    [return: MarshalAs(UnmanagedType.I1)]
    private static extern bool SeekSettleRaw(double confirmed, double target, double elapsedSeconds, out double shown);

    [DllImport(Library, EntryPoint = "goosic_string_free")]
    private static extern void StringFree(IntPtr value);

    /// <summary>The gain, 0 to 1, for a volume slider position from 0 to 1, on an audio taper.</summary>
    public static double VolumeGain(double position) => VolumeGainRaw(position);

    /// <summary>The slider position, 0 to 1, that plays at a gain from 0 to 1.</summary>
    public static double VolumePosition(double gain) => VolumePositionRaw(gain);

    /// <summary><c>m:ss</c>, or <c>h:mm:ss</c> past an hour; nonsense reads <c>0:00</c>.</summary>
    public static string TimeText(double seconds)
    {
        var raw = TimeTextRaw(seconds);
        if (raw == IntPtr.Zero)
        {
            return "0:00";
        }

        try
        {
            return Marshal.PtrToStringUTF8(raw) ?? "0:00";
        }
        finally
        {
            StringFree(raw);
        }
    }

    /// <summary>
    /// Whether a report says the track has played through. With Automix off the page stops a
    /// moment short of the length and says "paused"; that is the end, unless the listener paused.
    /// </summary>
    public static bool HasFinished(string state, double position, double duration, bool listenerPaused) =>
        HasFinishedRaw(state, position, duration, listenerPaused);

    /// <summary>
    /// Whether the page's title names the song that was asked for, ignoring decoration such as
    /// "(Official Video)" and matching whole words only.
    /// </summary>
    public static bool IsSameSong(string? requestedTitle, string? pageTitle) =>
        requestedTitle is not null && pageTitle is not null && IsSameSongRaw(requestedTitle, pageTitle);

    /// <summary>
    /// Where the position line sits for a seek to <paramref name="target"/> asked
    /// <paramref name="elapsed"/> ago, when the player reports <paramref name="confirmed"/>, and
    /// whether that seek can be forgotten.
    /// </summary>
    public static (double Shown, bool Settled) SettleSeek(double confirmed, double target, TimeSpan elapsed)
    {
        var settled = SeekSettleRaw(confirmed, target, elapsed.TotalSeconds, out var shown);
        return (shown, settled);
    }
}
