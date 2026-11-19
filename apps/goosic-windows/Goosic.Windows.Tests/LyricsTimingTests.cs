using System;
using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

/// <summary>The macOS shell's LyricsTimingTests, case for case, so the two shells keep one rule.</summary>
public class LyricsTimingTests
{
    private static TimedLine L(long at, string text, long? end = null) => new(at, end, text, []);

    [Fact]
    public void ASuppliedEndDecidesTheSweepAndAStartOnlyWordRunsToTheNextWord()
    {
        var line = new TimedLine(1000, null, "One two three same far last",
        [
            new(1000, 1400, "One "),
            new(2000, null, "two "),
            new(2600, null, "three "),
            new(2600, null, "same "),
            new(9000, null, "far "),
            new(20_000, null, "last"),
        ]);
        var sweeps = LyricsTiming.Sweeps(line);
        Assert.Equal(new LyricsTiming.Sweep(1000, 1400, true), sweeps[0]);
        Assert.Equal(new LyricsTiming.Sweep(2000, 2600, false), sweeps[1]);
        // Words that start together cannot bound each other.
        Assert.Equal(2600 + LyricsTiming.SettleMs, sweeps[2].EndMs);
        // A long wait for the next word is a rest, not a held note.
        Assert.Equal(2600 + LyricsTiming.LongestInferredSweepMs, sweeps[3].EndMs);
        Assert.Equal(20_000 + LyricsTiming.LastWordSweepMs, sweeps[5].EndMs);
        Assert.Equal(0, sweeps[1].Progress(1999));
        Assert.Equal(0.5, sweeps[1].Progress(2300));
        Assert.Equal(1, sweeps[1].Progress(2600));
        Assert.Equal(0, sweeps[1].Progress(double.NaN));
        // A line without words gets no sweep.
        Assert.Empty(LyricsTiming.Sweeps(L(1000, "One two")));
    }

    [Fact]
    public void WordsDriveTheSweepOnlyWhenTheySpellTheLine()
    {
        var spelled = new TimedLine(0, null, "Hi all", [new(0, null, "Hi "), new(500, null, "all")]);
        Assert.Equal(2, spelled.SpelledWords.Count);
        var wrong = spelled with { Text = "Hi you" };
        Assert.Empty(wrong.SpelledWords);
    }

    [Fact]
    public void CharactersFillOneAfterAnotherAcrossTheirWord()
    {
        var sweep = new LyricsTiming.Sweep(0, 1000, false);
        Assert.Equal(0, LyricsTiming.CharacterFill(sweep, 0, 4, 0));
        Assert.Equal(1, LyricsTiming.CharacterFill(sweep, 0, 4, 250));
        Assert.Equal(0, LyricsTiming.CharacterFill(sweep, 1, 4, 250));
        Assert.Equal(0.5, LyricsTiming.CharacterFill(sweep, 2, 4, 625), 9);
        Assert.Equal(1, LyricsTiming.CharacterFill(sweep, 3, 4, 1000));
    }

    [Fact]
    public void InterludesComeOnlyFromWhatTheLyricsMarkAsUnsung()
    {
        TimedLine[] lines =
        [
            L(6000, "One"), L(10_000, ""), L(15_000, "Two"), L(16_000, ""), L(17_000, "Three"), L(90_000, ""),
        ];
        Assert.Equal(new LyricsTiming.Interlude(0, 6000), LyricsTiming.Introduction(lines, true));
        Assert.Equal(new LyricsTiming.Interlude(10_000, 15_000), LyricsTiming.EmptyLine(lines, true, 1));
        // A sung line followed by a long wait is not an interlude, nor is a short empty line, nor
        // the last line, whose end nothing marks.
        Assert.Null(LyricsTiming.EmptyLine(lines, true, 4));
        Assert.Null(LyricsTiming.EmptyLine(lines, true, 3));
        Assert.Null(LyricsTiming.EmptyLine(lines, true, 5));
        Assert.Null(LyricsTiming.Introduction(lines, false));
        Assert.Null(LyricsTiming.Introduction([L(1000, "Soon")], true));
    }

    [Fact]
    public void ALineRestsOnlyAtItsSuppliedEnd()
    {
        TimedLine[] lines = [L(1000, "One", 3000), L(9000, "Two"), L(12_000, "Three", 12_000)];
        Assert.False(LyricsTiming.Rests(lines, 0, 2999));
        Assert.True(LyricsTiming.Rests(lines, 0, 3000));
        // Without an end the line stays current, and an end that is not after the start is no end.
        Assert.False(LyricsTiming.Rests(lines, 1, 11_999));
        Assert.False(LyricsTiming.Rests(lines, 2, 60_000));
        Assert.False(LyricsTiming.Rests(lines, -1, 500));
        Assert.False(LyricsTiming.Rests(lines, 9, 500));

        // A line that ends just before the next begins stays lit: no rest, and none scheduled.
        TimedLine[] close = [L(1000, "One", 3000), L(3999, "Two", 6000)];
        Assert.False(LyricsTiming.Rests(close, 0, 3500));
        Assert.Empty(LyricsTiming.UpcomingChanges(close, true, 2500));
        Assert.Equal([3999L], LyricsTiming.UpcomingChanges(close, true, 3500));
        // The last line has nothing after it, so it rests at its end.
        Assert.True(LyricsTiming.Rests(close, 1, 6000));

        // Starts and ends inside the interpolation limit are both scheduled, once each.
        Assert.Equal([3000L], LyricsTiming.UpcomingChanges(lines, true, 2500));
        Assert.Equal([9000L], LyricsTiming.UpcomingChanges(lines, true, 8500));
        Assert.Empty(LyricsTiming.UpcomingChanges(lines, true, 3000));
        Assert.Empty(LyricsTiming.UpcomingChanges(lines, false, 2500));

        // The rest after a supplied end is an interlude when it is long enough.
        Assert.Equal(new LyricsTiming.Interlude(3000, 9000), LyricsTiming.InterludeAfter(lines, true, 0));
        Assert.Null(LyricsTiming.InterludeAfter(lines, true, 1));
        Assert.Null(LyricsTiming.InterludeAfter(lines, true, 2));

        // The line's end bounds its last word, which nothing else does.
        var line = new TimedLine(1000, 1900, "One", [new(1000, null, "One")]);
        Assert.Equal(1900, LyricsTiming.Sweeps(line)[0].EndMs);
    }

    [Fact]
    public void InterludeDotsFillInTurnAndFoldAwayBeforeTheNextLine()
    {
        var gap = new LyricsTiming.Interlude(0, 9000);
        Assert.Equal(0.25, gap.DotOpacity(0, 0));
        Assert.Equal(1, gap.DotOpacity(0, 3000));
        Assert.Equal(0.25, gap.DotOpacity(1, 3000));
        Assert.Equal(1, gap.DotOpacity(2, 9000));
        Assert.Equal(0, gap.Scale(9000, animated: true), 9);
        Assert.Equal(1, gap.Scale(4000, animated: false));
        Assert.Equal(1, gap.Scale(9000, animated: false));
    }

    [Fact]
    public void DisplayPositionRunsAtMostOneSecondAheadAndFreezes()
    {
        Assert.Equal(10.4, LyricsTiming.DisplayPosition(10, 0.4, false), 9);
        Assert.Equal(11, LyricsTiming.DisplayPosition(10, 5, false));
        Assert.Equal(10, LyricsTiming.DisplayPosition(10, -3, false));
        Assert.Equal(10, LyricsTiming.DisplayPosition(10, 0.4, true));
        Assert.Equal(10, LyricsTiming.DisplayPosition(10, double.NaN, false));
    }

    [Fact]
    public void AConfirmedPositionIsFollowedWithoutSteppingToIt()
    {
        // Close by: the clock stays where it was and runs a little fast or slow.
        var behind = LyricsTiming.Reanchor(10, 10.1, false);
        Assert.Equal(10, behind.Base);
        Assert.Equal(1.06, behind.Rate, 9);
        var ahead = LyricsTiming.Reanchor(10, 9.9, false);
        Assert.Equal(10, ahead.Base);
        Assert.Equal(0.94, ahead.Rate, 9);
        // A second later 60% of the gap has closed, and the clock never ran backwards.
        Assert.Equal(11.06, LyricsTiming.DisplayPosition(behind.Base, 1, false, behind.Rate), 9);
        // A seek, a stall or a pause is taken at once.
        Assert.Equal((42d, 1d), LyricsTiming.Reanchor(10, 42, false));
        Assert.Equal(10.1, LyricsTiming.Reanchor(10, 10.1, true).Base);
        Assert.Equal(5, LyricsTiming.Reanchor(double.NaN, 5, false).Base);
    }

    [Fact]
    public void ALineAlreadyPresentedSurvivesAConfirmationJustShortOfIt()
    {
        TimedLine[] lines = [L(1000, "One"), L(5000, "Two")];
        Assert.True(LyricsTiming.KeepsPresented(1, 0, lines, 4900));
        Assert.True(LyricsTiming.KeepsPresented(0, -1, lines, 900));
        // A real step back, such as a seek, is honoured.
        Assert.False(LyricsTiming.KeepsPresented(1, 0, lines, 3000));
        Assert.False(LyricsTiming.KeepsPresented(1, -1, lines, 900));
        Assert.False(LyricsTiming.KeepsPresented(1, 1, lines, 5200));
        Assert.False(LyricsTiming.KeepsPresented(7, 6, lines, 4900));
    }

    [Fact]
    public void TheActiveLineIsTheLastOneStarted()
    {
        TimedLine[] lines = [L(1000, "One"), L(5000, "Two")];
        Assert.Equal(-1, LyricsTiming.ActiveIndex(lines, true, 999));
        Assert.Equal(0, LyricsTiming.ActiveIndex(lines, true, 1000));
        Assert.Equal(1, LyricsTiming.ActiveIndex(lines, true, 60_000));
        Assert.Equal(-1, LyricsTiming.ActiveIndex(lines, false, 60_000));
        Assert.Equal(-1, LyricsTiming.ActiveIndex([], true, 60_000));
    }
    private static TimedLine[] Imported(long secondWordAt = 2000, string secondWord = "all", long? secondWordEnd = 4000) =>
    [
        new(1000, null, "Hi all", [new(1000, 2000, "Hi "), new(secondWordAt, secondWordEnd, secondWord)]),
        new(5000, "Next"),
    ];

    [Fact]
    public void ImportedTimingMatchesTextAndCannotOverlap()
    {
        Assert.True(LyricsTiming.Valid(Imported(), true));
        // A word may not start before the one before it ended.
        Assert.False(LyricsTiming.Valid(Imported(secondWordAt: 1500), true));
        // The words must spell the line.
        Assert.False(LyricsTiming.Valid(Imported(secondWord: "wrong"), true));
        // The last word may not run into the next line.
        Assert.False(LyricsTiming.Valid(Imported(secondWordEnd: 6000), true));
    }

    [Fact]
    public void ImportedDocumentsMustBeSyncedOrderedAndBounded()
    {
        Assert.False(LyricsTiming.Valid(Imported(), false));
        Assert.False(LyricsTiming.Valid([], true));
        Assert.False(LyricsTiming.Valid([L(2000, "b"), L(1000, "a")], true));
        Assert.False(LyricsTiming.Valid([L(1000, "a"), L(1000, "b")], true));
        Assert.True(LyricsTiming.Valid([L(1000, "Hi")], true));
    }
}
