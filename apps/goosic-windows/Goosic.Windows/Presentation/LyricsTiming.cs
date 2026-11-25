using System;
using System.Collections.Generic;
using System.Linq;

namespace Goosic.Windows.Presentation;

/// <summary>A word the provider timed: its start, its end when supplied, and its exact text.</summary>
public readonly record struct TimedWord(long AtMs, long? EndMs, string Text);

/// <summary>
/// A lyric line as the timing rules see it. <see cref="EndMs"/> is where the line stops being
/// sung, when the provider says; without it the line stays current until the next one starts.
/// </summary>
public sealed record TimedLine(long AtMs, long? EndMs, string Text, IReadOnlyList<TimedWord> Words)
{
    public TimedLine(long atMs, string text) : this(atMs, null, text, []) { }

    /// <summary>The words, but only when they spell the line exactly; otherwise the line is drawn whole.</summary>
    public IReadOnlyList<TimedWord> SpelledWords =>
        Words.Count > 0 && string.Concat(Words.Select(word => word.Text)) == Text ? Words : [];
}

/// <summary>
/// When lyric lines and words are drawn as sung. These are the macOS shell's rules from
/// <c>LyricsTiming.swift</c>, held to the same cases; <c>docs/LYRICS_TIMING.md</c> is the authority.
/// </summary>
/// <remarks>
/// Everything here only shapes drawing. None of it writes a position, makes a seek or decides
/// anything about playback; Rust stays the only authority over that.
/// </remarks>
public static class LyricsTiming
{
    /// <summary>How far the presentation may run ahead of the last confirmed position, in seconds.</summary>
    public const double InterpolationLimit = 1;

    /// <summary>How lit the part of the current line not yet sung is drawn.</summary>
    public const double UnsungOpacity = 0.4;

    /// <summary>The sweep of a word whose start equals the next word's.</summary>
    public const double SettleMs = 220;

    /// <summary>The sweep of a line's last word when nothing supplied bounds it.</summary>
    public const double LastWordSweepMs = 450;

    /// <summary>The longest a word without a supplied end is swept; past this a gap is more likely a rest.</summary>
    public const double LongestInferredSweepMs = 2_500;

    /// <summary>The shortest wait before the next line that is drawn as a rest.</summary>
    public const long RestMinMs = 1_000;

    /// <summary>The shortest stretch without singing that is drawn as an interlude.</summary>
    public const long InterludeMinMs = 4_000;

    /// <summary>The most lines an imported document may carry.</summary>
    public const int MaxImportedLines = 900;

    /// <summary>
    /// Whether an imported document's timing can be drawn: synced, lines in strictly increasing
    /// order, and words that spell their line exactly, run forward, and never overlap the next line.
    /// The macOS shell's <c>LyricsTiming.valid</c>, case for case. Imported timing only ever shapes
    /// drawing; it never changes what plays or where.
    /// </summary>
    public static bool Valid(IReadOnlyList<TimedLine> lines, bool synced)
    {
        if (!synced || lines.Count == 0 || lines.Count > MaxImportedLines)
        {
            return false;
        }

        long previous = -1;
        foreach (var line in lines)
        {
            if (line.AtMs < 0 || line.AtMs <= previous || line.Text.EnumerateRunes().Count() > 512)
            {
                return false;
            }

            previous = line.AtMs;
            if (line.Words.Count == 0)
            {
                continue;
            }

            if (line.Words.Count > 128 || string.Concat(line.Words.Select(word => word.Text)) != line.Text)
            {
                return false;
            }

            var end = line.AtMs;
            foreach (var word in line.Words)
            {
                if (word.AtMs < end || (word.EndMs ?? word.AtMs) < word.AtMs || word.Text.Length == 0)
                {
                    return false;
                }

                end = word.EndMs ?? word.AtMs;
            }
        }

        for (var index = 0; index + 1 < lines.Count; index++)
        {
            if (lines[index].Words.Count > 0 && lines[index].Words[^1].EndMs is { } last && last > lines[index + 1].AtMs)
            {
                return false;
            }
        }

        return true;
    }

    /// <summary>The line the position has reached, or -1 before the first line or for unsynced lyrics.</summary>
    public static int ActiveIndex(IReadOnlyList<TimedLine> lines, bool synced, long positionMs)
    {
        if (!synced || lines.Count == 0 || lines[0].AtMs > positionMs)
        {
            return -1;
        }

        var index = 0;
        for (var i = 0; i < lines.Count; i++)
        {
            if (lines[i].AtMs <= positionMs)
            {
                index = i;
            }
        }

        return index;
    }

    /// <summary>The span of the recording a word is drawn across.</summary>
    /// <param name="Supplied">Whether the provider supplied the end. An inferred end only ever shapes drawing.</param>
    public readonly record struct Sweep(double StartMs, double EndMs, bool Supplied)
    {
        public double DurationMs => EndMs - StartMs;

        public double Progress(double positionMs) =>
            !double.IsFinite(positionMs) || DurationMs <= 0
                ? 0
                : Math.Clamp((positionMs - StartMs) / DurationMs, 0, 1);
    }

    /// <summary>
    /// The sweep of every word in a line, in order. A supplied start/end pair is used as it is; a
    /// start-only word is swept until the next word in the same line begins, a boundary the
    /// provider did supply, for at most <see cref="LongestInferredSweepMs"/>. A line's last word is
    /// bounded by the line's own supplied end, or takes <see cref="LastWordSweepMs"/>.
    /// </summary>
    public static IReadOnlyList<Sweep> Sweeps(TimedLine line)
    {
        var words = line.Words;
        var result = new Sweep[words.Count];
        for (var index = 0; index < words.Count; index++)
        {
            var word = words[index];
            double start = word.AtMs;
            if (word.EndMs is { } end && end > word.AtMs)
            {
                result[index] = new Sweep(start, end, true);
                continue;
            }

            if (index + 1 >= words.Count)
            {
                result[index] = line.EndMs is { } lineEnd && lineEnd > start
                    ? new Sweep(start, Math.Min(lineEnd, start + LongestInferredSweepMs), false)
                    : new Sweep(start, start + LastWordSweepMs, false);
                continue;
            }

            double next = words[index + 1].AtMs;
            result[index] = new Sweep(start,
                next > start ? Math.Min(next, start + LongestInferredSweepMs) : start + SettleMs, false);
        }

        return result;
    }

    /// <summary>
    /// How lit each character of a swept word is, from 0 to 1: the characters fill one after
    /// another across the word's sweep.
    /// </summary>
    public static double CharacterFill(Sweep sweep, int character, int characters, double positionMs) =>
        Math.Clamp(sweep.Progress(positionMs) * Math.Max(1, characters) - character, 0, 1);

    /// <summary>
    /// Where a line comes to rest: its supplied end, when the next line is far enough off, or when
    /// it is the last line. Without a supplied end the line stays current until the next starts.
    /// </summary>
    public static long? RestStart(IReadOnlyList<TimedLine> lines, int index)
    {
        if (index < 0 || index >= lines.Count || lines[index].EndMs is not { } end || end <= lines[index].AtMs)
        {
            return null;
        }

        if (index + 1 < lines.Count && lines[index + 1].AtMs - end < RestMinMs)
        {
            return null;
        }

        return end;
    }

    /// <summary>Whether the current line has finished and the next has not begun.</summary>
    public static bool Rests(IReadOnlyList<TimedLine> lines, int index, long positionMs) =>
        RestStart(lines, index) is { } end && positionMs >= end;

    /// <summary>
    /// The moments inside the interpolation limit after <paramref name="positionMs"/> at which the
    /// presentation changes: a line starting, or a line reaching its rest. In order, without repeats.
    /// </summary>
    public static IReadOnlyList<long> UpcomingChanges(IReadOnlyList<TimedLine> lines, bool synced, long positionMs)
    {
        if (!synced)
        {
            return [];
        }

        var limit = positionMs + (long)(InterpolationLimit * 1_000);
        var moments = new SortedSet<long>();
        for (var index = 0; index < lines.Count; index++)
        {
            var start = lines[index].AtMs;
            if (start > positionMs && start <= limit)
            {
                moments.Add(start);
            }

            if (RestStart(lines, index) is { } end && end > positionMs && end <= limit)
            {
                moments.Add(end);
            }
        }

        return [.. moments];
    }

    /// <summary>
    /// Whether a line the presentation already moved to should stay current when a confirmed
    /// position lands just short of its start. Without this, a clock that ran a few milliseconds
    /// ahead would flick back to the previous line and forward again.
    /// </summary>
    public static bool KeepsPresented(int presented, int confirmed, IReadOnlyList<TimedLine> lines, long positionMs)
    {
        if (presented < 0 || presented >= lines.Count || presented != confirmed + 1)
        {
            return false;
        }

        var lead = lines[presented].AtMs - positionMs;
        return lead > 0 && lead <= 250;
    }

    /// <summary>
    /// The position to draw at: the confirmed one carried forward by the time since, never by more
    /// than <see cref="InterpolationLimit"/>. Frozen while nothing is playing.
    /// </summary>
    public static double DisplayPosition(double confirmed, double elapsedSeconds, bool frozen, double rate = 1) =>
        frozen || !double.IsFinite(elapsedSeconds)
            ? confirmed
            : confirmed + Math.Min(InterpolationLimit, Math.Max(0, elapsedSeconds)) * rate;

    /// <summary>
    /// Where the presentation's clock continues from when a new position is confirmed. Within 0.3
    /// seconds the clock keeps what it was showing and runs slightly fast or slow, closing 60% of the
    /// gap over the next second; a seek or stall, or anything while frozen, is taken at once.
    /// </summary>
    public static (double Base, double Rate) Reanchor(double predicted, double confirmed, bool frozen)
    {
        var error = confirmed - predicted;
        return frozen || !double.IsFinite(predicted) || Math.Abs(error) >= 0.3
            ? (confirmed, 1)
            : (predicted, 1 + error * 0.6);
    }

    /// <summary>A stretch the provider marked as having nothing sung.</summary>
    public readonly record struct Interlude(long StartMs, long EndMs)
    {
        public double Progress(double positionMs) =>
            !double.IsFinite(positionMs) || EndMs <= StartMs
                ? 0
                : Math.Clamp((positionMs - StartMs) / (EndMs - StartMs), 0, 1);

        /// <summary>How lit one of the three dots is: they fill one after another across the interlude.</summary>
        public double DotOpacity(int dot, double positionMs) =>
            0.25 + 0.75 * Math.Clamp(Progress(positionMs) * 3 - dot, 0, 1);

        /// <summary>A slow breath, then a swell and a collapse over the last 600 ms. Still without motion.</summary>
        public double Scale(double positionMs, bool animated)
        {
            if (!animated || !double.IsFinite(positionMs))
            {
                return 1;
            }

            var breath = 1 + 0.08 * Math.Sin(positionMs / 1_800 * 2 * Math.PI);
            var remaining = EndMs - positionMs;
            if (remaining >= 600)
            {
                return breath;
            }

            var x = Math.Clamp(1 - remaining / 600, 0, 1);
            return breath * (x < 0.35 ? 1 + 0.25 * x / 0.35 : 1.25 * (1 - (x - 0.35) / 0.65));
        }
    }

    /// <summary>
    /// The interlude after a sung line: from its supplied end to the next line's start, when that
    /// is at least <see cref="InterludeMinMs"/>. A line without a supplied end has none.
    /// </summary>
    public static Interlude? InterludeAfter(IReadOnlyList<TimedLine> lines, bool synced, int index)
    {
        if (!synced || index < 0 || index + 1 >= lines.Count || lines[index].EndMs is not { } end
            || end <= lines[index].AtMs || string.IsNullOrWhiteSpace(lines[index].Text)
            || lines[index + 1].AtMs - end < InterludeMinMs)
        {
            return null;
        }

        return new Interlude(end, lines[index + 1].AtMs);
    }

    /// <summary>The introduction, when the first line begins <see cref="InterludeMinMs"/> or more in.</summary>
    public static Interlude? Introduction(IReadOnlyList<TimedLine> lines, bool synced) =>
        synced && lines.Count > 0 && lines[0].AtMs >= InterludeMinMs ? new Interlude(0, lines[0].AtMs) : null;

    /// <summary>A line the provider left empty, standing for <see cref="InterludeMinMs"/> or more of music.</summary>
    public static Interlude? EmptyLine(IReadOnlyList<TimedLine> lines, bool synced, int index)
    {
        if (!synced || index < 0 || index + 1 >= lines.Count || !string.IsNullOrWhiteSpace(lines[index].Text)
            || lines[index + 1].AtMs - lines[index].AtMs < InterludeMinMs)
        {
            return null;
        }

        return new Interlude(lines[index].AtMs, lines[index + 1].AtMs);
    }
}
