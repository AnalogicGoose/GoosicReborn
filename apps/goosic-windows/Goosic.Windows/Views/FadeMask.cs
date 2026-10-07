using System;
using System.Numerics;
using Goosic.Windows.Service;
using Microsoft.UI.Composition;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Hosting;

namespace Goosic.Windows.Views;

/// <summary>
/// Fades an element out through a gradient's alpha, so whatever is behind it shows through.
/// </summary>
/// <remarks>
/// XAML has no opacity mask, and the usual substitute is a gradient of the background's colour
/// laid over the content. That only works over a flat background: over the playing cover's colours
/// it is a dark band across the top of the window. Here the content is drawn through the mask
/// instead, as the macOS shell does, so it thins out and the real background is what remains.
/// <para>
/// Both elements go on being laid out and hit-tested as usual. Only their own drawing is switched
/// off, at the compositor, and a single visual above them draws the one through the other.
/// </para>
/// </remarks>
internal static class FadeMask
{
    /// <param name="host">The panel holding both elements; nothing else may be inside it.</param>
    /// <param name="content">What is faded.</param>
    /// <param name="mask">Opaque where the content shows, transparent where it does not.</param>
    /// <returns>Whether the mask is in place; when it is not, the content draws as it always did.</returns>
    internal static bool Attach(FrameworkElement host, UIElement content, UIElement mask)
    {
        try
        {
            mask.Visibility = Visibility.Visible;
            var compositor = ElementCompositionPreview.GetElementVisual(host).Compositor;
            var brush = compositor.CreateMaskBrush();
            brush.Source = Redirect(compositor, content);
            brush.Mask = Redirect(compositor, mask);
            var visual = compositor.CreateSpriteVisual();
            visual.RelativeSizeAdjustment = Vector2.One;
            visual.Brush = brush;
            ElementCompositionPreview.SetElementChildVisual(host, visual);
            return true;
        }
        catch (Exception error)
        {
            // Without it the page is merely not faded.
            mask.Visibility = Visibility.Collapsed;
            ElementCompositionPreview.GetElementVisual(content).Opacity = 1;
            BridgeLog.Write($"fade mask unavailable: {error.GetType().Name}");
            return false;
        }
    }

    /// <summary>A brush that draws the element live, with the element's own drawing turned off.</summary>
    private static CompositionBrush Redirect(Compositor compositor, UIElement element)
    {
        var visual = ElementCompositionPreview.GetElementVisual(element);
        var surface = compositor.CreateVisualSurface();
        surface.SourceVisual = visual;
        var size = compositor.CreateExpressionAnimation("source.Size");
        size.SetReferenceParameter("source", visual);
        surface.StartAnimation(nameof(surface.SourceSize), size);
        var brush = compositor.CreateSurfaceBrush(surface);
        brush.Stretch = CompositionStretch.None;
        brush.HorizontalAlignmentRatio = 0;
        brush.VerticalAlignmentRatio = 0;
        // The surface draws the visual's content whatever its own opacity is.
        visual.Opacity = 0;
        return brush;
    }
}
