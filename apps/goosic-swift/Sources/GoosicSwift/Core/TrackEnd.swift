import Foundation

/// When a finished track should move the queue on.
///
/// The Swift copy of `has_finished` and `should_advance_after_end` in `goosic-shell-support`'s
/// `playback` module. Until the Swift shell links that crate, the two are held to the same cases
/// (`TrackEndTests` and the crate's tests), and a change to one is a change to both.
enum TrackEnd {
    /// How close to its length a stopped track has to be for the stop to count as its end.
    static let tolerance: Double = 1.5

    /// Whether a report says the track has played through.
    ///
    /// A player normally says `ended`. With YouTube Music's Automix off -- see
    /// `OfficialBridge.automixOffScript` -- the official page instead stops a fraction of a second
    /// short of the length and says `paused`; a shell waiting for `ended` sat silent after every
    /// song. A pause that close to the end is the end, unless the listener paused it.
    static func hasFinished(state: String, position: Double, duration: Double, listenerPaused: Bool) -> Bool {
        if state == "ended" { return true }
        guard state == "paused", !listenerPaused,
              duration.isFinite, duration > 0, position.isFinite else {
            return false
        }
        return position >= duration - tolerance
    }

    /// Whether a sample means the queue should move on.
    ///
    /// Advertisements end too and never advance the queue, and a player reports its end
    /// repeatedly, so only the first report for a video counts.
    static func shouldAdvance(
        state: String,
        isAdvertisement: Bool,
        videoID: String,
        position: Double,
        duration: Double,
        lastEndedVideoID: String?,
        listenerPaused: Bool
    ) -> Bool {
        !isAdvertisement
            && lastEndedVideoID != videoID
            && hasFinished(state: state, position: position, duration: duration, listenerPaused: listenerPaused)
    }
}
