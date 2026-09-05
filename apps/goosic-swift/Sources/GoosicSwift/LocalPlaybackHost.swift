#if os(macOS)
import AVFoundation
import Foundation

/// A confirmed event from the local decoded-file renderer.
struct LocalPlaybackEvent {
    let generation: UInt64
    let videoID: String
    let sequence: UInt64
    let state: String
    let currentTime: Double
    let duration: Double
    let volume: Double
    let isMuted: Bool
}

/// The sole local-file audio renderer. It accepts only the decoded cache path returned by Rust;
/// it never opens a WebM source, starts a network request, or reads account state.
@MainActor
final class LocalPlaybackHost: NSObject, AVAudioPlayerDelegate {
    private var player: AVAudioPlayer?
    private var timer: Timer?
    private var generation: UInt64?
    private var videoID: String?
    private var sequence: UInt64 = 0
    private var preferredVolume = 1.0
    private var muted = false

    var onEvent: ((LocalPlaybackEvent) -> Void)?
    var onStatus: ((String) -> Void)?

    var loadedVideoID: String? { videoID }
    var isLoaded: Bool { player != nil }

    /// Opens a decoded cache file. The caller must already hold the Rust local-file lease.
    func prepare(localFile: String, videoID: String, generation: UInt64) throws {
        let url = URL(fileURLWithPath: localFile, isDirectory: false)
        guard url.isFileURL, FileManager.default.fileExists(atPath: url.path) else {
            throw NSError(domain: "GoosicPlayback", code: 2, userInfo: [
                NSLocalizedDescriptionKey: "Rust returned a local audio path that is not present on disk.",
            ])
        }
        let previousGeneration = self.generation
        let previousSequence = self.sequence
        stop()
        let next = try AVAudioPlayer(contentsOf: url)
        guard next.prepareToPlay() else {
            throw NSError(domain: "GoosicPlayback", code: 3, userInfo: [
                NSLocalizedDescriptionKey: "AVFoundation could not prepare the decoded local audio file.",
            ])
        }
        next.delegate = self
        next.volume = muted ? 0 : Float(preferredVolume)
        player = next
        self.videoID = videoID
        self.generation = generation
        // A same-owner track replacement keeps the same Rust generation, so its samples must
        // continue after the previous track. A newly claimed generation starts at sequence one.
        sequence = previousGeneration == generation ? previousSequence : 0
    }

    @discardableResult
    func play() -> Bool {
        guard let player, let generation, let videoID else {
            onStatus?("Prepare a decoded local file before pressing play.")
            return false
        }
        guard player.play() else {
            onStatus?("AVFoundation rejected the local audio play request.")
            return false
        }
        scheduleTimer()
        emit(state: "playing", generation: generation, videoID: videoID, player: player)
        return true
    }

    func pause() {
        guard let player, let generation, let videoID else { return }
        player.pause()
        stopTimer()
        emit(state: "paused", generation: generation, videoID: videoID, player: player)
    }

    /// Stops local audio synchronously before its Rust lease is released or changed.
    func stop() {
        player?.delegate = nil
        player?.stop()
        stopTimer()
        player = nil
        generation = nil
        videoID = nil
        sequence = 0
    }

    /// Stops the current file but retains the lease identity and sample counter for an immediate
    /// same-owner track replacement.
    func stopForReplacement() {
        player?.delegate = nil
        player?.stop()
        stopTimer()
        player = nil
    }

    func seek(to seconds: Double) {
        guard let player, let generation, let videoID,
              seconds.isFinite, seconds >= 0, player.duration > 0 else { return }
        player.currentTime = min(seconds, player.duration)
        emit(state: player.isPlaying ? "playing" : "paused", generation: generation, videoID: videoID, player: player)
    }

    func setVolume(_ volume: Double) {
        guard volume.isFinite else { return }
        preferredVolume = min(max(volume, 0), 1)
        if !muted { player?.volume = Float(preferredVolume) }
    }

    func setMuted(_ muted: Bool) {
        self.muted = muted
        player?.volume = muted ? 0 : Float(preferredVolume)
    }

    private func scheduleTimer() {
        stopTimer()
        timer = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in
            Task { @MainActor [weak self] in
                guard let self else { return }
                guard let player = self.player,
                      let generation = self.generation,
                      let videoID = self.videoID else { return }
                self.emit(state: player.isPlaying ? "playing" : "paused", generation: generation, videoID: videoID, player: player)
                if !player.isPlaying { self.stopTimer() }
            }
        }
    }

    private func stopTimer() {
        timer?.invalidate()
        timer = nil
    }

    private func emit(state: String, generation: UInt64, videoID: String, player: AVAudioPlayer) {
        sequence &+= 1
        onEvent?(LocalPlaybackEvent(
            generation: generation,
            videoID: videoID,
            sequence: sequence,
            state: state,
            currentTime: max(0, player.currentTime),
            duration: max(0, player.duration),
            volume: muted ? 0 : Double(player.volume),
            isMuted: muted
        ))
    }

    nonisolated func audioPlayerDidFinishPlaying(_ player: AVAudioPlayer, successfully flag: Bool) {
        let finishedPlayerID = ObjectIdentifier(player)
        Task { @MainActor [weak self] in
            guard let self,
                  let generation = self.generation,
                  let videoID = self.videoID,
                  let player = self.player,
                  ObjectIdentifier(player) == finishedPlayerID else { return }
            self.stopTimer()
            self.emit(state: "ended", generation: generation, videoID: videoID, player: player)
        }
    }
}
#else
import Foundation

struct LocalPlaybackEvent {
    let generation: UInt64
    let videoID: String
    let sequence: UInt64
    let state: String
    let currentTime: Double
    let duration: Double
    let volume: Double
    let isMuted: Bool
}

/// Explicit non-macOS stub. AVFoundation local playback is not silently emulated elsewhere.
@MainActor
final class LocalPlaybackHost {
    var onEvent: ((LocalPlaybackEvent) -> Void)?
    var onStatus: ((String) -> Void)?
    var loadedVideoID: String? { nil }
    var isLoaded: Bool { false }

    func prepare(localFile: String, videoID: String, generation: UInt64) throws {
        throw NSError(domain: "GoosicPlayback", code: 4, userInfo: [
            NSLocalizedDescriptionKey: "Local downloaded-file playback is only available on macOS.",
        ])
    }
    @discardableResult func play() -> Bool { onStatus?("Local downloaded-file playback is only available on macOS."); return false }
    func pause() {}
    func stop() {}
    func stopForReplacement() {}
    func seek(to seconds: Double) {}
    func setVolume(_ volume: Double) {}
    func setMuted(_ muted: Bool) {}
}
#endif
