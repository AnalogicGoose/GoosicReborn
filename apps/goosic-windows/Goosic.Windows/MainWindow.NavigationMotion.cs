using System;
using System.Collections.Generic;
using System.Linq;
using System.Numerics;
using Goosic.Windows.Presentation;
using Microsoft.UI.Composition;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Hosting;

namespace Goosic.Windows;

public sealed partial class MainWindow
{
    private readonly HashSet<UIElement> _catalogArrivals = [];
    private CompositionPropertySet? _indicatorEdges;
    private ToggleButton? _indicatorTarget;
    private float? _indicatorTop;
    private bool _indicatorExpanded;

    private void OnCatalogItemLoaded(object sender, RoutedEventArgs e) =>
        RevealCatalogElement(sender, NavigationMotionRules.ItemOffset);

    private void OnCatalogSectionLoaded(object sender, RoutedEventArgs e) =>
        RevealCatalogElement(sender, NavigationMotionRules.HeadingOffset);

    private void RevealCatalogElement(object sender, float offset)
    {
        if (sender is not FrameworkElement item || !AnimationsEnabled || _compositor is null) return;
        if (item.ActualWidth <= 0 || item.ActualHeight <= 0) return;
        var point = item.TransformToVisual(ContentScroller).TransformPoint(new global::Windows.Foundation.Point());
        if (point.Y > ContentScroller.ActualHeight || point.Y + item.ActualHeight < 0
            || point.X > ContentScroller.ActualWidth || point.X + item.ActualWidth < 0) return;

        _catalogArrivals.Add(item);
        ElementCompositionPreview.SetIsTranslationEnabled(item, true);
        var visual = ElementCompositionPreview.GetElementVisual(item);
        var duration = TimeSpan.FromMilliseconds(NavigationMotionRules.ArrivalMilliseconds);
        // Each view starts on arrival, with no synthetic sequence or page-wide opacity layer.
        visual.StartAnimation("Translation", Vector("Translation", new Vector3(0, offset, 0), Vector3.Zero, duration, _easeOut!));
        visual.StartAnimation("Opacity", Scalar("Opacity", 0, (float)item.Opacity, duration, _easeOut!));
    }

    private void OnCatalogItemUnloaded(object sender, RoutedEventArgs e)
    {
        if (sender is not UIElement item || !_catalogArrivals.Remove(item)) return;
        var visual = ElementCompositionPreview.GetElementVisual(item);
        visual.StopAnimation("Translation");
        visual.StopAnimation("Opacity");
        visual.Properties.InsertVector3("Translation", Vector3.Zero);
        visual.Opacity = (float)item.Opacity;
    }

    private void UpdateSidebarIndicator(bool immediate = false)
    {
        if (SidebarSelectionIndicator is null || Sidebar.XamlRoot is null || _compositor is null) return;
        var search = _sidebarExpanded ? SidebarSearchTab : SidebarSearchButton;
        var items = _sidebarExpanded ? SidebarItems : SidebarRailItems;
        var selected = search.IsChecked == true ? search
            : items.Children.OfType<ToggleButton>().FirstOrDefault(item => item.IsChecked == true);
        if (selected is null || selected.ActualHeight == 0)
        {
            SidebarSelectionIndicator.Visibility = Visibility.Collapsed;
            _indicatorTarget = null;
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
                _indicatorTarget = null;
                return;
            }
        }

        var y = (float)(point.Y + (selected.ActualHeight - NavigationMotionRules.IndicatorHeight) / 2 - SidebarLayout.Padding.Top);
        if (!immediate && _indicatorTarget == selected && _indicatorTop == y && _indicatorExpanded == _sidebarExpanded) return;
        var animate = !immediate && AnimationsEnabled && _indicatorTop.HasValue && _indicatorTarget is not null
            && _indicatorTarget != selected && _indicatorExpanded == _sidebarExpanded
            && SidebarSelectionIndicator.Visibility == Visibility.Visible;

        if (_indicatorEdges is null)
        {
            _indicatorEdges = _compositor.CreatePropertySet();
            _indicatorEdges.InsertScalar("Top", y);
            _indicatorEdges.InsertScalar("Bottom", y + NavigationMotionRules.IndicatorHeight);
            _indicatorEdges.InsertScalar("Height", NavigationMotionRules.IndicatorHeight);
            var visual = ElementCompositionPreview.GetElementVisual(SidebarSelectionIndicator);
            ElementCompositionPreview.SetIsTranslationEnabled(SidebarSelectionIndicator, true);
            visual.CenterPoint = Vector3.Zero;
            var position = _compositor.CreateExpressionAnimation("Vector3(0, edges.Top, 0)");
            position.SetReferenceParameter("edges", _indicatorEdges);
            visual.StartAnimation("Translation", position);
            var height = _compositor.CreateExpressionAnimation("Vector3(1, Max(1, edges.Bottom - edges.Top) / edges.Height, 1)");
            height.SetReferenceParameter("edges", _indicatorEdges);
            visual.StartAnimation("Scale", height);
        }

        if (animate)
        {
            var topLeads = NavigationMotionRules.TopEdgeLeads(_indicatorTop!.Value, y);
            AnimateIndicatorEdge("Top", y, topLeads);
            AnimateIndicatorEdge("Bottom", y + NavigationMotionRules.IndicatorHeight, !topLeads);
        }
        else
        {
            _indicatorEdges.StopAnimation("Top");
            _indicatorEdges.StopAnimation("Bottom");
            _indicatorEdges.InsertScalar("Top", y);
            _indicatorEdges.InsertScalar("Bottom", y + NavigationMotionRules.IndicatorHeight);
        }

        SidebarSelectionIndicator.Visibility = Visibility.Visible;
        _indicatorTarget = selected;
        _indicatorTop = y;
        _indicatorExpanded = _sidebarExpanded;
    }

    private void AnimateIndicatorEdge(string edge, float target, bool leading)
    {
        var motion = _compositor!.CreateScalarKeyFrameAnimation();
        // An implicit initial keyframe retargets from the current presentation on rapid clicks.
        motion.InsertKeyFrame(1, target, _easeOut!);
        motion.Duration = TimeSpan.FromMilliseconds(leading
            ? NavigationMotionRules.LeadingMilliseconds : NavigationMotionRules.TrailingMilliseconds);
        motion.DelayTime = TimeSpan.FromMilliseconds(leading ? 0 : NavigationMotionRules.TrailingDelayMilliseconds);
        _indicatorEdges!.StartAnimation(edge, motion);
    }

    private void OnSidebarNavigationScrolled(object? sender, ScrollViewerViewChangedEventArgs e) => UpdateSidebarIndicator(immediate: true);
}
