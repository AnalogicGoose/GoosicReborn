using System;

namespace Goosic.Windows.Presentation;

/// <summary>
/// The decisions an artist page's hero makes the same way on every platform: how tall it is for a
/// given width, which colour the page takes from the banner, and how the banner gives way as the
/// page scrolls.
/// </summary>
/// <remarks>
/// The height and the tint are answered by <c>goosic-shell-support</c>'s <c>hero</c> module, which the
/// Mac asks too. The scroll motion below is drawing, and stays here.
/// <c>docs/ARTIST_PAGE.md</c> describes the hero they serve.
/// </remarks>
public static class ArtistHero
{
    /// <summary>A banner's proportions, width over height: YouTube Music serves them at 2880 by 1200.</summary>
    public const double BannerAspect = 2.4;

    /// <summary>The shortest hero, so a narrow window still shows the name and its buttons over the picture.</summary>
    public const double MinHeight = 360;

    /// <summary>The tallest hero, so a wide window does not turn the whole first screen into picture.</summary>
    public const double MaxHeight = 620;

    /// <summary>How much of the banner, from its foot, the page colour is sampled from.</summary>
    public const double TintBand = 0.25;

    /// <summary>
    /// The brightest relative luminance the page colour may have, so white text over it keeps at
    /// least 4.5:1 contrast: (1 + 0.05) / (0.18 + 0.05) ≈ 4.57.
    /// </summary>
    public const double MaxTintLuminance = 0.18;

    /// <summary>The page colour before a picture has been sampled, a deep wine, in sRGB.</summary>
    public static readonly byte[] DefaultTint = [61, 13, 31];

    /// <summary>The banner's blur, in pixels, once half the hero has scrolled away.</summary>
    public const double MaxBlur = 30;

    /// <summary>The hero's height, in the same units as <paramref name="width"/>.</summary>
    public static double Height(double width) => PlaybackRules.HeroHeight(width);

    /// <summary>The colour the page takes from a banner, as 8-bit sRGB.</summary>
    /// <param name="rgba">A small bitmap of the banner, four bytes a pixel, rows from the top.</param>
    /// <returns>
    /// The bottom band averaged in linear light and darkened, never lightened, until white text
    /// over it is readable; <c>null</c> for a bitmap too small to hold the pixels it claims.
    /// </returns>
    public static byte[]? Tint(ReadOnlySpan<byte> rgba, int width, int height) =>
        PlaybackRules.HeroTint(rgba, width, height);

    /// <summary>How far the hero has scrolled away, from 0 at rest to 1 once it has gone.</summary>
    public static double Progress(double scrolled, double heroHeight) =>
        heroHeight <= 0 ? 0 : Math.Clamp(scrolled / heroHeight, 0, 1);

    /// <summary>The banner's blur: full by the time half the hero has gone, none without transparency effects.</summary>
    public static double Blur(double progress, bool transparencyEffects) =>
        transparencyEffects ? Math.Min(progress / 0.5, 1) * MaxBlur : 0;

    /// <summary>The banner's opacity: it starts dissolving at a third and has gone with the hero's foot.</summary>
    public static double PictureOpacity(double progress) => 1 - Math.Clamp((progress - 0.3) / 0.6, 0, 1);
}
