using System;

namespace Goosic.Windows.Presentation;

/// <summary>
/// The decisions an artist page's hero makes the same way on every platform: how tall it is for a
/// given width, which colour the page takes from the banner, and how the banner gives way as the
/// page scrolls.
/// </summary>
/// <remarks>
/// The Windows copy of <c>goosic-shell-support</c>'s <c>hero</c> module and of macOS's
/// <c>ArtistHero.swift</c>, held to the same test cases; a change to one is a change to all three.
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
    public static double Height(double width) => Math.Clamp(width / BannerAspect, MinHeight, MaxHeight);

    /// <summary>The colour the page takes from a banner, as 8-bit sRGB.</summary>
    /// <param name="rgba">A small bitmap of the banner, four bytes a pixel, rows from the top.</param>
    /// <returns>
    /// The bottom band averaged in linear light and darkened, never lightened, until white text
    /// over it is readable; <c>null</c> for a bitmap too small to hold the pixels it claims.
    /// </returns>
    public static byte[]? Tint(ReadOnlySpan<byte> rgba, int width, int height)
    {
        if (width <= 0 || height <= 0 || rgba.Length < width * height * 4)
        {
            return null;
        }

        var band = Math.Clamp((int)Math.Ceiling(height * TintBand), 1, height);
        Span<double> sum = stackalloc double[3];
        for (var row = height - band; row < height; row++)
        {
            for (var column = 0; column < width; column++)
            {
                var offset = (row * width + column) * 4;
                for (var channel = 0; channel < 3; channel++)
                {
                    sum[channel] += Linear(rgba[offset + channel]);
                }
            }
        }

        var count = (double)band * width;
        var red = sum[0] / count;
        var green = sum[1] / count;
        var blue = sum[2] / count;
        var luminance = 0.2126 * red + 0.7152 * green + 0.0722 * blue;
        if (luminance > MaxTintLuminance)
        {
            var scale = MaxTintLuminance / luminance;
            red *= scale;
            green *= scale;
            blue *= scale;
        }

        return [Encode(red), Encode(green), Encode(blue)];
    }

    /// <summary>How far the hero has scrolled away, from 0 at rest to 1 once it has gone.</summary>
    public static double Progress(double scrolled, double heroHeight) =>
        heroHeight <= 0 ? 0 : Math.Clamp(scrolled / heroHeight, 0, 1);

    /// <summary>The banner's blur: full by the time half the hero has gone, none without transparency effects.</summary>
    public static double Blur(double progress, bool transparencyEffects) =>
        transparencyEffects ? Math.Min(progress / 0.5, 1) * MaxBlur : 0;

    /// <summary>The banner's opacity: it starts dissolving at a third and has gone with the hero's foot.</summary>
    public static double PictureOpacity(double progress) => 1 - Math.Clamp((progress - 0.3) / 0.6, 0, 1);

    private static double Linear(byte channel)
    {
        var value = channel / 255.0;
        return value <= 0.04045 ? value / 12.92 : Math.Pow((value + 0.055) / 1.055, 2.4);
    }

    private static byte Encode(double value)
    {
        value = Math.Clamp(value, 0, 1);
        var srgb = value <= 0.0031308 ? value * 12.92 : 1.055 * Math.Pow(value, 1 / 2.4) - 0.055;
        return (byte)Math.Round(srgb * 255, MidpointRounding.AwayFromZero);
    }
}
