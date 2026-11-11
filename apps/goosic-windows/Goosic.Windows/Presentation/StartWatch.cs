using System;

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
/// </remarks>
internal static class StartWatch
{
    /// <summary>How long a loaded page may sit paused before it is asked, once, to play.</summary>
    internal static readonly TimeSpan NudgeAfter = TimeSpan.FromSeconds(4);

    /// <summary>How long a loaded page may go without playing before the track is given up on.</summary>
    internal static readonly TimeSpan GiveUpAfter = TimeSpan.FromSeconds(20);

    /// <param name="sinceLoaded">Time since the page finished loading for this request.</param>
    /// <param name="heardPlaying">Whether the page has reported playing, content or advertisement.</param>
    /// <param name="listenerTookOver">Whether the listener pressed Play or Pause since the load.</param>
    /// <param name="nudged">Whether the page has already been asked to play.</param>
    internal static StartWatchAction Decide(TimeSpan sinceLoaded, bool heardPlaying, bool listenerTookOver, bool nudged)
    {
        // A pause the listener chose is theirs to end, and a track that has played has started.
        if (heardPlaying || listenerTookOver)
        {
            return StartWatchAction.Done;
        }

        if (sinceLoaded >= GiveUpAfter)
        {
            return StartWatchAction.GiveUp;
        }

        return !nudged && sinceLoaded >= NudgeAfter ? StartWatchAction.Nudge : StartWatchAction.Wait;
    }
}
