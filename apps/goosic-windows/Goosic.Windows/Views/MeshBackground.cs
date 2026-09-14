/*
 * MeshBackground.cs — draws NowPlayingMesh as a field of soft colour blobs.
 *
 * The WinUI counterpart of NowPlayingMeshBackground in NativeMacFullPlayer.swift, ported from the
 * previous Goosic application's now-playing-background.tsx.
 *
 * Copyright (C) Oscar Mantilla, George Shyshov, and Goosic contributors.
 *
 * This program is free software: you can redistribute it and/or modify it under the terms of
 * the GNU General Public License as published by the Free Software Foundation, version 3.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
 * without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
 * See the GNU General Public License for more details. A copy is in LICENSE-GPL-3.0 at the
 * repository root.
 */
using System;
using System.Collections.Generic;
using System.IO;
using System.Threading.Tasks;
using Goosic.Windows.ViewModels;
using Microsoft.UI;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Windows.Graphics.Imaging;
using Windows.UI;

namespace Goosic.Windows.Views;

/// <summary>
/// The animated field of colour blobs sampled from the playing cover.
/// </summary>
/// <remarks>
/// Each blob is a radial gradient with a transparent falloff, so overlapping blobs fuse into one
/// field. SwiftUI blurs the field as well; WinUI has no element blur without Win2D, so the falloff
/// is wider instead and the field is oversized so the drift never shows an edge. Motion stops while
/// the system's animation setting is off, as Reduce Motion does on macOS.
/// </remarks>
internal sealed class MeshBackground : Grid
{
    private readonly Canvas _field = new() { IsHitTestVisible = false };
    private readonly List<(Rectangle Blob, ScaleTransform Scale, TranslateTransform Offset, MeshCell Cell)> _blobs = [];
    private readonly CompositeTransform _drift = new();
    private readonly DateTime _started = DateTime.UtcNow;
    private readonly bool _animate = new global::Windows.UI.ViewManagement.UISettings().AnimationsEnabled;
    private IReadOnlyList<MeshSample>? _palette;
    private bool _rendering;
    private DateTime _lastFrame;

    internal MeshBackground()
    {
        IsHitTestVisible = false;
        Background = new SolidColorBrush(Colors.Black);
        Children.Add(_field);
        _field.RenderTransform = _drift;
        SizeChanged += (_, _) => Layout();
        Loaded += (_, _) => StartRendering();
        Unloaded += (_, _) => StopRendering();
    }

    /// <summary>Reads a cover file into a palette, or null if it cannot be read.</summary>
    internal static async Task<IReadOnlyList<MeshSample>?> PaletteAsync(string file)
    {
        try
        {
            using var stream = File.OpenRead(file);
            var decoder = await BitmapDecoder.CreateAsync(stream.AsRandomAccessStream());
            var transform = new BitmapTransform
            {
                ScaledWidth = NowPlayingMesh.SampleSide,
                ScaledHeight = NowPlayingMesh.SampleSide,
                InterpolationMode = BitmapInterpolationMode.Linear,
            };
            var data = await decoder.GetPixelDataAsync(
                BitmapPixelFormat.Rgba8, BitmapAlphaMode.Straight, transform,
                ExifOrientationMode.IgnoreExifOrientation, ColorManagementMode.DoNotColorManage);
            return NowPlayingMesh.Palette(data.DetachPixelData(), NowPlayingMesh.SampleSide, NowPlayingMesh.SampleSide);
        }
        catch (Exception)
        {
            return null;
        }
    }

    /// <summary>Shows a palette; null keeps the field dark.</summary>
    internal void SetPalette(IReadOnlyList<MeshSample>? palette)
    {
        _palette = palette;
        _field.Children.Clear();
        _blobs.Clear();
        if (palette is null)
        {
            Background = new SolidColorBrush(Colors.Black);
            return;
        }

        Background = new SolidColorBrush(ColorOf(palette[0], 255));
        foreach (var cell in NowPlayingMesh.Cells(palette))
        {
            var color = ColorOf(cell.Color, 255);
            var brush = new RadialGradientBrush
            {
                Center = new global::Windows.Foundation.Point(cell.X, cell.Y),
                GradientOrigin = new global::Windows.Foundation.Point(cell.X, cell.Y),
                RadiusX = 0.95,
                RadiusY = 0.95,
            };
            brush.GradientStops.Add(new GradientStop { Color = color, Offset = 0 });
            brush.GradientStops.Add(new GradientStop { Color = color, Offset = 0.3 });
            brush.GradientStops.Add(new GradientStop { Color = ColorOf(cell.Color, 0), Offset = 0.9 });
            var scale = new ScaleTransform();
            var offset = new TranslateTransform();
            var blob = new Rectangle
            {
                Fill = brush,
                RenderTransform = new TransformGroup { Children = { scale, offset } },
                RenderTransformOrigin = new global::Windows.Foundation.Point(0.5, 0.5),
            };
            _field.Children.Add(blob);
            _blobs.Add((blob, scale, offset, cell));
        }

        Layout();
        Render();
    }

    private static Color ColorOf(MeshSample sample, byte alpha) =>
        Color.FromArgb(alpha, (byte)sample.Red, (byte)sample.Green, (byte)sample.Blue);

    private double SlotWidth => ActualWidth * 1.6 / NowPlayingMesh.GridSide;

    private double SlotHeight => ActualHeight * 1.6 / NowPlayingMesh.GridSide;

    private void Layout()
    {
        var fieldWidth = ActualWidth * 1.6;
        var fieldHeight = ActualHeight * 1.6;
        _field.Width = fieldWidth;
        _field.Height = fieldHeight;
        _field.Margin = new Thickness(-ActualWidth * 0.3, -ActualHeight * 0.3, -ActualWidth * 0.3, -ActualHeight * 0.3);
        _drift.CenterX = fieldWidth / 2;
        _drift.CenterY = fieldHeight / 2;
        for (var index = 0; index < _blobs.Count; index++)
        {
            var (blob, _, _, _) = _blobs[index];
            // Each blob is drawn two slots wide so its falloff reaches well into its neighbours.
            blob.Width = SlotWidth * 2;
            blob.Height = SlotHeight * 2;
            Canvas.SetLeft(blob, index % NowPlayingMesh.GridSide * SlotWidth - SlotWidth * 0.5);
            Canvas.SetTop(blob, index / NowPlayingMesh.GridSide * SlotHeight - SlotHeight * 0.5);
        }

        Render();
    }

    private void StartRendering()
    {
        if (_rendering || !_animate)
        {
            Render();
            return;
        }

        _rendering = true;
        CompositionTarget.Rendering += OnRendering;
    }

    private void StopRendering()
    {
        if (_rendering)
        {
            CompositionTarget.Rendering -= OnRendering;
            _rendering = false;
        }
    }

    private void OnRendering(object? sender, object e)
    {
        // Thirty frames a second is plenty for a field that moves this slowly.
        var now = DateTime.UtcNow;
        if (now - _lastFrame < TimeSpan.FromMilliseconds(33) || Visibility != Visibility.Visible)
        {
            return;
        }

        _lastFrame = now;
        Render();
    }

    private void Render()
    {
        if (_palette is null || ActualWidth <= 0)
        {
            return;
        }

        var time = (DateTime.UtcNow - _started).TotalSeconds;
        var drift = _animate ? NowPlayingMesh.Drift(time) : NowPlayingMesh.RestingDrift;
        _drift.ScaleX = drift.Scale;
        _drift.ScaleY = drift.Scale;
        _drift.Rotation = drift.Degrees;
        _drift.TranslateX = drift.Dx * _field.Width;
        _drift.TranslateY = drift.Dy * _field.Height;
        for (var index = 0; index < _blobs.Count; index++)
        {
            var (blob, scale, offset, cell) = _blobs[index];
            var pose = _animate ? NowPlayingMesh.Breathe(index, time) : NowPlayingMesh.RestingCell;
            scale.ScaleX = cell.Scale * pose.Scale;
            scale.ScaleY = cell.Scale * pose.Scale;
            offset.X = pose.Dx * SlotWidth;
            offset.Y = pose.Dy * SlotHeight;
            blob.Opacity = pose.Opacity;
        }
    }
}
