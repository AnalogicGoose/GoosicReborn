// Linux publishes MPRIS from Platform/Linux. What remains here is the no-op for platforms with
// no system media integration yet.
#if !os(macOS) && !os(Linux)
import Foundation

/// Explicit non-macOS no-op. MediaPlayer is a macOS system framework.
@MainActor
final class SystemMediaControls {
    init(model: GoosicAppModel) {}
    func update(snapshot: SystemMediaPlaybackSnapshot) {}
}
#endif
