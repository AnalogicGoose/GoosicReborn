using System;

namespace Goosic.Windows.Presentation;

public enum WindowWidthClass { Narrow, Medium, Wide }

/// <summary>What the window looks like at a given width, decided without touching a control.</summary>
/// <remarks>
/// Kept apart from <c>MainWindow</c> so the breakpoints can be tested: a wrong threshold shows up
/// as a clipped pill or a sidebar covering the page, which no compiler notices.
/// </remarks>
public static class WindowLayout
{
    public const double NarrowBelow = 820;
    public const double WideFrom = 1200;
    public const double PanelGap = 8;
    public const double ContentGutter = 26;

    /// <summary>The compact pill's two button rows need this much; below it they would clip.</summary>
    public const double MinimumWidth = 480;

    /// <summary>How far the pill floats from the panels and the window's edge on either side.</summary>
    public const double PlayerEdgeClearance = 20;

    /// <summary>The pill's width where there is room: the macOS player's, from <c>docs/UI_DESIGN.md</c>.</summary>
    public const double PlayerMaximumWidth = 740;

    /// <summary>
    /// The narrowest pill that holds everything on one row: its padding, the five transport
    /// buttons, the five utility buttons, the gaps between the three groups and a title still
    /// long enough to read.
    /// </summary>
    public const double PlayerSingleRowMinimum = 540;

    /// <summary>
    /// What the two-row pill needs: its padding and the two button groups side by side. The
    /// minimum window has to leave at least this between the pill's clearances.
    /// </summary>
    public const double PlayerTwoRowMinimum = 384;
    public const double MinimumHeight = 560;

    public static WindowWidthClass Classify(double width) => width switch
    {
        < NarrowBelow => WindowWidthClass.Narrow,
        < WideFrom => WindowWidthClass.Medium,
        _ => WindowWidthClass.Wide,
    };

    public static double SidebarWidth(WindowWidthClass widthClass, double windowWidth) => widthClass switch
    {
        WindowWidthClass.Wide => 280,
        WindowWidthClass.Medium => 236,
        _ => Math.Max(220, Math.Min(280, windowWidth - 32)),
    };

    public static double SidePanelWidth(WindowWidthClass widthClass, double windowWidth) =>
        widthClass == WindowWidthClass.Wide ? 360 : Math.Max(280, Math.Min(340, windowWidth - 24));

    /// <summary>
    /// Where the pill sits and how wide it is, from the window's left edge.
    /// </summary>
    /// <remarks>
    /// Its home is the middle of the page as the page is without the side panel. Opening the
    /// panel does not move it from there while it still clears the panel, because a control that
    /// jumps aside for something it was never under reads as a glitch. Only when the panel would
    /// reach it does it step left, by exactly as much as it has to, and only when there is no
    /// room left to step into does it get narrower.
    /// </remarks>
    public static (double Left, double Width) PlayerPlacement(double windowWidth, double leftInset, double rightInset)
    {
        var nearest = leftInset + PlayerEdgeClearance;
        var farthest = windowWidth - rightInset - PlayerEdgeClearance;
        var width = Math.Clamp(farthest - nearest, 0, PlayerMaximumWidth);
        var home = Math.Max(nearest, (leftInset + windowWidth - width) / 2);
        return (Math.Max(nearest, Math.Min(home, farthest - width)), width);
    }

    /// <summary>Whether the pill, given this much room, puts its buttons on a row under the song.</summary>
    public static bool PlayerUsesTwoRows(double availableWidth) => availableWidth < PlayerSingleRowMinimum;

    /// <summary>On a narrow window the panels float over the page instead of pushing it aside.</summary>
    public static bool PanelsOverlayContent(WindowWidthClass widthClass) => widthClass == WindowWidthClass.Narrow;

    /// <summary>How far the page is pushed in by a panel along one edge.</summary>
    public static double Inset(WindowWidthClass widthClass, bool panelVisible, double panelWidth) =>
        !PanelsOverlayContent(widthClass) && panelVisible ? PanelGap + panelWidth : 0;

    /// <summary>
    /// Whether choosing a destination in the sidebar should also close it: only where the sidebar
    /// covers the page, since otherwise it would hide what was just opened.
    /// </summary>
    public static bool ClosesSidebarAfterNavigation(WindowWidthClass widthClass) => PanelsOverlayContent(widthClass);

    /// <summary>The window's minimum size in raw pixels, which is what the presenter takes.</summary>
    public static (int Width, int Height) MinimumPixels(double rasterizationScale) =>
        ((int)Math.Ceiling(MinimumWidth * rasterizationScale), (int)Math.Ceiling(MinimumHeight * rasterizationScale));
}
