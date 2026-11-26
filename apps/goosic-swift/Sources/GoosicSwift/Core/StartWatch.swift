import Foundation
import GoosicShellSupportFFI

/// What to do about a requested track that has not been heard playing yet.
enum StartWatchAction: Equatable {
    /// Keep waiting; the page may still be loading.
    case wait
    /// Ask the page to play, once, as the listener's click already did.
    case nudge
    /// The page will not play it: say so and let the queue move on.
    case giveUp
    /// Nothing more to watch for.
    case done
}

/// `start_watch` in goosic-shell-support, which Windows asks too.
///
/// The official page sometimes loads a track and never starts it -- a `play()` the page
/// interrupted, a swapped-in version with an empty media element, something only the page can
/// show -- and reports "paused" for all of them, as it does for a track the listener paused.
/// Without a deadline the player sat silent and the click looked like it did nothing.
enum StartWatch {
    static func decide(
        sinceLoaded: TimeInterval, heardPlaying: Bool, listenerTookOver: Bool, nudged: Bool
    ) -> StartWatchAction {
        switch goosic_start_watch(sinceLoaded, heardPlaying, listenerTookOver, nudged) {
        case 1: return .nudge
        case 2: return .giveUp
        case 3: return .done
        default: return .wait
        }
    }
}

/// `shows_loading` in goosic-shell-support, which Windows asks too: a requested track not yet
/// heard, or one the page reports buffering, shows as loading unless the listener paused.
enum PlayerLoading {
    static func shows(awaitingStart: Bool, buffering: Bool, listenerPaused: Bool) -> Bool {
        goosic_player_shows_loading(awaitingStart, buffering, listenerPaused)
    }
}

/// What the shared play script concluded; see `OfficialBridge.listenerPlayScript`.
enum PlayOutcome: Equatable {
    case played
    case noMedia
    /// A newer play or a pause was made while this one was still trying.
    case superseded
    /// The script has not finished yet.
    case pending
    /// The media element refused, with the name of the error the page gave.
    case failed(String)

    init(_ answer: String) {
        switch answer {
        case "played": self = .played
        case "no-media": self = .noMedia
        case "superseded": self = .superseded
        case "pending": self = .pending
        default:
            let prefix = "failed:"
            let name = answer.hasPrefix(prefix) ? String(answer.dropFirst(prefix.count)) : ""
            self = .failed(name.isEmpty ? "Error" : name)
        }
    }
}
