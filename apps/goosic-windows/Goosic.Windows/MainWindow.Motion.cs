using System;
using System.Numerics;
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
/// setting is off, none are attached and everything appears at once, as it did before.
/// </remarks>
public sealed partial class MainWindow
{
    private static readonly TimeSpan ShowDuration = TimeSpan.FromMilliseconds(240);
    private static readonly TimeSpan HideDuration = TimeSpan.FromMilliseconds(150);
    private Compositor? _compositor;
    private CompositionEasingFunction? _easeOut;
    private CompositionEasingFunction? _easeIn;

    private bool AnimationsEnabled => _uiSettings.AnimationsEnabled;

    private void WireMotion()
    {
        if (!AnimationsEnabled)
        {
            return;
        }

        _compositor = ElementCompositionPreview.GetElementVisual(RootGrid).Compositor;
        // Decelerating in and accelerating out, like the system's own flyouts.
        _easeOut = _compositor.CreateCubicBezierEasingFunction(new Vector2(0.1f, 0.9f), new Vector2(0.2f, 1f));
        _easeIn = _compositor.CreateCubicBezierEasingFunction(new Vector2(0.7f, 0f), new Vector2(1f, 0.5f));

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
    }

    /// <summary>Makes an element fade and slide in from <paramref name="offset"/> when shown, and back out when hidden.</summary>
    private void AttachShowHide(UIElement element, Vector3 offset, TimeSpan? showDuration = null)
    {
        if (_compositor is null)
        {
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
