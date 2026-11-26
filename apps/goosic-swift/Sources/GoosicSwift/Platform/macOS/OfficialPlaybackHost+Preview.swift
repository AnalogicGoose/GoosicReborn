#if os(macOS) && GOOSIC_PREVIEW_NO_WEBKIT
import Foundation

/// Preview-only host: WebKit is intentionally disabled in the preview application.
@MainActor
final class OfficialPlaybackHost {
    var onEvent: ((OfficialPlaybackEvent) -> Void)?
    var onStatus: ((String) -> Void)?
    var onDiagnostics: ((String) -> Void)?
    private(set) var loadedVideoID: String?

    func load(
        videoID: String,
        generation: UInt64,
        volume: Double = 1,
        muted: Bool = false,
        adoptions: Int = 0,
        startPaused: Bool = false
    ) {
        loadedVideoID = nil
        onStatus?("Official playback is disabled in this preview.")
    }

    func play(fromListener: Bool = true) { onStatus?("Official playback is disabled in this preview.") }
    func pause() { onStatus?("Official playback is disabled in this preview.") }
    func probePage() {}
    var onPageAdvanced: ((String) -> Void)?
    var onPageSwappedBeforePlaying: ((OfficialPageSwap) -> Void)?
    var onNeverStarted: ((String) -> Void)?
    var onLoadingChanged: ((Bool) -> Void)?
    func seek(to seconds: Double) { onStatus?("Official playback is disabled in this preview.") }
    func setVolume(_ volume: Double) { onStatus?("Official playback is disabled in this preview.") }
    func setMuted(_ muted: Bool) { onStatus?("Official playback is disabled in this preview.") }
    /// Mirrors the macOS host: drops the lease-bound event identity so a late event from the old
    /// document cannot be forwarded. This stub emits no events, so only the document is dropped.
    func invalidateExpectations() { loadedVideoID = nil }
    func quiesce(completion: @escaping @MainActor () -> Void) { completion() }
    func bind(profile: OfficialPlaybackProfile) {}
    func detach(completion: (@MainActor () -> Void)? = nil) { completion?() }
    func detach() { detach(completion: nil) }
}

#endif
