using System;
using System.Linq;
using System.Numerics;
using Goosic.Windows.Service;
using Goosic.Windows.ViewModels;
using Microsoft.UI.Composition;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Hosting;

namespace Goosic.Windows;

/// <summary>
/// How panels and pages move as they appear and go.
/// </summary>
/// <remarks>
/// Every animation here is a composition animation on the element's visual: it runs on the
/// compositor thread, touches only opacity and translation, and never causes a layout pass, so it
/// costs nothing on the UI thread and keeps running smoothly while a page is loading. Showing and
/// hiding stay plain <c>Visibility</c> changes in the rest of the window; the implicit show and
/// hide animations attached here are what turn those into motion. When Windows' "Animation effects"
/// setting is off, or Goosic's own Reduce motion or Efficiency mode is on, none are attached and
/// everything appears at once. Those can change while the window is open, so the animations are
/// attached and removed again rather than decided once.
/// </remarks>
public sealed partial class MainWindow
{
    private static readonly TimeSpan ShowDuration = TimeSpan.FromMilliseconds(240);
    private static readonly TimeSpan HideDuration = TimeSpan.FromMilliseconds(150);
    private Compositor? _compositor;
    private CompositionEasingFunction? _easeOut;
    private CompositionEasingFunction? _easeIn;

    /// <summary>
    /// Decorative motion runs only when Windows allows it and neither Reduce motion nor
    /// Efficiency mode asks Goosic to skip it.
    /// </summary>
    private bool AnimationsEnabled =>
        _uiSettings.AnimationsEnabled && !Model.ReduceMotion && !ShellPreferences.EfficiencyMode;

    private bool _motionAttached;

    private void WireMotion()
    {
        _compositor = ElementCompositionPreview.GetElementVisual(RootGrid).Compositor;
        // Decelerating in and accelerating out, like the system's own flyouts.
        _easeOut = _compositor.CreateCubicBezierEasingFunction(new Vector2(0.1f, 0.9f), new Vector2(0.2f, 1f));
        _easeIn = _compositor.CreateCubicBezierEasingFunction(new Vector2(0.7f, 0f), new Vector2(1f, 0.5f));
        WirePlayerPressFeedback();
        ApplyMotion();
        if (OperatingSystem.IsWindowsVersionAtLeast(10, 0, 19041))
        {
            _uiSettings.AnimationsEnabledChanged += (_, _) => DispatcherQueue.TryEnqueue(ApplyMotion);
        }
        // Preferences arrive after the window, and the setting can change while it is open.
        Model.PropertyChanged += (_, e) =>
        {
            if (e.PropertyName == nameof(ShellViewModel.ReduceMotion))
            {
                ApplyMotion();
            }
            else if (e.PropertyName == nameof(ShellViewModel.NowPlayingArtwork) && Model.NowPlayingArtwork is not null)
            {
                FadePlayerArtwork(NowPlayingArtwork);
            }
        };
    }

    /// <summary>Attaches or removes the show and hide animations to match <see cref="AnimationsEnabled"/>.</summary>
    private void ApplyMotion()
    {
        var enabled = AnimationsEnabled;
        if (enabled == _motionAttached)
        {
            return;
        }

        _motionAttached = enabled;
        AttachShowHide(Sidebar, new Vector3(-20, 0, 0));
        AttachShowHide(SidePanel, new Vector3(28, 0, 0));
        AttachShowHide(QueuePanel, new Vector3(0, 10, 0));
        AttachShowHide(LyricsPanel, new Vector3(0, 10, 0));
        AttachShowHide(QueueUndoBar, new Vector3(0, 8, 0));
        AttachShowHide(FullPlayer, new Vector3(0, 36, 0));
        AttachShowHide(FullPlayerLyrics, new Vector3(0, 12, 0));
        AttachShowHide(BackButton, new Vector3(-8, 0, 0));
        AttachShowHide(ToastHost, new Vector3(0, 10, 0));
        AttachShowHide(ArtworkBackdrop, Vector3.Zero, ShowDuration * 2);
        foreach (var icon in new UIElement[] { PillPlayIcon, PillPauseIcon, FullPlayIcon, FullPauseIcon })
        {
            AttachSymbolReplace(icon);
        }

        // The position line and its times change through their own properties, which the
        // framework animates when given a transition and sets at once when given none.
        var reveal = TimeSpan.FromMilliseconds(150);
        PillLine.ScaleTransition = enabled ? new Vector3Transition { Duration = reveal } : null;
        PillElapsed.OpacityTransition = enabled ? new ScalarTransition { Duration = reveal } : null;
        PillRemaining.OpacityTransition = enabled ? new ScalarTransition { Duration = reveal } : null;
        if (!enabled)
        {
            foreach (var element in PlayerPressTargets().Concat([PillCoverButton, PillCoverHover, NowPlayingArtwork,
                FullPlayerCover]))
            {
                var visual = ElementCompositionPreview.GetElementVisual(element);
                visual.StopAnimation("Scale");
                visual.StopAnimation("Opacity");
                visual.Scale = Vector3.One;
                visual.Opacity = (float)element.Opacity;
            }
        }
    }

    private UIElement[] PlayerPressTargets() =>
    [
        ShuffleButton, PreviousButton, PlayButton, NextButton, RepeatButton,
        PillLikeButton, PillMoreButton, LyricsButton, QueueButton, VolumeButton,
    ];

    /// <summary>
    /// Gives every control in the pill the macOS player's press: it dips under the pointer and
    /// springs back on release.
    /// </summary>
    /// <remarks>
    /// A button marks its own pointer events handled, so these are attached to hear them anyway.
    /// The pill itself never scales: a bar that swells under a passing pointer reads as the whole
    /// window flinching.
    /// </remarks>
    private void WirePlayerPressFeedback()
    {
        var press = new Microsoft.UI.Xaml.Input.PointerEventHandler((sender, _) => SpringPlayerScale((UIElement)sender, 0.88f));
        var release = new Microsoft.UI.Xaml.Input.PointerEventHandler((sender, _) => SpringPlayerScale((UIElement)sender, 1));
        foreach (var control in PlayerPressTargets())
        {
            control.AddHandler(UIElement.PointerPressedEvent, press, true);
            control.AddHandler(UIElement.PointerReleasedEvent, release, true);
            control.AddHandler(UIElement.PointerCanceledEvent, release, true);
            control.AddHandler(UIElement.PointerCaptureLostEvent, release, true);
        }
    }

    /// <summary>
    /// Makes play and pause trade places the way a symbol does on macOS: the one leaving shrinks
    /// and fades as the one arriving grows into the same spot.
    /// </summary>
    private void AttachSymbolReplace(UIElement icon)
    {
        if (_compositor is null)
        {
            return;
        }

        if (!_motionAttached)
        {
            ElementCompositionPreview.SetImplicitShowAnimation(icon, null);
            ElementCompositionPreview.SetImplicitHideAnimation(icon, null);
            return;
        }

        // Scaled about its middle, wherever layout has put it by the time it is shown.
        var visual = ElementCompositionPreview.GetElementVisual(icon);
        var centre = _compositor.CreateExpressionAnimation("Vector3(this.Target.Size.X / 2, this.Target.Size.Y / 2, 0)");
        visual.StartAnimation("CenterPoint", centre);

        var duration = TimeSpan.FromMilliseconds(220);
        var small = new Vector3(0.4f, 0.4f, 1);
        var show = _compositor.CreateAnimationGroup();
        show.Add(Scalar("Opacity", 0, 1, duration, _easeOut!));
        show.Add(Vector("Scale", small, Vector3.One, duration, _easeOut!));
        var hide = _compositor.CreateAnimationGroup();
        hide.Add(Scalar("Opacity", 1, 0, duration, _easeOut!));
        hide.Add(Vector("Scale", Vector3.One, small, duration, _easeOut!));
        ElementCompositionPreview.SetImplicitShowAnimation(icon, show);
        ElementCompositionPreview.SetImplicitHideAnimation(icon, hide);
    }

    /// <summary>
    /// Pushes Previous or Next a little way in the direction it points and lets it settle back,
    /// so the press reads as sending the song that way.
    /// </summary>
    private void NudgePlayerControl(UIElement? control, int direction)
    {
        if (control is null || _compositor is null || !AnimationsEnabled) return;
        ElementCompositionPreview.SetIsTranslationEnabled(control, true);
        var nudge = _compositor.CreateVector3KeyFrameAnimation();
        nudge.InsertKeyFrame(0, Vector3.Zero);
        nudge.InsertKeyFrame(0.3f, new Vector3(5 * direction, 0, 0), _easeOut);
        nudge.InsertKeyFrame(1, Vector3.Zero, _easeOut);
        nudge.Duration = TimeSpan.FromMilliseconds(380);
        ElementCompositionPreview.GetElementVisual(control).StartAnimation("Translation", nudge);
    }

    private Microsoft.UI.Dispatching.DispatcherQueueTimer? _playerGlide;

    /// <summary>
    /// Lets the pill travel to its next place instead of appearing there, for the one layout
    /// pass that follows a panel opening or closing.
    /// </summary>
    /// <remarks>
    /// Only for that pass. Left on, the same animation would run while the window is being
    /// resized, and the pill would trail behind the edge it is supposed to be attached to.
    /// </remarks>
    private void GlidePlayerOnce()
    {
        if (_compositor is null || !AnimationsEnabled) return;
        var visual = ElementCompositionPreview.GetElementVisual(PlayerPill);
        var glide = _compositor.CreateVector3KeyFrameAnimation();
        glide.Target = "Offset";
        glide.InsertExpressionKeyFrame(1, "this.FinalValue", _easeOut);
        glide.Duration = TimeSpan.FromMilliseconds(320);
        var moves = _compositor.CreateImplicitAnimationCollection();
        moves["Offset"] = glide;
        visual.ImplicitAnimations = moves;

        if (_playerGlide is null)
        {
            _playerGlide = DispatcherQueue.CreateTimer();
            _playerGlide.IsRepeating = false;
            _playerGlide.Interval = TimeSpan.FromMilliseconds(400);
            _playerGlide.Tick += (_, _) => ElementCompositionPreview.GetElementVisual(PlayerPill).ImplicitAnimations = null;
        }

        _playerGlide.Stop();
        _playerGlide.Start();
    }

    private void SpringPlayerScale(UIElement element, float target)
    {
        if (_compositor is null || !AnimationsEnabled) return;
        var visual = ElementCompositionPreview.GetElementVisual(element);
        visual.CenterPoint = new Vector3(visual.Size.X / 2, visual.Size.Y / 2, 0);
        var spring = _compositor.CreateSpringVector3Animation();
        spring.FinalValue = new Vector3(target, target, 1);
        spring.DampingRatio = 0.7f;
        spring.Period = TimeSpan.FromMilliseconds(220);
        visual.StartAnimation("Scale", spring);
    }

    private void FadePlayerElement(UIElement element, float opacity)
    {
        var visual = ElementCompositionPreview.GetElementVisual(element);
        var previous = visual.Opacity;
        element.Opacity = opacity;
        if (_compositor is not null && AnimationsEnabled)
        {
            visual.StartAnimation("Opacity", Scalar("Opacity", previous, opacity,
                TimeSpan.FromMilliseconds(150), _easeOut!));
        }
    }

    private void FadePlayerArtwork(UIElement element)
    {
        if (_compositor is null || !AnimationsEnabled) return;
        ElementCompositionPreview.GetElementVisual(element).StartAnimation("Opacity",
            Scalar("Opacity", 0, 1, TimeSpan.FromMilliseconds(600), _easeOut!));
    }

    /// <summary>Makes an element fade and slide in from <paramref name="offset"/> when shown, and back out when hidden.</summary>
    private void AttachShowHide(UIElement element, Vector3 offset, TimeSpan? showDuration = null)
    {
        if (_compositor is null)
        {
            return;
        }

        if (!_motionAttached)
        {
            ElementCompositionPreview.SetImplicitShowAnimation(element, null);
            ElementCompositionPreview.SetImplicitHideAnimation(element, null);
            return;
        }

        ElementCompositionPreview.SetIsTranslationEnabled(element, true);

        var show = _compositor.CreateAnimationGroup();
        show.Add(Scalar("Opacity", 0, 1, showDuration ?? ShowDuration, _easeOut!));
        if (offset != Vector3.Zero)
        {
            show.Add(Vector("Translation", offset, Vector3.Zero, showDuration ?? ShowDuration, _easeOut!));
        }

        var hide = _compositor.CreateAnimationGroup();
        hide.Add(Scalar("Opacity", 1, 0, HideDuration, _easeIn!));
        if (offset != Vector3.Zero)
        {
            hide.Add(Vector("Translation", Vector3.Zero, offset, HideDuration, _easeIn!));
        }

        ElementCompositionPreview.SetImplicitShowAnimation(element, show);
        ElementCompositionPreview.SetImplicitHideAnimation(element, hide);
    }

    /// <summary>Brings a freshly loaded page up from slightly below, so a new page reads as arriving.</summary>
    private void PlayPageEntrance()
    {
        if (_compositor is null || !AnimationsEnabled)
        {
            return;
        }

        ElementCompositionPreview.SetIsTranslationEnabled(ContentStack, true);
        var visual = ElementCompositionPreview.GetElementVisual(ContentStack);
        visual.StartAnimation("Opacity", Scalar("Opacity", 0, 1, ShowDuration, _easeOut!));
        visual.StartAnimation("Translation", Vector("Translation", new Vector3(0, 14, 0), Vector3.Zero, ShowDuration, _easeOut!));
    }

    private ScalarKeyFrameAnimation Scalar(string target, float from, float to, TimeSpan duration, CompositionEasingFunction easing)
    {
        var animation = _compositor!.CreateScalarKeyFrameAnimation();
        animation.Target = target;
        animation.InsertKeyFrame(0, from);
        animation.InsertKeyFrame(1, to, easing);
        animation.Duration = duration;
        return animation;
    }

    private Vector3KeyFrameAnimation Vector(string target, Vector3 from, Vector3 to, TimeSpan duration, CompositionEasingFunction easing)
    {
        var animation = _compositor!.CreateVector3KeyFrameAnimation();
        animation.Target = target;
        animation.InsertKeyFrame(0, from);
        animation.InsertKeyFrame(1, to, easing);
        animation.Duration = duration;
        return animation;
    }
}
