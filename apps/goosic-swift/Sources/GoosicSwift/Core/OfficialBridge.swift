import Foundation
import GoosicShellSupportFFI

/// A validated event emitted by the one official YouTube Music renderer.
struct OfficialPlaybackEvent {
    let generation: UInt64
    let videoID: String
    let sequence: UInt64
    let state: String
    let currentTime: Double
    let duration: Double
    let isAdvertisement: Bool
    let volume: Double
    let isMuted: Bool
}

/// The official page replaced the requested track with another video before any of it played.
struct OfficialPageSwap: Equatable {
    let requestedVideoID: String
    let pageVideoID: String
    /// The page's own title for the video it switched to; empty when it could not be read.
    let pageTitle: String
    /// Whether the requested load was the listener's own choice rather than a version already
    /// loaded in its place.
    let mayAdopt: Bool
}

struct OfficialPlaybackProfile: Equatable {
    /// Guest storage is intentionally stable. Account profiles will supply their own stable UUID
    /// once account isolation is connected; cookies are never exported from this store.
    let identifier: UUID

    static let guest = OfficialPlaybackProfile(
        identifier: UUID(uuidString: "8E4CA2CD-373A-46E3-A5B0-9A2A7B3B5084")!
    )
}

/// The half of the official playback bridge that is not tied to a WebKit implementation.
///
/// Every script, limit and check here is answered by `goosic-shell-support`'s `bridge` module,
/// which the Linux and Windows hosts ask too, so what counts as a trustworthy event cannot depend
/// on which engine is running. A Swift copy of `rejection_reason` would be a second answer to
/// "may this play".
enum OfficialBridge {
    /// The only origin this bridge speaks to.
    static let allowedHost = sharedText(goosic_bridge_allowed_host())
    /// The script message handler's name, on every platform.
    static let name = sharedText(goosic_bridge_handler_name())
    /// A bridge message larger than this is refused before it is parsed.
    static let maxBodyBytes = Int(goosic_bridge_max_body_bytes())
    /// YouTube Music refuses to run its player under a bare engine user agent and shows
    /// "not optimized for your browser" instead. Naming a Safari version makes the agent a
    /// complete Safari string, which is what these engines actually are.
    static let safariUserAgentSuffix = sharedText(goosic_bridge_safari_user_agent_suffix())

    /// The wire shape a page observer posts across the bridge.
    struct Event: Codable {
        let version: Int
        let token: String
        let generation: UInt64
        let videoID: String
        let sequence: UInt64
        let state: String
        let currentTime: Double
        let duration: Double
        let isAdvertisement: Bool
        let volume: Double
        let muted: Bool
        /// The media is neither paused nor ended but cannot advance: it is seeking or short of
        /// data. `nil` from an older observer. `state` keeps its meaning.
        let buffering: Bool?
        /// `Date.now()` in the page when `currentTime` was read, in epoch milliseconds. `nil`
        /// from an older observer.
        let sampledAt: Double?

        enum CodingKeys: String, CodingKey {
            case version, token, generation, sequence, state, currentTime, duration, isAdvertisement
            case volume, muted, buffering, sampledAt
            case videoID = "videoId"
        }
    }

    /// Why a message from the page is not trustworthy, or `nil` when it is.
    ///
    /// A `nil` expectation means nothing is loaded, which every check treats as a mismatch.
    static func rejectionReason(
        body: Data,
        expectedToken: String?,
        expectedGeneration: UInt64?,
        expectedVideoID: String?,
        lastSequence: UInt64
    ) -> String? {
        guard let text = String(data: body, encoding: .utf8) else {
            return "the message body was not valid UTF-8"
        }
        // Rust takes the generation as a plain number. With none expected nothing is loaded, so
        // the token is withheld too and the event is refused as coming from another document.
        let token = expectedGeneration == nil ? "" : (expectedToken ?? "")
        let raw = goosic_bridge_validate_event(
            text, token, expectedGeneration ?? 0, expectedVideoID ?? "", lastSequence
        )
        let verdict = sharedText(raw)
        guard let data = verdict.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return "the shared bridge rule gave no verdict"
        }
        if object["accepted"] as? Bool == true { return nil }
        return object["reason"] as? String ?? "the shared bridge rule refused it"
    }

    /// The same check for an event already decoded.
    static func rejectionReason(
        for event: Event,
        expectedToken: String?,
        expectedGeneration: UInt64?,
        expectedVideoID: String?,
        lastSequence: UInt64
    ) -> String? {
        guard let body = try? JSONEncoder().encode(event) else {
            return "it reported an impossible position or duration"
        }
        return rejectionReason(
            body: body, expectedToken: expectedToken, expectedGeneration: expectedGeneration,
            expectedVideoID: expectedVideoID, lastSequence: lastSequence
        )
    }

    static func isValidVideoID(_ videoID: String) -> Bool {
        goosic_bridge_is_valid_video_id(videoID)
    }

    /// Chooses the media element that is actually playing, rather than whichever audio or video
    /// tag happened to be first in the page. YouTube Music can retain a paused element while it
    /// swaps in the one carrying the next track or an advertisement.
    static let activeMediaElementScript = sharedText(goosic_bridge_active_media_element_script())

    /// Turns the official page's Automix off, answering `absent`, `off`, `turned off` or
    /// `still on`.
    ///
    /// With Automix on, YouTube Music fades into a track of its own choosing some seconds before
    /// the requested one ends. The observer sees another video, the page is paused, and the queue
    /// moves on -- having cut the song's last seconds. With it off, the page plays the track
    /// through and reports its end (see `TrackEnd`). The switch sits in the page's Up Next tab,
    /// which renders after the player, so a host runs this until it answers something other than
    /// `absent`, up to `automixAttempts` times half a second apart.
    static let automixOffScript = sharedText(goosic_bridge_automix_off_script())

    static let automixAttempts = Int(goosic_bridge_automix_attempts())

    /// The per-load page observer.
    ///
    /// Identity is injected rather than read from the URL: the official app rewrites its own
    /// location and drops query items it does not recognize. The video id is still read live so
    /// that a client-side navigation to a different track is reported honestly and rejected.
    static func observerScript(token: String, generation: UInt64, videoID: String) -> String {
        sharedText(goosic_bridge_observer_script(token, generation, videoID))
    }

    /// Clears the page's Media Session metadata and action handlers so macOS has one owner:
    /// `SystemMediaControls`. The script contains no Goosic state, URLs, or credentials.
    static let mediaSessionGuardScript = sharedText(goosic_bridge_media_session_guard_script())
}
