using System;
using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public class LyricsMotionTests
{
    [Fact]
    public void RowsAtOrAboveTheNewLineMoveFirstAndEachBelowWaitsFiftyMilliseconds()
    {
        Assert.Equal(TimeSpan.Zero, LyricsMotion.Delay(2, 5));
        Assert.Equal(TimeSpan.Zero, LyricsMotion.Delay(5, 5));
        Assert.Equal(TimeSpan.FromMilliseconds(50), LyricsMotion.Delay(6, 5));
        Assert.Equal(TimeSpan.FromMilliseconds(150), LyricsMotion.Delay(8, 5));
        Assert.Equal(TimeSpan.FromMilliseconds(500), LyricsMotion.Delay(40, 5));
    }

    [Fact]
    public void EachRowFollowsTheCurveMeasuredFromMusic()
    {
        // From the recording: a quarter in 50 ms, half in 100 ms, three quarters in 170 ms, 90% in
        // 240 ms and 98% in 340 ms, without overshoot. The spring is within a few points of each.
        Assert.InRange(LyricsMotion.Covered(0.05), 0.15, 0.30);
        Assert.InRange(LyricsMotion.Covered(0.10), 0.40, 0.55);
        Assert.InRange(LyricsMotion.Covered(0.17), 0.70, 0.80);
        Assert.InRange(LyricsMotion.Covered(0.24), 0.86, 0.94);
        Assert.InRange(LyricsMotion.Covered(0.34), 0.96, 1.0);
        Assert.Equal(0, LyricsMotion.Covered(0));
        Assert.True(LyricsMotion.Covered(5) <= 1);
    }

    [Fact]
    public void TheColumnIsPaddedSoAnyRowCanReachTheAnchor()
    {
        Assert.Equal((288, 512), LyricsMotion.Padding(800, immersive: true));
        Assert.Equal((160, 160), LyricsMotion.Padding(100, immersive: true));
        Assert.Equal((120, 120), LyricsMotion.Padding(100, immersive: false));
    }

    [Fact]
    public void TheTargetPutsTheRowAtTheAnchorWithinReach()
    {
        Assert.Equal(1000 + 36 - 360, LyricsMotion.Target(1000, 100, 1000, 5000), 9);
        Assert.Equal(0, LyricsMotion.Target(10, 100, 1000, 5000));
        Assert.Equal(200, LyricsMotion.Target(9000, 100, 1000, 200));
    }

    [Fact]
    public void SpringsUseSwiftUIsResponseAsTheUndampedPeriod()
    {
        // .smooth(duration: 0.55): at one response a critically damped spring is 98.6% of the way.
        Assert.Equal(1 - (1 + 2 * Math.PI) * Math.Exp(-2 * Math.PI), LyricsMotion.Spring(0.55, 0.55), 6);
        // Half way in about 0.15 s, which a spring read as several times slower would not be.
        Assert.InRange(LyricsMotion.Spring(0.15, 0.55), 0.5, 0.6);
        Assert.Equal(LyricsMotion.Covered(0.1), LyricsMotion.Spring(0.1, 0.4), 9);
    }

    [Fact]
    public void TheCoverOvershootsALittleAndSettles()
    {
        var peak = 0.0;
        for (var t = 0.0; t < 1; t += 0.005)
        {
            peak = Math.Max(peak, LyricsMotion.Spring(t, LyricsMotion.CoverResponse, LyricsMotion.CoverDamping));
        }

        // A bounce of 0.25 overshoots by a few percent, never by much.
        Assert.InRange(peak, 1.01, 1.05);
        var settle = LyricsMotion.SettlingTime(LyricsMotion.CoverResponse, LyricsMotion.CoverDamping);
        Assert.Equal(1, LyricsMotion.Spring(settle, LyricsMotion.CoverResponse, LyricsMotion.CoverDamping), 3);
        Assert.InRange(LyricsMotion.Spring(LyricsMotion.SettlingTime(0.55), 0.55), 0.999, 1);
    }
}
