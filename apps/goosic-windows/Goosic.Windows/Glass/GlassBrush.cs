using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;
using Windows.UI;

namespace Goosic.Windows.Glass;

/// <summary>Semantic material for native presenter templates; foreground remains in XAML.</summary>
public sealed class GlassBrush : XamlCompositionBrushBase
{
    public GlassStyle MaterialStyle { get; set; } = GlassStyle.Menu;
    protected override void OnConnected()
    {
        GlassSystem.EnsureInitialized();
        GlassSystem.Changed += Refresh;
        Refresh();
    }

    private void Refresh()
    {
        var dark = Application.Current.RequestedTheme == ApplicationTheme.Dark;
        var fallback = GlassSystem.Current.HighContrast
            ? new global::Windows.UI.ViewManagement.UISettings().GetColorValue(global::Windows.UI.ViewManagement.UIColorType.Background)
            : dark ? Color.FromArgb(255, 26, 26, 30) : Color.FromArgb(255, 242, 242, 245);
        var old = CompositionBrush;
        CompositionBrush = WinUiGlassRenderer.Shared.CreatePopupBrush(MaterialStyle, dark, fallback, 0);
        old?.Dispose();
    }

    protected override void OnDisconnected()
    {
        GlassSystem.Changed -= Refresh;
        CompositionBrush?.Dispose();
        CompositionBrush = null;
    }
}
