#if !os(macOS)
import Foundation

/// Explicit non-macOS no-op. MediaPlayer is a macOS system framework.
@MainActor
final class SystemMediaControls {
    init(model: GoosicAppModel) {}
    func update(snapshot: SystemMediaPlaybackSnapshot) {}
}
#endif
