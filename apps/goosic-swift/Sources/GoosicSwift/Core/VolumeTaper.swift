import Foundation
import GoosicShellSupportFFI

/// Maps a volume slider's position to the gain the player plays at, on an audio taper.
///
/// The curve is `volume_gain` in `goosic-shell-support`, the one Windows uses too: cubing the
/// position, so half way is about -18 dB and zero is true silence. The model and the saved
/// preference keep holding the gain; only the slider and its percentage work in positions.
enum VolumeTaper {
    /// The gain, 0 to 1, for a slider position from 0 to 1.
    static func gain(atPosition position: Double) -> Double {
        goosic_volume_gain(position)
    }

    /// The slider position, 0 to 1, that plays at a gain from 0 to 1.
    static func position(forGain gain: Double) -> Double {
        goosic_volume_position(gain)
    }
}

/// The playback rules asked on every sample, answered by `goosic-shell-support`.
enum PlaybackRules {
    /// `seconds` as `m:ss`, or `h:mm:ss` past an hour; nonsense reads `0:00`.
    static func timeText(_ seconds: Double) -> String {
        guard let raw = goosic_time_text(seconds) else { return "0:00" }
        defer { goosic_string_free(raw) }
        return String(cString: raw)
    }

    /// Where the position line sits for a seek to `target` asked `elapsed` seconds ago, when the
    /// player reports `confirmed`, and whether that seek can now be forgotten.
    static func settleSeek(confirmed: Double, target: Double, elapsed: TimeInterval) -> (shown: Double, settled: Bool) {
        var shown = confirmed
        let settled = goosic_seek_settle(confirmed, target, elapsed, &shown)
        return (shown, settled)
    }
}
