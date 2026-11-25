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

/// The position the renderer last confirmed, for the lyrics alone, and never throttled.
///
/// `PlaybackClock` is copied about once a second so the progress bars redraw that often. The
/// lyrics interpolate between confirmations and may run ahead by only
/// `LyricsTiming.interpolationLimit`, so anchoring them on a value up to a second stale made
/// the sweep stall every second and the highlight jump back on pause. This one is written on
/// every accepted sample of the loaded recording and observed only by the lyrics view.
@MainActor
final class LyricsClock: ObservableObject {
    @Published var position: Double = 0
}
