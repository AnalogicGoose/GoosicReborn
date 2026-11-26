import Foundation
import GoosicShellSupportFFI

/// `AdPlaybackRecovery` in goosic-shell-support. An ad's stopped media can be reported after its
/// DOM marker disappears; wait for content before accepting another end, and resume content the
/// handoff left paused once.
struct AdPlaybackRecovery {
    private var state = GoosicAdRecovery(awaiting_content: false, resume_requested: false)

    var awaitingContent: Bool { state.awaiting_content }

    mutating func observe(advertisement: Bool, state sampleState: String, listenerPaused: Bool) -> Bool {
        goosic_ad_recovery_observe(&state, advertisement, sampleState, listenerPaused)
    }
}

/// When a play's natural end moves the queue: `EndGuard` in goosic-shell-support, which Windows
/// asks too. An end counts only for the requested play, after it was heard playing, never during
/// or straight after an advertisement, and once; content an advertisement left paused is resumed.
struct EndGuard {
    private var state = GoosicEndGuard(heard: false, handled: false, awaiting_content: false, resume_requested: false)

    /// Starts a new play, a repeat of the same song included.
    mutating func begin() {
        goosic_end_guard_begin(&state)
    }

    /// Feeds one accepted sample of the requested play.
    mutating func observe(
        advertisement: Bool, state sampleState: String, position: Double, duration: Double,
        listenerPaused: Bool
    ) -> (advance: Bool, resume: Bool) {
        var resume = false
        let advance = goosic_end_guard_observe(
            &state, true, advertisement, sampleState, position, duration, listenerPaused, &resume
        )
        return (advance, resume)
    }

    /// The page moved past the requested play by itself; true the first time only.
    mutating func pageEnded() -> Bool {
        goosic_end_guard_page_ended(&state, true)
    }
}

/// Whether a report says a track has played through.
///
/// Whether a track has played through is `has_finished` in `goosic-shell-support`, which Windows
/// asks too. This keeps only the once-per-video check around it.
enum TrackEnd {
    /// Whether a report says the track has played through, including the page's pause a moment
    /// short of the end when Automix is off -- unless the listener paused it.
    static func hasFinished(state: String, position: Double, duration: Double, listenerPaused: Bool) -> Bool {
        goosic_track_has_finished(state, position, duration, listenerPaused)
    }

}

/// What to make of the official page replacing the requested track with another video.
///
/// The rules are `page_swap` in `goosic-shell-support`, the ones Windows asks too.
enum PageSwap {
    /// Whether a sample shows the requested track itself playing. Until one does, a different
    /// video under the same load cannot be the track's end.
    static func heardPlaying(state: String, duration: Double, isAdvertisement: Bool) -> Bool {
        goosic_track_heard_playing(state, duration, isAdvertisement)
    }

    /// Whether the page's title names the song that was asked for, ignoring decoration such as
    /// "(Official Video)", and matching whole words only.
    static func isSameSong(requested: String?, page: String?) -> Bool {
        guard let requested, let page else { return false }
        return goosic_is_same_song(requested, page)
    }

    /// How many times one choice may be replaced by another version of the same song: the page
    /// can swap a song for its video and, once set to prefer the song, the video back. A third
    /// swap is the page going round in circles. `page_swap::MAX_ADOPTIONS` in the crate decides;
    /// this mirrors it for the host's own bookkeeping.
    static let maxAdoptions = 2

    enum Decision: Equatable {
        /// Load the page's version in place of the requested one, still presented as the song
        /// the listener chose.
        case adopt(videoID: String)
        /// The page would not play the requested track: say so and move past it.
        case refused
        /// The report is about a track that is no longer the current one.
        case ignore
    }

    /// What the shell does with a switch that happened before the requested track was heard.
    ///
    /// `loadedVideoID` is what the host has loaded now, which may be a version adopted earlier;
    /// `currentTitle` is the title of the song the listener chose.
    static func decide(_ swap: OfficialPageSwap, loadedVideoID: String?, currentTitle: String?) -> Decision {
        let answer = swap.requestedVideoID.withCString { requested in
            swap.pageVideoID.withCString { page in
                swap.pageTitle.withCString { title in
                    (currentTitle ?? "").withCString { chosen in
                        if let loadedVideoID {
                            return loadedVideoID.withCString { loaded in
                                goosic_early_switch_counted(requested, page, title, UInt32(max(0, swap.adoptions)), loaded, chosen)
                            }
                        }
                        return goosic_early_switch_counted(requested, page, title, UInt32(max(0, swap.adoptions)), nil, chosen)
                    }
                }
            }
        }
        switch answer {
        case 2: return .adopt(videoID: swap.pageVideoID)
        case 1: return .refused
        default: return .ignore
        }
    }
}
