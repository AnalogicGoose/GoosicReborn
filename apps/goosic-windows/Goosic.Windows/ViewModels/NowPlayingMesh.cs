/*
 * NowPlayingMesh.cs — the palette, seeded layout, and motion behind the full-screen player's
 * procedural background, and the rule that asks YouTube's image servers for a larger cover.
 *
 * Ported from apps/goosic-swift/Sources/GoosicSwift/Core/NowPlayingMesh.swift, itself ported from
 * the previous Goosic application: src/components/layout/now-playing-background.tsx (palette
 * extraction, the seeded mesh grid), src/index.css (the drift and breathe keyframes), and
 * getHighResVariant from src/components/shared/thumbnail.tsx. That implementation was written
 * independently from the technique described by frigopedro/Apple-Music-Background, which carries
 * no license; no code from that repository is used here.
 *
 * Copyright (C) Oscar Mantilla, George Shyshov, and Goosic contributors.
 *
 * This program is free software: you can redistribute it and/or modify it under the terms of
 * the GNU General Public License as published by the Free Software Foundation, version 3.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
 * without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
 * See the GNU General Public License for more details. A copy is in LICENSE-GPL-3.0 at the
 * repository root.
 */
using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text.RegularExpressions;

namespace Goosic.Windows.ViewModels;

/// <summary>One colour the cover really contains, and how common it is relative to the most common one.</summary>
internal readonly record struct MeshSample(double Red, double Green, double Blue, double Weight);

/// <summary>One blob of the mesh: its colour and where it sits in its grid slot.</summary>
internal readonly record struct MeshCell(MeshSample Color, double X, double Y, double Scale);

internal readonly record struct MeshCellPose(double Opacity, double Dx, double Dy, double Scale);

internal readonly record struct MeshDriftPose(double Dx, double Dy, double Scale, double Degrees);

/// <summary>
/// The same rules as the macOS shell's <c>NowPlayingMesh</c>, kept to the same numbers so a cover
/// looks alike on both.
/// </summary>
internal static class NowPlayingMesh
{
    internal const int SampleSide = 48;
    internal const int GridSide = 6;
    internal const int PaletteSize = 5;

    /// <summary>The cover's dominant colours, most common first, from RGBA bytes; null without opaque pixels.</summary>
    internal static IReadOnlyList<MeshSample>? Palette(byte[] pixels, int width, int height)
    {
        if (width <= 0 || height <= 0 || pixels.Length < width * height * 4)
        {
            return null;
        }

        var buckets = new Dictionary<int, (double R, double G, double B, int Count)>();
        var order = new List<int>();
        for (var y = 0; y < height; y += 2)
        {
            for (var x = 0; x < width; x += 2)
            {
                var offset = (y * width + x) * 4;
                if (pixels[offset + 3] < 200)
                {
                    continue;
                }

                byte red = pixels[offset], green = pixels[offset + 1], blue = pixels[offset + 2];
                var key = (red >> 4) << 8 | (green >> 4) << 4 | (blue >> 4);
                if (!buckets.TryGetValue(key, out var bucket))
                {
                    order.Add(key);
                }

                buckets[key] = (bucket.R + red, bucket.G + green, bucket.B + blue, bucket.Count + 1);
            }
        }

        var candidates = order
            .Select((key, rank) =>
            {
                var bucket = buckets[key];
                return (Rank: rank, R: bucket.R / bucket.Count, G: bucket.G / bucket.Count, B: bucket.B / bucket.Count, bucket.Count);
            })
            .OrderByDescending(candidate => candidate.Count)
            .ThenBy(candidate => candidate.Rank);

        var selected = new List<(double R, double G, double B, int Count)>();
        foreach (var candidate in candidates)
        {
            var distinct = selected.All(color =>
            {
                double dr = candidate.R - color.R, dg = candidate.G - color.G, db = candidate.B - color.B;
                return Math.Sqrt(dr * dr + dg * dg + db * db) > 36;
            });
            if (distinct)
            {
                selected.Add((candidate.R, candidate.G, candidate.B, candidate.Count));
            }

            if (selected.Count == PaletteSize)
            {
                break;
            }
        }

        if (selected.Count == 0)
        {
            return null;
        }

        double largest = selected[0].Count;
        var samples = selected
            .Select(color => new MeshSample(Math.Round(color.R), Math.Round(color.G), Math.Round(color.B), color.Count / largest))
            .ToList();
        var distinctCount = samples.Count;
        while (samples.Count < PaletteSize)
        {
            samples.Add(samples[samples.Count % distinctCount]);
        }

        return samples;
    }

    /// <summary>A 32-bit FNV-1a of the palette, so the same cover always gets the same arrangement.</summary>
    internal static uint Seed(IReadOnlyList<MeshSample> palette)
    {
        uint hash = 2_166_136_261;
        foreach (var sample in palette)
        {
            var text = $"rgb({(int)sample.Red} {(int)sample.Green} {(int)sample.Blue}):"
                + sample.Weight.ToString("F4", CultureInfo.InvariantCulture);
            foreach (var unit in text)
            {
                hash = unchecked((hash ^ unit) * 16_777_619);
            }
        }

        return hash;
    }

    /// <summary>The field's blobs, row by row, placed by a generator seeded from the palette.</summary>
    internal static IReadOnlyList<MeshCell> Cells(IReadOnlyList<MeshSample> palette)
    {
        if (palette.Count == 0)
        {
            return [];
        }

        var state = Seed(palette);
        if (state == 0)
        {
            state = 1;
        }

        double Random()
        {
            state = unchecked(state * 1_664_525 + 1_013_904_223);
            return state / 4_294_967_296.0;
        }

        var total = palette.Sum(sample => sample.Weight);
        MeshSample Pick()
        {
            var target = Random() * total;
            foreach (var sample in palette)
            {
                target -= sample.Weight;
                if (target <= 0)
                {
                    return sample;
                }
            }

            return palette[0];
        }

        var cells = new List<MeshCell>(GridSide * GridSide);
        for (var i = 0; i < GridSide * GridSide; i++)
        {
            // Evaluated in this order on purpose: colour, then x, y, and scale.
            var color = Pick();
            var x = (20 + Random() * 60) / 100;
            var y = (20 + Random() * 60) / 100;
            var scale = 1.05 + Random() * 0.45;
            cells.Add(new MeshCell(color, x, y, scale));
        }

        return cells;
    }

    internal static double AlternatingProgress(double time, double duration, double delay)
    {
        if (duration <= 0)
        {
            return 0;
        }

        var elapsed = (time - delay) / duration;
        var cycle = Math.Floor(elapsed);
        var fraction = elapsed - cycle;
        return ((long)cycle % 2 != 0) ? 1 - fraction : fraction;
    }

    internal static double Ease(double progress) => (1 - Math.Cos(Math.PI * Math.Clamp(progress, 0, 1))) / 2;

    /// <summary>The whole field's slow 28-second drift, through three keyframes.</summary>
    internal static MeshDriftPose Drift(double time)
    {
        MeshDriftPose[] frames =
        [
            new(-0.03, -0.02, 1.12, -1.5),
            new(0.02, 0.03, 1.17, 1),
            new(0.04, -0.01, 1.13, 2),
        ];
        var progress = AlternatingProgress(time, 28, -8);
        var (from, to, local) = progress < 0.5
            ? (frames[0], frames[1], progress / 0.5)
            : (frames[1], frames[2], (progress - 0.5) / 0.5);
        var eased = Ease(local);
        double Mix(double a, double b) => a + (b - a) * eased;
        return new MeshDriftPose(Mix(from.Dx, to.Dx), Mix(from.Dy, to.Dy), Mix(from.Scale, to.Scale), Mix(from.Degrees, to.Degrees));
    }

    internal static readonly MeshDriftPose RestingDrift = new(-0.02, -0.01, 1.12, -1);

    /// <summary>One blob's breathing: its own period and phase, every third in reverse.</summary>
    internal static MeshCellPose Breathe(int index, double time)
    {
        var duration = 18 + index % 7 * 2;
        var delay = -((index * 1.37) % 19);
        var eased = Ease(AlternatingProgress(time, duration, delay));
        if (index % 3 == 1)
        {
            eased = 1 - eased;
        }

        return new MeshCellPose(0.82 + 0.18 * eased, -0.05 + 0.10 * eased, -0.04 + 0.08 * eased, 0.94 + 0.14 * eased);
    }

    internal static readonly MeshCellPose RestingCell = new(1, -0.03, -0.02, 1);

    /// <summary>A larger render of a YouTube Music cover, or null when no upgrade applies.</summary>
    /// <remarks>A video's <c>maxresdefault.jpg</c> does not always exist, so callers fall back to the original.</remarks>
    internal static string? HighResolutionVariant(string url, int size = 1080)
    {
        string? Replace(string pattern, string replacement)
        {
            var expression = new Regex(pattern);
            return expression.IsMatch(url) ? expression.Replace(url, replacement, 1) : null;
        }

        return Replace(@"=w\d+-h\d+", $"=w{size}-h{size}")
            ?? Replace(@"=s\d+", $"=s{size}")
            ?? Replace(@"(/vi/[^/]+/)[^./]+\.jpg", "${1}maxresdefault.jpg")
            ?? (Regex.IsMatch(url, @"(?:lh3\.googleusercontent\.com|yt3\.ggpht\.com)") && !url.Contains('=')
                ? $"{url}=w{size}-h{size}-l90-rj"
                : null);
    }
}
