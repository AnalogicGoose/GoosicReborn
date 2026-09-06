import Foundation

/// A validated sample from the local decoded-file renderer. The shape is shared: Rust's
/// authority accepts the same sequence contract whichever platform produced it.
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
