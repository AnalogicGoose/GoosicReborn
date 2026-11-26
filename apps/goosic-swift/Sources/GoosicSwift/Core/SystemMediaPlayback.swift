import Foundation
import GoosicShellSupportFFI

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

    /// What the system shows. When it is active, and the clamped clock, are
    /// `NowPlaying::from_snapshot` in `goosic-shell-support`; only the text is filled in here.
    static func make(from snapshot: SystemMediaPlaybackSnapshot) -> SystemMediaNowPlayingProjection {
        let shown = snapshot.projected
        let track = shown.is_active ? snapshot.track : nil
        return SystemMediaNowPlayingProjection(
            title: track?.title.nilIfEmpty,
            artist: track?.artist.nilIfEmpty,
            album: track?.album.nilIfEmpty,
            artworkURL: track?.thumbnail.flatMap(URL.init(string:)),
            elapsedTime: shown.elapsed_time,
            duration: shown.duration,
            playbackRate: shown.playback_rate,
            playbackState: shown.playback_state == 2 ? .playing : shown.playback_state == 1 ? .paused : .stopped,
            isActive: shown.is_active
        )
    }
}

extension SystemMediaPlaybackSnapshot {
    /// The projection and command availability Rust computes from this confirmed state.
    var projected: GoosicMediaProjection {
        var input = GoosicMediaInput(
            has_track: track != nil,
            is_ready: isReady,
            has_owner: owner != .none,
            current_time: currentTime,
            duration: duration,
            is_paused: isPaused,
            is_advertisement: isAdvertisement,
            has_queue: hasQueue,
            is_idle: transition == .idle
        )
        var output = GoosicMediaProjection()
        goosic_media_project(&input, &output)
        return output
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

    /// Which buttons may work: `CommandAvailability::from_snapshot` in `goosic-shell-support`.
    static func make(from snapshot: SystemMediaPlaybackSnapshot) -> SystemMediaCommandAvailability {
        let allowed = snapshot.projected
        return SystemMediaCommandAvailability(
            play: allowed.play,
            pause: allowed.pause,
            togglePlayPause: allowed.toggle_play_pause,
            next: allowed.next,
            previous: allowed.previous,
            changePosition: allowed.change_position,
            stop: allowed.stop,
            changeVolume: allowed.change_volume
        )
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
