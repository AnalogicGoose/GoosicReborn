using System;
using System.Collections.Generic;

namespace Goosic.Windows.Glass;

/// <summary>A surface's rectangle in its window's scene coordinates, in DIPs.</summary>
public readonly record struct GlassBounds(double X, double Y, double Width, double Height, double Radius)
{
    public double Right => X + Width;
    public double Bottom => Y + Height;
    public bool IsEmpty => Width <= 0 || Height <= 0;

    public bool Intersects(GlassBounds other) =>
        !IsEmpty && !other.IsEmpty
        && other.X < Right && X < other.Right
        && other.Y < Bottom && Y < other.Bottom;

    /// <summary>The same rectangle expressed relative to <paramref name="origin"/>'s top-left corner.</summary>
    public GlassBounds RelativeTo(GlassBounds origin) => this with { X = X - origin.X, Y = Y - origin.Y };
}

/// <summary>
/// The signed-distance model of a glass surface, ported from LiquidGlass/src/glass.frag.
/// </summary>
/// <remarks>
/// Kept free of WinUI so the test project compiles it as-is. Distances are in DIPs; negative is
/// inside the shape.
/// </remarks>
public static class GlassGeometry
{
    public const double IndexOfRefraction = 1.5;

    /// <summary>
    /// A rounded rectangle with continuous (smoothed) corners. The corner starts at
    /// radius * (1 + smoothing) and is a superellipse of exponent 2 + 2.4 * smoothing, which keeps
    /// the corner's midpoint where a circular arc of the original radius would put it.
    /// </summary>
    public static double SignedDistance(double px, double py, double halfWidth, double halfHeight,
        double radius, double smoothing)
    {
        var r = Math.Clamp(radius * (1 + smoothing), 0, Math.Min(halfWidth, halfHeight));
        var n = 2 + 2.4 * smoothing;
        var qx = Math.Abs(px) - halfWidth + r;
        var qy = Math.Abs(py) - halfHeight + r;
        var safe = Math.Max(r, 1e-3);
        var cx = Math.Max(qx, 0) / safe;
        var cy = Math.Max(qy, 0) / safe;
        var corner = Math.Pow(Math.Pow(cx, n) + Math.Pow(cy, n), 1 / n) * r;
        return Math.Min(Math.Max(qx, qy), 0) + corner - r;
    }

    /// <summary>One pixel of anti-aliasing across the edge.</summary>
    public static double HardCoverage(double distance) => Math.Clamp(0.5 - distance, 0, 1);

    /// <summary>The coverage of the shape after a Gaussian blur of standard deviation <paramref name="sigma"/>.</summary>
    public static double BlurredCoverage(double distance, double sigma) =>
        0.5 - 0.5 * Erf(distance / (Math.Max(sigma, 1e-3) * Math.Sqrt(2)));

    public static double Erf(double x) => Math.Sign(x) * Math.Sqrt(1 - Math.Exp(-1.2732395 * x * x));

    /// <summary>The bevel's height profile: 0 at the rim, 1 where the flat top begins.</summary>
    public static double SurfaceHeight(double t)
    {
        var s = 1 - t;
        return Math.Pow(1 - s * s * s * s, 0.25);
    }

    public static double SurfaceSlope(double t)
    {
        var s = 1 - t;
        var s3 = s * s * s;
        return s3 * Math.Pow(Math.Max(1 - s3 * s, 1e-4), -0.75);
    }

    /// <summary>How far the bevel is allowed to reach in from the rim.</summary>
    public static double Bezel(double depth, double halfWidth, double halfHeight) =>
        Math.Max(Math.Min(depth, Math.Min(halfWidth, halfHeight)), 1);

    /// <summary>
    /// The sideways displacement, in DIPs, that the proof of concept's refraction gives a
    /// vertical ray entering the bevel at <paramref name="t"/>.
    /// </summary>
    public static double RefractionOffset(double t, double thickness, double bezel)
    {
        t = Math.Clamp(t, 0, 1);
        var slope = SurfaceSlope(t) * thickness / bezel;
        // A 1-D section through the rim: normal (slope, 1), incident (0, -1), glass of IOR 1.5.
        var length = Math.Sqrt(slope * slope + 1);
        var nx = slope / length;
        var nz = 1 / length;
        var eta = 1 / IndexOfRefraction;
        var cos = -nz;
        var k = 1 - eta * eta * (1 - cos * cos);
        if (k < 0)
        {
            return 0;
        }

        var factor = eta * cos + Math.Sqrt(k);
        var rx = -factor * nx;
        var rz = -eta - factor * nz;
        return Math.Abs(rx) / Math.Max(-rz, 1e-3) * SurfaceHeight(t) * thickness;
    }

    /// <summary>
    /// The largest refraction offset over the bevel, which is what a lens scale has to reproduce
    /// at the rim.
    /// </summary>
    public static double PeakRefractionOffset(double thickness, double bezel)
    {
        var peak = 0.0;
        for (var i = 0; i <= 64; i++)
        {
            peak = Math.Max(peak, RefractionOffset(i / 64.0, thickness, bezel));
        }

        return peak;
    }

    /// <summary>
    /// The nine-grid inset, in DIPs, beyond which every baked edge feature is constant along the
    /// edge and may be stretched.
    /// </summary>
    /// <remarks>
    /// The corner, the specular band and the inner glows all live within this distance of the
    /// rim. A surface narrower than twice the inset is baked at its own half-size in that
    /// direction instead, so a stretch never crosses a feature.
    /// </remarks>
    public static double NineGridInset(double radius, double smoothing, double depth, double halfWidth,
        double halfHeight)
    {
        var corner = radius * (1 + smoothing);
        var features = Math.Max(corner + 12, Math.Max(depth + 2, corner + 2));
        return Math.Max(1, Math.Min(features, Math.Floor(Math.Min(halfWidth, halfHeight))));
    }

    /// <summary>
    /// For each surface, ordered bottom to top, the surfaces below it that it overlaps, expressed
    /// in its own coordinates.
    /// </summary>
    /// <remarks>
    /// This is what makes z-order matter: only a surface above another one gives up tint and
    /// refraction over it, so bringing the lower one forward reverses which of the two responds.
    /// </remarks>
    public static IReadOnlyList<GlassBounds>[] ResolveStack(IReadOnlyList<GlassBounds> bottomToTop)
    {
        var result = new IReadOnlyList<GlassBounds>[bottomToTop.Count];
        for (var upper = 0; upper < bottomToTop.Count; upper++)
        {
            var below = new List<GlassBounds>();
            for (var lower = 0; lower < upper; lower++)
            {
                if (bottomToTop[upper].Intersects(bottomToTop[lower]))
                {
                    below.Add(bottomToTop[lower].RelativeTo(bottomToTop[upper]));
                }
            }

            result[upper] = below;
        }

        return result;
    }
}
