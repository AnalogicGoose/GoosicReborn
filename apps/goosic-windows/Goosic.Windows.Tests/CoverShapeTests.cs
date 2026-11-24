using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

/// <summary>
/// The same cases as the <c>cover_aspect</c> and <c>fit_cover</c> tests in <c>goosic-shell-support</c>
/// and the macOS <c>CoverShapeTests</c>, so a cover takes one shape on every platform.
/// </summary>
public class CoverShapeTests
{
    [Fact]
    public void CoverAspectFollowsThePictureWithinBounds()
    {
        Assert.Equal(1, CoverShape.Aspect(544, 544));
        Assert.Equal(16.0 / 9.0, CoverShape.Aspect(1280, 720), 9);
        Assert.Equal(4.0 / 3.0, CoverShape.Aspect(480, 360), 9);
        Assert.Equal(CoverShape.MaxAspect, CoverShape.Aspect(3000, 300));
        Assert.Equal(CoverShape.MinAspect, CoverShape.Aspect(100, 1000));
        Assert.Equal(1, CoverShape.Aspect(0, 720));
        Assert.Equal(1, CoverShape.Aspect(double.NaN, 720));
    }

    [Fact]
    public void ACoverFitsItsSlotOnTheLongerSide()
    {
        Assert.Equal((520.0, 520.0), CoverShape.Fit(1, 520, 520));
        var wide = CoverShape.Fit(16.0 / 9.0, 520, 520);
        Assert.Equal(520, wide.Width);
        Assert.Equal(292.5, wide.Height, 9);
        var tall = CoverShape.Fit(9.0 / 16.0, 520, 520);
        Assert.Equal(292.5, tall.Width, 9);
        Assert.Equal(520, tall.Height);
        Assert.Equal((0.0, 0.0), CoverShape.Fit(1, 0, 0));
    }
}
