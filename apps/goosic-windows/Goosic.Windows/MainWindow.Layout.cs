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
        WindowLayout.Inset(_widthClass, Sidebar.Visibility == Visibility.Visible, Sidebar.Width);

    private double RightInset =>
        WindowLayout.Inset(_widthClass, SidePanel.Visibility == Visibility.Visible, SidePanel.Width);

    private void OnRootSizeChanged(object sender, SizeChangedEventArgs e)
    {
        var next = WindowLayout.Classify(e.NewSize.Width);
        var changed = next != _widthClass;
        _widthClass = next;
        Sidebar.Width = WindowLayout.SidebarWidth(next, e.NewSize.Width);
        SidePanel.Width = WindowLayout.SidePanelWidth(next, e.NewSize.Width);
        if (changed && next == WindowWidthClass.Narrow)
        {
            Sidebar.Visibility = Visibility.Collapsed;
        }

        ApplyPlayerLayout(next);
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

    private void ApplyPlayerLayout(WindowWidthClass widthClass)
    {
        var compact = widthClass == WindowWidthClass.Narrow;
        PlayerPill.Height = compact ? 116 : 72;
        PlayerPill.CornerRadius = new CornerRadius(compact ? 20 : 24);
        PlayerGrid.Padding = compact ? new Thickness(12, 8, 12, 8) : new Thickness(14, 0, 16, 0);
        PlayerGrid.RowDefinitions[0].Height = new GridLength(1, GridUnitType.Star);
        PlayerGrid.RowDefinitions[1].Height = compact ? GridLength.Auto : new GridLength(0);

        Grid.SetRow(PlayerMetadata, 0);
        Grid.SetColumn(PlayerMetadata, compact ? 0 : 1);
        Grid.SetColumnSpan(PlayerMetadata, compact ? 3 : 1);
        PlayerMetadata.Margin = compact ? new Thickness(0, 0, 0, 4) : new Thickness(14, 0, 8, 0);

        Grid.SetRow(TransportControls, compact ? 1 : 0);
        Grid.SetColumn(TransportControls, 0);
        Grid.SetRow(PlayerUtilities, compact ? 1 : 0);
        Grid.SetColumn(PlayerUtilities, 2);
        PillVolumeInline.Visibility = compact ? Visibility.Collapsed : Visibility.Visible;
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
        var compact = _widthClass == WindowWidthClass.Narrow;
        ContentStack.Padding = new Thickness(left, 56, right, compact ? 174 : 138);
        PlayerPill.Margin = new Thickness(LeftInset + 16, 0, RightInset + 16, 14);
        ToastHost.Margin = new Thickness(LeftInset + 16, 0, RightInset + 16, PlayerPill.Height + 26);
        // Full height, as on macOS: the player pill sits beside the panel rather than under it.
        SidePanel.Margin = new Thickness(8, 48, 8, 8);
        foreach (var carousel in _carousels)
        {
            FitCarousel(carousel, left, right);
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
            UpdateCarouselButtons(carousel);
        }
    }

    private void OnCarouselUnloaded(object sender, RoutedEventArgs e)
    {
        if (sender is ScrollViewer carousel)
        {
            carousel.ViewChanged -= OnCarouselViewChanged;
            _carousels.Remove(carousel);
        }
    }

    private void OnCarouselViewChanged(object? sender, ScrollViewerViewChangedEventArgs e)
    {
        if (sender is ScrollViewer carousel)
        {
            UpdateCarouselButtons(carousel);
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

    /// <summary>
    /// Scrolls the page, not the row, when the wheel turns over a row of cards.
    /// </summary>
    /// <remarks>
    /// A scroller that can only move sideways turns a vertical wheel into sideways movement, so
    /// scrolling down the page stalled on every shelf and slid its cards instead. A plain wheel is
    /// handed to the page here, before the row sees it; a horizontal wheel or Shift+wheel still
    /// moves the row, and so do its arrows, touch and the touchpad.
    /// </remarks>
    private void OnCarouselWheel(object sender, PointerRoutedEventArgs e)
    {
        var point = e.GetCurrentPoint(ContentScroller);
        var shift = (e.KeyModifiers & VirtualKeyModifiers.Shift) != 0;
        if (point.Properties.IsHorizontalMouseWheel || shift)
        {
            return;
        }

        e.Handled = true;
        ContentScroller.ChangeView(null, ContentScroller.VerticalOffset - point.Properties.MouseWheelDelta, null);
    }
}
