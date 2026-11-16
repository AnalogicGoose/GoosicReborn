using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public class LyricsAppearanceTests
{
    private static LyricLineLook Line(int index, int current, bool resting = false, bool synced = true,
        bool following = true, bool pointed = false, bool plain = false) =>
        LyricsAppearance.Line(index, current, resting, synced, following, pointed, plain);

    [Fact]
    public void TheLineBeingSungIsSharpAndWhole()
    {
        Assert.Equal(new LyricLineLook(1, 0), Line(4, 4));
    }

    [Fact]
    public void WhatIsAboutToBeSungStaysReadableAndSharperThanWhatWasSung()
    {
        var next = Line(5, 4);
        Assert.Equal(0.6, next.Opacity, 3);
        Assert.Equal(0.5, next.Blur, 3);
        var later = Line(8, 4);
        Assert.Equal(0.39, later.Opacity, 3);
        Assert.Equal(2, later.Blur, 3);

        var previous = Line(3, 4);
        Assert.Equal(0.34, previous.Opacity, 3);
        Assert.Equal(1.4, previous.Blur, 3);
        Assert.True(next.Opacity > previous.Opacity);
        Assert.True(next.Blur < previous.Blur);
    }

    [Fact]
    public void DistantLinesStayLegibleAndTheBlurStopsGrowing()
    {
        Assert.Equal(new LyricLineLook(0.3, 3), Line(60, 4));
        Assert.Equal(new LyricLineLook(0.16, 3), Line(0, 60));
    }

    [Fact]
    public void WhileNothingIsSungTheFinishedLineAndEverythingAfterItAreEquallyReadable()
    {
        // At rest the current line is no longer lit, and it reads like the lines still to come.
        Assert.False(LyricsAppearance.Lit(4, 4, resting: true));
        Assert.Equal(new LyricLineLook(0.62, 0), Line(4, 4, resting: true));
        Assert.Equal(new LyricLineLook(0.62, 0), Line(9, 4, resting: true));
        // What was sung before it still falls back.
        Assert.Equal(Line(3, 4), Line(3, 4, resting: true));
        // Before the first line every line is still to come.
        Assert.Equal(new LyricLineLook(0.62, 0), Line(0, -1));
        Assert.Equal(Line(0, -1), Line(30, -1));
    }

    [Fact]
    public void ALineBeingPointedAtOrReadIsNeverSoftened()
    {
        Assert.Equal(new LyricLineLook(1, 0), Line(9, 4, pointed: true));
        Assert.Equal(new LyricLineLook(1, 0), Line(9, 4, following: false));
    }

    [Fact]
    public void UnsyncedLyricsHaveNoLineToFavour()
    {
        Assert.Equal(new LyricLineLook(0.85, 0), Line(3, -1, synced: false));
        Assert.Equal(Line(3, -1, synced: false), Line(9, -1, synced: false));
    }

    [Fact]
    public void PlainEffectsDimWithoutBlurring()
    {
        var line = Line(9, 4, plain: true);
        Assert.Equal(0, line.Blur);
        Assert.True(line.Opacity < 1);
        Assert.Equal(new LyricLineLook(1, 0), Line(4, 4, plain: true));
    }
}
