import Foundation

/// Kept in step with goosic-shell-support::motion_artwork.
enum MotionArtworkPolicy {
    static let metadataLimit = 2 * 1024 * 1024
    static let videoLimit = 16 * 1024 * 1024
    static let durationLimit = 60.0

    static func allows(_ url: URL) -> Bool {
        url.scheme == "https" && url.user == nil && url.password == nil
            && (url.port == nil || url.port == 443)
            && ["itunes.apple.com", "music.apple.com", "mvod.itunes.apple.com"].contains(url.host ?? "")
    }

    static func durationMatches(_ requested: Double, _ candidate: Double) -> Bool {
        requested.isFinite && candidate.isFinite && requested > 0 && candidate > 0
            && abs(requested - candidate) <= 5
    }
}
