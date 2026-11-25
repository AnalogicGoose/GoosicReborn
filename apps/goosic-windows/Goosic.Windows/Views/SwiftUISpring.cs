using System;
using System.Collections.Generic;
using System.Globalization;
using System.Numerics;
using Goosic.Windows.Presentation;
using Microsoft.UI.Composition;

namespace Goosic.Windows.Views;

/// <summary>
/// Composition animations that follow a SwiftUI spring exactly (<see cref="LyricsMotion.Spring"/>),
/// sampled as linear keyframes. They take over from wherever the value is when they start.
/// </summary>
/// <remarks>
/// Built once per compositor, response, damping and target, then reused: an animation is a template,
/// and starting it copies it, so changing the delay before each start is safe.
/// </remarks>
internal static class SwiftUISpring
{
    private const int Samples = 40;
    private static readonly Dictionary<(Compositor, double, double, string), KeyFrameAnimation> Cache = [];

    /// <summary>A scalar moving from its current value to <paramref name="value"/>.</summary>
    internal static ScalarKeyFrameAnimation Scalar(Compositor compositor, double response, double damping, float value)
    {
        var key = (compositor, response, damping, "s" + value.ToString("R", CultureInfo.InvariantCulture));
        if (Cache.TryGetValue(key, out var cached))
        {
            return (ScalarKeyFrameAnimation)cached;
        }

        var animation = compositor.CreateScalarKeyFrameAnimation();
        animation.SetScalarParameter("target", value);
        Fill(compositor, animation, response, damping, (progress, k) =>
            animation.InsertExpressionKeyFrame(progress, $"this.StartingValue + (target - this.StartingValue) * {k}",
                compositor.CreateLinearEasingFunction()));
        Cache[key] = animation;
        return animation;
    }

    /// <summary>A vector moving from its current value to <paramref name="value"/>.</summary>
    internal static Vector3KeyFrameAnimation Vector3(Compositor compositor, double response, double damping, Vector3 value)
    {
        var key = (compositor, response, damping, FormattableString.Invariant($"v{value.X:R},{value.Y:R},{value.Z:R}"));
        if (Cache.TryGetValue(key, out var cached))
        {
            return (Vector3KeyFrameAnimation)cached;
        }

        var animation = compositor.CreateVector3KeyFrameAnimation();
        animation.SetVector3Parameter("target", value);
        Fill(compositor, animation, response, damping, (progress, k) =>
            animation.InsertExpressionKeyFrame(progress, $"this.StartingValue + (target - this.StartingValue) * {k}",
                compositor.CreateLinearEasingFunction()));
        Cache[key] = animation;
        return animation;
    }

    /// <summary>A scalar from 0 to 1 along the curve, for an expression to read as progress.</summary>
    internal static ScalarKeyFrameAnimation Progress(Compositor compositor, double response, double damping = 1)
    {
        var key = (compositor, response, damping, "progress");
        if (Cache.TryGetValue(key, out var cached))
        {
            return (ScalarKeyFrameAnimation)cached;
        }

        var animation = compositor.CreateScalarKeyFrameAnimation();
        var linear = compositor.CreateLinearEasingFunction();
        animation.InsertKeyFrame(0, 0, linear);
        Fill(compositor, animation, response, damping, (progress, k) =>
            animation.InsertKeyFrame(progress, float.Parse(k, CultureInfo.InvariantCulture), linear));
        Cache[key] = animation;
        return animation;
    }

    private static void Fill(Compositor compositor, KeyFrameAnimation animation, double response, double damping,
        Action<float, string> insert)
    {
        var settle = LyricsMotion.SettlingTime(response, damping);
        animation.Duration = TimeSpan.FromSeconds(settle);
        for (var i = 1; i <= Samples; i++)
        {
            var progress = (float)i / Samples;
            var covered = i == Samples ? 1 : LyricsMotion.Spring(progress * settle, response, damping);
            insert(progress, covered.ToString("0.######", CultureInfo.InvariantCulture));
        }
    }
}
