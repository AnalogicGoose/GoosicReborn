using System;

namespace Goosic.Windows.Presentation;

/// <summary>
/// Maps a volume slider's position to the gain the page plays at, on an audio taper.
/// </summary>
/// <remarks>
/// Loudness is heard logarithmically, so a slider that moves the gain in a straight line spends
/// almost all of its travel between "loud" and "louder": half way up was already far too loud.
/// Cubing the position is the usual approximation of an audio-taper potentiometer -- half way is
/// about -18 dB and a tenth of the way about -60 dB -- and unlike a pure decibel curve it still
/// reaches true silence at zero. The page and the saved preference keep holding the gain; only
/// the slider, its percentage, and the keyboard's steps work in positions.
/// </remarks>
public static class VolumeTaper
{
    /// <summary>The gain, 0 to 1, for a slider position from 0 to 1.</summary>
    public static double ToGain(double position)
    {
        var p = Math.Clamp(position, 0, 1);
        return p * p * p;
    }

    /// <summary>The slider position, 0 to 1, that plays at a gain from 0 to 1.</summary>
    public static double ToPosition(double gain) => Math.Cbrt(Math.Clamp(gain, 0, 1));
}
