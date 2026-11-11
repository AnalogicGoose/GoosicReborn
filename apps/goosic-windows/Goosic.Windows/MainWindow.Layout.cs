using System;
using System.Collections.Generic;
using System.IO;
using System.Threading.Tasks;
using Goosic.Windows.Presentation;
using Goosic.Windows.Service;
using Goosic.Windows.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Windows.ApplicationModel.DataTransfer;
using Windows.Media;
using Windows.System;

namespace Goosic.Windows;

public sealed partial class MainWindow : Window
{
    private const double ContentGutter = WindowLayout.ContentGutter;
    private readonly List<ScrollViewer> _carousels = [];

    private bool PanelsOverlayContent => WindowLayout.PanelsOverlayContent(_widthClass);

    private double LeftInset =>
        WindowLayout.NavigationInset(_sidebarDocked == true, _sidebarExpanded,
            WindowLayout.SidebarWidth(_widthClass, RootGrid.ActualWidth));

    private double RightInset =>
        WindowLayout.Inset(_widthClass, SidePanel.Visibility == Visibility.Visible, SidePanel.Width);

    private void OnRootSizeChanged(object sender, SizeChangedEventArgs e)
    {
        // A reveal stopped part-way leaves its inset where it stopped and cuts the sidebar off, so
        // the clip goes entirely rather than being paused.
        ClearSidebarRevealClip();
        var next = WindowLayout.Classify(e.NewSize.Width);
        var changed = next != _widthClass;
        _widthClass = next;
        SidePanel.Width = WindowLayout.SidePanelWidth(next, e.NewSize.Width);
        // The first layout takes the default for its width; later ones keep the listener's choice
        // until the window crosses the docking breakpoint. Only presentation changes here.
        var docked = WindowLayout.SidebarDocks(e.NewSize.Width);
        var previouslyDocked = _sidebarDocked;
        var expanded = previouslyDocked is { } wasDocked
            ? WindowLayout.SidebarExpandedAfterResize(wasDocked, docked, _sidebarExpanded)
            : docked;
        _sidebarDocked = docked;
        // Docking changes the sidebar's form even when it stays expanded: an overlay panel opened
        // below the breakpoint must lose its floating margins and its dismiss layer once the window
        // grows past it, or that layer keeps swallowing clicks on a page laid out beside a dock.
        if (expanded != _sidebarExpanded || changed || previouslyDocked != docked)
        {
            SetSidebarExpanded(expanded, moveFocus: false);
        }

        Sidebar.Width = _sidebarExpanded ? WindowLayout.SidebarWidth(next, e.NewSize.Width) : WindowLayout.SidebarRailWidth;
        ApplyInsets();
    }

    /// <summary>Stops the window from shrinking past the narrowest layout that still fits.</summary>
    /// <remarks>The presenter takes raw pixels, so the limit follows the window's scale.</remarks>
    private void ApplyMinimumWindowSize()
    {
        if (AppWindow.Presenter is Microsoft.UI.Windowing.OverlappedPresenter presenter
            && RootGrid.XamlRoot is { } root)
        {
            (presenter.PreferredMinimumWidth, presenter.PreferredMinimumHeight) =
                WindowLayout.MinimumPixels(root.RasterizationScale);
        }
    }

    /// <summary>
    /// Lays the pill out on one row, or on two where one would crush the title.
    /// </summary>
    /// <remarks>
    /// Decided by the room the pill actually has between the panels, not by the window's width
    /// class: a medium window with both panels open leaves less than a narrow one with neither.
    /// Returns whether the two-row layout is in use, for the page's clearance under it.
    /// </remarks>
    private bool ApplyPlayerLayout()
    {
        var available = RootGrid.ActualWidth - LeftInset - RightInset - 2 * WindowLayout.PlayerEdgeClearance;
        var compact = WindowLayout.PlayerUsesTwoRows(available);
        PlayerPill.Height = compact ? 102 : 64;
        PlayerPill.CornerRadius = new CornerRadius(compact ? 24 : 28);
        PlayerGrid.Padding = compact ? new Thickness(16, 8, 16, 8) : new Thickness(16, 0, 16, 0);
        PlayerGrid.RowSpacing = compact ? 4 : 0;
        PlayerGrid.RowDefinitions[0].Height = new GridLength(1, GridUnitType.Star);
        PlayerGrid.RowDefinitions[1].Height = compact ? GridLength.Auto : new GridLength(0);

        Grid.SetRow(PlayerMetadata, 0);
        Grid.SetColumn(PlayerMetadata, compact ? 0 : 1);
        Grid.SetColumnSpan(PlayerMetadata, compact ? 3 : 1);

        Grid.SetRow(TransportControls, compact ? 1 : 0);
        Grid.SetColumn(TransportControls, 0);
        Grid.SetRow(PlayerUtilities, compact ? 1 : 0);
        Grid.SetColumn(PlayerUtilities, 2);
        return compact;
    }

    /// <summary>
    /// Keeps the content clear of the floating panels without clipping it at their edge.
    /// </summary>
    /// <remarks>
    /// The page is padded so its first card and its headings start beside the sidebar, while each
    /// row of cards is pulled back out to the window's edges and padded in again: its first card
    /// lines up with the page, and scrolling it slides the cards under the glass rather than
    /// cutting them off at a margin.
    /// </remarks>
    private void ApplyInsets()
    {
        var left = LeftInset + ContentGutter;
        var right = RightInset + ContentGutter;
        var compact = ApplyPlayerLayout();
        var clearance = WindowLayout.PlayerEdgeClearance;
        // Title-bar controls clear the temporary panel even though the page keeps its rail inset.
        TitleBarStrip.Margin = new Thickness(Sidebar.Width + 56, 0, 0, 0);
        BackButton.Margin = new Thickness(Sidebar.Width + 16, 14, 0, 0);
        SearchChrome.Margin = new Thickness(left, 56, right, 0);
        var top = Model.IsSearchPage ? 56 + Math.Max(100, SearchChrome.ActualHeight) + 48 : 56;
        ContentStack.Padding = new Thickness(left, top, right, compact ? 166 : 128);
        LayOutArtistHero();
        FitSearchBrowseCards(left, right);
        if (RootGrid.ActualWidth > 0)
        {
            // Placed by hand rather than centred between the panels: see PlayerPlacement.
            var (pillLeft, pillWidth) = WindowLayout.PlayerPlacement(RootGrid.ActualWidth, LeftInset, RightInset);
            PlayerPill.Width = pillWidth;
            PlayerPill.Margin = new Thickness(pillLeft, 0, 0, 12);
        }

        ToastHost.Margin = new Thickness(LeftInset + clearance, 0, RightInset + clearance, PlayerPill.Height + 24);
        // Full height, as on macOS: the player pill sits beside the panel rather than under it.
        SidePanel.Margin = new Thickness(8, 48, 8, 8);
        foreach (var carousel in _carousels)
        {
            FitCarousel(carousel, left, right);
        }
    }

    private void OnSearchChromeSizeChanged(object sender, SizeChangedEventArgs e) => ApplyInsets();

    private void FitSearchBrowseCards(double left, double right)
    {
        var available = Math.Max(0, Math.Min(1000, RootGrid.ActualWidth - left - right));
        BrowseCategoryGrid.Width = available;
        var columns = available >= 860 ? 4 : available >= 420 ? 2 : 1;
        if (BrowseCategoryGrid.ColumnDefinitions.Count == columns)
        {
            return;
        }

        BrowseCategoryGrid.ColumnDefinitions.Clear();
        BrowseCategoryGrid.RowDefinitions.Clear();
        for (var index = 0; index < columns; index++)
        {
            BrowseCategoryGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        }

        for (var index = 0; index < 4 / columns; index++)
        {
            BrowseCategoryGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        }

        for (var index = 0; index < BrowseCategoryGrid.Children.Count; index++)
        {
            Grid.SetColumn((FrameworkElement)BrowseCategoryGrid.Children[index], index % columns);
            Grid.SetRow((FrameworkElement)BrowseCategoryGrid.Children[index], index / columns);
        }
    }

    private static void FitCarousel(ScrollViewer carousel, double left, double right)
    {
        carousel.Margin = new Thickness(-left, 0, -right, 0);
        // Padded through a border: an ItemsControl's own Padding never reaches its panel.
        if (carousel.Content is Border row)
        {
            row.Padding = new Thickness(left, 0, right, 0);
        }
    }

    private void OnCarouselLoaded(object sender, RoutedEventArgs e)
    {
        if (sender is ScrollViewer carousel && !_carousels.Contains(carousel))
        {
            _carousels.Add(carousel);
            FitCarousel(carousel, LeftInset + ContentGutter, RightInset + ContentGutter);
            carousel.ViewChanged += OnCarouselViewChanged;
            carousel.AddHandler(UIElement.PointerWheelChangedEvent,
                new PointerEventHandler(OnCarouselWheelBubble), handledEventsToo: true);
            UpdateCarouselButtons(carousel);
            _ = LoadMoreIfCarouselNearEndAsync(carousel);
        }
    }

    private void OnCarouselUnloaded(object sender, RoutedEventArgs e)
    {
        if (sender is ScrollViewer carousel)
        {
            carousel.ViewChanged -= OnCarouselViewChanged;
            carousel.RemoveHandler(UIElement.PointerWheelChangedEvent,
                new PointerEventHandler(OnCarouselWheelBubble));
            _carousels.Remove(carousel);
        }
    }

    private void OnCarouselViewChanged(object? sender, ScrollViewerViewChangedEventArgs e)
    {
        if (sender is ScrollViewer carousel)
        {
            UpdateCarouselButtons(carousel);
            _ = LoadMoreIfCarouselNearEndAsync(carousel);
        }
    }

    /// <summary>
    /// Asks for the rest of a list that grows sideways once the listener nears its end, the way
    /// a page asks when its own end comes into view. The Songs and Videos tabs, the only lists that
    /// continue this way, hold that one shelf.
    /// </summary>
    private async Task LoadMoreIfCarouselNearEndAsync(ScrollViewer carousel)
    {
        if (Model.HasMore && Model.MoreGoesSideways
            && carousel.HorizontalOffset > carousel.ScrollableWidth - 900)
        {
            await Model.LoadMoreAsync();
        }
    }

    private static void UpdateCarouselButtons(ScrollViewer carousel)
    {
        if (VisualTreeHelper.GetParent(carousel) is not Panel shelf ||
            shelf.Children.Count == 0 || shelf.Children[0] is not Grid header)
        {
            return;
        }

        foreach (var child in header.Children)
        {
            if (child is not StackPanel arrows)
            {
                continue;
            }

            foreach (var item in arrows.Children)
            {
                if (item is Button { Tag: string direction } arrow)
                {
                    arrow.IsEnabled = direction == "-1"
                        ? carousel.HorizontalOffset > 0.5
                        : carousel.HorizontalOffset < carousel.ScrollableWidth - 0.5;
                }
            }
        }
    }

    private static bool IsVerticalPageWheel(PointerRoutedEventArgs e, UIElement relativeTo) =>
        !e.GetCurrentPoint(relativeTo).Properties.IsHorizontalMouseWheel
        && (e.KeyModifiers & VirtualKeyModifiers.Shift) == 0;

    private void OnCarouselWheel(object sender, PointerRoutedEventArgs e)
    {
        if (IsVerticalPageWheel(e, ContentScroller))
        {
            // Skip the shelf's native conversion of a vertical wheel into horizontal motion.
            e.Handled = true;
        }
    }

    private void OnCarouselWheelBubble(object sender, PointerRoutedEventArgs e)
    {
        if (IsVerticalPageWheel(e, ContentScroller))
        {
            // The shelf's class handler has now been skipped. Resume bubbling to the page,
            // which retains native wheel distance, animation and rapid-tick accumulation.
            e.Handled = false;
        }
    }
}
