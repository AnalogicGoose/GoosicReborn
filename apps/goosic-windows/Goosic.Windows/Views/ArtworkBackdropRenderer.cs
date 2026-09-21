using System;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Threading.Tasks;
using Microsoft.Graphics.Canvas;
using Microsoft.Graphics.Canvas.Effects;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.Graphics.Imaging;

namespace Goosic.Windows.Views;

/// <summary>Produces one cached, blurred bitmap when the playing cover changes.</summary>
internal sealed class ArtworkBackdropRenderer
{
    private const float RenderSize = 720;
    private string? _lastSource;
    private BitmapImage? _lastImage;

    internal async Task<BitmapImage?> RenderAsync(string source)
    {
        if (_lastSource == source)
        {
            return _lastImage;
        }

        var cache = Path.Combine(Path.GetTempPath(), "Goosic", "backdrops");
        Directory.CreateDirectory(cache);
        var key = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(source)));
        var output = Path.Combine(cache, key + ".png");
        if (!File.Exists(output))
        {
            using var device = CanvasDevice.GetSharedDevice();
            using var bitmap = await CanvasBitmap.LoadAsync(device, source);
            using var target = new CanvasRenderTarget(device, RenderSize, RenderSize, 96);
            var scale = Math.Max(RenderSize / (float)bitmap.SizeInPixels.Width,
                RenderSize / (float)bitmap.SizeInPixels.Height);
            var width = bitmap.SizeInPixels.Width * scale;
            var height = bitmap.SizeInPixels.Height * scale;
            var transform = System.Numerics.Matrix3x2.CreateScale(scale) *
                System.Numerics.Matrix3x2.CreateTranslation((RenderSize - width) / 2, (RenderSize - height) / 2);
            using (var session = target.CreateDrawingSession())
            {
                session.Clear(Microsoft.UI.Colors.Black);
                session.DrawImage(new GaussianBlurEffect
                {
                    Source = new BorderEffect
                    {
                        Source = new Transform2DEffect { Source = bitmap, TransformMatrix = transform },
                        ExtendX = CanvasEdgeBehavior.Clamp,
                        ExtendY = CanvasEdgeBehavior.Clamp,
                    },
                    BlurAmount = 42,
                    BorderMode = EffectBorderMode.Hard,
                });
            }

            await target.SaveAsync(output, CanvasBitmapFileFormat.Png);
        }

        _lastSource = source;
        _lastImage = new BitmapImage(new Uri(output));
        return _lastImage;
    }
}
