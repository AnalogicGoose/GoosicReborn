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
