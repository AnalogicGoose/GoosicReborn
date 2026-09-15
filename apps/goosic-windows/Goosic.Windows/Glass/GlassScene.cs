using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Controls;

namespace Goosic.Windows.Glass;

/// <summary>One shared scene and renderer for one XAML window root.</summary>
public sealed class GlassScene : IDisposable
{
    private static readonly Dictionary<XamlRoot, GlassScene> Scenes = [];
    private readonly List<GlassRegistration> _surfaces = [];
    private readonly Stopwatch _clock = Stopwatch.StartNew();
    private readonly Queue<double> _frameTimes = new();
    private long _lastFrameTicks;
    private readonly SolidColorBrush _transparent = new(Microsoft.UI.Colors.Transparent);
    private TextBlock? _hud;
    private int _frames;
    private readonly int _benchmarkFrames = int.TryParse(Environment.GetEnvironmentVariable("GOOSIC_GLASS_BENCHMARK_FRAMES"), out var count) ? Math.Max(0, count) : 0;

    private GlassScene(FrameworkElement root)
    {
        Root = root;
        GlassSystem.EnsureInitialized();
        Renderer = WinUiGlassRenderer.Shared;
        CompositionTarget.Rendering += OnRendering;
        root.Unloaded += OnRootUnloaded;
        if (Enum.TryParse<GlassDebugLayer>(Environment.GetEnvironmentVariable("GOOSIC_GLASS_DEBUG"), true, out var layer))
            DebugLayer = layer;
    }

    public FrameworkElement Root { get; }
    public WinUiGlassRenderer Renderer { get; }
    public GlassQuality Quality => GlassSystem.Quality;
    public GlassDebugLayer DebugLayer { get; set; }
    public double FramesPerSecond { get; private set; }
    public double P95FrameMilliseconds { get; private set; }
    public double RendererMilliseconds { get; private set; }
    public IReadOnlyList<GlassRegistration> Surfaces => _surfaces;

    public static GlassScene Attach(FrameworkElement root)
    {
        if (root.XamlRoot is null)
        {
            throw new InvalidOperationException("The window root must be loaded before attaching glass.");
        }

        if (!Scenes.TryGetValue(root.XamlRoot, out var scene))
        {
            scene = new GlassScene(root);
            Scenes.Add(root.XamlRoot, scene);
            if ((scene._benchmarkFrames > 0 || scene.DebugLayer != GlassDebugLayer.None) && root is Grid grid)
            {
                scene._hud = new TextBlock { FontSize = 12, IsTextSelectionEnabled = true };
                var panel = new Border { Child = scene._hud, Padding = new Thickness(12), CornerRadius = new CornerRadius(12),
                    HorizontalAlignment = HorizontalAlignment.Right, VerticalAlignment = VerticalAlignment.Top,
                    Margin = new Thickness(0, 54, 16, 0) };
                grid.Children.Add(panel);
                scene.Register(panel, GlassStyle.Menu);
            }
        }
        return scene;
    }

    internal static GlassScene? GetForRoot(FrameworkElement root) =>
        root.XamlRoot is not null && Scenes.TryGetValue(root.XamlRoot, out var scene) ? scene : null;

    internal static GlassScene? Find(FrameworkElement element) =>
        element.XamlRoot is not null && Scenes.TryGetValue(element.XamlRoot, out var scene) ? scene : null;

    public void Register(FrameworkElement element, GlassStyle style)
    {
        var existing = _surfaces.FirstOrDefault(item => ReferenceEquals(item.Element, element));
        if (existing is not null)
        {
            existing.Style = style;
            return;
        }

        var registration = new GlassRegistration(element, style, _surfaces.Count);
        // Insert the material before the existing foreground, never over its text and controls.
        if (element is not Border border) return;
        var content = border.Child;
        border.Child = null;
        var layers = new Grid();
        var host = new Grid { IsHitTestVisible = false };
        layers.Children.Add(host);
        if (content is not null) layers.Children.Add(content);
        border.Child = layers;
        registration.Host = host;
        registration.Fallback = border.Background;
        registration.Content = content;
        border.Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent);
        registration.Visuals = Renderer.Attach(host);
        _surfaces.Add(registration);
    }

    public void Unregister(FrameworkElement element)
    {
        var registration = _surfaces.FirstOrDefault(item => ReferenceEquals(item.Element, element));
        if (registration is null)
        {
            return;
        }
        if (registration.Visuals is not null) Renderer.Detach(registration.Visuals);
        if (registration.Element is Border border && border.Child is Grid layers)
        {
            if (registration.Content is not null) layers.Children.Remove(registration.Content);
            border.Child = registration.Content;
            border.Background = registration.Fallback;
        }
        _surfaces.Remove(registration);
    }

    private void OnRendering(object? sender, object args)
    {
        var ticks = _clock.ElapsedTicks;
        if (_lastFrameTicks != 0)
        {
            var milliseconds = (ticks - _lastFrameTicks) * 1000.0 / Stopwatch.Frequency;
            _frameTimes.Enqueue(milliseconds);
            while (_frameTimes.Count > 240)
            {
                _frameTimes.Dequeue();
            }
            FramesPerSecond = milliseconds > 0 ? 1000.0 / milliseconds : 0;
            if (_frameTimes.Count >= 10)
            {
                var samples = _frameTimes.OrderBy(value => value).ToArray();
                P95FrameMilliseconds = samples[(int)Math.Ceiling(samples.Length * 0.95) - 1];
            }
        }
        _lastFrameTicks = ticks;
        var started = Stopwatch.GetTimestamp();
        var ordered = new List<GlassRegistration>();
        Collect(Root, ordered);
        var bounds = new List<GlassBounds>();
        foreach (var item in ordered)
        {
            if (item.Host is null || item.Visuals is null) continue;
            var visible = IsVisible(item.Element);
            item.Visuals.Root.IsVisible = visible;
            if (!visible) continue;
            var point = item.Host.TransformToVisual(Root).TransformPoint(new global::Windows.Foundation.Point());
            var radius = item.Element is Border border ? border.CornerRadius.TopLeft : 0;
            var current = new GlassBounds(point.X, point.Y, item.Host.ActualWidth, item.Host.ActualHeight, radius);
            var below = bounds.Where(b => b.Intersects(current)).Select(b => b.RelativeTo(current)).ToArray();
            item.Host.Background = Quality == GlassQuality.Fallback
                ? new SolidColorBrush(new global::Windows.UI.ViewManagement.UISettings().GetColorValue(global::Windows.UI.ViewManagement.UIColorType.Background))
                : _transparent;
            try
            {
                Renderer.Update(item.Visuals, new GlassSurfaceFrame(item.Style, (float)current.Width,
                    (float)current.Height, (float)radius, (float)Root.XamlRoot.RasterizationScale,
                    Root.ActualTheme == ElementTheme.Dark, Quality, DebugLayer, below, null));
            }
            catch (Exception error)
            {
                item.Visuals.Root.IsVisible = false;
                GlassSystem.ReportRendererUnsupported($"{error.GetType().Name}: {error.Message}");
            }
            bounds.Add(current);
        }
        RendererMilliseconds = Stopwatch.GetElapsedTime(started).TotalMilliseconds;
        _frames++;
        if (_frames % 30 == 0 && _hud is not null)
            _hud.Text = $"Glass {Quality} | {ordered.Count} surfaces\n{FramesPerSecond:F1} FPS | p95 {P95FrameMilliseconds:F2} ms\nUI submission {RendererMilliseconds:F3} ms | {DebugLayer}";
        if (_benchmarkFrames > 0 && _frames == _benchmarkFrames)
            System.IO.File.WriteAllText(System.IO.Path.Combine(System.IO.Path.GetTempPath(), $"goosic-glass-benchmark-{Environment.ProcessId}.txt"),
                $"frames={_frames}\np95_frame_ms={P95FrameMilliseconds:F3}\nfps={FramesPerSecond:F2}\nui_submission_ms={RendererMilliseconds:F3}\nGPU timing: not measured\n");
    }

    private static bool IsVisible(DependencyObject element)
    {
        for (DependencyObject? current = element; current is not null; current = VisualTreeHelper.GetParent(current))
            if (current is UIElement ui && (ui.Visibility != Visibility.Visible || ui.Opacity == 0)) return false;
        return true;
    }

    private void Collect(DependencyObject parent, List<GlassRegistration> ordered)
    {
        var match = _surfaces.FirstOrDefault(s => ReferenceEquals(s.Element, parent));
        if (match is not null) ordered.Add(match);
        var children = new List<UIElement>();
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(parent); i++)
            if (VisualTreeHelper.GetChild(parent, i) is UIElement child) children.Add(child);
        foreach (var child in children.OrderBy(Canvas.GetZIndex)) Collect(child, ordered);
    }

    private void OnRootUnloaded(object sender, RoutedEventArgs args) => Dispose();

    public void Dispose()
    {
        CompositionTarget.Rendering -= OnRendering;
        Root.Unloaded -= OnRootUnloaded;
        foreach (var surface in _surfaces.ToArray()) Unregister(surface.Element);
        if (Root.XamlRoot is not null)
        {
            Scenes.Remove(Root.XamlRoot);
        }
    }
}

public sealed class GlassRegistration(FrameworkElement element, GlassStyle style, int zIndex)
{
    internal Grid? Host { get; set; }
    internal Brush? Fallback { get; set; }
    internal UIElement? Content { get; set; }
    internal WinUiGlassRenderer.GlassSurfaceVisuals? Visuals { get; set; }
    public FrameworkElement Element { get; } = element;
    public GlassStyle Style { get; internal set; } = style;
    public int ZIndex { get; } = zIndex;
}
