using System;
using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public sealed class LyricsPositionTests
{
    private static readonly DateTimeOffset Now = new(2026, 1, 1, 0, 0, 0, TimeSpan.Zero);

    [Fact]
    public void WithoutASeekTheReportedPositionIsFollowed()
    {
        Assert.Equal(42.0, LyricsPosition.Choose(42, null, false, Now));
    }

    [Fact]
    public void AHeldSeekSkipsTheSampleSoItsTargetIsNotConfirmedAgain()
    {
        var pending = new PendingSeek(120, Now);
        Assert.Null(LyricsPosition.Choose(10, pending, false, Now.AddMilliseconds(300)));
    }

    [Fact]
    public void ASeekThatLandedFollowsTheReportedPosition()
    {
        var pending = new PendingSeek(120, Now);
        Assert.Equal(120.4, LyricsPosition.Choose(120.4, pending, false, Now.AddMilliseconds(300)));
    }

    [Fact]
    public void ASeekPastItsWindowFollowsTheReportedPosition()
    {
        var pending = new PendingSeek(120, Now);
        Assert.Equal(10.0, LyricsPosition.Choose(10, pending, false, Now.AddSeconds(2)));
    }

    [Fact]
    public void ScrubbingFollowsTheReportedPositionEvenWithASeekHeld()
    {
        Assert.Equal(55.0, LyricsPosition.Choose(55, null, true, Now));
        Assert.Equal(55.0, LyricsPosition.Choose(55, new PendingSeek(120, Now), true, Now));
    }

    [Fact]
    public void ANegativeReportIsClampedToTheStart()
    {
        Assert.Equal(0.0, LyricsPosition.Choose(-1, null, false, Now));
    }
}
