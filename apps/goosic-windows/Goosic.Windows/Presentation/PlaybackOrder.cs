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

    /// <summary>How many times one choice may be replaced by another version of the same song.</summary>
    /// <remarks>
    /// One was not enough. The page follows its Song/Video switch: a song can be swapped for its
    /// music video, and the video swapped back once the page is set to prefer the song. Refusing
    /// that second swap skipped songs the page would play. A third is the page going round in
    /// circles. The crate's <c>page_swap::MAX_ADOPTIONS</c> and the Mac's
    /// <c>PageSwap.maxAdoptions</c>.
    /// </remarks>
    public const int MaxAdoptions = 2;

    /// <summary>Whether a choice already replaced <paramref name="adoptions"/> times may be replaced again.</summary>
    public static bool MayAdopt(int adoptions) => adoptions < MaxAdoptions;

    /// <summary>Sets the official page's Song/Video switch to Song, reporting what it found.</summary>
    /// <remarks>
    /// A song with both an audio version and a music video plays whichever the switch prefers,
    /// and the page swaps the requested id for the other version before it starts. Goosic plays
    /// the song, as the web app does when Song is chosen. The switch is disabled when a track has
    /// one version, and a video with no song version plays as a video. The copy of the crate's
    /// <c>bridge::SONG_VERSION_SCRIPT</c>; it answers absent, song, switched, pending or
    /// "no song version".
    /// </remarks>
    public const string SongVersionScript = """
        (() => {
          const toggle = document.querySelector('ytmusic-av-toggle');
          const song = toggle?.querySelector('.song-button');
          if (!song) return 'absent';
          if (song.getAttribute('aria-pressed') === 'true') return 'song';
          if (toggle.hasAttribute('toggle-disabled')) return 'no song version';
          song.click();
          return song.getAttribute('aria-pressed') === 'true' ? 'switched' : 'pending';
        })()
        """;

    /// <summary>Whether a host stops running <see cref="SongVersionScript"/> after this answer.</summary>
    public static bool SongVersionSettled(string? answer) =>
        answer is "song" or "switched" or "no song version";

    /// <summary>How many times, half a second apart, a host runs <see cref="SongVersionScript"/> after a load.</summary>
    public const int SongVersionAttempts = 20;

    /// <summary>
    /// Whether the page's title names the song that was asked for. YouTube Music swaps a song for
    /// its other version before it starts, which is the same song under another id, not the page
    /// moving on. The rule is <c>is_same_song</c> in <c>goosic-shell-support</c>.
    /// </summary>
    public static bool IsSameSong(string? requestedTitle, string? pageTitle) =>
        PlaybackRules.IsSameSong(requestedTitle, pageTitle);
}
