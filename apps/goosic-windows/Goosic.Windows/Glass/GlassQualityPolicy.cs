using System;

namespace Goosic.Windows.Glass;

/// <summary>What the operating system and the hardware currently allow.</summary>
public readonly record struct GlassEnvironment(
    bool HighContrast,
    bool TransparencyEffects,
    bool AnimationsEnabled,
    bool EnergySaver,
    bool OnBattery,
    bool RendererSupported,
    string? Override);

/// <summary>
/// Turns the environment into one quality tier, so every surface in every window agrees.
/// </summary>
/// <remarks>
/// Accessibility outranks everything, the developer override included: a user who asked for high
/// contrast or no transparency gets solid surfaces even while someone is benchmarking Ultra.
/// </remarks>
public static class GlassQualityPolicy
{
    public static GlassQuality Select(GlassEnvironment environment)
    {
        if (environment.HighContrast || !environment.TransparencyEffects || !environment.RendererSupported)
        {
            return GlassQuality.Fallback;
        }

        if (TryParse(environment.Override, out var forced))
        {
            return forced;
        }

        if (environment.EnergySaver)
        {
            return GlassQuality.Low;
        }

        return environment.OnBattery ? GlassQuality.Medium : GlassQuality.High;
    }

    public static bool ReducedMotion(GlassEnvironment environment) => !environment.AnimationsEnabled;

    public static bool TryParse(string? value, out GlassQuality quality)
    {
        quality = GlassQuality.High;
        return !string.IsNullOrWhiteSpace(value)
            && Enum.TryParse(value.Trim(), ignoreCase: true, out quality)
            && Enum.IsDefined(quality);
    }
}
