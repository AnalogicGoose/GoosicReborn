using System;

namespace Goosic.Windows.Glass;

/// <summary>What a set of baked edge textures depends on, and nothing else.</summary>
/// <remarks>
/// The textures are nine-grids: every feature that varies along an edge lives within the inset,
/// so one bake serves every surface with the same corner, material and pixel density, at any size.
/// </remarks>
public readonly record struct GlassBakeKey(
    int InsetPixels,
    float Scale,
    float Radius,
    float Smoothing,
    float Depth,
    float Refraction,
    float LightIntensity,
    float LightAngleDegrees,
    float Splay,
    bool DarkTint)
{
    public static GlassBakeKey For(GlassMaterial material, double radius, double width, double height, double scale)
    {
        var inset = GlassGeometry.NineGridInset(radius, material.Smoothing, material.Depth, width / 2, height / 2);
        return new GlassBakeKey(
            Math.Max(1, (int)Math.Floor(inset * scale)),
            (float)scale,
            (float)radius,
            material.Smoothing,
            material.Depth,
            material.Refraction,
            material.LightIntensity,
            material.LightAngleDegrees,
            material.Splay,
            material.DarkTint);
    }

    public int Size => InsetPixels * 2 + 1;
}

public readonly record struct GlassShadowKey(
    int InsetPixels,
    float Margin,
    float Radius,
    float Smoothing,
    float Strength,
    bool DarkTint)
{
    /// <summary>Shadows are soft enough to bake at half a pixel per DIP on every display.</summary>
    public const float Scale = 0.5f;
    public const float Sigma = 24f;

    public static float OffsetFor(bool dark) => dark ? 18f : 8f;
    public static float OpacityFor(bool dark) => dark ? 0.45f : 0.25f;

    public static GlassShadowKey For(GlassMaterial material, double radius, double width, double height)
    {
        var offset = OffsetFor(material.DarkTint);
        var margin = 3 * Sigma + offset;
        var corner = radius * (1 + material.Smoothing);
        var shape = Math.Min(Math.Min(width, height) / 2, corner + 3 * Sigma + offset);
        return new GlassShadowKey(
            Math.Max(1, (int)Math.Floor((shape + margin) * Scale)),
            margin,
            (float)radius,
            material.Smoothing,
            material.Shadow,
            material.DarkTint);
    }

    public int Size => InsetPixels * 2 + 1;
}

/// <summary>Premultiplied BGRA8 textures for one nine-grid bake.</summary>
public sealed record GlassBakedTextures(
    int Size,
    int InsetPixels,
    float Scale,
    byte[] Shape,
    byte[] EdgeAdd,
    byte[] EdgeBurn,
    byte[] Lens,
    double LensPeakOffset);

/// <summary>
/// Evaluates the proof of concept's fragment shader once, on the CPU, into small nine-grid
/// textures that the compositor stretches and samples on the GPU.
/// </summary>
/// <remarks>
/// Composition effect graphs cannot run a custom pixel shader, so the parts of the shader that do
/// not depend on the backdrop — the continuous-corner mask, the rim light, the inner glows, the
/// burned outline and the bevel's refraction profile — are baked. This is an upload of computed
/// values when a corner radius or display density is first seen, never a readback, and never
/// part of a frame.
/// </remarks>
public static class GlassTextureBaker
{
    private const double EdgeGlowSigma = 3.3;
    private const double EdgeGlowOffset = 40;
    private const double LightEdgeGlow = 0.157;
    private const double DarkEdgeGlow = 0.102;
    private const double LightOutlineBurn = 1 - 0.859;
    private const double DarkOutlineBurn = 1 - 0.651;

    /// <summary>
    /// Linear Burn subtracts a constant; the compositor darkens by multiplying instead. Scaling
    /// the burn keeps the rim about as dark on mid-tones, where it is most visible.
    /// </summary>
    private const double BurnToMultiply = 1.5;

    public static GlassBakedTextures Bake(GlassBakeKey key)
    {
        var size = key.Size;
        var scale = (double)key.Scale;
        var half = (key.InsetPixels + 0.5) / scale;
        var radius = key.Radius;
        var smoothing = key.Smoothing;
        var pixel = 1 / scale;

        var shape = new byte[size * size * 4];
        var edgeAdd = new byte[size * size * 4];
        var edgeBurn = new byte[size * size * 4];
        var lens = new byte[size * size * 4];

        var bezel = GlassGeometry.Bezel(key.Depth, half, half);
        var thickness = key.Refraction * key.Depth;
        var peak = thickness > 0 ? GlassGeometry.PeakRefractionOffset(thickness, bezel) : 0;
        var angle = key.LightAngleDegrees * Math.PI / 180;
        var lightX = Math.Sin(angle);
        var lightY = Math.Cos(angle);
        var glowColor = key.DarkTint ? DarkEdgeGlow : LightEdgeGlow;
        var burn = Math.Min(1, (key.DarkTint ? DarkOutlineBurn : LightOutlineBurn) * BurnToMultiply);
        var splayReach = bezel * (0.2 + 0.8 * key.Splay);

        double Sd(double x, double y) => GlassGeometry.SignedDistance(x, y, half, half, radius, smoothing);

        for (var row = 0; row < size; row++)
        {
            var y = (row + 0.5) / scale - half;
            for (var column = 0; column < size; column++)
            {
                var x = (column + 0.5) / scale - half;
                var index = (row * size + column) * 4;
                var sd = Sd(x, y);

                // D2D displacement maps use 0.5 as no movement. Keep every texel neutral until
                // the SDF bevel below supplies a signed X/Y ray offset.
                WriteDisplacement(lens, index, 0, 0);

                // The outer half pixel is the burned outline; the glass body starts one pixel in.
                var outer = GlassGeometry.HardCoverage(sd * scale + 0.5);
                if (outer <= 0)
                {
                    continue;
                }

                var body = GlassGeometry.HardCoverage(sd * scale + 1);
                WriteGray(shape, index, outer);

                // Outline: the three zero-blur drop shadows of the Figma layer, moved inside the
                // bounds because a surface cannot draw past its own rectangle.
                var ringAll = Math.Max(outer - body, 0);
                var ringLeft = Math.Max(GlassGeometry.HardCoverage(
                    GlassGeometry.SignedDistance(x + 1.25 * pixel, y, half - 0.75 * pixel, half - 0.75 * pixel,
                        radius - 0.75 * pixel, smoothing) * scale + 0.5) - body, 0);
                var ringRight = Math.Max(GlassGeometry.HardCoverage(
                    GlassGeometry.SignedDistance(x - 1.25 * pixel, y, half - 0.75 * pixel, half - 0.75 * pixel,
                        radius - 0.75 * pixel, smoothing) * scale + 0.5) - body, 0);
                var darkening = 1 - (1 - burn * ringAll) * (1 - burn * ringLeft) * (1 - burn * ringRight);
                WriteBlackAlpha(edgeBurn, index, darkening);

                if (body <= 0)
                {
                    continue;
                }

                var dist = Math.Max(-(sd + pixel), 0);
                var t = Math.Clamp(dist / bezel, 0, 1);

                var nx = Sd(x + 0.5, y) - Sd(x - 0.5, y);
                var ny = Sd(x, y + 0.5) - Sd(x, y - 0.5);
                var nLength = Math.Sqrt(nx * nx + ny * ny);
                if (nLength > 1e-5)
                {
                    nx /= nLength;
                    ny /= nLength;
                }
                else
                {
                    nx = ny = 0;
                }

                var facing = nx * lightX + ny * lightY;
                var lit = Math.Pow(Math.Max(facing, 0), 1.5);
                var band = 1 - SmoothStep(0, splayReach, dist);
                var add = key.LightIntensity * 0.12 * lit * band;

                var distPixels = dist * scale;
                var edge = 1 - SmoothStep(0.5, 1.5, distPixels);
                var edgeLight = 0.45 * Math.Pow(Math.Max(-facing, 0), 3) + 0.17 * Math.Pow(Math.Max(facing, 0), 3);
                add += key.LightIntensity * edge * edgeLight;

                var glowHalf = half + EdgeGlowOffset;
                var glowTop = 1 - GlassGeometry.BlurredCoverage(
                    GlassGeometry.SignedDistance(x, y - EdgeGlowOffset, glowHalf, glowHalf, radius, smoothing), EdgeGlowSigma);
                var glowBottom = 1 - GlassGeometry.BlurredCoverage(
                    GlassGeometry.SignedDistance(x, y + EdgeGlowOffset, glowHalf, glowHalf, radius, smoothing), EdgeGlowSigma);
                add += glowColor * (glowTop + glowBottom);
                WriteGray(edgeAdd, index, Math.Clamp(add, 0, 1) * body);

                if (peak > 0)
                {
                    var displacement = GlassGeometry.RefractionOffset(t, thickness, bezel) / peak * body;
                    // The shader's refracted ray bends toward the inside, opposite the outward SDF
                    // normal. R/G carry signed normalized X/Y offsets for DisplacementMapEffect.
                    WriteDisplacement(lens, index, -nx * displacement, -ny * displacement);
                }
            }
        }

        return new GlassBakedTextures(size, key.InsetPixels, key.Scale, shape, edgeAdd, edgeBurn, lens, peak);
    }

    /// <summary>The drop shadow of the Fill + Shadow layer, outside the shape only.</summary>
    public static byte[] BakeShadow(GlassShadowKey key)
    {
        var size = key.Size;
        var scale = (double)GlassShadowKey.Scale;
        var extent = (key.InsetPixels + 0.5) / scale;
        var half = Math.Max(1, extent - key.Margin);
        var offset = GlassShadowKey.OffsetFor(key.DarkTint);
        var opacity = GlassShadowKey.OpacityFor(key.DarkTint) * key.Strength;
        var pixels = new byte[size * size * 4];

        for (var row = 0; row < size; row++)
        {
            var y = (row + 0.5) / scale - extent;
            for (var column = 0; column < size; column++)
            {
                var x = (column + 0.5) / scale - extent;
                var inside = GlassGeometry.HardCoverage(
                    GlassGeometry.SignedDistance(x, y, half, half, key.Radius, key.Smoothing) * scale);
                if (inside >= 1)
                {
                    continue;
                }

                var shadow = GlassGeometry.BlurredCoverage(
                    GlassGeometry.SignedDistance(x, y - offset, half, half, key.Radius, key.Smoothing), GlassShadowKey.Sigma);
                var alpha = opacity * shadow * (1 - inside);
                if (alpha < 0.002)
                {
                    continue;
                }

                // Black at this alpha, premultiplied: only the alpha byte carries it.
                pixels[(row * size + column) * 4 + 3] = ToByte(alpha);
            }
        }

        return pixels;
    }

    private static void WriteGray(byte[] pixels, int index, double value)
    {
        var channel = ToByte(value);
        pixels[index] = channel;
        pixels[index + 1] = channel;
        pixels[index + 2] = channel;
        pixels[index + 3] = channel;
    }

    private static void WriteBlackAlpha(byte[] pixels, int index, double value)
    {
        pixels[index + 3] = ToByte(value);
    }

    private static void WriteDisplacement(byte[] pixels, int index, double x, double y)
    {
        // Upload format is BGRA8; the effect reads logical red and green channels.
        pixels[index] = 128;
        pixels[index + 1] = ToByte(0.5 + 0.5 * Math.Clamp(y, -1, 1));
        pixels[index + 2] = ToByte(0.5 + 0.5 * Math.Clamp(x, -1, 1));
        pixels[index + 3] = 255;
    }

    private static byte ToByte(double value) => (byte)Math.Clamp(Math.Round(value * 255), 0, 255);

    private static double SmoothStep(double edge0, double edge1, double x)
    {
        var t = Math.Clamp((x - edge0) / Math.Max(edge1 - edge0, 1e-6), 0, 1);
        return t * t * (3 - 2 * t);
    }
}
