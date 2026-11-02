using System;
using System.Linq;
using System.Collections.Generic;
using System.Numerics;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Hosting;
using Microsoft.UI.Xaml.Media;

namespace Goosic.Windows;

public sealed partial class MainWindow
{
    private int _catalogArrivalIndex;
    private readonly HashSet<UIElement> _catalogArrivals = [];
    private float? _sidebarIndicatorY;

    private void OnCatalogItemLoaded(object sender, RoutedEventArgs e)
    {
        if (sender is not FrameworkElement item || !AnimationsEnabled || _compositor is null) return;
        var point = item.TransformToVisual(ContentScroller).TransformPoint(new global::Windows.Foundation.Point());
        if (point.Y > ContentScroller.ActualHeight || point.Y + item.ActualHeight < 0) return;
        _catalogArrivals.Add(item);
        ElementCompositionPreview.SetIsTranslationEnabled(item, true);
        var settle = Vector("Translation", new Vector3(0, 8, 0), Vector3.Zero,
            TimeSpan.FromMilliseconds(180), _easeOut!);
        settle.DelayTime = TimeSpan.FromMilliseconds(Math.Min(_catalogArrivalIndex++, 7) * 18);
        settle.DelayBehavior = Microsoft.UI.Composition.AnimationDelayBehavior.SetInitialValueBeforeDelay;
        ElementCompositionPreview.GetElementVisual(item).StartAnimation("Translation", settle);
    }

    private void OnCatalogItemUnloaded(object sender, RoutedEventArgs e)
    {
        if (sender is not UIElement item) return;
        _catalogArrivals.Remove(item);
        var visual = ElementCompositionPreview.GetElementVisual(item);
        visual.StopAnimation("Translation");
        visual.Properties.InsertVector3("Translation", Vector3.Zero);
    }

    private void UpdateSidebarIndicator()
    {
        if (SidebarSelectionIndicator is null || Sidebar.XamlRoot is null) return;
        var search = _sidebarExpanded ? SidebarSearchTab : SidebarSearchButton;
        var items = _sidebarExpanded ? SidebarItems : SidebarRailItems;
        var selected = search.IsChecked == true ? search
            : items.Children.OfType<ToggleButton>().FirstOrDefault(item => item.IsChecked == true);
        if (selected is null || selected.ActualHeight == 0)
        {
            SidebarSelectionIndicator.Visibility = Visibility.Collapsed;
            return;
        }

        var point = selected.TransformToVisual(SidebarLayout).TransformPoint(new global::Windows.Foundation.Point());
        if (selected != search)
        {
            var viewport = _sidebarExpanded ? SidebarNavigation : SidebarRail;
            var top = viewport.TransformToVisual(SidebarLayout).TransformPoint(new global::Windows.Foundation.Point()).Y;
            if (point.Y < top || point.Y + selected.ActualHeight > top + viewport.ActualHeight)
            {
                SidebarSelectionIndicator.Visibility = Visibility.Collapsed;
                return;
            }
        }

        var y = (float)(point.Y + (selected.ActualHeight - 20) / 2 - SidebarLayout.Padding.Top);
        if (SidebarSelectionIndicator.Visibility == Visibility.Visible && _sidebarIndicatorY == y) return;
        SidebarSelectionIndicator.TranslationTransition = AnimationsEnabled && _sidebarIndicatorY.HasValue
            ? new Vector3Transition { Duration = TimeSpan.FromMilliseconds(180) } : null;
        SidebarSelectionIndicator.Translation = new Vector3(0, y, 0);
        SidebarSelectionIndicator.Visibility = Visibility.Visible;
        _sidebarIndicatorY = y;
    }

    private void OnSidebarNavigationScrolled(object? sender, ScrollViewerViewChangedEventArgs e) => UpdateSidebarIndicator();
}
