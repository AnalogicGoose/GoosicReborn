using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Goosic.Windows.Glass;

/// <summary>
/// A semantic glass surface. The attached property also lets existing Borders become glass
/// without changing their layout or content tree.
/// </summary>
public sealed class GlassSurface : ContentControl
{
    public GlassSurface()
    {
        DefaultStyleKey = typeof(ContentControl);
        Background = new GlassBrush { MaterialStyle = MaterialStyle };
        Loaded += OnLoaded;
        Unloaded += OnUnloaded;
    }

    public GlassStyle MaterialStyle
    {
        get => (GlassStyle)GetValue(MaterialStyleProperty);
        set => SetValue(MaterialStyleProperty, value);
    }

    public static readonly DependencyProperty MaterialStyleProperty = DependencyProperty.Register(
        nameof(MaterialStyle), typeof(GlassStyle), typeof(GlassSurface),
        new PropertyMetadata(GlassStyle.Regular, OnMaterialStyleChanged));

    public static GlassStyle GetGlassStyle(DependencyObject value) =>
        (GlassStyle)value.GetValue(GlassStyleProperty);

    public static void SetGlassStyle(DependencyObject value, GlassStyle style) =>
        value.SetValue(GlassStyleProperty, style);

    public static readonly DependencyProperty GlassStyleProperty = DependencyProperty.RegisterAttached(
        "GlassStyle", typeof(GlassStyle), typeof(GlassSurface),
        new PropertyMetadata(GlassStyle.Regular, OnAttachedStyleChanged));

    private static void OnMaterialStyleChanged(DependencyObject sender, DependencyPropertyChangedEventArgs args)
    {
        if (sender is GlassSurface control)
            control.Background = new GlassBrush { MaterialStyle = (GlassStyle)args.NewValue };
        if (sender is GlassSurface surface && surface.XamlRoot?.Content is FrameworkElement root)
        {
            GlassScene.GetForRoot(root)?.Register(surface, (GlassStyle)args.NewValue);
        }
    }

    private static void OnAttachedStyleChanged(DependencyObject sender, DependencyPropertyChangedEventArgs args)
    {
        if (sender is not FrameworkElement element)
        {
            return;
        }

        element.Loaded -= OnAttachedLoaded;
        element.Unloaded -= OnAttachedUnloaded;
        if (args.NewValue is GlassStyle)
        {
            element.Loaded += OnAttachedLoaded;
            element.Unloaded += OnAttachedUnloaded;
            if (element.IsLoaded)
            {
                RegisterAttached(element);
            }
        }
    }

    private static void OnAttachedLoaded(object sender, RoutedEventArgs args) =>
        RegisterAttached((FrameworkElement)sender);

    private static void RegisterAttached(FrameworkElement element)
    {
        if (element.XamlRoot?.Content is FrameworkElement root)
        {
            GlassScene.GetForRoot(root)?.Register(element, GetGlassStyle(element));
        }
    }

    private static void OnAttachedUnloaded(object sender, RoutedEventArgs args) =>
        GlassScene.Find((FrameworkElement)sender)?.Unregister((FrameworkElement)sender);

    private void OnLoaded(object sender, RoutedEventArgs args)
    {
        if (XamlRoot?.Content is FrameworkElement root)
        {
            GlassScene.GetForRoot(root)?.Register(this, MaterialStyle);
        }
    }

    private void OnUnloaded(object sender, RoutedEventArgs args) => GlassScene.Find(this)?.Unregister(this);
}
