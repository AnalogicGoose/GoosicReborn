using System;
using System.Collections.Generic;
using System.Linq;
using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public class EndGuardTests
{
    [Fact]
    public void OnlyTheCurrentPlaysHeardEndCountsAndOnlyOnce()
    {
        var guard = new PlaybackRules.EndGuardState();
        PlaybackRules.EndGuardBegin(ref guard);
        Assert.False(PlaybackRules.EndGuardObserve(ref guard, true, false, "ended", 229, 229, false).Advance);
        PlaybackRules.EndGuardObserve(ref guard, true, false, "playing", 10, 229, false);
        Assert.False(PlaybackRules.EndGuardObserve(ref guard, false, false, "ended", 229, 229, false).Advance);
        Assert.True(PlaybackRules.EndGuardObserve(ref guard, true, false, "ended", 229, 229, false).Advance);
        Assert.False(PlaybackRules.EndGuardObserve(ref guard, true, false, "ended", 229, 229, false).Advance);
        Assert.False(PlaybackRules.EndGuardPageEnded(ref guard, true));
    }

    [Fact]
    public void ContentAnAdvertisementLeftPausedIsResumedNotEnded()
    {
        var guard = new PlaybackRules.EndGuardState();
        PlaybackRules.EndGuardObserve(ref guard, true, false, "playing", 1, 229, false);
        Assert.False(PlaybackRules.EndGuardObserve(ref guard, true, true, "ended", 30, 30, false).Advance);
        var after = PlaybackRules.EndGuardObserve(ref guard, true, false, "paused", 229, 229, false);
        Assert.True(after.Resume);
        Assert.False(after.Advance);
    }
}

public class TrackEndTests
{
    [Theory]
    [InlineData("ended", 10, 229, false, true)]
    [InlineData("paused", 228.8, 229, false, true)]
    [InlineData("paused", 228.8, 229, true, false)]
    [InlineData("paused", 120, 229, false, false)]
    [InlineData("paused", 0, 0, false, false)]
    [InlineData("playing", 229, 229, false, false)]
    public void APauseAtTheLastMomentIsTheEndUnlessTheListenerPaused(
        string state, double position, double duration, bool listenerPaused, bool expected) =>
        Assert.Equal(expected, PlaybackOrder.IsFinished(state, position, duration, listenerPaused));
}

public class SeekSettleTests
{
    private static readonly DateTimeOffset At = new(2026, 9, 17, 12, 0, 0, TimeSpan.Zero);

    [Fact]
    public void ARequestedSeekIsShownUntilThePlayerLandsNearIt()
    {
        var pending = new PendingSeek(120, At);
        Assert.Equal((120d, false), SeekSettle.Show(30, pending, At.AddMilliseconds(300)));
        Assert.Equal((119.5d, true), SeekSettle.Show(119.5, pending, At.AddMilliseconds(400)));
    }

    [Fact]
    public void APlayerThatNeverLandsIsBelievedAfterASecond() =>
        Assert.Equal((30d, true), SeekSettle.Show(30, new PendingSeek(120, At), At.AddSeconds(1)));

    [Fact]
    public void WithoutASeekThePlayerIsShown() =>
        Assert.Equal((42d, true), SeekSettle.Show(42, null, At));
}

public class TimeTextTests
{
    [Theory]
    [InlineData(0, "0:00")]
    [InlineData(9, "0:09")]
    [InlineData(222, "3:42")]
    [InlineData(3_723, "1:02:03")]
    [InlineData(-1, "0:00")]
    [InlineData(double.NaN, "0:00")]
    public void TimesReadAsTheMacWritesThem(double seconds, string expected) =>
        Assert.Equal(expected, PlaybackRules.TimeText(seconds));
}
