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

/// When a finished track should move the queue on.
///
/// Whether a track has played through is `has_finished` in `goosic-shell-support`, which Windows
/// asks too. This keeps only the once-per-video check around it.
enum TrackEnd {
    /// Whether a report says the track has played through, including the page's pause a moment
    /// short of the end when Automix is off -- unless the listener paused it.
    static func hasFinished(state: String, position: Double, duration: Double, listenerPaused: Bool) -> Bool {
        goosic_track_has_finished(state, position, duration, listenerPaused)
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
                                goosic_early_switch(requested, page, title, swap.mayAdopt, loaded, chosen)
                            }
                        }
                        return goosic_early_switch(requested, page, title, swap.mayAdopt, nil, chosen)
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
