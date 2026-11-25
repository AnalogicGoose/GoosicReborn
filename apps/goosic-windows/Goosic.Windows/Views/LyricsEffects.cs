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
using Microsoft.UI.Xaml.Media.Animation;
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

    /// <summary>
    /// A line's emphasis: macOS's <c>.smooth(duration: 0.55)</c>, a critically damped spring whose
    /// response is 0.55 s, drawn on SwiftUI's own curve (<see cref="SwiftUISpring"/>).
    /// </summary>
    private static readonly Motion Change = Motion.Spring(0.55);

    /// <summary>A line revealed by the pointer or focus: macOS's <c>.easeOut(duration: 0.2)</c>.</summary>
    private static readonly Motion PointerChange = Motion.EaseOut(0.2);

    /// <summary>Interlude dots fade in and out: macOS's <c>.smooth(duration: 0.45)</c>.</summary>
    private static readonly Motion DotsChange = Motion.Spring(0.45);

    /// <summary>
    /// The panel's sung line changes colour with its emphasis. A cubic ease-out over 0.6 s tracks
    /// the 0.55 s critically damped spring to within a few percent at every point.
    /// </summary>
    private static readonly TimeSpan InkChange = TimeSpan.FromMilliseconds(600);

    /// <summary>The pointer's pill behind a line: macOS's <c>.easeOut(duration: 0.15)</c>.</summary>
    private static readonly Motion HoverChange = Motion.EaseOut(0.15);

    /// <summary>How strongly the pointer's pill shows: the text colour at 8%, as on macOS.</summary>
    private const float HoverOpacity = 0.08f;

    /// <summary>The soft leading edge of the sweep, in multiples of the font size, from macOS.</summary>
    private const double FeatherEm = 0.5;

    /// <summary>Roughly how wide a bold character is, in multiples of the font size, until layout says.</summary>
    private const double EstimatedCharacterEm = 0.55;

    /// <summary>The glow around the line being sung: white at 20%, as on macOS.</summary>
    private const float GlowOpacity = 0.2f;


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
    private readonly Func<bool> _canSeek;
    private readonly Dictionary<TextBlock, Line> _lines = [];
    private readonly Dictionary<UIElement, CompositionPropertySet> _shifted = [];
    private readonly Dictionary<UIElement, RowMotion> _motions = [];
    private CompositionPropertySet? _scroll;
    private int _shiftVersion;
    private Compositor? _compositor;
    private CompositionEffectFactory? _blur;
    private CompositionEasingFunction? _ease;
    private bool _blurUnavailable;
    private bool _rendering;
    private bool _visible;
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
    /// <param name="canSeek">Whether a line may seek right now, which is when the pointer's pill shows.</param>
    /// <param name="accent">What the line being sung is written in, where it is not simply the text colour.</param>
    internal LyricsEffects(ScrollViewer scroller, StackPanel content, ItemsControl items, FrameworkElement? intro,
        Button resume, bool immersive, LyricsFollower follower, Func<bool> animated, Func<bool> plain,
        Func<bool> canSeek, Brush? accent = null)
    {
        _accent = accent;
        _canSeek = canSeek;
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
        if (!following)
        {
            // The rows follow the scroller's own position while they move; the listener's scroll
            // must not be fought, so they are put back at once.
            StopShifts();
        }

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
            else if (child is Border { Tag: "hover" } hover)
            {
                line.Hover = hover;
            }
            else if (child is Border { Tag: "glow" } glowHost)
            {
                line.GlowHost = glowHost;
            }
        }

        _lines[text] = line;
        Ink(line);
        // Kept on the line so Remove can take them off again: a row that unloads and loads again
        // would otherwise answer for two lines, one of them holding disposed visuals.
        TypedEventHandler<FrameworkElement, object> theme = (_, _) => Ink(line);
        PointerEventHandler entered = (_, _) => Point(line, true);
        PointerEventHandler exited = (_, _) => Point(line, false);
        host.ActualThemeChanged += theme;
        host.PointerEntered += entered;
        host.PointerExited += exited;
        host.PointerCanceled += exited;
        line.Detach += () =>
        {
            host.ActualThemeChanged -= theme;
            host.PointerEntered -= entered;
            host.PointerExited -= exited;
            host.PointerCanceled -= exited;
        };
        // Keyboard focus is on the line's button, not its text: a focused line is being read, so it
        // is drawn sharp and is not swept, as on macOS.
        for (var node = VisualTreeHelper.GetParent(host); node is not null; node = VisualTreeHelper.GetParent(node))
        {
            if (node is Button button)
            {
                RoutedEventHandler got = (_, _) => Focus(line, true);
                RoutedEventHandler lost = (_, _) => Focus(line, false);
                button.GotFocus += got;
                button.LostFocus += lost;
                line.Detach += () =>
                {
                    button.GotFocus -= got;
                    button.LostFocus -= lost;
                };
                break;
            }
        }
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

            line.Removed = true;
            line.Detach?.Invoke();
            line.InkStory?.Stop();
            // Unhooked from the tree before they are disposed, so nothing draws a disposed visual.
            if (line.Sprite is not null)
            {
                ElementCompositionPreview.SetElementChildVisual(line.Host, null);
            }

            if (line.Glow is not null && line.GlowHost is not null)
            {
                ElementCompositionPreview.SetElementChildVisual(line.GlowHost, null);
            }

            line.Sprite?.Dispose();
            line.Surface?.Dispose();
            line.Brush?.Dispose();
            line.Glow?.Dispose();
            line.Sprite = null;
            line.Surface = null;
            line.Brush = null;
            line.Glow = null;
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
        if (!animate || Math.Abs(shift) < 0.5)
        {
            StopShifts();
            _scroller.ChangeView(null, offset, null, disableAnimation: true);
            return;
        }

        // With no earlier row to come from, back to the row already followed, or from far away,
        // the column moves as one on the same spring, as macOS's withAnimation(rowSpring) does.
        var together = from == int.MinValue || from == _followed || Math.Abs(shift) > _scroller.ViewportHeight * 1.5;
        var old = (float)_scroller.VerticalOffset;
        _scroller.ChangeView(null, offset, null, disableAnimation: true);
        var leader = Math.Max(index, 0);
        var version = ++_shiftVersion;
        if (_intro is not null)
        {
            Shift(_intro, old, (float)shift, together ? TimeSpan.Zero : LyricsMotion.Delay(-1, leader));
        }

        for (var order = 0; order < _items.Items.Count; order++)
        {
            if (_items.ContainerFromIndex(order) is UIElement row)
            {
                Shift(row, old, (float)shift, together ? TimeSpan.Zero : LyricsMotion.Delay(order, leader));
            }
        }

        // Once every row has settled and the scroll has landed, the rows go back to plain layout.
        var settle = _scroller.DispatcherQueue.CreateTimer();
        settle.IsRepeating = false;
        settle.Interval = LyricsMotion.RowDelay * LyricsMotion.MaxDelayedRows + TimeSpan.FromSeconds(1.5);
        settle.Tick += (_, _) =>
        {
            settle.Stop();
            if (version == _shiftVersion && Math.Abs(_scroller.VerticalOffset - offset) < 1)
            {
                StopShifts();
            }
        };
        settle.Start();
    }

    /// <summary>
    /// Moves one row from where it was drawn before the line change to its new place, on its own
    /// critically damped spring after its own delay.
    /// </summary>
    /// <remarks>
    /// The scroll and the rows' offsets must change in the same frame, and a scroller applies a
    /// view change asynchronously, a frame or so after it is asked. Starting the offsets beside it
    /// made every line change twitch by the distance between two lines. So the offset is an
    /// expression of the scroller's own live position on the compositor: until the scroll lands the
    /// row is moved by its offset alone (<c>-shift·c</c>), and from the frame it lands, by what the
    /// scroll has not yet covered (<c>shift·(1 − c)</c>). The two agree at every instant, whichever
    /// frame the scroll lands in.
    /// </remarks>
    private void Shift(UIElement row, float old, float shift, TimeSpan delay)
    {
        var visual = ElementCompositionPreview.GetElementVisual(row);
        _compositor ??= visual.Compositor;
        _scroll ??= ElementCompositionPreview.GetScrollViewerManipulationPropertySet(_scroller);
        ElementCompositionPreview.SetIsTranslationEnabled(row, true);
        // A row still on its way from the last line change is not snapped to its place: what it
        // had left to travel is carried into this move and given back on a spring of its own.
        var now = DateTimeOffset.Now;
        var residual = _motions.TryGetValue(row, out var previous) ? previous.Remaining(now) : 0f;
        _motions[row] = new RowMotion(now, delay, shift, residual);
        if (_shifted.Remove(row, out var replaced))
        {
            replaced.Dispose();
        }

        var progress = _compositor.CreatePropertySet();
        progress.InsertScalar("c", 0);
        progress.InsertScalar("r", 0);
        var spring = SwiftUISpring.Progress(_compositor, LyricsMotion.RowSpringPeriod.TotalSeconds);
        spring.DelayTime = delay;
        spring.DelayBehavior = AnimationDelayBehavior.SetInitialValueBeforeDelay;
        progress.StartAnimation("c", spring);
        // The cached animation is a template: its delay is set again before every start.
        spring.DelayTime = TimeSpan.Zero;
        progress.StartAnimation("r", spring);
        var offset = _compositor.CreateExpressionAnimation(
            "Vector3(0, (Abs(-s.Translation.Y - old) < 0.5 ? -shift * p.c : shift * (1 - p.c)) + residual * (1 - p.r), 0)");
        offset.SetReferenceParameter("s", _scroll);
        offset.SetReferenceParameter("p", progress);
        offset.SetScalarParameter("old", old);
        offset.SetScalarParameter("shift", shift);
        offset.SetScalarParameter("residual", residual);
        visual.StartAnimation("Translation", offset);
        _shifted[row] = progress;
    }

    /// <summary>
    /// One row's move, kept so a move that interrupts it knows how far the row still had to go: the
    /// same curves the compositor is drawing, evaluated here, on the assumption that the scroll has
    /// landed, which it does within a frame or two of being asked.
    /// </summary>
    private readonly record struct RowMotion(DateTimeOffset StartedAt, TimeSpan Delay, float Shift, float Residual)
    {
        internal float Remaining(DateTimeOffset now)
        {
            var response = LyricsMotion.RowSpringPeriod.TotalSeconds;
            var elapsed = (now - StartedAt).TotalSeconds;
            return (float)(Shift * (1 - LyricsMotion.Spring(elapsed - Delay.TotalSeconds, response))
                + Residual * (1 - LyricsMotion.Spring(elapsed, response)));
        }
    }

    /// <summary>Puts every moved row back at its laid-out place, as when the listener takes over the scroll.</summary>
    private void StopShifts()
    {
        _shiftVersion++;
        foreach (var row in _shifted.Keys)
        {
            var visual = ElementCompositionPreview.GetElementVisual(row);
            visual.StopAnimation("Translation");
            visual.Properties.InsertVector3("Translation", Vector3.Zero);
        }

        foreach (var progress in _shifted.Values)
        {
            progress.Dispose();
        }

        _shifted.Clear();
        _motions.Clear();
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
        // The view reaches past the text on both sides (its host's negative margin) so the pointer's
        // pill, the glow and the dots' swell are not clipped; this puts the text back where it was.
        var side = _immersive ? 16 : 12;
        _content.Padding = new Thickness(side, top, side, bottom);
        if (Following && _follower.Synced)
        {
            _content.UpdateLayout();
            Follow(animate: false);
        }
    }

    private void Point(Line line, bool pointed)
    {
        if (!line.Removed && line.Pointed != pointed)
        {
            line.Pointed = pointed;
            Apply(line, _animated(), PointerChange);
            // The pill marks what a click would do, so it shows only on a line that can seek.
            if (line.Hover is { } hover)
            {
                var visual = ElementCompositionPreview.GetElementVisual(hover);
                _compositor ??= visual.Compositor;
                var show = pointed && line.Model.Synced && _canSeek();
                Animate(visual, "Opacity", show ? HoverOpacity : 0, _animated(), HoverChange, TimeSpan.Zero);
            }
        }
    }

    private void Focus(Line line, bool focused)
    {
        if (!line.Removed && line.Focused != focused)
        {
            line.Focused = focused;
            Apply(line, _animated(), PointerChange);
        }
    }

    private void UpdateResume() =>
        _resume.Visibility = _follower.Synced && !Following ? Visibility.Visible : Visibility.Collapsed;

    private void Apply(Line line, bool animate, Motion? motion = null)
    {
        var index = _follower.Index;
        var resting = _follower.Resting;
        var lit = LyricsAppearance.Lit(line.Model.Index, index, resting);
        var plain = _plain();
        var look = LyricsAppearance.Line(line.Model.Index, index, resting, line.Model.Synced, Following,
            line.Pointed || line.Focused, plain);

        if (line.Ink is not null && _accent is SolidColorBrush accent)
        {
            Tint(line, lit ? accent.Color : line.Plain, animate);
        }

        Sweep(line, lit && line.Model.Synced && Following && animate && !plain
            && line.Model.Timed.SpelledWords.Count > 0 && !line.Focused);

        // The line being sung glows softly, as on macOS; a contrast theme or reduced effects leave it out.
        var glow = lit && !plain && line.Model.EmptyInterlude is null;
        if (glow != line.Glowing && (glow || line.Glow is not null))
        {
            line.Glowing = glow;
            if (EnsureGlow(line) is { } shadow)
            {
                Animate(shadow, "Opacity", glow ? GlowOpacity : 0, animate, Change, TimeSpan.Zero);
            }
        }

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
        var delay = motion is null ? LyricsMotion.RowDelay * Math.Clamp(offset, 0, 6) : TimeSpan.Zero;
        var time = motion ?? Change;
        var softened = look.Blur > 0 && EnsureBlur(line);
        var text = ElementCompositionPreview.GetElementVisual(line.Text);
        Animate(text, "Opacity", softened ? 0 : (float)look.Opacity, animate, time, delay);
        if (line.Sprite is not null)
        {
            // The blurred copy lies over the text. macOS animates the blur radius itself, so the copy
            // comes in from no blur at all and goes out by losing its blur while it fades: the line
            // sharpens or softens continuously rather than dissolving between two fixed pictures.
            if (softened && !line.Soft && animate)
            {
                Animate(line.Brush!.Properties, "Blur.BlurAmount", 0, false, time, TimeSpan.Zero);
            }

            Animate(line.Sprite, "Opacity", softened ? (float)look.Opacity : 0, animate, time, delay);
            Animate(line.Brush!.Properties, "Blur.BlurAmount", softened ? (float)look.Blur : 0, animate, time, delay);
        }

        line.Soft = softened;
    }

    /// <summary>
    /// Changes the panel line's colour with the rest of its emphasis, as SwiftUI animates a
    /// foreground style inside the line's animation. Swapping the brush made the sung line flash pink.
    /// </summary>
    private static void Tint(Line line, Color target, bool animate)
    {
        if (line.Ink is not { } ink || line.InkTarget == target)
        {
            return;
        }

        line.InkTarget = target;
        if (!animate)
        {
            line.InkStory?.Stop();
            line.InkStory = null;
            ink.Color = target;
            return;
        }

        // A new storyboard takes over from wherever the last one had got to.
        var change = new ColorAnimation
        {
            To = target,
            Duration = InkChange,
            EnableDependentAnimation = true,
            EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut },
        };
        Storyboard.SetTarget(change, ink);
        Storyboard.SetTargetProperty(change, nameof(SolidColorBrush.Color));
        var story = new Storyboard();
        story.Children.Add(change);
        story.Begin();
        line.InkStory = story;
    }

    /// <summary>The panel's own brush for a line, so its colour can move; re-read when the theme changes.</summary>
    private void Ink(Line line)
    {
        if (line.Removed || _accent is not SolidColorBrush accent)
        {
            return;
        }

        line.Text.ClearValue(TextBlock.ForegroundProperty);
        line.Plain = line.Text.Foreground is SolidColorBrush plain ? plain.Color : Colors.White;
        line.InkStory?.Stop();
        line.InkStory = null;
        var lit = LyricsAppearance.Lit(line.Model.Index, _follower.Index, _follower.Resting);
        line.InkTarget = lit ? accent.Color : line.Plain;
        line.Ink = new SolidColorBrush(line.InkTarget);
        line.Text.Foreground = line.Ink;
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

        var color = line.Ink is not null ? line.InkTarget
            : line.Text.Foreground is SolidColorBrush solid ? solid.Color : Colors.White;
        // The panel's line is still turning from its plain colour to the accent; the sweep finishes that.
        line.From = line.Ink?.Color ?? color;
        var glyphs = new List<Glyph>();
        var sweeps = LyricsTiming.Sweeps(line.Model.Timed);
        var words = line.Model.Timed.SpelledWords;
        var estimate = line.Text.FontSize * EstimatedCharacterEm;
        line.Text.Inlines.Clear();
        for (var word = 0; word < words.Count; word++)
        {
            var elements = StringInfo.GetTextElementEnumerator(words[word].Text);
            var character = 0;
            while (elements.MoveNext())
            {
                // Drawn at full strength to begin with: the unsung dimming fades in with the line's
                // own emphasis, so a line becoming current never flashes dark first.
                var brush = new SolidColorBrush(line.From);
                var run = new Run { Text = (string)elements.Current, Foreground = brush };
                line.Text.Inlines.Add(run);
                glyphs.Add(new Glyph(brush, run, sweeps[word], word, character * estimate, estimate, 1));
                character++;
            }
        }

        line.Glyphs = glyphs;
        line.Measured = false;
        line.SweptAt = DateTimeOffset.Now;
        line.Color = color;
        _swept = line;
    }

    /// <summary>
    /// Where each character of the swept line sits within its word, from the laid-out text, so the
    /// sweep's soft edge travels at the speed the words are actually drawn. Until layout has run, or
    /// for a word that wraps, each character is taken to be the same width.
    /// </summary>
    private static void Measure(Line line)
    {
        if (line.Measured || line.Glyphs is not { Count: > 0 } glyphs)
        {
            return;
        }

        var measured = new (double Left, double Width)[glyphs.Count];
        for (var i = 0; i < glyphs.Count; i++)
        {
            var start = glyphs[i].Run.ContentStart.GetCharacterRect(LogicalDirection.Forward);
            var end = glyphs[i].Run.ContentEnd.GetCharacterRect(LogicalDirection.Backward);
            if (start.Height <= 0 || Math.Abs(start.Y - end.Y) > 0.5 || end.X < start.X)
            {
                return;
            }

            measured[i] = (start.X, end.X - start.X);
        }

        for (var i = 0; i < glyphs.Count;)
        {
            var word = glyphs[i].Word;
            var origin = measured[i].Left;
            var consumed = 0.0;
            var wraps = false;
            var j = i;
            for (; j < glyphs.Count && glyphs[j].Word == word; j++)
            {
                wraps |= j > i && measured[j].Left + 0.5 < measured[j - 1].Left;
            }

            for (var k = i; k < j; k++)
            {
                if (!wraps)
                {
                    glyphs[k] = glyphs[k] with { Left = measured[k].Left - origin, Width = measured[k].Width };
                }
                else
                {
                    glyphs[k] = glyphs[k] with { Left = consumed, Width = measured[k].Width };
                }

                consumed += measured[k].Width;
            }

            i = j;
        }

        line.Measured = true;
    }

    private static Color Mix(Color from, Color to, double amount)
    {
        byte Channel(byte a, byte b) => (byte)Math.Round(a + (b - a) * Math.Clamp(amount, 0, 1));
        return Color.FromArgb(Channel(from.A, to.A), Channel(from.R, to.R), Channel(from.G, to.G), Channel(from.B, to.B));
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

    /// <summary>
    /// Every display frame, not a timed sixtieth: on a 144 Hz screen a 60-a-second cap drew on an
    /// uneven two-or-three-frame cadence, which showed as the sweep stuttering.
    /// </summary>
    private void OnRendering(object? sender, object e) => Draw(DateTimeOffset.Now);

    private void Draw(DateTimeOffset now)
    {
        var position = _follower.PositionMs(now);
        if (_swept is { Glyphs: { } glyphs } swept)
        {
            Measure(swept);
            var color = swept.Color;
            var feather = swept.Text.FontSize * FeatherEm;
            // How far the line is into its swept treatment: it eases in with the rest of the
            // line's emphasis. A stopped clock draws it whole, since nothing would finish the fade.
            var level = _follower.Running ? LyricsMotion.Spring((now - swept.SweptAt).TotalSeconds, Change.Time.TotalSeconds) : 1;
            var unsung = 1 - (1 - LyricsTiming.UnsungOpacity) * level;
            color = Mix(swept.From, color, level);
            var widths = new Dictionary<int, double>();
            foreach (var glyph in glyphs)
            {
                widths[glyph.Word] = Math.Max(widths.GetValueOrDefault(glyph.Word), glyph.Left + glyph.Width);
            }

            for (var i = 0; i < glyphs.Count; i++)
            {
                var glyph = glyphs[i];
                // The sweep's edge starts a feather before the word and ends a feather after it, so
                // the word is fully lit exactly at its end; behind the edge it fades over a feather.
                var edge = glyph.Sweep.Progress(position) * (widths[glyph.Word] + feather);
                var lit = Math.Clamp((edge - (glyph.Left + glyph.Width / 2)) / feather, 0, 1);
                var alpha = unsung + lit * (1 - unsung);
                if (Math.Abs(alpha - glyph.Alpha) < 0.004)
                {
                    continue;
                }

                glyph.Brush.Color = WithAlpha(color, alpha);
                glyphs[i] = glyph with { Alpha = alpha };
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
        var right = 0.0;
        foreach (var child in dots.Children)
        {
            if (child is Ellipse ellipse)
            {
                ellipse.Opacity = gap.DotOpacity(dot++, position);
                right = Math.Max(right, ellipse.ActualOffset.X + ellipse.ActualWidth);
            }
        }

        // The dots swell about their own middle, as macOS's scaleEffect on the HStack does. About
        // the middle of a row as wide as the column, they slid sideways as they breathed and were
        // pushed off the column's left edge as they swelled.
        var visual = ElementCompositionPreview.GetElementVisual(dots);
        var scale = (float)gap.Scale(position, animated);
        var width = right > 0 ? right : dots.ActualWidth;
        visual.CenterPoint = new Vector3((float)(width / 2), (float)(dots.ActualHeight / 2), 0);
        visual.Scale = new Vector3(scale, scale, 1);
    }

    private void FadeDots(UIElement dots, bool active, bool animate)
    {
        var visual = ElementCompositionPreview.GetElementVisual(dots);
        _compositor ??= visual.Compositor;
        Animate(visual, "Opacity", active ? 1 : 0, animate, DotsChange, TimeSpan.Zero);
    }

    private void Animate(CompositionObject target, string property, float value, bool animate, Motion motion,
        TimeSpan delay)
    {
        if (!animate || _compositor is null)
        {
            target.StopAnimation(property);
            if (target is Visual visual)
            {
                visual.Opacity = value;
            }
            else if (target is DropShadow shadow)
            {
                shadow.Opacity = value;
            }
            else
            {
                ((CompositionPropertySet)target).InsertScalar(property, value);
            }

            return;
        }

        if (motion.IsSpring)
        {
            var spring = SwiftUISpring.Scalar(_compositor, motion.Time.TotalSeconds, 1, value);
            spring.DelayTime = delay;
            target.StartAnimation(property, spring);
            return;
        }

        // SwiftUI's and CSS's ease-out.
        _ease ??= _compositor.CreateCubicBezierEasingFunction(new Vector2(0, 0), new Vector2(0.58f, 1));
        var animation = _compositor.CreateScalarKeyFrameAnimation();
        animation.InsertKeyFrame(1, value, _ease);
        animation.Duration = motion.Time;
        animation.DelayTime = delay;
        target.StartAnimation(property, animation);
    }

    /// <summary>How a value moves: a critically damped spring with this response, or an ease-out this long.</summary>
    private readonly record struct Motion(TimeSpan Time, bool IsSpring)
    {
        internal static Motion Spring(double response) => new(TimeSpan.FromSeconds(response), true);

        internal static Motion EaseOut(double seconds) => new(TimeSpan.FromSeconds(seconds), false);
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

    /// <summary>
    /// The soft white glow behind the line being sung, the shape of its text, built the first time
    /// the line is sung. It hangs from an empty element laid out under the text, because anything
    /// hung from the line's host would be drawn over the text and wash it out.
    /// </summary>
    private DropShadow? EnsureGlow(Line line)
    {
        if (line.Glow is not null)
        {
            return (DropShadow)line.Glow.Shadow;
        }

        if (line.GlowHost is null)
        {
            return null;
        }

        try
        {
            var text = ElementCompositionPreview.GetElementVisual(line.Text);
            _compositor ??= text.Compositor;
            var shadow = _compositor.CreateDropShadow();
            shadow.Color = Colors.White;
            shadow.BlurRadius = _immersive ? 8 : 5;
            shadow.Offset = Vector3.Zero;
            shadow.Opacity = 0;
            shadow.Mask = line.Text.GetAlphaMask();
            var sprite = _compositor.CreateSpriteVisual();
            sprite.Shadow = shadow;
            var size = _compositor.CreateExpressionAnimation("text.Size");
            size.SetReferenceParameter("text", text);
            sprite.StartAnimation(nameof(sprite.Size), size);
            // The glow's element shares the text's top-left corner, so the sprite needs no offset.
            ElementCompositionPreview.SetElementChildVisual(line.GlowHost, sprite);
            line.Glow = sprite;
            return shadow;
        }
        catch (Exception error)
        {
            BridgeLog.Write($"lyric glow unavailable: {error.GetType().Name}");
            line.Glowing = false;
            return null;
        }
    }

    /// <summary>One character of the swept line, the brush that lights it, and where it sits in its word.</summary>
    private readonly record struct Glyph(SolidColorBrush Brush, Run Run, LyricsTiming.Sweep Sweep, int Word,
        double Left, double Width, double Alpha);

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
        internal bool Focused { get; set; }
        internal bool Removed { get; set; }
        internal Action? Detach { get; set; }
        internal bool Soft { get; set; }
        internal SolidColorBrush? Ink { get; set; }
        internal Color Plain { get; set; }
        internal Color From { get; set; }
        internal Color InkTarget { get; set; }
        internal Storyboard? InkStory { get; set; }
        internal bool Glowing { get; set; }
        internal bool Measured { get; set; }
        internal DateTimeOffset SweptAt { get; set; }
        internal Border? Hover { get; set; }
        internal Border? GlowHost { get; set; }
        internal SpriteVisual? Glow { get; set; }
        internal LyricLineLook? Look { get; set; }
        internal List<Glyph>? Glyphs { get; set; }
        internal Color Color { get; set; }
        internal CompositionVisualSurface? Surface { get; set; }
        internal CompositionEffectBrush? Brush { get; set; }
        internal SpriteVisual? Sprite { get; set; }
    }
}
