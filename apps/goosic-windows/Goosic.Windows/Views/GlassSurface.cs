using System;
using System.Numerics;
using Microsoft.Graphics.Canvas;
using Microsoft.Graphics.Canvas.Effects;
using Microsoft.UI;
using Microsoft.UI.Composition;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Hosting;
using Microsoft.UI.Xaml.Media;
using Windows.UI;

namespace Goosic.Windows.Views;

/// <summary>
/// A "liquid glass" backdrop: what lies behind the panel, blurred and made more vivid, under a
/// light tint, a specular sheen and a bright rim.
/// </summary>
/// <remarks>
/// <para>
/// Written for this shell with the Composition APIs, after the look of Apple's Liquid Glass and of
/// community WinUI experiments with it; no code is taken from those, which carry no licence.
/// </para>
/// <para>
/// It is an empty element placed behind a panel's content, because a composition child visual
/// draws above an element's own XAML children. The backdrop brush samples the live window
/// content, so shelves and covers scrolling under the panel show through it as they move.
/// Composition effect graphs cannot run Direct2D's displacement or turbulence effects, which is
/// what true refraction needs; the glass is instead suggested the way the eye reads it -- a
/// saturated blur, a sheen that falls off from the top edge, and a rim that is brighter where
/// light would catch it.
/// </para>
/// <para>
/// If effects are unavailable the surface falls back to the in-app acrylic brush, so a panel is
/// never left without a background.
/// </para>
/// </remarks>
internal sealed class GlassSurface : Grid
{
    public static readonly DependencyProperty RadiusProperty = DependencyProperty.Register(
        nameof(Radius), typeof(double), typeof(GlassSurface), new PropertyMetadata(14.0, (d, _) => ((GlassSurface)d).Resize()));

    public static readonly DependencyProperty BlurAmountProperty = DependencyProperty.Register(
        nameof(BlurAmount), typeof(double), typeof(GlassSurface), new PropertyMetadata(28.0));

    private Compositor? _compositor;
    private ContainerVisual? _root;
    private CompositionRoundedRectangleGeometry? _clipShape;
    private CompositionRoundedRectangleGeometry? _rimShape;
    private ShapeVisual? _rim;
    private SpriteVisual? _sheen;
    private CompositionEffectBrush? _glass;

    public GlassSurface()
    {
        IsHitTestVisible = false;
        Loaded += (_, _) => Build();
        Unloaded += (_, _) => TearDown();
        SizeChanged += (_, _) => Resize();
        ActualThemeChanged += (_, _) => { TearDown(); Build(); };
    }

    public double Radius
    {
        get => (double)GetValue(RadiusProperty);
        set => SetValue(RadiusProperty, value);
    }

    public double BlurAmount
    {
        get => (double)GetValue(BlurAmountProperty);
        set => SetValue(BlurAmountProperty, value);
    }

    private bool IsDark => ActualTheme == ElementTheme.Dark;

    private void Build()
    {
        if (_root is not null)
        {
            return;
        }

        try
        {
            _compositor = ElementCompositionPreview.GetElementVisual(this).Compositor;
            var compositor = _compositor;

            // Backdrop -> blur -> saturation, then the tint composited over it.
            var tint = IsDark ? Color.FromArgb(70, 22, 22, 28) : Color.FromArgb(90, 255, 255, 255);
            var graph = new CompositeEffect
            {
                Mode = CanvasComposite.SourceOver,
                Sources =
                {
                    new SaturationEffect
                    {
                        Saturation = 1.8f,
                        Source = new GaussianBlurEffect
                        {
                            BlurAmount = (float)BlurAmount,
                            BorderMode = EffectBorderMode.Hard,
                            Optimization = EffectOptimization.Balanced,
                            Source = new CompositionEffectSourceParameter("backdrop"),
                        },
                    },
                    new ColorSourceEffect { Color = tint },
                },
            };
            _glass = compositor.CreateEffectFactory(graph).CreateBrush();
            _glass.SetSourceParameter("backdrop", compositor.CreateBackdropBrush());

            var body = compositor.CreateSpriteVisual();
            body.Brush = _glass;
            body.RelativeSizeAdjustment = Vector2.One;

            // The sheen: light falling on the upper part of the glass.
            var sheenBrush = compositor.CreateLinearGradientBrush();
            sheenBrush.StartPoint = new Vector2(0.5f, 0);
            sheenBrush.EndPoint = new Vector2(0.5f, 0.6f);
            sheenBrush.ColorStops.Add(compositor.CreateColorGradientStop(0, Color.FromArgb(IsDark ? (byte)34 : (byte)70, 255, 255, 255)));
            sheenBrush.ColorStops.Add(compositor.CreateColorGradientStop(1, Color.FromArgb(0, 255, 255, 255)));
            _sheen = compositor.CreateSpriteVisual();
            _sheen.Brush = sheenBrush;
            _sheen.RelativeSizeAdjustment = Vector2.One;

            // The rim: brightest at the top-left and bottom-right corners, faint along the middle.
            var rimBrush = compositor.CreateLinearGradientBrush();
            rimBrush.StartPoint = new Vector2(0, 0);
            rimBrush.EndPoint = new Vector2(1, 1);
            rimBrush.ColorStops.Add(compositor.CreateColorGradientStop(0f, Color.FromArgb(150, 255, 255, 255)));
            rimBrush.ColorStops.Add(compositor.CreateColorGradientStop(0.35f, Color.FromArgb(28, 255, 255, 255)));
            rimBrush.ColorStops.Add(compositor.CreateColorGradientStop(0.7f, Color.FromArgb(20, 255, 255, 255)));
            rimBrush.ColorStops.Add(compositor.CreateColorGradientStop(1f, Color.FromArgb(110, 255, 255, 255)));
            _rimShape = compositor.CreateRoundedRectangleGeometry();
            var rimSprite = compositor.CreateSpriteShape(_rimShape);
            rimSprite.StrokeBrush = rimBrush;
            rimSprite.StrokeThickness = 1.2f;
            _rim = compositor.CreateShapeVisual();
            _rim.Shapes.Add(rimSprite);

            _root = compositor.CreateContainerVisual();
            _root.RelativeSizeAdjustment = Vector2.One;
            _root.Children.InsertAtTop(body);
            _root.Children.InsertAtTop(_sheen);
            _root.Children.InsertAtTop(_rim);

            _clipShape = compositor.CreateRoundedRectangleGeometry();
            _root.Clip = compositor.CreateGeometricClip(_clipShape);

            ElementCompositionPreview.SetElementChildVisual(this, _root);
            Background = null;
            Resize();
        }
        catch (Exception)
        {
            // No effects on this device: the in-app acrylic is the nearest thing that still works.
            TearDown();
            Background = (Brush)Application.Current.Resources["GoosicSidebarAcrylicBrush"];
            CornerRadius = new CornerRadius(Radius);
        }
    }

    private void Resize()
    {
        if (_root is null || _clipShape is null || _rimShape is null || _rim is null)
        {
            return;
        }

        var size = new Vector2((float)ActualWidth, (float)ActualHeight);
        var radius = (float)Math.Min(Radius, Math.Min(ActualWidth, ActualHeight) / 2);
        _clipShape.Size = size;
        _clipShape.CornerRadius = new Vector2(radius);

        // The rim is inset by half its stroke so the whole line lies inside the clip.
        _rim.Size = size;
        _rimShape.Offset = new Vector2(0.6f);
        _rimShape.Size = new Vector2(Math.Max(0, size.X - 1.2f), Math.Max(0, size.Y - 1.2f));
        _rimShape.CornerRadius = new Vector2(Math.Max(0, radius - 0.6f));
    }

    private void TearDown()
    {
        ElementCompositionPreview.SetElementChildVisual(this, null);
        _glass?.Dispose();
        _root?.Dispose();
        _glass = null;
        _root = null;
        _rim = null;
        _sheen = null;
        _clipShape = null;
        _rimShape = null;
    }
}
