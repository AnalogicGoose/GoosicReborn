using System;
using Microsoft.UI.Dispatching;
using Windows.System.Power;
using Windows.UI.ViewManagement;

namespace Goosic.Windows.Glass;

/// <summary>
/// The one place that watches the operating system on glass's behalf: contrast, transparency,
/// animations, power, and whether the renderer turned out to be supported at all.
/// </summary>
/// <remarks>
/// Every scene and every popup brush reads the same tier from here, so a change of setting
/// reaches the sidebar and an open menu in the same frame rather than one window at a time.
/// </remarks>
public static class GlassSystem
{
    private static UISettings? _ui;
    private static AccessibilitySettings? _accessibility;
    private static DispatcherQueue? _queue;
    private static bool _rendererSupported = true;
    private static string? _override = Environment.GetEnvironmentVariable("GOOSIC_GLASS_QUALITY");

    public static event Action? Changed;

    public static GlassEnvironment Current { get; private set; } = new(false, true, true, false, false, true, null);

    public static GlassQuality Quality { get; private set; } = GlassQuality.High;

    public static bool ReducedMotion { get; private set; }

    /// <summary>Why the renderer is running solid surfaces, when it is; empty otherwise.</summary>
    public static string UnsupportedReason { get; private set; } = "";

    /// <summary>A developer override of the tier. Accessibility settings still win over it.</summary>
    public static string? QualityOverride
    {
        get => _override;
        set
        {
            _override = value;
            Refresh();
        }
    }

    public static void EnsureInitialized()
    {
        if (_queue is not null)
        {
            return;
        }

        _queue = DispatcherQueue.GetForCurrentThread();
        _ui = new UISettings();
        _accessibility = new AccessibilitySettings();
        // These preference brokers can be absent in an unpackaged process. Reading their current
        // values still works, so event delivery is an optional enhancement rather than a launch
        // requirement.
        try
        {
            _ui.AdvancedEffectsEnabledChanged += (_, _) => RefreshOnUiThread();
            if (OperatingSystem.IsWindowsVersionAtLeast(10, 0, 19041))
                _ui.AnimationsEnabledChanged += (_, _) => RefreshOnUiThread();
            _accessibility.HighContrastChanged += (_, _) => RefreshOnUiThread();
        }
        catch (Exception)
        {
        }
        try
        {
            PowerManager.EnergySaverStatusChanged += (_, _) => RefreshOnUiThread();
            PowerManager.BatteryStatusChanged += (_, _) => RefreshOnUiThread();
        }
        catch (Exception)
        {
            // A machine without the power broker keeps its tier; nothing else depends on it.
        }

        Refresh();
    }

    internal static void ReportRendererUnsupported(string reason)
    {
        ReportRendererWarning(reason);

        if (!_rendererSupported)
        {
            return;
        }

        _rendererSupported = false;
        UnsupportedReason = reason;
        Goosic.Windows.Service.BridgeLog.Write($"glass renderer unsupported: {reason}");
        RefreshOnUiThread();
    }

    internal static void ReportRendererWarning(string reason)
    {
        try
        {
            System.IO.File.AppendAllText(
                System.IO.Path.Combine(System.IO.Path.GetTempPath(), "goosic-glass.log"),
                $"{DateTimeOffset.Now:O} {reason}{Environment.NewLine}");
        }
        catch (Exception)
        {
        }
    }

    private static void RefreshOnUiThread()
    {
        if (_queue is null || _queue.HasThreadAccess)
        {
            Refresh();
        }
        else
        {
            _queue.TryEnqueue(Refresh);
        }
    }

    private static void Refresh()
    {
        bool energySaver = false, onBattery = false;
        try
        {
            energySaver = PowerManager.EnergySaverStatus == EnergySaverStatus.On;
            onBattery = PowerManager.BatteryStatus == BatteryStatus.Discharging;
        }
        catch (Exception)
        {
        }

        Current = new GlassEnvironment(
            _accessibility?.HighContrast ?? false,
            _ui?.AdvancedEffectsEnabled ?? true,
            _ui?.AnimationsEnabled ?? true,
            energySaver,
            onBattery,
            _rendererSupported,
            _override);
        var quality = GlassQualityPolicy.Select(Current);
        var reducedMotion = GlassQualityPolicy.ReducedMotion(Current);
        var changed = quality != Quality || reducedMotion != ReducedMotion;
        if (quality != Quality)
        {
            Goosic.Windows.Service.BridgeLog.Write($"glass quality {quality} (transparency={Current.TransparencyEffects}, "
                + $"highContrast={Current.HighContrast}, battery={Current.OnBattery}, saver={Current.EnergySaver}, renderer={Current.RendererSupported})");
        }
        Quality = quality;
        ReducedMotion = reducedMotion;
        if (changed)
        {
            Changed?.Invoke();
        }
    }
}
