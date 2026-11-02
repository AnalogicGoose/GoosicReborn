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

    [Theory]
    [InlineData(539.9, true)]
    [InlineData(540, false)]
    [InlineData(740, false)]
    public void ThePlayerTakesTwoRowsOnlyWhereOneWouldCrushTheTitle(double available, bool twoRows) =>
        Assert.Equal(twoRows, WindowLayout.PlayerUsesTwoRows(available));

    [Fact]
    public void TheMinimumWindowStillFitsTheTwoRowPlayer()
    {
        var available = WindowLayout.MinimumWidth - 2 * WindowLayout.PlayerEdgeClearance;
        Assert.True(WindowLayout.PlayerUsesTwoRows(available));
        Assert.True(available >= WindowLayout.PlayerTwoRowMinimum);
    }

    [Theory]
    [InlineData(WindowWidthClass.Narrow, true)]
    [InlineData(WindowWidthClass.Narrow, false)]
    [InlineData(WindowWidthClass.Medium, false)]
    [InlineData(WindowWidthClass.Wide, false)]
    public void TheRailAlwaysKeepsContentAndEssentialPlayerActionsReachable(WindowWidthClass widthClass, bool expanded)
    {
        var inset = WindowLayout.NavigationInset(widthClass, expanded, 280);
        Assert.Equal(WindowLayout.PanelGap + WindowLayout.SidebarRailWidth, inset);
        var available = WindowLayout.MinimumWidth - inset - 2 * WindowLayout.PlayerEdgeClearance;
        Assert.True(available >= WindowLayout.PlayerTwoRowMinimum);
    }

    [Theory]
    [InlineData(WindowWidthClass.Medium)]
    [InlineData(WindowWidthClass.Wide)]
    public void ExpandedNavigationReservesItsWidthOnlyWhenDocked(WindowWidthClass widthClass) =>
        Assert.Equal(288, WindowLayout.NavigationInset(widthClass, true, 280));

    [Fact]
    public void ThePlayerSitsInTheMiddleOfThePageBesideTheSidebar()
    {
        var (left, width) = WindowLayout.PlayerPlacement(2000, 288, 0);
        Assert.Equal(WindowLayout.PlayerMaximumWidth, width);
        Assert.Equal((288 + 2000 - 740) / 2.0, left);
    }

    [Fact]
    public void OpeningThePanelLeavesThePlayerAloneWhileItStillClearsIt()
    {
        var closed = WindowLayout.PlayerPlacement(2000, 288, 0);
        var open = WindowLayout.PlayerPlacement(2000, 288, 368);
        Assert.Equal(closed, open);
    }

    [Fact]
    public void ThePlayerStepsAsideOnlyAsFarAsThePanelReaches()
    {
        var closed = WindowLayout.PlayerPlacement(1500, 288, 0);
        var (left, width) = WindowLayout.PlayerPlacement(1500, 288, 368);
        Assert.Equal(WindowLayout.PlayerMaximumWidth, width);
        Assert.True(left < closed.Left);
        Assert.Equal(1500 - 368 - WindowLayout.PlayerEdgeClearance, left + width);
    }

    [Fact]
    public void ThePlayerNarrowsOnlyWhenThereIsNowhereLeftToStep()
    {
        var (left, width) = WindowLayout.PlayerPlacement(1250, 244, 348);
        Assert.Equal(244 + WindowLayout.PlayerEdgeClearance, left);
        Assert.Equal(1250 - 244 - 348 - 2 * WindowLayout.PlayerEdgeClearance, width);
    }

    [Fact]
    public void NarrowPanelsNeverOverflowTheMinimumWindow()
    {
        var width = WindowLayout.MinimumWidth;
        Assert.True(WindowLayout.SidebarWidth(WindowWidthClass.Narrow, width) <= width - 32);
        Assert.True(WindowLayout.SidePanelWidth(WindowWidthClass.Narrow, width) <= width - 24);
    }

    [Theory]
    [InlineData(1.0, 552, 560)]
    [InlineData(1.5, 828, 840)]
    [InlineData(1.25, 690, 700)]
    [InlineData(1.75, 966, 980)]
    public void MinimumSizeFollowsTheDisplayScale(double scale, int width, int height) =>
        Assert.Equal((width, height), WindowLayout.MinimumPixels(scale));
}
