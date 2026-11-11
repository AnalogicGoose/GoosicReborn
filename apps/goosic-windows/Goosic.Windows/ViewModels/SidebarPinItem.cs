using System;
using Goosic.Windows.Presentation;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media.Imaging;

namespace Goosic.Windows.ViewModels;

/// <summary>A pin as the sidebar draws it: the stored pin, plus its artwork loaded small.</summary>
public sealed class SidebarPinItem
{
    internal SidebarPinItem(SidebarPin pin)
    {
        Pin = pin;
        if (Uri.TryCreate(pin.Thumbnail, UriKind.Absolute, out var uri) && uri.Scheme is "https" or "http")
        {
            Artwork = new BitmapImage(uri) { DecodePixelWidth = 48 };
        }
    }

    internal SidebarPin Pin { get; }

    public string Title => Pin.Title;

    public string Glyph => Pin.Glyph;

    public BitmapImage? Artwork { get; }

    /// <summary>An artist is a circle and anything else a rounded square, as everywhere else.</summary>
    public CornerRadius ArtworkCornerRadius => new(Pin.Kind == "artist" ? 12 : 4);
}
