using System;
using System.Collections.Generic;
using Windows.System;

namespace Goosic.Windows.Presentation;

public enum ShellCommand
{
    Next,
    Previous,
    VolumeUp,
    VolumeDown,
    ToggleMute,
    SeekForward,
    SeekBackward,
    ToggleShuffle,
    CycleRepeat,
    Search,
    ToggleLyrics,
    ToggleQueue,
    Back,
    FullScreen,
    ToggleFullPlayer,
    Escape,
    ToggleSidebar,
    Settings,
    Like,
}

/// <summary>What focus is on when Space is pressed, as far as Space is concerned.</summary>
public enum SpaceTarget { Other, TextInput, Control }

/// <summary>The keyboard shortcuts and the arithmetic behind them, free of any control.</summary>
public static class ShellKeyboard
{
    public const double SeekStepSeconds = 10;
    public const double VolumeStep = 0.05;

    public static IReadOnlyList<(VirtualKey Key, VirtualKeyModifiers Modifiers, ShellCommand Command)> Bindings { get; } =
    [
        (VirtualKey.Right, VirtualKeyModifiers.Control, ShellCommand.Next),
        (VirtualKey.Left, VirtualKeyModifiers.Control, ShellCommand.Previous),
        (VirtualKey.Up, VirtualKeyModifiers.Control, ShellCommand.VolumeUp),
        (VirtualKey.Down, VirtualKeyModifiers.Control, ShellCommand.VolumeDown),
        (VirtualKey.M, VirtualKeyModifiers.Control, ShellCommand.ToggleMute),
        (VirtualKey.Right, VirtualKeyModifiers.Shift, ShellCommand.SeekForward),
        (VirtualKey.Left, VirtualKeyModifiers.Shift, ShellCommand.SeekBackward),
        (VirtualKey.S, VirtualKeyModifiers.Control, ShellCommand.ToggleShuffle),
        (VirtualKey.R, VirtualKeyModifiers.Control, ShellCommand.CycleRepeat),
        (VirtualKey.F, VirtualKeyModifiers.Control, ShellCommand.Search),
        (VirtualKey.L, VirtualKeyModifiers.Control, ShellCommand.ToggleLyrics),
        (VirtualKey.Q, VirtualKeyModifiers.Control, ShellCommand.ToggleQueue),
        (VirtualKey.Left, VirtualKeyModifiers.Menu, ShellCommand.Back),
        (VirtualKey.F11, VirtualKeyModifiers.None, ShellCommand.FullScreen),
        (VirtualKey.F, VirtualKeyModifiers.Control | VirtualKeyModifiers.Shift, ShellCommand.ToggleFullPlayer),
        (VirtualKey.Escape, VirtualKeyModifiers.None, ShellCommand.Escape),
        (VirtualKey.B, VirtualKeyModifiers.Control, ShellCommand.ToggleSidebar),
        // Ctrl+Comma opens settings in nearly every desktop app, Apple's included.
        ((VirtualKey)188, VirtualKeyModifiers.Control, ShellCommand.Settings),
        // Spotify's shortcut for liking the song that is playing.
        (VirtualKey.B, VirtualKeyModifiers.Menu | VirtualKeyModifiers.Shift, ShellCommand.Like),
    ];

    /// <summary>
    /// Space toggles playback everywhere except where it already means something: a character in
    /// a text box, or activation of a focused button or slider.
    /// </summary>
    public static bool SpaceTogglesPlayback(SpaceTarget focus) => focus == SpaceTarget.Other;

    /// <summary>Where a seek key lands, or null when the track cannot be seeked.</summary>
    public static double? SeekTarget(double position, double duration, double delta, bool seekable) =>
        seekable && duration > 0 ? Math.Clamp(position + delta, 0, duration) : null;

    /// <summary>The gain one volume key lands on: a step along the slider's taper, not of the gain.</summary>
    public static double NudgedVolume(double volume, double delta) =>
        VolumeTaper.ToGain(VolumeTaper.ToPosition(volume) + delta);
}
