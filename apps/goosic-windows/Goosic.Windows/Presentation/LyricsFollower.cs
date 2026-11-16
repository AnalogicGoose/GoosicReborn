using System;
using System.Collections.Generic;

namespace Goosic.Windows.Presentation;

/// <summary>
/// Which lyric line is drawn as current, and whether it is at rest, between confirmed positions.
/// </summary>
/// <remarks>
/// The page confirms the position about once a second, and a line chosen from that alone changed
/// up to a second late, after its first words had been sung. This keeps a clock that runs on from
/// each confirmation, slews to the next one instead of stepping (<see cref="LyricsTiming.Reanchor"/>),
/// and lists the line starts and rests due before the next confirmation so each can be drawn on
/// time. It is the macOS shell's <c>follow</c> with the time passed in, so it can be tested.
/// It only ever draws: it never writes a position or asks for a seek.
/// </remarks>
public sealed class LyricsFollower
{
    private IReadOnlyList<TimedLine> _lines = [];
    private bool _synced;
    private double _base;
    private double _rate = 1;
    private DateTimeOffset _sampledAt;
    private bool _anchored;
    private bool _frozen = true;
    private readonly Queue<long> _pending = new();

    /// <summary>The line drawn as current, or -1 before the first line.</summary>
    public int Index { get; private set; } = -1;

    /// <summary>The current line has reached its supplied end and the next has not begun.</summary>
    public bool Resting { get; private set; }

    public IReadOnlyList<TimedLine> Lines => _lines;

    public bool Synced => _synced;

    /// <summary>Takes a new document; nothing is current until a position is confirmed.</summary>
    public void Load(IReadOnlyList<TimedLine> lines, bool synced)
    {
        _lines = lines;
        _synced = synced;
        _anchored = false;
        _frozen = true;
        _rate = 1;
        _pending.Clear();
        Index = -1;
        Resting = false;
    }

    /// <summary>Forgets the document, as for a new track.</summary>
    public void Clear() => Load([], false);

    /// <summary>The position to draw at, in milliseconds.</summary>
    public double PositionMs(DateTimeOffset now) =>
        LyricsTiming.DisplayPosition(_base, (now - _sampledAt).TotalSeconds, _frozen, _rate) * 1_000;

    /// <summary>Whether the clock is running, which is what a sweep or a row of dots needs to move.</summary>
    public bool Running => !_frozen && _anchored;

    /// <summary>Takes a confirmed position. Returns whether the current line or its rest changed.</summary>
    /// <param name="confirmedSeconds">The position the page confirmed, or a pending seek's target.</param>
    /// <param name="frozen">Paused, an advertisement, or anything else that stops the position advancing.</param>
    public bool Confirm(double confirmedSeconds, bool frozen, DateTimeOffset now)
    {
        var before = (Index, Resting);
        var predicted = _anchored
            ? LyricsTiming.DisplayPosition(_base, (now - _sampledAt).TotalSeconds, false, _rate)
            : double.NaN;
        (_base, _rate) = LyricsTiming.Reanchor(predicted, confirmedSeconds, frozen);
        _sampledAt = now;
        _anchored = true;
        _frozen = frozen;
        _pending.Clear();

        var position = (long)(_base * 1_000);
        var current = LyricsTiming.ActiveIndex(_lines, _synced, position);
        if (LyricsTiming.KeepsPresented(Index, current, _lines, position))
        {
            // The line already shown is about to start; it is neither stepped back nor at rest.
            Resting = false;
        }
        else
        {
            var rests = LyricsTiming.Rests(_lines, current, position);
            // A rest already shown survives a confirmation just short of the line's end.
            if (!rests && Resting && current == Index && current >= 0 && _lines[current].EndMs is { } end
                && end - position is >= 1 and <= 250)
            {
                rests = true;
            }

            Index = current;
            Resting = rests;
        }

        if (!frozen)
        {
            foreach (var moment in LyricsTiming.UpcomingChanges(_lines, _synced, position))
            {
                _pending.Enqueue(moment);
            }
        }

        return before != (Index, Resting);
    }

    /// <summary>
    /// Stops the clock where it is, as when an advertisement takes over: nothing runs on and
    /// nothing is scheduled until a position is confirmed again.
    /// </summary>
    public void Freeze(DateTimeOffset now)
    {
        if (!_anchored || _frozen)
        {
            return;
        }

        _base = PositionMs(now) / 1_000;
        _sampledAt = now;
        _rate = 1;
        _frozen = true;
        _pending.Clear();
    }

    /// <summary>When the next line start or rest is due, or null when none is before the next confirmation.</summary>
    public DateTimeOffset? NextChange => _pending.TryPeek(out var moment) ? Due(moment) : null;

    /// <summary>Applies every change that is due by <paramref name="now"/>. Returns whether anything changed.</summary>
    public bool Advance(DateTimeOffset now)
    {
        var before = (Index, Resting);
        while (_pending.TryPeek(out var moment) && Due(moment) <= now)
        {
            _pending.Dequeue();
            Index = LyricsTiming.ActiveIndex(_lines, _synced, moment);
            Resting = LyricsTiming.Rests(_lines, Index, moment);
        }

        return before != (Index, Resting);
    }

    private DateTimeOffset Due(long moment) =>
        _sampledAt + TimeSpan.FromSeconds((moment / 1_000.0 - _base) / _rate);
}
