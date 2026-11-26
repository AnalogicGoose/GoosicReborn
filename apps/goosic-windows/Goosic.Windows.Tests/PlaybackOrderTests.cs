using System;
using System.Collections.Generic;
using System.Linq;
using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public class PlaybackOrderTests
{
    [Theory]
    [InlineData("abc", "abc", true, true)]
    [InlineData("abc", "abc", false, false)]
    [InlineData("old", "new", true, false)]
    [InlineData(null, "new", true, false)]
    public void OnlyTheCurrentPlaysHeardEndCounts(string? ended, string current, bool armed, bool expected) =>
        Assert.Equal(expected, PlaybackOrder.AcceptsEnd(ended, current, armed));
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
