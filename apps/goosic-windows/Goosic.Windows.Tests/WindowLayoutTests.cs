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
    }

    [Theory]
    [InlineData(WindowWidthClass.Medium)]
    [InlineData(WindowWidthClass.Wide)]
    public void DockedPanelsInsetThePageOnlyWhileVisible(WindowWidthClass widthClass)
    {
        Assert.Equal(WindowLayout.PanelGap + 280, WindowLayout.Inset(widthClass, true, 280));
        Assert.Equal(0, WindowLayout.Inset(widthClass, false, 280));
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
    [InlineData(true)]
    [InlineData(false)]
    public void TheOverlayPanelNeverPushesThePage(bool expanded)
    {
        var inset = WindowLayout.NavigationInset(docked: false, expanded, 280);
        Assert.Equal(WindowLayout.PanelGap + WindowLayout.SidebarRailWidth, inset);
        var available = WindowLayout.MinimumWidth - inset - 2 * WindowLayout.PlayerEdgeClearance;
        Assert.True(available >= WindowLayout.PlayerTwoRowMinimum);
    }

    [Fact]
    public void OpeningTheOverlayPanelDoesNotMoveOrResizeThePlayer()
    {
        var closed = WindowLayout.NavigationInset(docked: false, false, 236);
        var open = WindowLayout.NavigationInset(docked: false, true, 236);
        Assert.Equal(WindowLayout.PlayerPlacement(900, closed, 0), WindowLayout.PlayerPlacement(900, open, 0));
    }

    [Fact]
    public void ADockedSidebarReservesItsWidth()
    {
        Assert.Equal(280, WindowLayout.NavigationInset(docked: true, true, 280));
        Assert.Equal(WindowLayout.PanelGap + WindowLayout.SidebarRailWidth,
            WindowLayout.NavigationInset(docked: true, false, 280));
    }

    [Theory]
    [InlineData(1007, false)]
    [InlineData(1008, true)]
    [InlineData(1600, true)]
    [InlineData(WindowLayout.MinimumWidth, false)]
    public void TheSidebarDocksFromWinUIsBreakpoint(double width, bool docks) =>
        Assert.Equal(docks, WindowLayout.SidebarDocks(width));

    [Theory]
    [InlineData(1008)]
    [InlineData(1100)]
    [InlineData(1199)]
    [InlineData(1200)]
    [InlineData(1600)]
    public void AnOpenSidePanelNeverSqueezesThePlayerBelowItsMinimum(double width)
    {
        var widthClass = WindowLayout.Classify(width);
        var panel = WindowLayout.Inset(widthClass, panelVisible: true, WindowLayout.SidePanelWidth(widthClass, width));
        var docked = WindowLayout.SidebarDocks(width, panel);
        var navigation = WindowLayout.NavigationInset(docked, expanded: true,
            WindowLayout.SidebarWidth(widthClass, width));
        var player = width - navigation - panel - 2 * WindowLayout.PlayerEdgeClearance;
        Assert.True(player >= WindowLayout.PlayerTwoRowMinimum,
            $"at {width} px the player gets {player} px with the side panel open");
    }

    [Fact]
    public void JustPastTheBreakpointTheSidebarGivesWayToAnOpenPanel()
    {
        var width = WindowLayout.DockedSidebarFrom;
        var widthClass = WindowLayout.Classify(width);
        var panel = WindowLayout.Inset(widthClass, panelVisible: true, WindowLayout.SidePanelWidth(widthClass, width));
        Assert.False(WindowLayout.SidebarDocks(width, panel), "both would leave the player 384 px");
        Assert.True(WindowLayout.SidebarDocks(width), "closing the panel docks it again");
    }

    [Fact]
    public void ADockedSidebarStillLeavesThePlayerOneRow()
    {
        var width = WindowLayout.DockedSidebarFrom;
        var sidebar = WindowLayout.SidebarWidth(WindowLayout.Classify(width), width);
        var inset = WindowLayout.NavigationInset(docked: true, true, sidebar);
        Assert.False(WindowLayout.PlayerUsesTwoRows(width - inset - 2 * WindowLayout.PlayerEdgeClearance));
    }

    [Theory]
    [InlineData(false, true, false, true)]   // widening past the breakpoint expands
    [InlineData(true, false, true, false)]   // narrowing past it returns to the rail
    [InlineData(true, true, false, false)]   // a collapse at a wide width is kept
    [InlineData(false, false, true, true)]   // an overlay left open stays open
    [InlineData(true, true, true, true)]
    public void TheListenersChoiceHoldsUntilTheWindowCrossesTheBreakpoint(
        bool wasDocked, bool nowDocked, bool expanded, bool expected) =>
        Assert.Equal(expected, WindowLayout.SidebarExpandedAfterResize(wasDocked, nowDocked, expanded));

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
