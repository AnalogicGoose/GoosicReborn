import Foundation

/// Swift copy of `AdPlaybackRecovery` in goosic-shell-support. An ad's stopped media can be
/// reported after its DOM marker disappears; wait for content before accepting another end.
struct AdPlaybackRecovery {
    private(set) var awaitingContent = false
    private var resumeRequested = false

    mutating func observe(advertisement: Bool, state: String, listenerPaused: Bool) -> Bool {
        if advertisement {
            awaitingContent = true
            resumeRequested = false
            return false
        }
        if state == "playing" {
            awaitingContent = false
            resumeRequested = false
            return false
        }
        if awaitingContent && state == "paused" && !listenerPaused && !resumeRequested {
            resumeRequested = true
            return true
        }
        return false
    }
}

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

/// What to make of the official page replacing the requested track with another video.
///
/// The Swift copy of `page_swap` in `goosic-shell-support`, and of the Windows shell's
/// `PlaybackOrder.IsSameSong` and the early-switch rule in its `OfficialPlaybackHost`. The three
/// are held to the same cases (`PageSwapTests`, the crate's tests and `SameSongTests`), and a
/// change to one is a change to all of them.
enum PageSwap {
    /// Whether a sample shows the requested track itself playing.
    ///
    /// Until one does, a different video under the same load cannot be the track's end: either
    /// YouTube Music swapped in another version of the same song before starting it, or it gave
    /// up on a track it would not play. Treating either as the end skipped the song without a
    /// word and started the next row of the list instead.
    static func heardPlaying(state: String, duration: Double, isAdvertisement: Bool) -> Bool {
        state == "playing" && !isAdvertisement && duration.isFinite && duration > 0
    }

    /// Whether the page's title names the song that was asked for.
    ///
    /// YouTube Music swaps a song for its other version -- the audio track for the music video,
    /// or back -- before it starts. That is the same song under another id. The titles differ
    /// only by decoration such as "(Official Video)" or "(feat. …)", so both are reduced to their
    /// words, and one must contain the other as whole words, so "Down" is not "Downtown".
    static func isSameSong(requested: String?, page: String?) -> Bool {
        let requested = songWords(requested)
        let page = songWords(page)
        guard !requested.isEmpty, !page.isEmpty else { return false }
        return " \(page) ".contains(" \(requested) ") || " \(requested) ".contains(" \(page) ")
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
        guard swap.requestedVideoID == loadedVideoID else { return .ignore }
        if swap.mayAdopt,
           swap.pageVideoID != swap.requestedVideoID,
           isSameSong(requested: currentTitle, page: swap.pageTitle) {
            return .adopt(videoID: swap.pageVideoID)
        }
        return .refused
    }

    private static func songWords(_ title: String?) -> String {
        guard let title, !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return ""
        }
        // Drop bracketed decoration: (Official Video), [Lyrics], (feat. …) and the like.
        let text = title.replacingOccurrences(
            of: #"[\(\[][^\)\]]*[\)\]]"#, with: " ", options: .regularExpression)
        var words = ""
        for character in text.lowercased() {
            // An apostrophe joins a word rather than splitting it: "don't" and "dont" are one word.
            if character == "'" || character == "\u{2019}" { continue }
            words.append(character.isLetter || character.isNumber ? character : " ")
        }
        return words.split(separator: " ").joined(separator: " ")
    }
}
