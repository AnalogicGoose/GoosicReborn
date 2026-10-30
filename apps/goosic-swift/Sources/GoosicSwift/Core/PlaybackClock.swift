import Combine

/// The playing position the renderer last confirmed, published on its own.
///
/// The position changes once a second for as long as anything plays. Published by the app
/// model, every tick invalidated every view that reads the model, and each one was a dropped
/// frame or more on a scrolling page. Kept here, a tick reaches only the views that show time
/// (the timelines and lyrics), which observe this object directly.
@MainActor
final class PlaybackClock: ObservableObject {
    @Published var position: Double = 0
}
