using System;
using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public class LyricsFollowerTests
{
    private static readonly DateTimeOffset T0 = new(2026, 11, 16, 12, 0, 0, TimeSpan.Zero);

    private static DateTimeOffset At(double seconds) => T0 + TimeSpan.FromSeconds(seconds);

    private static LyricsFollower Follower(params TimedLine[] lines)
    {
        var follower = new LyricsFollower();
        follower.Load(lines, synced: true);
        return follower;
    }

    [Fact]
    public void ALineChangesAtItsOwnStartRatherThanAtTheNextConfirmation()
    {
        var follower = Follower(new(1000, "One"), new(5400, "Two"));
        follower.Confirm(5.0, frozen: false, At(0));
        Assert.Equal(0, follower.Index);
        // The second line is due 0.4 s after the confirmation, before the next one arrives.
        Assert.Equal(At(0.4), follower.NextChange);
        Assert.False(follower.Advance(At(0.39)));
        Assert.True(follower.Advance(At(0.4)));
        Assert.Equal(1, follower.Index);
        Assert.Null(follower.NextChange);
    }

    [Fact]
    public void NothingIsScheduledWhilePaused()
    {
        var follower = Follower(new(1000, "One"), new(5400, "Two"));
        follower.Confirm(5.0, frozen: true, At(0));
        Assert.Null(follower.NextChange);
        Assert.False(follower.Running);
        Assert.Equal(5000, follower.PositionMs(At(10)));
    }

    [Fact]
    public void FreezingStopsTheClockWhereItIsAndCancelsWhatWasDue()
    {
        var follower = Follower(new TimedLine(1000, "One"), new TimedLine(5400, "Two"));
        follower.Confirm(5.0, frozen: false, At(0));
        follower.Freeze(At(0.2));
        Assert.Null(follower.NextChange);
        Assert.False(follower.Running);
        Assert.Equal(5200, follower.PositionMs(At(3)), 6);
        Assert.Equal(0, follower.Index);
    }

    [Fact]
    public void ALineAlreadyShownSurvivesAConfirmationJustShortOfIt()
    {
        var follower = Follower(new(1000, "One"), new(5000, "Two"));
        follower.Confirm(4.5, false, At(0));
        follower.Advance(At(0.5));
        Assert.Equal(1, follower.Index);
        // The page confirms a position 100 ms short, and more than 0.3 s from the clock, so the
        // clock takes it; the line already shown stays.
        follower.Confirm(4.9, false, At(0.9));
        Assert.Equal(1, follower.Index);
    }

    [Fact]
    public void ASeekBackIsTakenAtOnce()
    {
        var follower = Follower(new(1000, "One"), new(5000, "Two"));
        follower.Confirm(6, false, At(0));
        Assert.Equal(1, follower.Index);
        Assert.True(follower.Confirm(2, false, At(0.5)));
        Assert.Equal(0, follower.Index);
    }

    [Fact]
    public void ALineRestsAtItsSuppliedEndAndTheNextLineTakesOver()
    {
        var follower = Follower(new TimedLine(1000, 3000, "One", []), new TimedLine(9000, "Two"));
        follower.Confirm(2.5, false, At(0));
        Assert.Equal(0, follower.Index);
        Assert.False(follower.Resting);
        Assert.True(follower.Advance(At(0.5)));
        Assert.True(follower.Resting);
        Assert.Equal(0, follower.Index);
        follower.Confirm(8.5, false, At(6));
        Assert.True(follower.Resting);
        follower.Advance(At(6.5));
        Assert.Equal(1, follower.Index);
        Assert.False(follower.Resting);
    }

    [Fact]
    public void ARestAlreadyShownSurvivesAConfirmationJustShortOfTheEnd()
    {
        var follower = Follower(new TimedLine(1000, 3000, "One", []), new TimedLine(9000, "Two"));
        follower.Confirm(2.5, false, At(0));
        follower.Advance(At(0.5));
        Assert.True(follower.Resting);
        // 200 ms short of the end, far enough from the clock that it is taken as it is.
        follower.Confirm(2.8, false, At(1.2));
        Assert.True(follower.Resting);
    }

    [Fact]
    public void TheClockSlewsInsteadOfSteppingToACloseConfirmation()
    {
        var follower = Follower(new TimedLine(0, "One"));
        follower.Confirm(10, false, At(0));
        Assert.Equal(10_500, follower.PositionMs(At(0.5)), 6);
        // The clock showed 10.5 s; the page says 10.6 s. It keeps 10.5 and runs 6% fast.
        follower.Confirm(10.6, false, At(0.5));
        Assert.Equal(10_500, follower.PositionMs(At(0.5)), 6);
        Assert.Equal(11_560, follower.PositionMs(At(1.5)), 6);
    }

    [Fact]
    public void ANewDocumentStartsWithNothingCurrent()
    {
        var follower = Follower(new TimedLine(0, "One"));
        follower.Confirm(1, false, At(0));
        follower.Load([new TimedLine(0, "Other")], true);
        Assert.Equal(-1, follower.Index);
        Assert.Null(follower.NextChange);
        follower.Clear();
        Assert.Empty(follower.Lines);
    }
}
