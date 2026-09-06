import Foundation

/// The state needed to project Goosic's confirmed playback into the system media controls.
/// Keeping this value type independent from MediaPlayer makes the policy testable on every
/// platform and keeps the adapter from becoming another source of playback truth.
struct SystemMediaPlaybackSnapshot: Equatable {
    var track: GoosicTrack?
    var currentTime: Double
    var duration: Double
    var isPaused: Bool
    var owner: GoosicOwner
    var isAdvertisement: Bool
    var hasQueue: Bool
    var transition: PlaybackTransition
    var volume: Double
    var isMuted: Bool
    /// True only after a validated event for this exact owner/generation/video.
    var isReady: Bool

    init(
        track: GoosicTrack?,
        currentTime: Double,
        duration: Double,
        isPaused: Bool,
        owner: GoosicOwner,
        isAdvertisement: Bool,
        hasQueue: Bool,
        transition: PlaybackTransition,
        volume: Double,
        isMuted: Bool,
        isReady: Bool = false
    ) {
        self.track = track
        self.currentTime = currentTime
        self.duration = duration
        self.isPaused = isPaused
        self.owner = owner
        self.isAdvertisement = isAdvertisement
        self.hasQueue = hasQueue
        self.transition = transition
        self.volume = volume
        self.isMuted = isMuted
        self.isReady = isReady
    }
}

enum SystemMediaPlaybackState: Equatable {
    case stopped
    case paused
    case playing
}

struct SystemMediaNowPlayingProjection: Equatable {
    var title: String?
    var artist: String?
    var album: String?
    var artworkURL: URL?
    var elapsedTime: Double
    var duration: Double
    var playbackRate: Double
    var playbackState: SystemMediaPlaybackState
    var isActive: Bool

    static func make(from snapshot: SystemMediaPlaybackSnapshot) -> SystemMediaNowPlayingProjection {
        let active = snapshot.isReady && snapshot.owner != .none && snapshot.track != nil
        let safeDuration = snapshot.duration.isFinite && snapshot.duration > 0 ? snapshot.duration : 0
        let rawElapsed = snapshot.currentTime.isFinite ? max(0, snapshot.currentTime) : 0
        let elapsed = safeDuration > 0 ? min(rawElapsed, safeDuration) : rawElapsed
        // Advertisement status affects which commands are available, not the confirmed media
        // state. A playing ad is still reported as playing; play/pause remains model-governed.
        let playing = active && !snapshot.isPaused
        let track = active ? snapshot.track : nil

        return SystemMediaNowPlayingProjection(
            title: track?.title.nilIfEmpty,
            artist: track?.artist.nilIfEmpty,
            album: track?.album.nilIfEmpty,
            artworkURL: track?.thumbnail.flatMap(URL.init(string:)),
            elapsedTime: elapsed,
            duration: safeDuration,
            playbackRate: playing ? 1 : 0,
            playbackState: active ? (playing ? .playing : .paused) : .stopped,
            isActive: active
        )
    }
}

enum SystemMediaCommand: Hashable {
    case play
    case pause
    case togglePlayPause
    case next
    case previous
    case changePosition
    case stop
    case changeVolume
}

struct SystemMediaCommandAvailability: Equatable {
    var play: Bool
    var pause: Bool
    var togglePlayPause: Bool
    var next: Bool
    var previous: Bool
    var changePosition: Bool
    var stop: Bool
    var changeVolume: Bool

    subscript(command: SystemMediaCommand) -> Bool {
        switch command {
        case .play: return play
        case .pause: return pause
        case .togglePlayPause: return togglePlayPause
        case .next: return next
        case .previous: return previous
        case .changePosition: return changePosition
        case .stop: return stop
        case .changeVolume: return changeVolume
        }
    }

    static func make(from snapshot: SystemMediaPlaybackSnapshot) -> SystemMediaCommandAvailability {
        let active = snapshot.isReady && snapshot.owner != .none && snapshot.track != nil
        let ready = active && snapshot.transition == .idle
        let playing = active && !snapshot.isPaused
        let paused = active && snapshot.isPaused
        let contentCommands = ready && !snapshot.isAdvertisement

        return SystemMediaCommandAvailability(
            play: ready && paused,
            pause: ready && playing,
            togglePlayPause: ready,
            next: contentCommands && snapshot.hasQueue,
            previous: contentCommands && snapshot.hasQueue,
            changePosition: contentCommands && snapshot.duration.isFinite && snapshot.duration > 0,
            stop: ready,
            changeVolume: ready && !snapshot.isAdvertisement
        )
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
