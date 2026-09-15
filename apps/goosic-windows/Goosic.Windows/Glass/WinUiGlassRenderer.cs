using System;
using System.Collections.Generic;
using System.Numerics;
using Microsoft.Graphics.Canvas;
using Microsoft.Graphics.Canvas.Effects;
using Microsoft.Graphics.Canvas.UI.Composition;
using Microsoft.UI.Composition;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Hosting;
using Windows.Graphics.DirectX;
using Windows.Graphics.Effects;
using Windows.UI;

namespace Goosic.Windows.Glass;

/// <summary>What one surface needs drawn this frame. Built by <see cref="GlassScene"/>.</summary>
internal readonly record struct GlassSurfaceFrame(
    GlassStyle Style,
    float Width,
    float Height,
    float Radius,
    float RasterizationScale,
    bool Dark,
    GlassQuality Quality,
    GlassDebugLayer Debug,
    IReadOnlyList<GlassBounds> Below,
    ScrollViewer? ScrollEdgeSource);

/// <summary>
/// The WinUI Composition implementation of Liquid Glass, shared by every window on the UI thread.
/// </summary>
/// <remarks>
/// <para>
/// Every surface samples one <see cref="CompositionBackdropBrush"/> through a compiled effect graph
/// cached per style, tier and theme, so the backdrop is the real XAML content behind the surface —
/// lower glass included — in the window's own coordinates, and a surface above another re-frosts
/// it instead of stacking a second capture. Nothing here copies pixels back from the GPU.
/// </para>
/// <para>
/// The parts of the proof of concept's shader that do not depend on the backdrop are baked once
/// per corner radius and display density into nine-grid textures
/// (<see cref="GlassTextureBaker"/>), shared by every surface with that geometry. The only per-frame
/// drawing is the stack mask, redrawn on the GPU with Win2D when the glass beneath a surface moves.
/// </para>
/// </remarks>
public sealed class WinUiGlassRenderer : IDisposable
{
    private static WinUiGlassRenderer? _shared;

    private readonly Compositor _compositor;
    private readonly DispatcherQueue _queue;
    private readonly CompositionBackdropBrush _backdrop;
    private readonly Dictionary<string, CompositionEffectFactory?> _factories = [];
    private readonly Dictionary<GlassBakeKey, BakedTextures> _bakes = [];
    private readonly Dictionary<GlassShadowKey, BakedShadow> _shadows = [];
    private readonly HashSet<GlassSurfaceVisuals> _surfaces = [];
    private CanvasDevice _device;
    private readonly CompositionGraphicsDevice _graphics;

    private WinUiGlassRenderer(Compositor compositor)
    {
        _compositor = compositor;
        _queue = DispatcherQueue.GetForCurrentThread();
        _backdrop = compositor.CreateBackdropBrush();
        _device = CanvasDevice.GetSharedDevice();
        _device.DeviceLost += OnDeviceLost;
        _graphics = CanvasComposition.CreateCompositionGraphicsDevice(compositor, _device);
        _graphics.RenderingDeviceReplaced += (_, _) => _queue.TryEnqueue(RedrawAfterDeviceLoss);
    }

    public static WinUiGlassRenderer Shared =>
        _shared ??= new WinUiGlassRenderer(Microsoft.UI.Xaml.Media.CompositionTarget.GetCompositorForCurrentThread());

    /// <summary>Distinct compiled effect graphs; one per style, tier, theme and debug view in use.</summary>
    public int EffectGraphCount => _factories.Count;

    /// <summary>Distinct baked edge geometries on the GPU.</summary>
    public int BakedGeometryCount => _bakes.Count + _shadows.Count;

    /// <summary>How many times the drawing device has been lost and rebuilt this session.</summary>
    public int DeviceResets { get; private set; }

    /// <summary>Stack masks redrawn since the last call to <see cref="TakeStackRedraws"/>.</summary>
    private int _stackRedraws;

    public int TakeStackRedraws()
    {
        var count = _stackRedraws;
        _stackRedraws = 0;
        return count;
    }

    // ---- Surfaces -------------------------------------------------------------------------------

    internal GlassSurfaceVisuals Attach(Microsoft.UI.Xaml.UIElement host)
    {
        var root = _compositor.CreateContainerVisual();
        root.RelativeSizeAdjustment = Vector2.One;
        var shadow = _compositor.CreateSpriteVisual();
        var material = _compositor.CreateSpriteVisual();
        var edge = _compositor.CreateSpriteVisual();
        material.RelativeSizeAdjustment = Vector2.One;
        edge.RelativeSizeAdjustment = Vector2.One;
        root.Children.InsertAtTop(shadow);
        root.Children.InsertAtTop(material);
        root.Children.InsertAtTop(edge);
        ElementCompositionPreview.SetElementChildVisual(host, root);
        var visuals = new GlassSurfaceVisuals(host, root, shadow, material, edge);
        _surfaces.Add(visuals);
        return visuals;
    }

    internal void Detach(GlassSurfaceVisuals visuals)
    {
        if (!_surfaces.Remove(visuals))
        {
            return;
        }

        ElementCompositionPreview.SetElementChildVisual(visuals.Host, null);
        Release(visuals);
        visuals.Material.Dispose();
        visuals.Edge.Dispose();
        visuals.Shadow.Dispose();
        visuals.Root.Dispose();
    }

    /// <summary>
    /// Brings a surface's visuals in line with its frame. Everything is keyed, so a frame that
    /// changed nothing costs a handful of comparisons.
    /// </summary>
    internal void Update(GlassSurfaceVisuals visuals, GlassSurfaceFrame frame)
    {
        var features = GlassTokens.Features(frame.Quality);
        if (!features.Backdrop || frame.Width < 1 || frame.Height < 1)
        {
            visuals.Root.IsVisible = false;
            return;
        }

        visuals.Root.IsVisible = true;
        var material = GlassTokens.For(frame.Style, frame.Dark);
        var chrome = frame.Style == GlassStyle.Window;
        var lens = features.Refraction && material.Refraction > 0;
        var stack = features.StackResponse && !chrome;

        var factoryKey = $"{frame.Style}|{frame.Quality}|{frame.Dark}|{frame.Debug}|{lens}|{stack}";
        if (visuals.FactoryKey != factoryKey)
        {
            var factory = Factory(factoryKey, () => BuildMaterial(material, frame.Quality, frame.Debug, chrome, lens, stack),
                lens ? ["Lens.TransformMatrix"] : []);
            if (factory is null)
            {
                visuals.Root.IsVisible = false;
                return;
            }

            visuals.Brush?.Dispose();
            visuals.Brush = factory.CreateBrush();
            visuals.Brush.SetSourceParameter("Backdrop", _backdrop);
            visuals.Material.Brush = visuals.Brush;
            visuals.FactoryKey = factoryKey;
            visuals.BoundKey = null;
            visuals.StackSignature = null;
            visuals.LensSize = default;
        }

        var brush = visuals.Brush!;
        if (chrome)
        {
            visuals.Edge.IsVisible = false;
            BindChrome(visuals, brush);
        }
        else
        {
            visuals.Edge.IsVisible = features.Edges;
            BindGeometry(visuals, brush, material, frame);
        }

        if (lens)
        {
            UpdateLens(visuals, brush, frame);
        }

        if (stack)
        {
            UpdateStack(visuals, brush, material, frame, features);
        }

        UpdateShadow(visuals, material, frame, features);
        UpdateScrollEdge(visuals, frame.ScrollEdgeSource);
    }

    private void BindChrome(GlassSurfaceVisuals visuals, CompositionEffectBrush brush)
    {
        if (visuals.BoundKey is "chrome")
        {
            return;
        }

        // A scroll edge fades out downwards instead of ending on a rim.
        var fade = _compositor.CreateLinearGradientBrush();
        fade.StartPoint = new Vector2(0, 0);
        fade.EndPoint = new Vector2(0, 1);
        fade.ColorStops.Add(_compositor.CreateColorGradientStop(0f, Color.FromArgb(255, 255, 255, 255)));
        fade.ColorStops.Add(_compositor.CreateColorGradientStop(0.55f, Color.FromArgb(235, 255, 255, 255)));
        fade.ColorStops.Add(_compositor.CreateColorGradientStop(1f, Color.FromArgb(0, 255, 255, 255)));
        brush.SetSourceParameter("Shape", fade);
        visuals.BoundKey = "chrome";
    }

    private void BindGeometry(GlassSurfaceVisuals visuals, CompositionEffectBrush brush, GlassMaterial material,
        GlassSurfaceFrame frame)
    {
        var key = GlassBakeKey.For(material, frame.Radius, frame.Width, frame.Height, frame.RasterizationScale);
        if (visuals.BoundKey is GlassBakeKey bound && bound == key && visuals.Bake is { Lost: false })
        {
            return;
        }

        var bake = Bake(key);
        if (!ReferenceEquals(visuals.Bake, bake))
        {
            visuals.Bake?.Release();
            bake.References++;
            visuals.Bake = bake;
        }

        brush.SetSourceParameter("Shape", NineGrid(bake.Shape, key.InsetPixels, key.Scale));
        visuals.Edge.Brush?.Dispose();
        visuals.Edge.Brush = NineGrid(bake.EdgeAdd, key.InsetPixels, key.Scale);
        if (GlassTokens.Features(frame.Quality).Refraction && material.Refraction > 0)
        {
            brush.SetSourceParameter("Lens", NineGrid(bake.Lens, key.InsetPixels, key.Scale));
        }
        visuals.LensPeak = bake.Textures.LensPeakOffset;
        visuals.BoundKey = key;
        visuals.LensSize = default;
    }

    private void UpdateLens(GlassSurfaceVisuals visuals, CompositionEffectBrush brush, GlassSurfaceFrame frame)
    {
        var size = new Vector2(frame.Width, frame.Height);
        if (visuals.LensSize == size)
        {
            return;
        }

        // The bevel bends rays inwards, so the rim shows content from nearer the centre. A lens
        // scaled about the centre, masked to the bevel's refraction profile, reproduces the
        // proof of concept's peak displacement at the rim on every edge.
        var halfWidth = frame.Width / 2f;
        var halfHeight = frame.Height / 2f;
        var offset = (float)Math.Min(visuals.LensPeak, Math.Min(18, 0.35 * Math.Min(halfWidth, halfHeight)));
        var scaleX = halfWidth / Math.Max(halfWidth - offset, 1f);
        var scaleY = halfHeight / Math.Max(halfHeight - offset, 1f);
        var center = new Vector2(halfWidth, halfHeight);
        var matrix = Matrix3x2.CreateTranslation(-center)
            * Matrix3x2.CreateScale(scaleX, scaleY)
            * Matrix3x2.CreateTranslation(center);
        brush.Properties.InsertMatrix3x2("Lens.TransformMatrix", matrix);
        visuals.LensSize = size;
    }

    private void UpdateStack(GlassSurfaceVisuals visuals, CompositionEffectBrush brush, GlassMaterial material,
        GlassSurfaceFrame frame, GlassFeatures features)
    {
        var maskScale = features.StackMaskScale * frame.RasterizationScale;
        var width = Math.Max(1, (int)Math.Ceiling(frame.Width * maskScale));
        var height = Math.Max(1, (int)Math.Ceiling(frame.Height * maskScale));
        var signature = StackSignature(frame.Below, width, height, material.BlurSigma);
        if (visuals.StackSignature == signature && visuals.StackSurface is not null)
        {
            return;
        }

        if (visuals.StackSurface is null)
        {
            visuals.StackSurface = _graphics.CreateDrawingSurface(new global::Windows.Foundation.Size(width, height),
                Microsoft.Graphics.DirectX.DirectXPixelFormat.B8G8R8A8UIntNormalized, Microsoft.Graphics.DirectX.DirectXAlphaMode.Premultiplied);
            visuals.StackBrush = _compositor.CreateSurfaceBrush(visuals.StackSurface);
            visuals.StackBrush.Stretch = CompositionStretch.Fill;
        }
        else if (visuals.StackSurface.Size.Width != width || visuals.StackSurface.Size.Height != height)
        {
            CanvasComposition.Resize(visuals.StackSurface, new global::Windows.Foundation.Size(width, height));
        }

        brush.SetSourceParameter("Stack", visuals.StackBrush);
        DrawStack(visuals.StackSurface, frame.Below, maskScale, material, frame.Radius);
        visuals.StackSignature = signature;
        visuals.StackBelow = frame.Below;
        visuals.StackScale = maskScale;
        visuals.StackMaterial = material;
        _stackRedraws++;
    }

    /// <summary>
    /// Coverage of the glass already beneath a surface, blurred by that surface's own frost, so
    /// the lower rim and its inner shadows fade into the response instead of cutting across it.
    /// </summary>
    private void DrawStack(CompositionDrawingSurface surface, IReadOnlyList<GlassBounds> below, float scale,
        GlassMaterial material, float radius)
    {
        using var session = CanvasComposition.CreateDrawingSession(surface);
        session.Clear(Color.FromArgb(0, 0, 0, 0));
        if (below.Count == 0)
        {
            return;
        }

        using var shapes = new CanvasCommandList(session);
        using (var draw = shapes.CreateDrawingSession())
        {
            foreach (var lower in below)
            {
                var corner = (float)(lower.Radius * (1 + material.Smoothing * 0.5) * scale);
                draw.FillRoundedRectangle(
                    (float)(lower.X * scale), (float)(lower.Y * scale),
                    (float)(lower.Width * scale), (float)(lower.Height * scale),
                    corner, corner, Color.FromArgb(255, 255, 255, 255));
            }
        }

        using var blur = new GaussianBlurEffect
        {
            Source = shapes,
            BlurAmount = Math.Max(0.5f, material.BlurSigma * scale),
            BorderMode = EffectBorderMode.Soft,
            Optimization = EffectOptimization.Speed,
        };
        session.DrawImage(blur);
    }

    private void UpdateShadow(GlassSurfaceVisuals visuals, GlassMaterial material, GlassSurfaceFrame frame,
        GlassFeatures features)
    {
        if (!features.Shadow || material.Shadow <= 0 || frame.Debug is not (GlassDebugLayer.None or GlassDebugLayer.Bounds or GlassDebugLayer.ZOrder))
        {
            if (visuals.ShadowKey is not null)
            {
                visuals.Shadow.Brush = null;
                visuals.ShadowBake?.Release();
                visuals.ShadowBake = null;
                visuals.ShadowKey = null;
            }

            return;
        }

        var key = GlassShadowKey.For(material, frame.Radius, frame.Width, frame.Height);
        if (visuals.ShadowKey == key && visuals.ShadowBake is { Lost: false })
        {
            return;
        }

        var bake = Shadow(key);
        visuals.ShadowBake?.Release();
        bake.References++;
        visuals.ShadowBake = bake;
        visuals.Shadow.Brush = NineGrid(bake.Surface, key.InsetPixels, GlassShadowKey.Scale);
        visuals.Shadow.RelativeSizeAdjustment = Vector2.One;
        visuals.Shadow.Size = new Vector2(key.Margin * 2, key.Margin * 2);
        visuals.Shadow.Offset = new Vector3(-key.Margin, -key.Margin, 0);
        visuals.ShadowKey = key;
    }

    private void UpdateScrollEdge(GlassSurfaceVisuals visuals, ScrollViewer? source)
    {
        if (ReferenceEquals(visuals.ScrollEdgeSource, source))
        {
            return;
        }

        visuals.Root.StopAnimation("Opacity");
        visuals.Root.Opacity = 1;
        visuals.ScrollEdgeSource = source;
        if (source is null)
        {
            return;
        }

        // Driven by the compositor from the scroller's own translation, so the edge appears as
        // content moves beneath it without waking the UI thread.
        var scroll = ElementCompositionPreview.GetScrollViewerManipulationPropertySet(source);
        var animation = _compositor.CreateExpressionAnimation("Clamp(-scroll.Translation.Y / 40, 0, 1)");
        animation.SetReferenceParameter("scroll", scroll);
        visuals.Root.StartAnimation("Opacity", animation);
    }

    private void Release(GlassSurfaceVisuals visuals)
    {
        visuals.Bake?.Release();
        visuals.Bake = null;
        visuals.ShadowBake?.Release();
        visuals.ShadowBake = null;
        visuals.Brush?.Dispose();
        visuals.Brush = null;
        visuals.Edge.Brush?.Dispose();
        visuals.Edge.Brush = null;
        visuals.StackBrush?.Dispose();
        visuals.StackBrush = null;
        visuals.StackSurface?.Dispose();
        visuals.StackSurface = null;
        visuals.FactoryKey = null;
        visuals.BoundKey = null;
        visuals.ShadowKey = null;
        TrimBakes();
    }

    // ---- Popup brushes --------------------------------------------------------------------------

    /// <summary>
    /// A size-independent glass for presenters that clip themselves: menus, flyouts, tooltips and
    /// dialogs.
    /// </summary>
    /// <remarks>
    /// Composited over <paramref name="fallback"/>: inside the window the backdrop is opaque and
    /// hides it, while a popup that Windows gives its own window has nothing behind it to sample
    /// and shows the fallback instead of an invisible plate.
    /// </remarks>
    internal CompositionBrush CreatePopupBrush(GlassStyle style, bool dark, Color fallback, float highlight)
    {
        var quality = GlassSystem.Quality;
        var features = GlassTokens.Features(quality);
        if (!features.Backdrop)
        {
            return _compositor.CreateColorBrush(fallback);
        }

        var material = GlassTokens.For(style, dark);
        var key = $"popup|{style}|{quality}|{dark}|{fallback}|{highlight:0.00}";
        var factory = Factory(key, () =>
        {
            IGraphicsEffectSource glass = Tint(Frost(material, quality), material);
            if (highlight > 0)
            {
                glass = Over(glass, new ColorSourceEffect
                {
                    Color = dark ? Color.FromArgb((byte)(highlight * 255), 255, 255, 255)
                                 : Color.FromArgb((byte)(highlight * 255), 0, 0, 0),
                });
            }

            return Over(new ColorSourceEffect { Color = fallback }, glass);
        }, []);
        if (factory is null)
        {
            return _compositor.CreateColorBrush(fallback);
        }

        var brush = factory.CreateBrush();
        brush.SetSourceParameter("Backdrop", _backdrop);
        return brush;
    }

    // ---- Effect graphs --------------------------------------------------------------------------

    private CompositionEffectFactory? Factory(string key, Func<IGraphicsEffect> build, string[] animatable)
    {
        if (_factories.TryGetValue(key, out var cached))
        {
            return cached;
        }

        CompositionEffectFactory? factory = null;
        try
        {
            factory = _compositor.CreateEffectFactory(build(), animatable);
            // Loading is asynchronous on some Windows/driver combinations. A Pending factory is
            // valid and its brush becomes live when compilation completes.
            if (factory.LoadStatus != CompositionEffectFactoryLoadStatus.Success
                && factory.LoadStatus != CompositionEffectFactoryLoadStatus.Pending)
            {
                GlassSystem.ReportRendererWarning($"effect graph {key} failed to load: {factory.LoadStatus}");
                factory = null;
            }
        }
        catch (Exception error)
        {
            GlassSystem.ReportRendererWarning($"effect graph {key} is not supported: {error.Message}");
            factory = null;
        }

        _factories[key] = factory;
        return factory;
    }

    private static IGraphicsEffect BuildMaterial(GlassMaterial material, GlassQuality quality, GlassDebugLayer debug,
        bool chrome, bool lens, bool stack)
    {
        var shape = new CompositionEffectSourceParameter("Shape");
        var backdrop = new CompositionEffectSourceParameter("Backdrop");
        var solid = new ColorSourceEffect { Color = material.DarkTint ? Color.FromArgb(255, 26, 26, 30) : Color.FromArgb(255, 242, 242, 245) };

        switch (debug)
        {
            case GlassDebugLayer.Backdrop:
                return Mask(Over(solid, backdrop), shape);
            case GlassDebugLayer.Mask:
                return Mask(new ColorSourceEffect { Color = Color.FromArgb(170, 255, 255, 255) }, shape);
            case GlassDebugLayer.BlurredStack:
                return Mask(Over(solid, Frost(material, quality)), shape);
        }

        IGraphicsEffectSource Refracted()
        {
            if (!lens)
            {
                return Frost(material, quality);
            }

            IGraphicsEffectSource shifted = new AlphaMaskEffect
            {
                Source = new Transform2DEffect { Name = "Lens", Source = Frost(material, quality) },
                AlphaMask = new CompositionEffectSourceParameter("Lens"),
            };
            if (stack)
            {
                shifted = new AlphaMaskEffect
                {
                    Source = shifted,
                    AlphaMask = StackAlpha(-GlassTokens.StackRefractionRelief, 1),
                };
            }

            // The compositor accepts only tree-shaped effect graphs. Both branches deliberately
            // own a blur node while still sampling the same named per-window backdrop.
            return Over(Frost(material, quality), shifted);
        }

        IGraphicsEffectSource body = Tint(Refracted(), material);
        // The shared backdrop already contains lower glass. Keeping a single refraction branch
        // makes the graph fit the compositor's hardware limit; the blurred stack mask still
        // suppresses repeated specular light below.

        // Edge textures are uploaded and cached with the geometry. They are kept outside this
        // material graph because Windows Composition allows four named sources per factory; the
        // backdrop, shape, lens and stack response are the four optical inputs that must remain.

        if (debug == GlassDebugLayer.OverlapResponse && stack)
        {
            body = Over(body, new AlphaMaskEffect
            {
                Source = new ColorSourceEffect { Color = Color.FromArgb(200, 255, 0, 170) },
                AlphaMask = StackAlpha(1, 0),
            });
        }

        return Mask(body, shape);
    }

    private static GaussianBlurEffect Frost(GlassMaterial material, GlassQuality quality) => new()
    {
        Source = new CompositionEffectSourceParameter("Backdrop"),
        BlurAmount = GlassTokens.BlurAmount(material, quality),
        BorderMode = EffectBorderMode.Hard,
        Optimization = GlassTokens.Features(quality).BlurOptimization switch
        {
            GlassBlurOptimization.Quality => EffectOptimization.Quality,
            GlassBlurOptimization.Balanced => EffectOptimization.Balanced,
            _ => EffectOptimization.Speed,
        },
    };

    /// <summary>The proof-of-concept tint folded into one affine, tree-shaped pass.</summary>
    private static IGraphicsEffectSource Tint(IGraphicsEffectSource input, GlassMaterial material)
    {
        var t = material.Tint;
        if (!material.DarkTint)
        {
            var amount = 0.62f * t;
            return new ColorMatrixEffect
            {
                Source = input,
                ColorMatrix = new Matrix5x4
                {
                    M11 = 1 - amount, M22 = 1 - amount, M33 = 1 - amount, M44 = 1,
                    M51 = amount, M52 = amount, M53 = amount,
                },
            };
        }

        // Two dark luminosity fills collapse into this affine luminosity shift. Keeping the
        // input on one path is required by Windows Composition's effect graph compiler.
        const float target = 0.102f;
        var k = t - 0.25f * t * t;
        return new ColorMatrixEffect
        {
            Source = input,
            ColorMatrix = new Matrix5x4
            {
                M11 = 1 - k * 0.3f, M21 = -k * 0.59f, M31 = -k * 0.11f, M41 = 0, M51 = k * target,
                M12 = -k * 0.3f, M22 = 1 - k * 0.59f, M32 = -k * 0.11f, M42 = 0, M52 = k * target,
                M13 = -k * 0.3f, M23 = -k * 0.59f, M33 = 1 - k * 0.11f, M43 = 0, M53 = k * target,
                M14 = 0, M24 = 0, M34 = 0, M44 = 1, M54 = 0,
            },
        };
    }

    /// <summary>The stack mask's coverage mapped to <c>a * coverage + b</c> in its alpha.</summary>
    private static ColorMatrixEffect StackAlpha(float a, float b) => new()
    {
        Source = new CompositionEffectSourceParameter("Stack"),
        ColorMatrix = new Matrix5x4 { M11 = 1, M22 = 1, M33 = 1, M44 = a, M54 = b },
    };

    private static ArithmeticCompositeEffect Mix(IGraphicsEffectSource from, IGraphicsEffectSource to, float amount) => new()
    {
        Source1 = from,
        Source2 = to,
        MultiplyAmount = 0,
        Source1Amount = 1 - amount,
        Source2Amount = amount,
        Offset = 0,
    };

    private static CompositeEffect Over(IGraphicsEffectSource below, IGraphicsEffectSource above)
    {
        var composite = new CompositeEffect { Mode = CanvasComposite.SourceOver };
        composite.Sources.Add(below);
        composite.Sources.Add(above);
        return composite;
    }

    private static AlphaMaskEffect Mask(IGraphicsEffectSource source, IGraphicsEffectSource mask) =>
        new() { Source = source, AlphaMask = mask };

    // ---- Baked textures -------------------------------------------------------------------------

    private CompositionNineGridBrush NineGrid(CompositionDrawingSurface surface, int insetPixels, float scale)
    {
        var source = _compositor.CreateSurfaceBrush(surface);
        source.Stretch = CompositionStretch.Fill;
        var grid = _compositor.CreateNineGridBrush();
        grid.Source = source;
        grid.SetInsets(insetPixels);
        grid.SetInsetScales(1 / scale);
        return grid;
    }

    private BakedTextures Bake(GlassBakeKey key)
    {
        if (_bakes.TryGetValue(key, out var cached) && !cached.Lost)
        {
            return cached;
        }

        if (cached is not null)
        {
            // The device was replaced: the surfaces are still there but empty.
            var kept = cached.Textures;
            Redraw(cached.Shape, kept.Shape, kept.Size);
            Redraw(cached.EdgeAdd, kept.EdgeAdd, kept.Size);
            Redraw(cached.EdgeBurn, kept.EdgeBurn, kept.Size);
            Redraw(cached.Lens, kept.Lens, kept.Size);
            cached.Lost = false;
            return cached;
        }

        var textures = GlassTextureBaker.Bake(key);
        var baked = new BakedTextures(textures,
            Upload(textures.Shape, textures.Size), Upload(textures.EdgeAdd, textures.Size),
            Upload(textures.EdgeBurn, textures.Size), Upload(textures.Lens, textures.Size));
        _bakes[key] = baked;
        return baked;
    }

    private BakedShadow Shadow(GlassShadowKey key)
    {
        if (_shadows.TryGetValue(key, out var cached) && !cached.Lost)
        {
            return cached;
        }

        if (cached is not null)
        {
            Redraw(cached.Surface, cached.Pixels, key.Size);
            cached.Lost = false;
            return cached;
        }

        var pixels = GlassTextureBaker.BakeShadow(key);
        var baked = new BakedShadow(pixels, Upload(pixels, key.Size));
        _shadows[key] = baked;
        return baked;
    }

    private CompositionDrawingSurface Upload(byte[] pixels, int size)
    {
        var surface = _graphics.CreateDrawingSurface(new global::Windows.Foundation.Size(size, size),
            Microsoft.Graphics.DirectX.DirectXPixelFormat.B8G8R8A8UIntNormalized, Microsoft.Graphics.DirectX.DirectXAlphaMode.Premultiplied);
        Redraw(surface, pixels, size);
        return surface;
    }

    private void Redraw(CompositionDrawingSurface surface, byte[] pixels, int size)
    {
        using var session = CanvasComposition.CreateDrawingSession(surface);
        session.Blend = CanvasBlend.Copy;
        using var bitmap = CanvasBitmap.CreateFromBytes(session, pixels, size, size,
            DirectXPixelFormat.B8G8R8A8UIntNormalized, 96, CanvasAlphaMode.Premultiplied);
        session.DrawImage(bitmap);
    }

    /// <summary>Drops geometry nobody is using once the cache has grown past what a window needs.</summary>
    private void TrimBakes()
    {
        if (_bakes.Count > 48)
        {
            foreach (var entry in new List<KeyValuePair<GlassBakeKey, BakedTextures>>(_bakes))
            {
                if (entry.Value.References <= 0)
                {
                    entry.Value.Dispose();
                    _bakes.Remove(entry.Key);
                }
            }
        }

        if (_shadows.Count > 24)
        {
            foreach (var entry in new List<KeyValuePair<GlassShadowKey, BakedShadow>>(_shadows))
            {
                if (entry.Value.References <= 0)
                {
                    entry.Value.Surface.Dispose();
                    _shadows.Remove(entry.Key);
                }
            }
        }
    }

    // ---- Device loss ----------------------------------------------------------------------------

    private void OnDeviceLost(CanvasDevice sender, object args)
    {
        _queue.TryEnqueue(() =>
        {
            sender.DeviceLost -= OnDeviceLost;
            _device = CanvasDevice.GetSharedDevice();
            _device.DeviceLost += OnDeviceLost;
            // Replacing the device raises RenderingDeviceReplaced, which redraws everything.
            CanvasComposition.SetCanvasDevice(_graphics, _device);
        });
    }

    /// <summary>
    /// Drawing-surface contents do not survive a lost device. The baked textures are kept as
    /// bytes, so they are uploaded again; stack masks are redrawn on the next scene tick.
    /// </summary>
    private void RedrawAfterDeviceLoss()
    {
        DeviceResets++;
        foreach (var bake in _bakes.Values)
        {
            bake.Lost = true;
        }

        foreach (var shadow in _shadows.Values)
        {
            shadow.Lost = true;
        }

        foreach (var surface in _surfaces)
        {
            surface.StackSignature = null;
            surface.BoundKey = null;
            surface.ShadowKey = null;
        }

        DeviceReplaced?.Invoke();
    }

    /// <summary>Raised on the UI thread after the drawing device was rebuilt.</summary>
    public event Action? DeviceReplaced;

    private static string StackSignature(IReadOnlyList<GlassBounds> below, int width, int height, float sigma)
    {
        var hash = new HashCode();
        hash.Add(width);
        hash.Add(height);
        hash.Add(sigma);
        foreach (var bounds in below)
        {
            hash.Add(Math.Round(bounds.X));
            hash.Add(Math.Round(bounds.Y));
            hash.Add(Math.Round(bounds.Width));
            hash.Add(Math.Round(bounds.Height));
            hash.Add(Math.Round(bounds.Radius));
        }

        return $"{below.Count}:{hash.ToHashCode()}";
    }

    public void Dispose()
    {
        foreach (var surface in new List<GlassSurfaceVisuals>(_surfaces))
        {
            Detach(surface);
        }

        foreach (var bake in _bakes.Values)
        {
            bake.Dispose();
        }

        foreach (var shadow in _shadows.Values)
        {
            shadow.Surface.Dispose();
        }

        _bakes.Clear();
        _shadows.Clear();
        foreach (var factory in _factories.Values)
        {
            factory?.Dispose();
        }

        _factories.Clear();
        _backdrop.Dispose();
        _graphics.Dispose();
        _device.DeviceLost -= OnDeviceLost;
        if (ReferenceEquals(_shared, this))
        {
            _shared = null;
        }
    }

    /// <summary>One bake on the GPU, with its bytes kept so a lost device can be refilled.</summary>
    internal sealed class BakedTextures(
        GlassBakedTextures textures,
        CompositionDrawingSurface shape,
        CompositionDrawingSurface edgeAdd,
        CompositionDrawingSurface edgeBurn,
        CompositionDrawingSurface lens) : IDisposable
    {
        public GlassBakedTextures Textures { get; } = textures;
        public CompositionDrawingSurface Shape { get; } = shape;
        public CompositionDrawingSurface EdgeAdd { get; } = edgeAdd;
        public CompositionDrawingSurface EdgeBurn { get; } = edgeBurn;
        public CompositionDrawingSurface Lens { get; } = lens;
        public int References { get; set; }
        public bool Lost { get; set; }

        public void Release() => References--;

        public void Dispose()
        {
            Shape.Dispose();
            EdgeAdd.Dispose();
            EdgeBurn.Dispose();
            Lens.Dispose();
        }
    }

    internal sealed class BakedShadow(byte[] pixels, CompositionDrawingSurface surface)
    {
        public byte[] Pixels { get; } = pixels;
        public CompositionDrawingSurface Surface { get; } = surface;
        public int References { get; set; }
        public bool Lost { get; set; }

        public void Release() => References--;
    }

    internal sealed class GlassSurfaceVisuals(
        Microsoft.UI.Xaml.UIElement host,
        ContainerVisual root,
        SpriteVisual shadow,
        SpriteVisual material,
        SpriteVisual edge)
    {
        public Microsoft.UI.Xaml.UIElement Host { get; } = host;
        public ContainerVisual Root { get; } = root;
        public SpriteVisual Shadow { get; } = shadow;
        public SpriteVisual Material { get; } = material;
        public SpriteVisual Edge { get; } = edge;
        public CompositionEffectBrush? Brush { get; set; }
        public string? FactoryKey { get; set; }
        public object? BoundKey { get; set; }
        internal BakedTextures? Bake { get; set; }
        internal BakedShadow? ShadowBake { get; set; }
        public GlassShadowKey? ShadowKey { get; set; }
        public double LensPeak { get; set; }
        public Vector2 LensSize { get; set; }
        public CompositionDrawingSurface? StackSurface { get; set; }
        public CompositionSurfaceBrush? StackBrush { get; set; }
        public string? StackSignature { get; set; }
        public IReadOnlyList<GlassBounds> StackBelow { get; set; } = [];
        public float StackScale { get; set; }
        public GlassMaterial StackMaterial { get; set; }
        public ScrollViewer? ScrollEdgeSource { get; set; }
    }
}
