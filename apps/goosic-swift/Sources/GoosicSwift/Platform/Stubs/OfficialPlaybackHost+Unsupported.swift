#if !os(macOS)
import Foundation
import SwiftCrossUI

/// Explicit non-macOS stub: the first supported host is AppKit/WebKit on macOS.
@MainActor
final class OfficialPlaybackHost {
    var onEvent: ((OfficialPlaybackEvent) -> Void)?
    var onStatus: ((String) -> Void)?
    var onDiagnostics: ((String) -> Void)?
    private(set) var loadedVideoID: String?

    func load(videoID: String, generation: UInt64) {
        loadedVideoID = nil
        onStatus?("Official playback host is only available on macOS.")
    }

    func play() { onStatus?("Official playback host is only available on macOS.") }
    func pause() { onStatus?("Official playback host is only available on macOS.") }
    func probePage() {}
    var onPageAdvanced: ((String) -> Void)?
    func seek(to seconds: Double) { onStatus?("Official playback host is only available on macOS.") }
    func setVolume(_ volume: Double) { onStatus?("Official playback host is only available on macOS.") }
    func setMuted(_ muted: Bool) { onStatus?("Official playback host is only available on macOS.") }
    /// Mirrors the macOS host: drops the lease-bound event identity so a late event from the old
    /// document cannot be forwarded. This stub emits no events, so only the document is dropped.
    func invalidateExpectations() { loadedVideoID = nil }
    func quiesce(completion: @escaping @MainActor () -> Void) { completion() }
    func bind(profile: OfficialPlaybackProfile) {}
    func detach(completion: (@MainActor () -> Void)? = nil) { completion?() }
    func detach() { detach(completion: nil) }
}

struct OfficialPlaybackSurface: View {
    let model: GoosicAppModel

    var body: some View {
        EmptyView()
    }
}
#endif
