using System;
using System.Collections.Generic;
using System.Numerics;
using System.Threading.Tasks;
using Goosic.Windows.Presentation;
using Goosic.Windows.Service;
using Goosic.Windows.ViewModels;
using Microsoft.Graphics.Canvas;
using Microsoft.Graphics.Canvas.Effects;
using Microsoft.UI;
using Microsoft.UI.Composition;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Hosting;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.Foundation;
using Windows.Graphics.Imaging;
using Windows.Storage;
using Color = Windows.UI.Color;

namespace Goosic.Windows;

/// <summary>
/// The artist page's hero, in the manner of an Apple Music artist page: the artist's banner across
/// the content area with the name and three buttons over its foot, and the whole page in the
/// colour sampled from the banner.
/// </summary>
/// <remarks>
/// <para>
/// It is two layers. <c>ArtistBackdrop</c> sits behind the page and does not scroll: the page
/// colour, the banner and its scrim. <c>ArtistHeroPanel</c> is the first thing in the page, a
/// transparent block of the same height holding the name and buttons, so they scroll up over the
/// picture. Composition draws the banner, cropped from its top, through a blur, and an expression
/// ties the blur and the banner's opacity to the page's scroll offset, so scrolling blurs the
/// picture and then dissolves it into the colour without any code running per frame.
/// </para>
/// <para>
/// The height, the colour and the scroll curves are <see cref="ArtistHero"/>'s, the same rules
/// macOS and Linux follow; <c>docs/ARTIST_PAGE.md</c> describes the whole of it.
/// </para>
/// </remarks>
public sealed partial class MainWindow
{
    private ArtistHeader? _heroHeader;
    private int _heroVersion;
    private Color _heroTint = DefaultHeroTint;
    private CompositionPropertySet? _heroScroll;
    private CompositionEffectBrush? _bannerBrush;
    private CompositionSurfaceBrush? _bannerSurface;
    private SpriteVisual? _bannerSprite;
    private LoadedImageSurface? _bannerImage;
    private bool _bannerBlurUnavailable;
    private Storyboard? _heroTintStory;
    private readonly List<(GradientStop Stop, double Alpha)> _scrimTintStops = [];

    private static readonly Color DefaultHeroTint =
        Color.FromArgb(255, ArtistHero.DefaultTint[0], ArtistHero.DefaultTint[1], ArtistHero.DefaultTint[2]);

    /// <summary>
    /// How far the hero has scrolled, 0 to 1, as a Composition expression over the page's scroll
    /// offset; <see cref="ArtistHero.Progress"/> is the same rule.
    /// </summary>
    private const string HeroProgress = "Clamp(-scroll.Translation.Y / Max(hero.Height, 1), 0, 1)";

    private double HeroWidth => Math.Max(0, RootGrid.ActualWidth - LeftInset);

    private void WireArtistHero()
    {
        Model.PropertyChanged += (_, args) =>
        {
            if (args.PropertyName == nameof(ShellViewModel.PageArtistHeader))
            {
                ShowArtistHero(Model.PageArtistHeader);
            }
            else if (args.PropertyName == nameof(ShellViewModel.HasTracks))
            {
                ArtistTopSongs.Visibility = _heroHeader is not null && Model.HasTracks
                    ? Visibility.Visible
                    : Visibility.Collapsed;
            }
        };
        _uiSettings.AdvancedEffectsEnabledChanged += (_, _) => DispatcherQueue.TryEnqueue(ApplyHeroEffects);
        _accessibility.HighContrastChanged += (_, _) => DispatcherQueue.TryEnqueue(ApplyHeroEffects);
    }

    /// <summary>Puts the hero up for an artist's header, or takes it down for any other page.</summary>
    private void ShowArtistHero(ArtistHeader? header)
    {
        if (header == _heroHeader)
        {
            return;
        }

        _heroHeader = header;
        var version = ++_heroVersion;
        var shown = header is not null;
        ArtistBackdrop.Visibility = shown ? Visibility.Visible : Visibility.Collapsed;
        ArtistTopSongs.Visibility = shown && Model.HasTracks ? Visibility.Visible : Visibility.Collapsed;
        // The page's text sits on the artist's colour, which is always dark, so it is drawn dark
        // whatever the app's theme; and the hero runs to the window's top rather than fading out
        // under the title bar.
        ContentMaskHost.RequestedTheme = shown ? ElementTheme.Dark : ElementTheme.Default;
        ContentMaskTop.Color = shown ? Colors.Black : Colors.Transparent;
        TitleFade.Visibility = shown || ContentMask.Visibility == Visibility.Visible
            ? Visibility.Collapsed
            : Visibility.Visible;

        if (_bannerSprite is not null)
        {
            _bannerSprite.IsVisible = false;
        }

        ArtistBannerFallback.Source = null;
        ArtistBannerFallback.Visibility = Visibility.Collapsed;
        ArtistPortraitImage.Source = null;
        ArtistPortrait.Visibility = Visibility.Collapsed;
        SetHeroTint(DefaultHeroTint, animate: false);
        if (header is null)
        {
            return;
        }

        ArtistAudience.Text = header.Audience ?? "";
        ArtistAudience.Visibility = header.Audience is null ? Visibility.Collapsed : Visibility.Visible;
        var facts = new List<string>();
        if (header.Audience is { } audience)
        {
            facts.Add(audience);
        }

        if (header.Subscribers is { } subscribers)
        {
            facts.Add(subscribers + " subscribers");
        }

        ArtistFacts.Text = string.Join(" · ", facts);
        ArtistDescription.Text = header.Description ?? "";
        ArtistAboutButton.IsEnabled = header.Description is not null || header.Subscribers is not null;
        LayOutArtistHero();
        // The blur outlives the page, so it is brought up to the current effect settings each time.
        ApplyHeroEffects();

        if (header.Banner is { } banner)
        {
            _ = ShowBannerAsync(banner, version);
        }
        else if (header.Portrait is { } portrait)
        {
            ArtistPortrait.Visibility = Visibility.Visible;
            _ = ShowPortraitAsync(portrait, version);
        }
    }

    /// <summary>Sizes the hero to the content area; called whenever the insets are applied.</summary>
    private void LayOutArtistHero()
    {
        if (_heroHeader is null)
        {
            return;
        }

        var width = HeroWidth;
        var height = ArtistHero.Height(width);
        // Out of the page's padding to the sidebar's edge, the window's right edge and its top.
        ArtistHeroPanel.Margin = new Thickness(-ContentGutter, -ContentStack.Padding.Top, -(RightInset + ContentGutter), 0);
        ArtistHeroPanel.Height = height;
        ArtistPicture.Margin = new Thickness(LeftInset, 0, 0, 0);
        ArtistPicture.Width = width;
        ArtistPicture.Height = height;
        ArtistPageShade.Margin = new Thickness(0, height, 0, 0);
        // Clear of the name and buttons, which take about 230 pixels from the hero's foot.
        var side = Math.Clamp(height - 290, 80, 240);
        ArtistPortrait.Width = side;
        ArtistPortrait.Height = side;
        ArtistPortrait.CornerRadius = new CornerRadius(side / 2);
        _heroScroll?.InsertScalar("Height", (float)height);
        RebuildScrim();
    }

    private async Task ShowBannerAsync(string remote, int version)
    {
        var path = await Model.LocalArtworkAsync(remote);
        if (path is null || version != _heroVersion)
        {
            return;
        }

        _ = SampleHeroTintAsync(path, version);
        if (!EnsureBannerBlur())
        {
            ArtistBannerFallback.Source = new BitmapImage(new Uri(path));
            ArtistBannerFallback.Visibility = Visibility.Visible;
            FadeWithScroll(ArtistBannerFallback);
            return;
        }

        try
        {
            var file = await StorageFile.GetFileFromPathAsync(path);
            var stream = await file.OpenReadAsync();
            if (version != _heroVersion)
            {
                stream.Dispose();
                return;
            }

            var image = LoadedImageSurface.StartLoadFromStream(stream);
            image.LoadCompleted += (_, args) =>
            {
                stream.Dispose();
                if (args.Status != LoadedImageSourceLoadStatus.Success)
                {
                    BridgeLog.Write($"artist banner not decoded: {args.Status}");
                }
            };
            _bannerSurface!.Surface = image;
            _bannerImage?.Dispose();
            _bannerImage = image;
            _bannerSprite!.IsVisible = true;
        }
        catch (Exception error)
        {
            BridgeLog.Write($"artist banner unavailable: {error.GetType().Name}");
        }
    }

    private async Task ShowPortraitAsync(string remote, int version)
    {
        var path = await Model.LocalArtworkAsync(remote);
        if (path is null || version != _heroVersion)
        {
            return;
        }

        ArtistPortraitImage.Source = new BitmapImage(new Uri(path)) { DecodePixelWidth = 480 };
        FadeWithScroll(ArtistPortrait);
        _ = SampleHeroTintAsync(path, version);
    }

    /// <summary>Builds the visual that draws the banner through a blur, the first time one is needed.</summary>
    private bool EnsureBannerBlur()
    {
        if (_bannerSprite is not null)
        {
            return true;
        }

        if (_bannerBlurUnavailable)
        {
            return false;
        }

        try
        {
            var compositor = ElementCompositionPreview.GetElementVisual(ArtistBannerHost).Compositor;
            _bannerSurface = compositor.CreateSurfaceBrush();
            // Faces sit in a banner's upper part, so a taller hero crops from the foot.
            _bannerSurface.Stretch = CompositionStretch.UniformToFill;
            _bannerSurface.HorizontalAlignmentRatio = 0.5f;
            _bannerSurface.VerticalAlignmentRatio = 0;
            var factory = compositor.CreateEffectFactory(new GaussianBlurEffect
            {
                Name = "Blur",
                BlurAmount = 0,
                BorderMode = EffectBorderMode.Hard,
                // Clamped, so the blur does not pull transparency in from beyond the picture's edges.
                Source = new BorderEffect
                {
                    ExtendX = CanvasEdgeBehavior.Clamp,
                    ExtendY = CanvasEdgeBehavior.Clamp,
                    Source = new CompositionEffectSourceParameter("Banner"),
                },
            }, ["Blur.BlurAmount"]);
            _bannerBrush = factory.CreateBrush();
            _bannerBrush.SetSourceParameter("Banner", _bannerSurface);
            _bannerSprite = compositor.CreateSpriteVisual();
            _bannerSprite.Brush = _bannerBrush;
            _bannerSprite.RelativeSizeAdjustment = Vector2.One;
            _bannerSprite.IsVisible = false;
            ElementCompositionPreview.SetElementChildVisual(ArtistBannerHost, _bannerSprite);

            var blur = compositor.CreateExpressionAnimation($"Min({HeroProgress} / 0.5, 1) * hero.MaxBlur");
            BindToScroll(blur);
            _bannerBrush.Properties.StartAnimation("Blur.BlurAmount", blur);
            var fade = compositor.CreateExpressionAnimation($"1 - Clamp(({HeroProgress} - 0.3) / 0.6, 0, 1)");
            BindToScroll(fade);
            _bannerSprite.StartAnimation("Opacity", fade);
            return true;
        }
        catch (Exception error)
        {
            // The banner still shows and fades where it can; it is only not blurred.
            _bannerBlurUnavailable = true;
            BridgeLog.Write($"artist banner blur unavailable: {error.GetType().Name}");
            return false;
        }
    }

    /// <summary>Dissolves a picture into the page colour as the hero scrolls away.</summary>
    private void FadeWithScroll(UIElement picture)
    {
        try
        {
            var visual = ElementCompositionPreview.GetElementVisual(picture);
            var fade = visual.Compositor.CreateExpressionAnimation($"1 - Clamp(({HeroProgress} - 0.3) / 0.6, 0, 1)");
            BindToScroll(fade);
            visual.StartAnimation("Opacity", fade);
        }
        catch (Exception error)
        {
            BridgeLog.Write($"artist picture fade unavailable: {error.GetType().Name}");
        }
    }

    private void BindToScroll(ExpressionAnimation expression)
    {
        var compositor = expression.Compositor;
        if (_heroScroll is null)
        {
            _heroScroll = compositor.CreatePropertySet();
            _heroScroll.InsertScalar("Height", (float)ArtistHero.Height(HeroWidth));
            _heroScroll.InsertScalar("MaxBlur", (float)ArtistHero.Blur(1, transparencyEffects: !PlainEffects));
        }

        expression.SetReferenceParameter("scroll", ElementCompositionPreview.GetScrollViewerManipulationPropertySet(ContentScroller));
        expression.SetReferenceParameter("hero", _heroScroll);
    }

    /// <summary>Follows Windows' transparency and contrast settings and Goosic's Efficiency mode.</summary>
    private void ApplyHeroEffects()
    {
        _heroScroll?.InsertScalar("MaxBlur", (float)ArtistHero.Blur(1, transparencyEffects: !PlainEffects));
        RebuildScrim();
    }

    /// <summary>Draws the picture into a small bitmap and takes the page colour from its foot.</summary>
    private async Task SampleHeroTintAsync(string path, int version)
    {
        try
        {
            var file = await StorageFile.GetFileFromPathAsync(path);
            using var stream = await file.OpenReadAsync();
            var decoder = await BitmapDecoder.CreateAsync(stream);
            const int width = 48;
            var height = Math.Max(1, (int)Math.Round(width * (double)decoder.PixelHeight / Math.Max(decoder.PixelWidth, 1)));
            var pixels = await decoder.GetPixelDataAsync(
                BitmapPixelFormat.Rgba8,
                BitmapAlphaMode.Straight,
                new BitmapTransform
                {
                    ScaledWidth = width,
                    ScaledHeight = (uint)height,
                    InterpolationMode = BitmapInterpolationMode.Fant,
                },
                ExifOrientationMode.RespectExifOrientation,
                ColorManagementMode.ColorManageToSRgb);
            var rgb = ArtistHero.Tint(pixels.DetachPixelData(), width, height);
            if (rgb is not null && version == _heroVersion)
            {
                SetHeroTint(Color.FromArgb(255, rgb[0], rgb[1], rgb[2]), AnimationsEnabled);
            }
        }
        catch (Exception error)
        {
            // The page keeps the default colour.
            BridgeLog.Write($"artist colour unavailable: {error.GetType().Name}");
        }
    }

    /// <summary>Gives the page, and the scrim that leads into it, the artist's colour.</summary>
    private void SetHeroTint(Color tint, bool animate)
    {
        var previous = _heroTint;
        _heroTintStory?.Stop();
        _heroTintStory = null;
        _heroTint = tint;
        ArtistPageColour.Color = tint;
        RebuildScrim();
        if (!animate || previous == tint)
        {
            return;
        }

        // The colours are already the new ones; the story only eases from the old to them.
        var story = new Storyboard();
        story.Children.Add(Ease(ArtistPageColour, previous, tint));
        foreach (var (stop, alpha) in _scrimTintStops)
        {
            story.Children.Add(Ease(stop, WithAlpha(previous, alpha), WithAlpha(tint, alpha)));
        }

        _heroTintStory = story;
        story.Begin();

        static ColorAnimation Ease(DependencyObject target, Color from, Color to)
        {
            var animation = new ColorAnimation
            {
                From = from,
                To = to,
                Duration = TimeSpan.FromSeconds(0.45),
                EasingFunction = new SineEase { EasingMode = EasingMode.EaseInOut },
                EnableDependentAnimation = true,
            };
            Storyboard.SetTarget(animation, target);
            Storyboard.SetTargetProperty(animation, "Color");
            return animation;
        }
    }

    /// <summary>
    /// The gradient that darkens the banner's foot into the page colour, so the picture ends in
    /// the page rather than at an edge and white text reads over any picture.
    /// </summary>
    private void RebuildScrim()
    {
        if (_heroHeader is null)
        {
            return;
        }

        var height = ArtistHero.Height(HeroWidth);
        var brush = new LinearGradientBrush { StartPoint = new Point(0, 0), EndPoint = new Point(0, 1) };
        _scrimTintStops.Clear();
        if (PlainEffects)
        {
            // Without transparency effects, or in a contrast theme, an opaque band of the colour.
            var top = 1 - Math.Min(height * 0.5, 240) / height;
            Add(Colors.Transparent, 0);
            Add(Colors.Transparent, top);
            AddTint(top, 1);
            AddTint(1, 1);
        }
        else
        {
            Add(Color.FromArgb(51, 0, 0, 0), 0);
            Add(Colors.Transparent, 0.22);
            AddTint(0.5, 0.25);
            AddTint(0.78, 0.85);
            AddTint(1, 1);
        }

        ArtistScrim.Fill = brush;

        void Add(Color color, double offset) =>
            brush.GradientStops.Add(new GradientStop { Color = color, Offset = offset });

        void AddTint(double offset, double alpha)
        {
            var stop = new GradientStop { Color = WithAlpha(_heroTint, alpha), Offset = offset };
            brush.GradientStops.Add(stop);
            _scrimTintStops.Add((stop, alpha));
        }
    }

    private static Color WithAlpha(Color color, double alpha) =>
        Color.FromArgb((byte)Math.Round(alpha * 255), color.R, color.G, color.B);
}
