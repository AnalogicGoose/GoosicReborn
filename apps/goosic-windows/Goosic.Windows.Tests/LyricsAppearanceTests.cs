using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public class LyricsAppearanceTests
{
    private static LyricLineLook Line(int index, int active, bool synced = true, bool following = true,
        bool pointed = false, bool immersive = true, bool plain = false) =>
        LyricsAppearance.Line(index, active, synced, following, pointed, immersive, plain);

    [Fact]
    public void TheLineBeingSungIsSharpWholeAndLifted()
    {
        Assert.Equal(new LyricLineLook(1, 0, 1, -2), Line(4, 4));
    }

    [Fact]
    public void LinesRecedeFurtherTheFurtherTheyAreFromIt()
    {
        var next = Line(5, 4);
        var later = Line(8, 4);
        Assert.Equal(0.32, next.Opacity, 3);
        Assert.Equal(1.45, next.Blur, 3);
        Assert.True(later.Opacity < next.Opacity);
        Assert.True(later.Blur > next.Blur);
        Assert.Equal(0.97, next.Scale);
        // The same distance behind the line reads the same as ahead of it.
        Assert.Equal(next, Line(3, 4));
    }

    [Fact]
    public void DistantLinesStayLegibleAndTheBlurStopsGrowing()
    {
        var far = Line(60, 4);
        Assert.Equal(0.16, far.Opacity);
        Assert.Equal(3, far.Blur);
    }

    [Fact]
    public void TheSidePanelIsSlightlyGentlerThanTheFullPlayer()
    {
        var panel = Line(5, 4, immersive: false);
        Assert.Equal(0.30, panel.Opacity, 3);
        Assert.Equal(1.15, panel.Blur, 3);
    }

    [Fact]
    public void BeforeTheFirstLineNothingIsNearerThanAnythingElse()
    {
        Assert.Equal(Line(0, -1), Line(30, -1));
        Assert.Equal(0.36, Line(0, -1).Opacity, 3);
    }

    [Fact]
    public void ALineBeingPointedAtOrReadIsNeverSoftened()
    {
        var pointed = Line(9, 4, pointed: true);
        Assert.Equal(1, pointed.Opacity);
        Assert.Equal(0, pointed.Blur);

        var read = Line(9, 4, following: false);
        Assert.Equal(new LyricLineLook(1, 0, 1, 0), read);
    }

    [Fact]
    public void UnsyncedLyricsHaveNoLineToFavour()
    {
        Assert.Equal(new LyricLineLook(0.85, 0, 1, 0), Line(3, -1, synced: false));
        Assert.Equal(Line(3, -1, synced: false), Line(9, -1, synced: false));
    }

    [Fact]
    public void PlainEffectsDimWithoutBlurring()
    {
        var line = Line(9, 4, plain: true);
        Assert.Equal(0, line.Blur);
        Assert.True(line.Opacity < 1);
        Assert.Equal(new LyricLineLook(1, 0, 1, -2), Line(4, 4, plain: true));
    }
}
