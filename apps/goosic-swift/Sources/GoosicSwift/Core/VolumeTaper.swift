import Foundation

/// Maps a volume slider's position to the gain the player plays at, on an audio taper.
///
/// Loudness is heard logarithmically, so a slider that moved the gain in a straight line spent
/// almost all of its travel between "loud" and "louder". Cubing the position is the usual
/// approximation of an audio-taper potentiometer -- half way is about -18 dB and a tenth of the
/// way about -60 dB -- and unlike a pure decibel curve it still reaches true silence at zero.
/// The model and the saved preference keep holding the gain; only the slider and its percentage
/// work in positions.
///
/// This is the same rule as `volume_gain` in `goosic-shell-support` and the Windows
/// `VolumeTaper`, held to the same test cases.
enum VolumeTaper {
    /// The gain, 0 to 1, for a slider position from 0 to 1.
    static func gain(atPosition position: Double) -> Double {
        let p = position.isFinite ? min(max(position, 0), 1) : 0
        return p * p * p
    }

    /// The slider position, 0 to 1, that plays at a gain from 0 to 1.
    static func position(forGain gain: Double) -> Double {
        gain.isFinite ? cbrt(min(max(gain, 0), 1)) : 0
    }
}
