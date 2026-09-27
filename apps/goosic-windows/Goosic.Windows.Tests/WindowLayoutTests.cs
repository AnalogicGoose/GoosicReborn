using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public class WindowLayoutTests
{
    [Theory]
    [InlineData(480, WindowWidthClass.Narrow)]
    [InlineData(819.9, WindowWidthClass.Narrow)]
    [InlineData(820, WindowWidthClass.Medium)]
    [InlineData(1199.9, WindowWidthClass.Medium)]
    [InlineData(1200, WindowWidthClass.Wide)]
    public void ClassifiesBreakpoints(double width, WindowWidthClass expected) =>
        Assert.Equal(expected, WindowLayout.Classify(width));

    [Fact]
    public void NarrowPanelsFloatOverThePageAndDoNotInsetIt()
    {
        Assert.Equal(0, WindowLayout.Inset(WindowWidthClass.Narrow, panelVisible: true, panelWidth: 280));
        Assert.True(WindowLayout.ClosesSidebarAfterNavigation(WindowWidthClass.Narrow));
    }

    [Theory]
    [InlineData(WindowWidthClass.Medium)]
    [InlineData(WindowWidthClass.Wide)]
    public void DockedPanelsInsetThePageOnlyWhileVisible(WindowWidthClass widthClass)
    {
        Assert.Equal(WindowLayout.PanelGap + 280, WindowLayout.Inset(widthClass, true, 280));
        Assert.Equal(0, WindowLayout.Inset(widthClass, false, 280));
        Assert.False(WindowLayout.ClosesSidebarAfterNavigation(widthClass));
    }

    [Fact]
    public void NarrowPanelsNeverOverflowTheMinimumWindow()
    {
        var width = WindowLayout.MinimumWidth;
        Assert.True(WindowLayout.SidebarWidth(WindowWidthClass.Narrow, width) <= width - 32);
        Assert.True(WindowLayout.SidePanelWidth(WindowWidthClass.Narrow, width) <= width - 24);
    }

    [Theory]
    [InlineData(1.0, 480, 560)]
    [InlineData(1.5, 720, 840)]
    [InlineData(1.25, 600, 700)]
    [InlineData(1.75, 840, 980)]
    public void MinimumSizeFollowsTheDisplayScale(double scale, int width, int height) =>
        Assert.Equal((width, height), WindowLayout.MinimumPixels(scale));
}
