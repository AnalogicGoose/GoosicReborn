#if !os(macOS)
import Foundation


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
