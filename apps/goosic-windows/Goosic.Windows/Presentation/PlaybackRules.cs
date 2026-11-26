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

    [DllImport(Library, EntryPoint = "goosic_player_shows_loading")]
    [return: MarshalAs(UnmanagedType.I1)]
    private static extern bool ShowsLoadingRaw(
        [MarshalAs(UnmanagedType.I1)] bool awaitingStart,
        [MarshalAs(UnmanagedType.I1)] bool buffering,
        [MarshalAs(UnmanagedType.I1)] bool listenerPaused);

    [DllImport(Library, EntryPoint = "goosic_string_free")]
    private static extern void StringFree(IntPtr value);

    [DllImport(Library, EntryPoint = "goosic_cover_aspect")]
    private static extern double CoverAspectRaw(double pixelWidth, double pixelHeight);

    [DllImport(Library, EntryPoint = "goosic_cover_fit")]
    private static extern void CoverFitRaw(double aspect, double maxWidth, double maxHeight, out double width, out double height);

    [DllImport(Library, EntryPoint = "goosic_hero_height")]
    private static extern double HeroHeightRaw(double width);

    [DllImport(Library, EntryPoint = "goosic_hero_tint")]
    [return: MarshalAs(UnmanagedType.I1)]
    private static extern bool HeroTintRaw(ref byte rgba, nuint length, nuint width, nuint height, [Out] byte[] rgb);

    [DllImport(Library, EntryPoint = "goosic_artwork_is_allowed")]
    [return: MarshalAs(UnmanagedType.I1)]
    private static extern bool ArtworkIsAllowedRaw([MarshalAs(UnmanagedType.LPUTF8Str)] string url);

    [DllImport(Library, EntryPoint = "goosic_artwork_cache_key")]
    private static extern IntPtr ArtworkCacheKeyRaw([MarshalAs(UnmanagedType.LPUTF8Str)] string remote);

    [DllImport(Library, EntryPoint = "goosic_artwork_max_bytes")]
    private static extern ulong ArtworkMaxBytesRaw();

    [DllImport(Library, EntryPoint = "goosic_end_guard_begin")]
    private static extern void EndGuardBeginRaw(ref EndGuardState guard);

    [DllImport(Library, EntryPoint = "goosic_end_guard_observe")]
    [return: MarshalAs(UnmanagedType.I1)]
    private static extern bool EndGuardObserveRaw(
        ref EndGuardState guard,
        [MarshalAs(UnmanagedType.I1)] bool isCurrent,
        [MarshalAs(UnmanagedType.I1)] bool advertisement,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string state,
        double position,
        double duration,
        [MarshalAs(UnmanagedType.I1)] bool listenerPaused,
        [MarshalAs(UnmanagedType.I1)] out bool resume);

    [DllImport(Library, EntryPoint = "goosic_end_guard_page_ended")]
    [return: MarshalAs(UnmanagedType.I1)]
    private static extern bool EndGuardPageEndedRaw(ref EndGuardState guard, [MarshalAs(UnmanagedType.I1)] bool isCurrent);

    /// <summary>What the shared end guard remembers for the current play.</summary>
    [StructLayout(LayoutKind.Sequential)]
    public struct EndGuardState
    {
        [MarshalAs(UnmanagedType.I1)] public bool Heard;
        [MarshalAs(UnmanagedType.I1)] public bool Handled;
        [MarshalAs(UnmanagedType.I1)] public bool AwaitingContent;
        [MarshalAs(UnmanagedType.I1)] public bool ResumeRequested;
    }

    /// <summary>Starts a new play, a repeat of the same song included.</summary>
    public static void EndGuardBegin(ref EndGuardState guard) => EndGuardBeginRaw(ref guard);

    /// <summary>
    /// Feeds one accepted sample: whether the queue should move on, and whether to ask the page to
    /// play because an advertisement left content paused.
    /// </summary>
    public static (bool Advance, bool Resume) EndGuardObserve(ref EndGuardState guard, bool isCurrent,
        bool advertisement, string state, double position, double duration, bool listenerPaused)
    {
        var advance = EndGuardObserveRaw(ref guard, isCurrent, advertisement, state, position, duration,
            listenerPaused, out var resume);
        return (advance, resume);
    }

    /// <summary>The page moved past the requested play by itself; true the first time only.</summary>
    public static bool EndGuardPageEnded(ref EndGuardState guard, bool isCurrent) =>
        EndGuardPageEndedRaw(ref guard, isCurrent);

    [DllImport(Library, EntryPoint = "goosic_media_project")]
    private static extern void MediaProjectRaw(in MediaInput input, out MediaProjection output);

    /// <summary>The confirmed state the system media controls are computed from.</summary>
    [StructLayout(LayoutKind.Sequential)]
    public struct MediaInput
    {
        [MarshalAs(UnmanagedType.I1)] public bool HasTrack;
        [MarshalAs(UnmanagedType.I1)] public bool IsReady;
        [MarshalAs(UnmanagedType.I1)] public bool HasOwner;
        public double CurrentTime;
        public double Duration;
        [MarshalAs(UnmanagedType.I1)] public bool IsPaused;
        [MarshalAs(UnmanagedType.I1)] public bool IsAdvertisement;
        [MarshalAs(UnmanagedType.I1)] public bool HasQueue;
        [MarshalAs(UnmanagedType.I1)] public bool IsIdle;
    }

    /// <summary>What the system shows and which of its buttons may work. State: 0 stopped, 1 paused, 2 playing.</summary>
    [StructLayout(LayoutKind.Sequential)]
    public struct MediaProjection
    {
        [MarshalAs(UnmanagedType.I1)] public bool IsActive;
        public byte PlaybackState;
        public double ElapsedTime;
        public double Duration;
        public double PlaybackRate;
        [MarshalAs(UnmanagedType.I1)] public bool Play;
        [MarshalAs(UnmanagedType.I1)] public bool Pause;
        [MarshalAs(UnmanagedType.I1)] public bool TogglePlayPause;
        [MarshalAs(UnmanagedType.I1)] public bool Next;
        [MarshalAs(UnmanagedType.I1)] public bool Previous;
        [MarshalAs(UnmanagedType.I1)] public bool ChangePosition;
        [MarshalAs(UnmanagedType.I1)] public bool Stop;
        [MarshalAs(UnmanagedType.I1)] public bool ChangeVolume;
    }

    /// <summary>The playing cover's width-to-height ratio, held between 9:16 and 16:9; square when unknown.</summary>
    public static double CoverAspect(double pixelWidth, double pixelHeight) => CoverAspectRaw(pixelWidth, pixelHeight);

    /// <summary>The size a cover of <paramref name="aspect"/> takes inside a slot without cropping.</summary>
    public static (double Width, double Height) CoverFit(double aspect, double maxWidth, double maxHeight)
    {
        CoverFitRaw(aspect, maxWidth, maxHeight, out var width, out var height);
        return (width, height);
    }

    /// <summary>An artist hero's height for a page <paramref name="width"/> wide.</summary>
    public static double HeroHeight(double width) => HeroHeightRaw(width);

    /// <summary>The colour an artist page takes from its banner, or <c>null</c> for a bitmap too small for its size.</summary>
    public static byte[]? HeroTint(ReadOnlySpan<byte> rgba, int width, int height)
    {
        if (width <= 0 || height <= 0 || rgba.IsEmpty)
        {
            return null;
        }

        var rgb = new byte[3];
        ref var first = ref System.Runtime.InteropServices.MemoryMarshal.GetReference(rgba);
        return HeroTintRaw(ref first, (nuint)rgba.Length, (nuint)width, (nuint)height, rgb) ? rgb : null;
    }

    /// <summary>Whether artwork may be fetched from <paramref name="url"/>: HTTPS from YouTube Music's image hosts only.</summary>
    public static bool ArtworkIsAllowed(string url) => ArtworkIsAllowedRaw(url);

    /// <summary>The file name a cover from <paramref name="remote"/> is cached under, 32 hex digits.</summary>
    public static string ArtworkCacheKey(string remote) => Take(ArtworkCacheKeyRaw(remote), "");

    /// <summary>The largest artwork download accepted, in bytes.</summary>
    public static long ArtworkMaxBytes { get; } = checked((long)ArtworkMaxBytesRaw());

    /// <summary>Projects confirmed state onto the system media controls.</summary>
    public static MediaProjection ProjectMedia(MediaInput input)
    {
        MediaProjectRaw(in input, out var output);
        return output;
    }

    private static string Take(IntPtr raw, string fallback)
    {
        if (raw == IntPtr.Zero)
        {
            return fallback;
        }

        try
        {
            return Marshal.PtrToStringUTF8(raw) ?? fallback;
        }
        finally
        {
            StringFree(raw);
        }
    }

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

    /// <summary>
    /// Whether the player shows that it is loading instead of Play or Pause: a requested track
    /// not yet heard, or one the page reports buffering, unless the listener paused.
    /// </summary>
    public static bool ShowsLoading(bool awaitingStart, bool buffering, bool listenerPaused) =>
        ShowsLoadingRaw(awaitingStart, buffering, listenerPaused);
}
