using System;
using System.Collections.Generic;
using System.Globalization;
using System.Numerics;
using Goosic.Windows.Presentation;
using Goosic.Windows.Service;
using Goosic.Windows.ViewModels;
using Microsoft.Graphics.Canvas.Effects;
using Microsoft.UI;
using Microsoft.UI.Composition;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Documents;
using Microsoft.UI.Xaml.Hosting;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Windows.Foundation;
using Windows.UI;

namespace Goosic.Windows.Views;

/// <summary>
/// Draws one lyrics view the way <see cref="LyricsAppearance"/> and <see cref="LyricsTiming"/>
/// describe, moves its rows the way <see cref="LyricsMotion"/> describes, and knows whether that
/// view is following the song or being read.
/// </summary>
/// <remarks>
/// A softened line is not the text made blurry: XAML text cannot be. The text's own drawing is
/// switched off at the compositor and a visual beside it draws the same text through a blur, so
/// the line still lays out, wraps and takes clicks as ordinary text. A sharp line is the text
/// itself again, which keeps the line being sung as crisp as any other text on screen.
/// <para>
/// The line being sung, when the provider timed its words, is swept: each character is its own run
/// whose brush brightens from <see cref="LyricsTiming.UnsungOpacity"/> to full across its word's
/// span. Only a brush's colour changes, so the line is never measured again; DirectWrite shapes
/// runs that differ only in colour together, so ligatures and joined scripts are kept. Only that one
/// line, or one row of interlude dots, is redrawn per frame, at most sixty times a second.
/// </para>
/// <para>
/// Everything else is opacity, blur and translation on the compositor.
/// </para>
/// </remarks>
internal sealed class LyricsEffects
{
    /// <summary>Room around the text for the blur to spread into.</summary>
    private const float Bleed = 12;

    /// <summary>A line's emphasis settles over 0.55 seconds without overshoot, as on macOS.</summary>
    private static readonly TimeSpan Change = TimeSpan.FromMilliseconds(550);

    /// <summary>A line revealed by the pointer eases faster; lines pass under a resting pointer on every change.</summary>
    private static readonly TimeSpan PointerChange = TimeSpan.FromMilliseconds(200);

    /// <summary>Interlude dots fade in and out over this.</summary>
    private static readonly TimeSpan DotsChange = TimeSpan.FromMilliseconds(450);

    private static readonly TimeSpan FrameInterval = TimeSpan.FromSeconds(1.0 / 60) - TimeSpan.FromMilliseconds(1);

    private readonly ScrollViewer _scroller;
    private readonly StackPanel _content;
    private readonly ItemsControl _items;
    private readonly FrameworkElement? _intro;
    private readonly Button _resume;
    private readonly bool _immersive;
    private readonly LyricsFollower _follower;
    private readonly Func<bool> _animated;
    private readonly Func<bool> _plain;
    private readonly Brush? _accent;
    private readonly Dictionary<TextBlock, Line> _lines = [];
    private Compositor? _compositor;
    private CompositionEffectFactory? _blur;
    private CompositionEasingFunction? _ease;
    private bool _blurUnavailable;
    private bool _rendering;
    private bool _visible;
    private DateTimeOffset _lastFrame;
    private int _followed = int.MinValue;
    private Line? _swept;
    private bool? _introActive;

    /// <param name="scroller">The view's scroller; turning its wheel is what stops the following.</param>
    /// <param name="content">The scroller's content, padded so any row can reach the anchor.</param>
    /// <param name="items">The lines.</param>
    /// <param name="intro">The introduction's dots above the first line, when the view has them.</param>
    /// <param name="resume">Shown while the view is being read; brings it back to the song.</param>
    /// <param name="immersive">The full-screen player, as opposed to the side panel.</param>
    /// <param name="follower">The clock the view draws by.</param>
    /// <param name="animated">Whether changes may move, asked each time.</param>
    /// <param name="plain">Whether to leave blur and the sweep out altogether, asked each time.</param>
    /// <param name="accent">What the line being sung is written in, where it is not simply the text colour.</param>
    internal LyricsEffects(ScrollViewer scroller, StackPanel content, ItemsControl items, FrameworkElement? intro,
        Button resume, bool immersive, LyricsFollower follower, Func<bool> animated, Func<bool> plain,
        Brush? accent = null)
    {
        _accent = accent;
        _scroller = scroller;
        _content = content;
        _items = items;
        _intro = intro;
        _resume = resume;
        _immersive = immersive;
        _follower = follower;
        _animated = animated;
        _plain = plain;
        // The scroller handles the wheel itself, so this listens to handled events too.
        scroller.AddHandler(UIElement.PointerWheelChangedEvent, new PointerEventHandler((_, _) => SetFollowing(false)), true);
        scroller.DirectManipulationStarted += (_, _) => SetFollowing(false);
        scroller.SizeChanged += (_, _) => Pad();
        Pad();
    }

    /// <summary>Hidden or frozen views do not keep a rendering callback alive.</summary>
    internal void SetVisible(bool visible)
    {
        _visible = visible;
        if (visible)
        {
            Refresh();
            Follow(animate: false);
        }
        else
        {
            EnsureRendering();
        }
    }

    internal void ClockChanged() => EnsureRendering();

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

    /// <summary>A new document: nothing has been followed in it yet.</summary>
    internal void Reset()
    {
        _followed = int.MinValue;
        _introActive = null;
    }

    /// <summary>Takes a line's text into the view as its template is realised.</summary>
    internal void Add(TextBlock text)
    {
        if (_lines.ContainsKey(text) || text.DataContext is not LyricLineViewModel model
            || VisualTreeHelper.GetParent(text) is not Panel host)
        {
            return;
        }

        var line = new Line(text, host, model);
        foreach (var child in host.Children)
        {
            if (child is StackPanel { Tag: "empty" } empty)
            {
                line.EmptyDots = empty;
            }
            else if (child is StackPanel { Tag: "after" } after)
            {
                line.AfterDots = after;
            }
        }

        _lines[text] = line;
        host.PointerEntered += (_, _) => Point(line, true);
        host.PointerExited += (_, _) => Point(line, false);
        host.PointerCanceled += (_, _) => Point(line, false);
        Apply(line, animate: false);
        UpdateResume();
        EnsureRendering();
    }

    internal void Remove(TextBlock text)
    {
        if (_lines.Remove(text, out var line))
        {
            if (ReferenceEquals(_swept, line))
            {
                _swept = null;
            }

            line.Sprite?.Dispose();
            line.Surface?.Dispose();
            line.Brush?.Dispose();
            UpdateResume();
            EnsureRendering();
        }
    }

    /// <summary>Redraws every line for the line now being sung, or the rest after it.</summary>
    internal void Refresh()
    {
        var animate = _animated();
        foreach (var line in _lines.Values)
        {
            Apply(line, animate);
        }

        var introActive = _follower.Synced && _follower.Index < 0;
        if (_intro is not null && introActive != _introActive)
        {
            _introActive = introActive;
            FadeDots(_intro, introActive, animate);
        }

        UpdateResume();
        EnsureRendering();
    }

    /// <summary>
    /// Moves the row being followed to the anchor: the dots while an interlude plays, otherwise the
    /// line, or the introduction before the first line.
    /// </summary>
    /// <remarks>
    /// The column scrolls to the new row at once and, in the same update, every row is drawn lower
    /// by exactly the distance scrolled, so nothing has moved. Each row then gives that offset back
    /// on its own spring after its own delay. When there is no earlier row to come from, or the
    /// listener scrolled far away, the column moves as one instead. Without motion it jumps.
    /// </remarks>
    internal void Follow(bool animate)
    {
        if (!Following || _scroller.Content is not UIElement content)
        {
            return;
        }

        var index = _follower.Index;
        var target = AnchorRow(index);
        if (target is null || target.ActualHeight <= 0)
        {
            return;
        }

        var top = target.TransformToVisual(content).TransformPoint(new Point(0, 0)).Y;
        var offset = LyricsMotion.Target(top, target.ActualHeight, _scroller.ViewportHeight, _scroller.ScrollableHeight);
        var shift = offset - _scroller.VerticalOffset;
        var from = _followed;
        _followed = index * 2 + (_follower.Resting ? 1 : 0);
        animate &= _animated();
        if (!animate)
        {
            _scroller.ChangeView(null, offset, null, disableAnimation: true);
            return;
        }

        if (from == int.MinValue || from == _followed || Math.Abs(shift) > _scroller.ViewportHeight * 1.5
            || Math.Abs(shift) < 0.5)
        {
            _scroller.ChangeView(null, offset, null, disableAnimation: false);
            return;
        }

        _scroller.ChangeView(null, offset, null, disableAnimation: true);
        var leader = Math.Max(index, 0);
        if (_intro is not null)
        {
            Shift(_intro, (float)shift, LyricsMotion.Delay(-1, leader));
        }

        for (var order = 0; order < _items.Items.Count; order++)
        {
            if (_items.ContainerFromIndex(order) is UIElement row)
            {
                Shift(row, (float)shift, LyricsMotion.Delay(order, leader));
            }
        }
    }

    /// <summary>Draws a row <paramref name="shift"/> lower at once, then springs it back to its place.</summary>
    private void Shift(UIElement row, float shift, TimeSpan delay)
    {
        var visual = ElementCompositionPreview.GetElementVisual(row);
        _compositor ??= visual.Compositor;
        ElementCompositionPreview.SetIsTranslationEnabled(row, true);
        var spring = _compositor.CreateSpringVector3Animation();
        spring.DampingRatio = 1;
        spring.Period = LyricsMotion.RowSpringPeriod;
        spring.InitialValue = new Vector3(0, shift, 0);
        spring.FinalValue = Vector3.Zero;
        spring.DelayTime = delay;
        spring.DelayBehavior = AnimationDelayBehavior.SetInitialValueBeforeDelay;
        visual.StartAnimation("Translation", spring);
    }

    private FrameworkElement? AnchorRow(int index)
    {
        if (index < 0)
        {
            return _intro is { Visibility: Visibility.Visible } ? _intro : _items.ContainerFromIndex(0) as FrameworkElement;
        }

        if (_follower.Resting)
        {
            foreach (var line in _lines.Values)
            {
                if (line.Model.Index == index && line.Model.InterludeAfter is not null && line.AfterDots is { } dots)
                {
                    return dots;
                }
            }
        }

        return _items.ContainerFromIndex(index) as FrameworkElement;
    }

    /// <summary>Pads the column so the first and last rows can reach the anchor too.</summary>
    private void Pad()
    {
        var (top, bottom) = LyricsMotion.Padding(_scroller.ViewportHeight > 0 ? _scroller.ViewportHeight : _scroller.ActualHeight,
            _immersive);
        _content.Padding = new Thickness(0, top, 0, bottom);
        if (Following && _follower.Synced)
        {
            _content.UpdateLayout();
            Follow(animate: false);
        }
    }

    private void Point(Line line, bool pointed)
    {
        if (line.Pointed != pointed)
        {
            line.Pointed = pointed;
            Apply(line, _animated(), PointerChange);
        }
    }

    private void UpdateResume() =>
        _resume.Visibility = _follower.Synced && !Following ? Visibility.Visible : Visibility.Collapsed;

    private void Apply(Line line, bool animate, TimeSpan? duration = null)
    {
        var index = _follower.Index;
        var resting = _follower.Resting;
        var lit = LyricsAppearance.Lit(line.Model.Index, index, resting);
        var plain = _plain();
        var look = LyricsAppearance.Line(line.Model.Index, index, resting, line.Model.Synced, Following,
            line.Pointed, plain);

        if (_accent is not null)
        {
            if (lit)
            {
                line.Text.Foreground = _accent;
            }
            else
            {
                line.Text.ClearValue(TextBlock.ForegroundProperty);
            }
        }

        Sweep(line, lit && line.Model.Synced && Following && animate && !plain
            && line.Model.Timed.SpelledWords.Count > 0 && line.Text.FocusState == FocusState.Unfocused);

        var emptyActive = lit && line.Model.EmptyInterlude is not null;
        var afterActive = resting && line.Model.Index == index && line.Model.InterludeAfter is not null;
        if (line.EmptyDots is { } empty && line.EmptyActive != emptyActive)
        {
            line.EmptyActive = emptyActive;
            FadeDots(empty, emptyActive, animate);
        }

        if (line.AfterDots is { } after && line.AfterActive != afterActive)
        {
            line.AfterActive = afterActive;
            FadeDots(after, afterActive, animate);
        }

        if (look == line.Look)
        {
            return;
        }

        line.Look = look;
        // Rows below the current one settle a beat apart, so a change travels down the column.
        var offset = line.Model.Index - Math.Max(index, -1);
        var delay = duration is null ? LyricsMotion.RowDelay * Math.Clamp(offset, 0, 6) : TimeSpan.Zero;
        var time = duration ?? Change;
        var softened = look.Blur > 0 && EnsureBlur(line);
        var text = ElementCompositionPreview.GetElementVisual(line.Text);
        Animate(text, "Opacity", softened ? 0 : (float)look.Opacity, animate, time, delay);
        if (line.Sprite is not null)
        {
            Animate(line.Sprite, "Opacity", softened ? (float)look.Opacity : 0, animate, time, delay);
            // Left at its last radius while the sharp text takes over, so it fades rather than snaps.
            if (softened)
            {
                Animate(line.Brush!.Properties, "Blur.BlurAmount", (float)look.Blur, animate, time, delay);
            }
        }
    }

    /// <summary>
    /// Splits the line into one run per character the first time it is swept, and puts it back as
    /// plain text when it stops being swept. The text and its font are the same either way, so the
    /// line wraps exactly as it did.
    /// </summary>
    private void Sweep(Line line, bool sweep)
    {
        if (sweep == (line.Glyphs is not null))
        {
            return;
        }

        if (!sweep)
        {
            line.Glyphs = null;
            line.Text.Text = line.Model.Shown;
            if (ReferenceEquals(_swept, line))
            {
                _swept = null;
            }

            return;
        }

        var color = line.Text.Foreground is SolidColorBrush solid ? solid.Color : Colors.White;
        var glyphs = new List<Glyph>();
        var sweeps = LyricsTiming.Sweeps(line.Model.Timed);
        var words = line.Model.Timed.SpelledWords;
        line.Text.Inlines.Clear();
        for (var word = 0; word < words.Count; word++)
        {
            var elements = StringInfo.GetTextElementEnumerator(words[word].Text);
            var characters = new StringInfo(words[word].Text).LengthInTextElements;
            var character = 0;
            while (elements.MoveNext())
            {
                var brush = new SolidColorBrush(WithAlpha(color, LyricsTiming.UnsungOpacity));
                line.Text.Inlines.Add(new Run { Text = (string)elements.Current, Foreground = brush });
                glyphs.Add(new Glyph(brush, sweeps[word], character++, characters, -1));
            }
        }

        line.Glyphs = glyphs;
        line.Color = color;
        _swept = line;
    }

    private static Color WithAlpha(Color color, double opacity) =>
        Color.FromArgb((byte)Math.Round(color.A * Math.Clamp(opacity, 0, 1)), color.R, color.G, color.B);

    /// <summary>Draws whatever moves with the music: the swept line and any active dots.</summary>
    private void EnsureRendering()
    {
        var live = _swept is not null || _introActive == true;
        foreach (var line in _lines.Values)
        {
            live |= line.EmptyActive || line.AfterActive;
        }

        live &= _visible && _follower.Running && _animated();
        if (!live && _visible)
        {
            Draw(DateTimeOffset.Now);
        }

        if (live == _rendering)
        {
            if (live)
            {
                Draw(DateTimeOffset.Now);
            }

            return;
        }

        _rendering = live;
        if (live)
        {
            CompositionTarget.Rendering += OnRendering;
            Draw(DateTimeOffset.Now);
        }
        else
        {
            CompositionTarget.Rendering -= OnRendering;
        }
    }

    private void OnRendering(object? sender, object e)
    {
        var now = DateTimeOffset.Now;
        if (now - _lastFrame < FrameInterval)
        {
            return;
        }

        _lastFrame = now;
        Draw(now);
    }

    private void Draw(DateTimeOffset now)
    {
        var position = _follower.PositionMs(now);
        if (_swept?.Glyphs is { } glyphs)
        {
            var color = _swept.Color;
            for (var i = 0; i < glyphs.Count; i++)
            {
                var glyph = glyphs[i];
                var fill = LyricsTiming.CharacterFill(glyph.Sweep, glyph.Character, glyph.Characters, position);
                if (Math.Abs(fill - glyph.Fill) < 0.004)
                {
                    continue;
                }

                glyph.Brush.Color = WithAlpha(color, LyricsTiming.UnsungOpacity + fill * (1 - LyricsTiming.UnsungOpacity));
                glyphs[i] = glyph with { Fill = fill };
            }
        }

        var animated = _animated();
        if (_introActive == true && _intro is Panel introDots && _follower.Lines.Count > 0
            && LyricsTiming.Introduction(_follower.Lines, _follower.Synced) is { } intro)
        {
            DrawDots(introDots, intro, position, animated);
        }

        foreach (var line in _lines.Values)
        {
            if (line.EmptyActive && line.EmptyDots is { } empty && line.Model.EmptyInterlude is { } gap)
            {
                DrawDots(empty, gap, position, animated);
            }

            if (line.AfterActive && line.AfterDots is { } after && line.Model.InterludeAfter is { } rest)
            {
                DrawDots(after, rest, position, animated);
            }
        }
    }

    /// <summary>Three dots that fill in turn across the interlude, breathe, and fold away before the next line.</summary>
    private static void DrawDots(Panel dots, LyricsTiming.Interlude gap, double position, bool animated)
    {
        var dot = 0;
        foreach (var child in dots.Children)
        {
            if (child is Ellipse ellipse)
            {
                ellipse.Opacity = gap.DotOpacity(dot++, position);
            }
        }

        var visual = ElementCompositionPreview.GetElementVisual(dots);
        var scale = (float)gap.Scale(position, animated);
        visual.CenterPoint = new Vector3((float)(dots.ActualWidth / 2), (float)(dots.ActualHeight / 2), 0);
        visual.Scale = new Vector3(scale, scale, 1);
    }

    private void FadeDots(UIElement dots, bool active, bool animate)
    {
        var visual = ElementCompositionPreview.GetElementVisual(dots);
        _compositor ??= visual.Compositor;
        Animate(visual, "Opacity", active ? 1 : 0, animate, DotsChange, TimeSpan.Zero);
    }

    private void Animate(CompositionObject target, string property, float value, bool animate, TimeSpan duration,
        TimeSpan delay)
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

        _ease ??= _compositor.CreateCubicBezierEasingFunction(new Vector2(0.2f, 0), new Vector2(0, 1));
        var animation = _compositor.CreateScalarKeyFrameAnimation();
        animation.InsertKeyFrame(1, value, _ease);
        animation.Duration = duration;
        animation.DelayTime = delay;
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
            // The lines still dim; they are only not blurred.
            _blurUnavailable = true;
            BridgeLog.Write($"lyric blur unavailable: {error.GetType().Name}");
            return false;
        }
    }

    /// <summary>One character of the swept line and the brush that lights it.</summary>
    private readonly record struct Glyph(SolidColorBrush Brush, LyricsTiming.Sweep Sweep, int Character, int Characters,
        double Fill);

    private sealed class Line(TextBlock text, Panel host, LyricLineViewModel model)
    {
        internal TextBlock Text { get; } = text;
        internal Panel Host { get; } = host;
        internal LyricLineViewModel Model { get; } = model;
        internal StackPanel? EmptyDots { get; set; }
        internal StackPanel? AfterDots { get; set; }
        internal bool EmptyActive { get; set; }
        internal bool AfterActive { get; set; }
        internal bool Pointed { get; set; }
        internal LyricLineLook? Look { get; set; }
        internal List<Glyph>? Glyphs { get; set; }
        internal Color Color { get; set; }
        internal CompositionVisualSurface? Surface { get; set; }
        internal CompositionEffectBrush? Brush { get; set; }
        internal SpriteVisual? Sprite { get; set; }
    }
}
