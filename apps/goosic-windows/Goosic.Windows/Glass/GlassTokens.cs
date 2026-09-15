using System;

namespace Goosic.Windows.Glass;

/// <summary>
/// The optical parameters of one material, in the units of the LiquidGlass proof of concept:
/// frost and depth in DIPs, the rest as the Figma "Glass" effect expresses them.
/// </summary>
/// <remarks>
/// This file has no WinUI dependency so the test project can compile it directly.
/// </remarks>
public readonly record struct GlassMaterial(
    float Frost,
    float Tint,
    float Refraction,
    float Depth,
    float LightIntensity,
    float LightAngleDegrees,
    float Splay,
    float Shadow,
    float Smoothing,
    bool DarkTint)
{
    /// <summary>The Gaussian standard deviation of the frost; the proof of concept blurs at frost / 2.</summary>
    public float BlurSigma => Frost / 2f;
}

/// <summary>What each quality tier keeps of the optical model.</summary>
public readonly record struct GlassFeatures(
    bool Backdrop,
    bool Refraction,
    bool Shadow,
    bool StackResponse,
    bool Edges,
    float StackMaskScale,
    float FrostScale,
    GlassBlurOptimization BlurOptimization);

public enum GlassBlurOptimization
{
    Speed,
    Balanced,
    Quality,
}

public static class GlassTokens
{
    // The calibrated "Liquid Glass - Regular - Large" values from LiquidGlass/src/glass.rs. Every
    // semantic style shares them; only frost, tint and the shadow's strength move with hierarchy.
    private const float Refraction = 2f;
    private const float Depth = 30f;
    private const float LightIntensity = 0.25f;
    private const float LightAngle = 0f;
    private const float Splay = 0.2f;
    private const float Smoothing = 0.6f;

    /// <summary>
    /// The proof of concept's stacked-glass constants: how much of the tint, refraction and
    /// specular light an upper surface gives up where glass is already beneath it.
    /// </summary>
    public const float StackTintRelief = 0.78f;
    public const float StackRefractionRelief = 0.88f;
    public const float StackSpecularRelief = 0.45f;

    public static GlassMaterial For(GlassStyle style, bool dark)
    {
        var (frost, tint, shadow) = style switch
        {
            // Thin and Window are the "Clear" profile: frost 6 and a 15 % tint.
            GlassStyle.Thin => (6f, 0.15f, 0f),
            GlassStyle.Window => (6f, 0.15f, 0f),
            GlassStyle.Control => (16f, 1f, 0.35f),
            GlassStyle.Navigation => (16f, 1f, 0.6f),
            GlassStyle.Player => (16f, 1f, 1f),
            GlassStyle.Prominent => (22f, 1f, 0.8f),
            GlassStyle.Menu => (22f, 1f, 1f),
            _ => (16f, 1f, 1f),
        };

        // Window chrome is a scroll edge, not an object: it has no rim to light and nothing to bend.
        var chrome = style == GlassStyle.Window;
        return new GlassMaterial(
            frost,
            tint,
            chrome ? 0f : Refraction,
            Depth,
            chrome ? 0f : LightIntensity,
            LightAngle,
            Splay,
            shadow,
            Smoothing,
            dark);
    }

    public static GlassFeatures Features(GlassQuality quality) => quality switch
    {
        GlassQuality.Ultra => new(true, true, true, true, true, 1f, 1f, GlassBlurOptimization.Quality),
        GlassQuality.High => new(true, true, true, true, true, 0.5f, 1f, GlassBlurOptimization.Balanced),
        GlassQuality.Medium => new(true, true, false, true, true, 0.5f, 1f, GlassBlurOptimization.Speed),
        // Low follows the proof of concept: refraction goes first, and the frost is cheaper.
        GlassQuality.Low => new(true, false, false, true, true, 0.25f, 0.6f, GlassBlurOptimization.Speed),
        _ => new(false, false, false, false, false, 0f, 0f, GlassBlurOptimization.Speed),
    };

    /// <summary>The blur radius actually requested from the compositor for a style at a tier.</summary>
    public static float BlurAmount(GlassMaterial material, GlassQuality quality) =>
        MathF.Max(0f, material.BlurSigma * Features(quality).FrostScale);
}
