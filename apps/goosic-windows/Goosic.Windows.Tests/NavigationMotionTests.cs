using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public class NavigationMotionTests
{
    [Theory]
    [InlineData(300, 100, true)]
    [InlineData(100, 300, false)]
    public void LeadingEdgeTracksTheDirectionOfNavigation(float from, float to, bool topLeads) =>
        Assert.Equal(topLeads, NavigationMotionRules.TopEdgeLeads(from, to));

    [Fact]
    public void ContentSettlesDeeperThanHeadingsAndTheIndicatorTailCatchesUp()
    {
        Assert.True(NavigationMotionRules.ItemOffset > NavigationMotionRules.HeadingOffset);
        Assert.True(NavigationMotionRules.HeadingOffset > 0);
        Assert.True(NavigationMotionRules.LeadingMilliseconds < NavigationMotionRules.TrailingMilliseconds);
        Assert.InRange(NavigationMotionRules.TrailingMilliseconds + NavigationMotionRules.TrailingDelayMilliseconds, 250, 350);
    }
}
