namespace Goosic.Windows.Presentation;

/// <summary>
/// The rules for when the page's track has ended, kept apart from the player so every case can
/// be tested. Which song plays next is not here: that is <c>QueueEngine</c> in
/// <c>goosic-shell-support</c>, reached through <c>SharedQueue</c>.
/// </summary>
public static class PlaybackOrder
{
    /// <summary>
    /// Whether a report says the track has finished; the rule is <c>has_finished</c> in
    /// <c>goosic-shell-support</c>.
    /// </summary>
    public static bool IsFinished(string state, double position, double duration, bool listenerPaused) =>
        PlaybackRules.HasFinished(state, position, duration, listenerPaused);

    /// <summary>
    /// Whether the page's title names the song that was asked for. YouTube Music swaps a song for
    /// its other version before it starts, which is the same song under another id, not the page
    /// moving on. The rule is <c>is_same_song</c> in <c>goosic-shell-support</c>.
    /// </summary>
    public static bool IsSameSong(string? requestedTitle, string? pageTitle) =>
        PlaybackRules.IsSameSong(requestedTitle, pageTitle);
}
