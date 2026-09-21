import Foundation

/// Boundary for platform playback implementations.
///
/// The macOS official implementation lives in `OfficialPlaybackHost` and is intentionally kept
/// separate from this legacy-compatible abstraction. Windows and Linux remain explicit stubs so
/// future renderers cannot bypass Rust's ownership authority.
protocol PlatformPlaybackHost {
    var owner: GoosicOwner { get }
    func prepare() throws
    func stop() throws
}

struct UnsupportedPlatformPlaybackHost: PlatformPlaybackHost {
    let owner: GoosicOwner

    func prepare() throws {
        throw NSError(domain: "GoosicPlayback", code: 1, userInfo: [
            NSLocalizedDescriptionKey: "Platform playback host is not implemented yet.",
        ])
    }

    func stop() throws {}
}
