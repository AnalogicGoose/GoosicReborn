namespace Goosic.Windows.Presentation;

/// <summary>
/// Maps a volume slider's position to the gain the page plays at, on an audio taper.
/// </summary>
/// <remarks>
/// The curve is <c>volume_gain</c> in <c>goosic-shell-support</c>, the one the Mac uses: cubing
/// the position, so half way is about -18 dB and zero is true silence. The page and the saved
/// preference keep holding the gain; only the slider, its percentage, and the keyboard's steps
/// work in positions.
/// </remarks>
public static class VolumeTaper
{
    /// <summary>The gain, 0 to 1, for a slider position from 0 to 1.</summary>
    public static double ToGain(double position) => PlaybackRules.VolumeGain(position);

    /// <summary>The slider position, 0 to 1, that plays at a gain from 0 to 1.</summary>
    public static double ToPosition(double gain) => PlaybackRules.VolumePosition(gain);
}
