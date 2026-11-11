using System.Collections.Generic;
using Goosic.Windows.Presentation;
using Goosic.Windows.Service;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Media;

namespace Goosic.Windows;

public sealed partial class MainWindow : Window
{
    /// <summary>The acrylic surfaces the transparency slider controls, by resource key.</summary>
    private static readonly string[] MaterialBrushKeys =
        ["GoosicSidebarAcrylicBrush", "GoosicPillAcrylicBrush", "GoosicMenuAcrylicBrush"];

    /// <summary>
    /// Each brush's tint as Palette.xaml designs it, read once before anything changes it, so the
    /// slider always maps from the design rather than from its own last result.
    /// </summary>
    private readonly Dictionary<AcrylicBrush, (double Tint, double Luminosity)> _designedMaterial = [];

    /// <summary>Set once the slider shows the stored value; earlier value changes are layout, not the listener.</summary>
    private bool _materialWired;

    private void WireMaterialTransparency()
    {
        MaterialTransparencySlider.Value = ShellPreferences.MaterialTransparency;
        _materialWired = true;
        ApplyMaterialTransparency(ShellPreferences.MaterialTransparency);
        ShowMaterialAvailability();
        _uiSettings.AdvancedEffectsEnabledChanged += (_, _) => DispatcherQueue.TryEnqueue(ShowMaterialAvailability);
        try
        {
            _accessibility.HighContrastChanged += (_, _) => DispatcherQueue.TryEnqueue(ShowMaterialAvailability);
        }
        catch (System.Runtime.InteropServices.COMException error)
            when (error.HResult == unchecked((int)0x80070490))
        {
            // Unpackaged desktops can refuse this event (the artist hero meets the same thing);
            // a contrast theme also changes the system colours, so that is the signal instead.
            _uiSettings.ColorValuesChanged += (_, _) => DispatcherQueue.TryEnqueue(ShowMaterialAvailability);
        }
    }

    private void OnMaterialTransparencyChanged(object sender, RangeBaseValueChangedEventArgs e)
    {
        if (!_materialWired)
        {
            return;
        }

        var transparency = MaterialTransparency.Sanitize((int)System.Math.Round(e.NewValue));
        ApplyMaterialTransparency(transparency);
        ShellPreferences.MaterialTransparency = transparency;
    }

    /// <summary>
    /// Retints every Goosic acrylic brush in the light and dark palettes. The brushes are shared
    /// theme resources, so every surface using one follows at once, and a theme switch later picks
    /// up a brush that is already retinted. Contrast themes define solid brushes under the same
    /// keys; those are not acrylic and are left alone.
    /// </summary>
    private void ApplyMaterialTransparency(int transparency)
    {
        foreach (var brush in MaterialBrushes())
        {
            if (!_designedMaterial.TryGetValue(brush, out var designed))
            {
                designed = (brush.TintOpacity, brush.TintLuminosityOpacity ?? brush.TintOpacity);
                _designedMaterial[brush] = designed;
            }

            brush.TintOpacity = MaterialTransparency.Opacity(designed.Tint, transparency);
            brush.TintLuminosityOpacity = MaterialTransparency.Opacity(designed.Luminosity, transparency);
        }

        MaterialTransparencyValue.Text = MaterialTransparency.Label(transparency);
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetItemStatus(
            MaterialTransparencySlider, MaterialTransparency.Label(transparency));
    }

    /// <summary>
    /// Acrylic is drawn solid when Windows' transparency effects are off or a contrast theme is on,
    /// and the slider would then move nothing; it says so instead of looking broken.
    /// </summary>
    private void ShowMaterialAvailability()
    {
        var available = _uiSettings.AdvancedEffectsEnabled && !_accessibility.HighContrast;
        MaterialTransparencySlider.IsEnabled = available;
        MaterialTransparencyFooter.Visibility = available ? Visibility.Collapsed : Visibility.Visible;
    }

    private static IEnumerable<AcrylicBrush> MaterialBrushes()
    {
        foreach (var dictionary in Application.Current.Resources.MergedDictionaries)
        {
            foreach (var theme in dictionary.ThemeDictionaries.Values)
            {
                if (theme is not ResourceDictionary palette)
                {
                    continue;
                }

                foreach (var key in MaterialBrushKeys)
                {
                    if (palette.TryGetValue(key, out var value) && value is AcrylicBrush brush)
                    {
                        yield return brush;
                    }
                }
            }
        }
    }
}
