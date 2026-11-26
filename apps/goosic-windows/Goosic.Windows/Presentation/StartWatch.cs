using System;
using System.Runtime.InteropServices;

namespace Goosic.Windows.Presentation;

/// <summary>What to do about a requested track that has not been heard playing yet.</summary>
internal enum StartWatchAction
{
    /// <summary>Keep waiting; the page may still be loading.</summary>
    Wait,

    /// <summary>Ask the page to play, once, as the listener's click already did.</summary>
    Nudge,

    /// <summary>The page will not play it: say so and let the queue move on.</summary>
    GiveUp,

    /// <summary>Nothing more to watch for.</summary>
    Done,
}

/// <summary>
/// Decides how long a requested track may sit silent before Goosic says it cannot be played.
/// </summary>
/// <remarks>
/// The official page sometimes loads a track and never starts it: it swaps in another version
/// that stays at an empty media element, or stops behind something only the page can show. The
/// page reports "paused" for all of those, exactly as it does for a track the listener paused,
/// so without a deadline the player sat on an empty track and the click looked like it did
/// nothing. A page that is merely slow reports "playing" while it buffers, which ends the watch.
/// The rule lives in goosic-shell-support, so the Mac, which had no deadline at all, follows the
/// same one.
/// </remarks>
internal static class StartWatch
{
    [DllImport("goosic_shell_support_ffi", EntryPoint = "goosic_start_watch")]
    private static extern byte DecideRaw(
        double sinceLoadedSeconds,
        [MarshalAs(UnmanagedType.I1)] bool heardPlaying,
        [MarshalAs(UnmanagedType.I1)] bool listenerTookOver,
        [MarshalAs(UnmanagedType.I1)] bool nudged);

    /// <summary>
    /// <c>start_watch</c> in goosic-shell-support, which the Mac asks too: wait, ask the page to
    /// play once after a few seconds, give up after twenty.
    /// </summary>
    /// <param name="sinceLoaded">Time since the page finished loading for this request.</param>
    /// <param name="heardPlaying">Whether the page has reported playing, content or advertisement.</param>
    /// <param name="listenerTookOver">Whether the listener pressed Play or Pause since the load.</param>
    /// <param name="nudged">Whether the page has already been asked to play.</param>
    internal static StartWatchAction Decide(TimeSpan sinceLoaded, bool heardPlaying, bool listenerTookOver, bool nudged) =>
        DecideRaw(sinceLoaded.TotalSeconds, heardPlaying, listenerTookOver, nudged) switch
        {
            1 => StartWatchAction.Nudge,
            2 => StartWatchAction.GiveUp,
            3 => StartWatchAction.Done,
            _ => StartWatchAction.Wait,
        };
}
