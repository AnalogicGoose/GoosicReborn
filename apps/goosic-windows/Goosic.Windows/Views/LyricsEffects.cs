using System;
using System.Collections.Generic;
using System.Numerics;
using Goosic.Windows.Presentation;
using Goosic.Windows.Service;
using Goosic.Windows.ViewModels;
using Microsoft.Graphics.Canvas.Effects;
using Microsoft.UI.Composition;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Hosting;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;

namespace Goosic.Windows.Views;

/// <summary>
/// Draws one lyrics view the way <see cref="LyricsAppearance"/> describes, and knows whether that
/// view is following the song or being read.
/// </summary>
/// <remarks>
/// A softened line is not the text made blurry: XAML text cannot be. The text's own drawing is
/// switched off at the compositor and a visual beside it draws the same text through a blur, so
/// the line still lays out, wraps and takes clicks as ordinary text. A sharp line is the text
/// itself again, which keeps the line being sung as crisp as any other text on screen.
/// <para>
/// Everything here is opacity, blur, scale and translation on the compositor. No line is measured
/// again when the song moves on.
/// </para>
/// </remarks>
internal sealed class LyricsEffects
{
    /// <summary>Room around the text for the blur to spread into.</summary>
    private const float Bleed = 12;

    private static readonly TimeSpan Change = TimeSpan.FromMilliseconds(400);

    private readonly ScrollViewer _scroller;
    private readonly Button _resume;
    private readonly bool _immersive;
    private readonly Func<bool> _animated;
    private readonly Func<bool> _plain;
    private readonly Brush? _accent;
    private readonly Dictionary<TextBlock, Line> _lines = [];
    private Compositor? _compositor;
    private CompositionEffectFactory? _blur;
    private CompositionEasingFunction? _ease;
    private bool _blurUnavailable;

    /// <param name="scroller">The view's scroller; turning its wheel is what stops the following.</param>
    /// <param name="resume">Shown while the view is being read; brings it back to the song.</param>
    /// <param name="immersive">The full-screen player, whose treatment is slightly stronger.</param>
    /// <param name="animated">Whether changes may move, asked each time.</param>
    /// <param name="plain">Whether to leave blur out altogether, asked each time.</param>
    /// <param name="accent">What the line being sung is written in, where it is not simply the text colour.</param>
    internal LyricsEffects(ScrollViewer scroller, Button resume, bool immersive, Func<bool> animated, Func<bool> plain,
        Brush? accent = null)
    {
        _accent = accent;
        _scroller = scroller;
        _resume = resume;
        _immersive = immersive;
        _animated = animated;
        _plain = plain;
        // The scroller handles the wheel itself, so this listens to handled events too.
        scroller.AddHandler(UIElement.PointerWheelChangedEvent, new PointerEventHandler((_, _) => SetFollowing(false)), true);
        scroller.DirectManipulationStarted += (_, _) => SetFollowing(false);
    }

    /// <summary>Whether the view scrolls with the song. False once the listener scrolls it themselves.</summary>
    internal bool Following { get; private set; } = true;

    /// <summary>Raised when following resumes, so the view can return to the line being sung.</summary>
    internal event Action? Resumed;

    internal void SetFollowing(bool following)
    {
        if (Following == following)
        {
            return;
        }

        Following = following;
        Refresh();
        if (following)
        {
            Resumed?.Invoke();
        }
    }

    /// <summary>Takes a line's text into the view as its template is realised.</summary>
    internal void Add(TextBlock text)
    {
        if (_lines.ContainsKey(text) || text.DataContext is not LyricLineViewModel model
            || VisualTreeHelper.GetParent(text) is not FrameworkElement host)
        {
            return;
        }

        var line = new Line(text, host, model);
        _lines[text] = line;
        host.PointerEntered += (_, _) => Point(line, true);
        host.PointerExited += (_, _) => Point(line, false);
        host.PointerCanceled += (_, _) => Point(line, false);
        text.SizeChanged += (_, _) => Anchor(line);
        Anchor(line);
        Apply(line, ActiveIndex(), animate: false);
        UpdateResume();
    }

    internal void Remove(TextBlock text)
    {
        if (_lines.Remove(text, out var line))
        {
            line.Sprite?.Dispose();
            line.Surface?.Dispose();
            line.Brush?.Dispose();
            UpdateResume();
        }
    }

    /// <summary>Redraws every line for the line now being sung.</summary>
    internal void Refresh()
    {
        var active = ActiveIndex();
        var animate = _animated();
        foreach (var line in _lines.Values)
        {
            Apply(line, active, animate);
        }

        UpdateResume();
    }

    private void Point(Line line, bool pointed)
    {
        if (line.Pointed != pointed)
        {
            line.Pointed = pointed;
            Apply(line, ActiveIndex(), _animated());
        }
    }

    private void UpdateResume()
    {
        var synced = false;
        foreach (var line in _lines.Values)
        {
            synced = line.Model.Synced;
            break;
        }

        _resume.Visibility = synced && !Following ? Visibility.Visible : Visibility.Collapsed;
    }

    private int ActiveIndex()
    {
        foreach (var line in _lines.Values)
        {
            if (line.Model.IsCurrent)
            {
                return line.Model.Index;
            }
        }

        return -1;
    }

    /// <summary>Scale turns about the line's leading edge, so its first letter stays where it was.</summary>
    private static void Anchor(Line line) =>
        line.Host.CenterPoint = new Vector3(0, (float)(line.Host.ActualHeight / 2), 0);

    private void Apply(Line line, int active, bool animate)
    {
        var look = LyricsAppearance.Line(line.Model.Index, active, line.Model.Synced, Following,
            line.Pointed, _immersive, _plain());
        if (look == line.Look)
        {
            return;
        }

        line.Look = look;
        if (_accent is not null)
        {
            if (line.Model.IsCurrent)
            {
                line.Text.Foreground = _accent;
            }
            else
            {
                line.Text.ClearValue(TextBlock.ForegroundProperty);
            }
        }

        var transition = animate ? new Vector3Transition { Duration = Change } : null;
        line.Host.ScaleTransition = transition;
        line.Host.TranslationTransition = transition;
        line.Host.Scale = new Vector3((float)look.Scale, (float)look.Scale, 1);
        line.Host.Translation = new Vector3(0, (float)look.Lift, 0);

        var softened = look.Blur > 0 && EnsureBlur(line);
        var text = ElementCompositionPreview.GetElementVisual(line.Text);
        Animate(text, "Opacity", softened ? 0 : (float)look.Opacity, animate);
        if (line.Sprite is not null)
        {
            Animate(line.Sprite, "Opacity", softened ? (float)look.Opacity : 0, animate);
            // Left at its last radius while the sharp text takes over, so it fades rather than snaps.
            if (softened)
            {
                Animate(line.Brush!.Properties, "Blur.BlurAmount", (float)look.Blur, animate);
            }
        }
    }

    private void Animate(CompositionObject target, string property, float value, bool animate)
    {
        if (!animate || _compositor is null)
        {
            target.StopAnimation(property);
            if (target is Visual visual)
            {
                visual.Opacity = value;
            }
            else
            {
                ((CompositionPropertySet)target).InsertScalar(property, value);
            }

            return;
        }

        var animation = _compositor.CreateScalarKeyFrameAnimation();
        animation.InsertKeyFrame(1, value, _ease);
        animation.Duration = Change;
        target.StartAnimation(property, animation);
    }

    /// <summary>Builds the visual that draws this line through a blur, the first time it needs one.</summary>
    private bool EnsureBlur(Line line)
    {
        if (line.Sprite is not null)
        {
            return true;
        }

        if (_blurUnavailable)
        {
            return false;
        }

        try
        {
            var text = ElementCompositionPreview.GetElementVisual(line.Text);
            _compositor ??= text.Compositor;
            _ease ??= _compositor.CreateCubicBezierEasingFunction(new Vector2(0.42f, 0), new Vector2(0.58f, 1));
            _blur ??= _compositor.CreateEffectFactory(new GaussianBlurEffect
            {
                Name = "Blur",
                BlurAmount = 1,
                BorderMode = EffectBorderMode.Soft,
                Source = new CompositionEffectSourceParameter("Text"),
            }, ["Blur.BlurAmount"]);

            var bleed = new Vector2(Bleed * 2, Bleed * 2);
            var surface = _compositor.CreateVisualSurface();
            surface.SourceVisual = text;
            surface.SourceOffset = new Vector2(-Bleed, -Bleed);
            var size = _compositor.CreateExpressionAnimation("text.Size + bleed");
            size.SetReferenceParameter("text", text);
            size.SetVector2Parameter("bleed", bleed);
            surface.StartAnimation(nameof(surface.SourceSize), size);

            var source = _compositor.CreateSurfaceBrush(surface);
            source.Stretch = CompositionStretch.None;
            source.HorizontalAlignmentRatio = 0;
            source.VerticalAlignmentRatio = 0;
            var brush = _blur.CreateBrush();
            brush.SetSourceParameter("Text", source);

            var sprite = _compositor.CreateSpriteVisual();
            sprite.Brush = brush;
            sprite.Opacity = 0;
            sprite.StartAnimation(nameof(sprite.Size), size);
            // The text sits at its host's origin; the sprite reaches a little past it on every side.
            var offset = _compositor.CreateExpressionAnimation("Vector3(text.Offset.X - b, text.Offset.Y - b, 0)");
            offset.SetReferenceParameter("text", text);
            offset.SetScalarParameter("b", Bleed);
            sprite.StartAnimation(nameof(sprite.Offset), offset);
            ElementCompositionPreview.SetElementChildVisual(line.Host, sprite);

            line.Surface = surface;
            line.Brush = brush;
            line.Sprite = sprite;
            return true;
        }
        catch (Exception error)
        {
            // The lines still dim and step back; they are only not blurred.
            _blurUnavailable = true;
            BridgeLog.Write($"lyric blur unavailable: {error.GetType().Name}");
            return false;
        }
    }

    private sealed class Line(TextBlock text, FrameworkElement host, LyricLineViewModel model)
    {
        internal TextBlock Text { get; } = text;
        internal FrameworkElement Host { get; } = host;
        internal LyricLineViewModel Model { get; } = model;
        internal bool Pointed { get; set; }
        internal LyricLineLook? Look { get; set; }
        internal CompositionVisualSurface? Surface { get; set; }
        internal CompositionEffectBrush? Brush { get; set; }
        internal SpriteVisual? Sprite { get; set; }
    }
}
