using System;
using System.IO;
using System.Threading.Tasks;
using Microsoft.Graphics.Canvas;
using Microsoft.Graphics.Canvas.Effects;

namespace Goosic.Windows.Views;

/// <summary>
/// Bakes a cover into the quiet, static backdrop used behind the ordinary shell.
/// </summary>
/// <remarks>
/// The effect is rendered once when the track changes. Resizing and scrolling then draw a plain
/// bitmap instead of keeping a live blur or the full-screen procedural mesh on the GPU.
/// </remarks>
internal sealed class ArtworkBackdropRenderer
{
    private const float OutputSide = 720;
    private string? _source;
    private string? _rendered;

    internal async Task<string?> RenderAsync(string source)
    {
        if (_source == source && _rendered is not null && File.Exists(_rendered))
        {
            return _rendered;
        }

        try
        {
            var device = CanvasDevice.GetSharedDevice();
            using var bitmap = await CanvasBitmap.LoadAsync(device, source);
            using var target = new CanvasRenderTarget(device, OutputSide, OutputSide, 96);
            var bounds = bitmap.Bounds;
            var scale = Math.Max(OutputSide / bounds.Width, OutputSide / bounds.Height);
            var width = bounds.Width * scale;
            var height = bounds.Height * scale;
            var sourceEffect = new Transform2DEffect
            {
                Source = bitmap,
                TransformMatrix = System.Numerics.Matrix3x2.CreateScale((float)scale)
                    * System.Numerics.Matrix3x2.CreateTranslation(
                        (float)((OutputSide - width) / 2),
                        (float)((OutputSide - height) / 2)),
            };
            var clamped = new BorderEffect
            {
                Source = sourceEffect,
                ExtendX = CanvasEdgeBehavior.Clamp,
                ExtendY = CanvasEdgeBehavior.Clamp,
            };
            var blurred = new GaussianBlurEffect
            {
                Source = clamped,
                BlurAmount = 48,
                BorderMode = EffectBorderMode.Hard,
                Optimization = EffectOptimization.Quality,
            };
            using (var drawing = target.CreateDrawingSession())
            {
                drawing.DrawImage(blurred);
            }

            var directory = Path.Combine(Path.GetTempPath(), "Goosic", "backdrops");
            Directory.CreateDirectory(directory);
            var output = Path.Combine(directory, Convert.ToHexString(
                System.Security.Cryptography.SHA256.HashData(System.Text.Encoding.UTF8.GetBytes(source))) + ".png");
            await target.SaveAsync(output, CanvasBitmapFileFormat.Png);
            _source = source;
            _rendered = output;
            return output;
        }
        catch (Exception)
        {
            return null;
        }
    }
}
