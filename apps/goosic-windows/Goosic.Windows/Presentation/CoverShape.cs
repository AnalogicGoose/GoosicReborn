using System;

namespace Goosic.Windows.Presentation;

/// <summary>The shape the playing cover is drawn at, taken from the picture itself.</summary>
/// <remarks>
/// A song's art is square and a music video's is wide, and cropping both to one fixed square cut
/// the sides off every video. The player bar and the full player instead take the shape of the
/// picture they have. The ratio is held between 9:16 and 16:9 so an odd banner cannot turn the
/// cover into a sliver, and a picture with no size yet is square, the shape most covers are.
/// This is the same rule as <c>cover_aspect</c> and <c>fit_cover</c> in <c>goosic-shell-support</c>
/// and the macOS <c>CoverShape</c>, held to the same test cases.
/// </remarks>
public static class CoverShape
{
    public const double MinAspect = 9.0 / 16.0;
    public const double MaxAspect = 16.0 / 9.0;

    /// <summary>The width-to-height ratio for a picture of the given pixel size.</summary>
    public static double Aspect(double pixelWidth, double pixelHeight)
    {
        if (!double.IsFinite(pixelWidth) || !double.IsFinite(pixelHeight) || pixelWidth <= 0 || pixelHeight <= 0)
        {
            return 1;
        }

        return Math.Clamp(pixelWidth / pixelHeight, MinAspect, MaxAspect);
    }

    /// <summary>
    /// The size a cover of <paramref name="aspect"/> takes inside a slot: as large as fits without
    /// cropping, touching the slot on its longer side.
    /// </summary>
    public static (double Width, double Height) Fit(double aspect, double maxWidth, double maxHeight)
    {
        aspect = Aspect(aspect, 1);
        var width = Math.Max(maxWidth, 0);
        var height = Math.Max(maxHeight, 0);
        return height == 0 || aspect >= width / height
            ? (width, width / aspect)
            : (height * aspect, height);
    }
}
