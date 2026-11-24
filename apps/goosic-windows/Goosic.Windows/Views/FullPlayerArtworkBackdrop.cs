using System;
using System.IO;
using System.Numerics;
using System.Threading.Tasks;
using Goosic.Windows.Service;
using Microsoft.Graphics.Canvas;
using Microsoft.Graphics.Canvas.Effects;
using Microsoft.UI.Composition;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Hosting;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.Graphics.DirectX;
using Windows.Graphics.Imaging;
using Windows.Storage.Streams;

namespace Goosic.Windows.Views;

/// <summary>Filters a small cover once, then moves only a finished texture on the compositor.</summary>
internal sealed class FullPlayerArtworkBackdrop : Grid
{
    private const int Side = 128;
    private const float DisplacementRange = Side;
    private static readonly Lazy<byte[]> DisplacementPixels = new(CreateDisplacementPixels);
    private readonly Image _base = new() { Stretch = Stretch.UniformToFill };
    private readonly Image _overlay = new() { Stretch = Stretch.UniformToFill, Opacity = 0.68 };
    private readonly RectangleGeometry _clip = new();
    private Visual? _overlayVisual;
    private string? _requestedFile;
    private int _request;
    private string? _cachedFile;
    private BitmapImage? _cachedTexture;
    private bool _animate;
    private bool _drifting;

    internal FullPlayerArtworkBackdrop()
    {
        IsHitTestVisible = false;
        Clip = _clip;
        Children.Add(_base);
        Children.Add(_overlay);
        Loaded += (_, _) =>
        {
            ElementCompositionPreview.SetIsTranslationEnabled(_overlay, true);
            _overlayVisual = ElementCompositionPreview.GetElementVisual(_overlay);
            _overlayVisual.Scale = new Vector3(1.46f, 1.46f, 1);
            UpdateBounds();
            UpdateMotion();
        };
        Unloaded += (_, _) =>
        {
            Update(file: null, animate: false);
            _overlayVisual = null;
        };
        SizeChanged += (_, _) => UpdateBounds();
    }

    internal void Update(string? file, bool animate)
    {
        _animate = animate;
        if (!string.Equals(_requestedFile, file, StringComparison.Ordinal))
        {
            _requestedFile = file;
            var request = ++_request;
            SetTexture(file is not null && file == _cachedFile ? _cachedTexture : null);
            if (file is not null && file != _cachedFile)
            {
                _ = LoadTextureAsync(file, request);
            }
        }

        UpdateMotion();
    }

    private void SetTexture(BitmapImage? texture)
    {
        _base.Source = texture;
        _overlay.Source = texture;
    }

    private void UpdateBounds()
    {
        _clip.Rect = new global::Windows.Foundation.Rect(0, 0, ActualWidth, ActualHeight);
        if (_overlayVisual is { } visual)
        {
            // Scale around the middle and rotate around the off-centre pivot of the enlarged image.
            var width = (float)ActualWidth;
            var height = (float)ActualHeight;
            var pivot = new Vector3(width * 0.28f, height * 0.69f, 0);
            visual.CenterPoint = pivot;
            visual.Properties.InsertVector3("Translation", (pivot - new Vector3(width / 2, height / 2, 0)) * 0.46f);
        }
    }

    private void UpdateMotion()
    {
        if (_overlayVisual is not { } visual)
        {
            return;
        }

        var drift = _animate && _overlay.Source is not null;
        if (drift == _drifting)
        {
            return;
        }

        _drifting = drift;
        const string angle = nameof(Visual.RotationAngleInDegrees);
        if (!drift)
        {
            visual.StopAnimation(angle);
            visual.RotationAngleInDegrees = 0;
            return;
        }

        using var ease = visual.Compositor.CreateCubicBezierEasingFunction(new Vector2(0.42f, 0), new Vector2(0.58f, 1));
        using var animation = visual.Compositor.CreateScalarKeyFrameAnimation();
        animation.Duration = TimeSpan.FromSeconds(22);
        animation.IterationBehavior = AnimationIterationBehavior.Forever;
        animation.InsertKeyFrame(0, 0);
        animation.InsertKeyFrame(0.25f, 16.6f, ease);
        animation.InsertKeyFrame(0.5f, 0, ease);
        animation.InsertKeyFrame(0.75f, -16.6f, ease);
        animation.InsertKeyFrame(1, 0, ease);
        visual.StartAnimation(angle, animation);
    }

    private async Task LoadTextureAsync(string file, int request)
    {
        try
        {
            using var stream = File.OpenRead(file);
            using var input = stream.AsRandomAccessStream();
            var decoder = await BitmapDecoder.CreateAsync(input);
            var scale = Side / (double)Math.Max(decoder.PixelWidth, decoder.PixelHeight);
            var width = (int)Math.Max(1, Math.Round(decoder.PixelWidth * scale));
            var height = (int)Math.Max(1, Math.Round(decoder.PixelHeight * scale));
            var pixels = await decoder.GetPixelDataAsync(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Premultiplied,
                new BitmapTransform { ScaledWidth = (uint)width, ScaledHeight = (uint)height },
                ExifOrientationMode.IgnoreExifOrientation, ColorManagementMode.ColorManageToSRgb);
            if (_request != request)
            {
                return;
            }

            var bytes = pixels.DetachPixelData();
            using var baked = await Task.Run(() => BakeAsync(bytes, width, height));
            if (_request != request)
            {
                return;
            }

            var image = new BitmapImage();
            await image.SetSourceAsync(baked);
            if (_request == request)
            {
                _cachedFile = file;
                _cachedTexture = image;
                SetTexture(image);
                UpdateMotion();
            }
        }
        catch (Exception error)
        {
            BridgeLog.Write($"full-player artwork backdrop unavailable: {error.GetType().Name}");
        }
    }

    private static async Task<InMemoryRandomAccessStream> BakeAsync(byte[] pixels, int width, int height)
    {
        using var device = CanvasDevice.GetSharedDevice();
        using var input = CanvasBitmap.CreateFromBytes(device, pixels, width, height, DirectXPixelFormat.B8G8R8A8UIntNormalized);
        using var displacement = CanvasBitmap.CreateFromBytes(device, DisplacementPixels.Value, Side, Side,
            DirectXPixelFormat.B8G8R8A8UIntNormalized);
        using var target = new CanvasRenderTarget(device, Side, Side, 96);
        var scale = Side / (float)Math.Min(width, height);
        using var fitted = new Transform2DEffect
        {
            Source = input,
            TransformMatrix = Matrix3x2.CreateScale(scale) *
                Matrix3x2.CreateTranslation((Side - width * scale) / 2, (Side - height * scale) / 2),
        };
        using var clamped = new BorderEffect
        {
            Source = fitted,
            ExtendX = CanvasEdgeBehavior.Clamp,
            ExtendY = CanvasEdgeBehavior.Clamp,
        };
        using var warp = new DisplacementMapEffect
        {
            Source = clamped,
            Displacement = displacement,
            Amount = DisplacementRange,
            XChannelSelect = EffectChannelSelect.Red,
            YChannelSelect = EffectChannelSelect.Green,
        };
        using var clipped = new CropEffect
        {
            Source = warp,
            SourceRectangle = new global::Windows.Foundation.Rect(0, 0, Side, Side),
        };
        using var blur = new GaussianBlurEffect { Source = clipped, BlurAmount = 8, BorderMode = EffectBorderMode.Hard };
        using var colour = new SaturationEffect { Source = blur, Saturation = 1.35f };
        using (var session = target.CreateDrawingSession())
        {
            session.Clear(Microsoft.UI.Colors.Black);
            session.DrawImage(colour);
        }

        var output = new InMemoryRandomAccessStream();
        try
        {
            await target.SaveAsync(output, CanvasBitmapFileFormat.Png);
            output.Seek(0);
            return output;
        }
        catch
        {
            output.Dispose();
            throw;
        }
    }

    private static byte[] CreateDisplacementPixels()
    {
        var pixels = new byte[Side * Side * 4];
        for (var y = 0; y < Side; y++)
        {
            for (var x = 0; x < Side; x++)
            {
                var dx = x + 0.5f - Side / 2f;
                var dy = y + 0.5f - Side / 2f;
                var weight = MathF.Max(0, 1 - MathF.Sqrt(dx * dx + dy * dy) / (Side * 0.66f));
                var angle = 2.9f * weight * weight;
                var sin = MathF.Sin(angle);
                var cos = MathF.Cos(angle);
                var index = (y * Side + x) * 4;
                // Direct2D reads signed offsets encoded around the midpoint of each channel.
                pixels[index + 1] = Encode(dx * sin + dy * cos - dy);
                pixels[index + 2] = Encode(dx * cos - dy * sin - dx);
                pixels[index + 3] = 255;
            }
        }

        return pixels;

        static byte Encode(float offset) => (byte)Math.Clamp(MathF.Round(255 * (0.5f + offset / DisplacementRange)), 0, 255);
    }
}
