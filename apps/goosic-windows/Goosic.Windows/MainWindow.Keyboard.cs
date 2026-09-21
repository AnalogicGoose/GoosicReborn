using System;
using System.Collections.Generic;
using System.IO;
using System.Threading.Tasks;
using Goosic.Windows.Presentation;
using Goosic.Windows.Service;
using Goosic.Windows.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Windows.ApplicationModel.DataTransfer;
using Windows.Media;
using Windows.System;

namespace Goosic.Windows;

public sealed partial class MainWindow : Window
{
    // ---- Keyboard ---------------------------------------------------------------------------

    /// <summary>
    /// The shortcuts a music app is expected to have.
    /// </summary>
    /// <remarks>
    /// Space is handled on preview rather than as an accelerator, so it stays a space inside the
    /// search box and still activates a focused button there; everywhere else it toggles playback.
    /// </remarks>
    private void WireKeyboard()
    {
        RootGrid.KeyboardAcceleratorPlacementMode = KeyboardAcceleratorPlacementMode.Hidden;
        foreach (var (key, modifiers, command) in ShellKeyboard.Bindings)
        {
            Accelerator(key, modifiers, () => Execute(command));
        }

        RootGrid.PreviewKeyDown += OnPreviewKeyDown;
    }

    private void Execute(ShellCommand command)
    {
        switch (command)
        {
            case ShellCommand.Next: _ = AdvanceAsync(forward: true, natural: false); break;
            case ShellCommand.Previous: _ = PreviousAsync(); break;
            case ShellCommand.VolumeUp: _ = NudgeVolumeAsync(ShellKeyboard.VolumeStep); break;
            case ShellCommand.VolumeDown: _ = NudgeVolumeAsync(-ShellKeyboard.VolumeStep); break;
            case ShellCommand.ToggleMute:
                if (Model.CanAdjustSound())
                {
                    _ = _playback?.ToggleMutedAsync();
                }
                break;
            case ShellCommand.SeekForward: _ = SeekByAsync(ShellKeyboard.SeekStepSeconds); break;
            case ShellCommand.SeekBackward: _ = SeekByAsync(-ShellKeyboard.SeekStepSeconds); break;
            case ShellCommand.ToggleShuffle: Model.ToggleShuffle(); break;
            case ShellCommand.CycleRepeat: Model.CycleRepeat(); break;
            case ShellCommand.Search: OpenSearch(); break;
            case ShellCommand.ToggleLyrics: _ = ToggleSidePanelAsync(SidePanelContent.Lyrics); break;
            case ShellCommand.ToggleQueue: _ = ToggleSidePanelAsync(SidePanelContent.Queue); break;
            case ShellCommand.Back: _ = GoBackAsync(); break;
            case ShellCommand.FullScreen:
                // In the player F11 fills the screen; elsewhere it opens the player.
                if (FullPlayer.Visibility == Visibility.Visible)
                {
                    SetWindowFullScreen(FullPlayerWindowToggle.IsChecked != true);
                }
                else if (Model.HasPlayback)
                {
                    SetFullPlayerOpen(true);
                }
                break;
            case ShellCommand.ToggleFullPlayer:
                if (FullPlayer.Visibility == Visibility.Visible || Model.HasPlayback)
                {
                    SetFullPlayerOpen(FullPlayer.Visibility != Visibility.Visible);
                }
                break;
            case ShellCommand.Escape:
                if (FullPlayer.Visibility == Visibility.Visible)
                {
                    SetFullPlayerOpen(false);
                }
                else if (_sidePanel.IsOpen)
                {
                    CloseSidePanel();
                }
                break;
            case ShellCommand.ToggleSidebar: ToggleSidebar(); break;
        }
    }

    /// <summary>Moves ten seconds either way, as the web player's seek keys do.</summary>
    private async Task SeekByAsync(double seconds)
    {
        if (_playback is not null
            && ShellKeyboard.SeekTarget(Model.PlaybackPosition, Model.PlaybackDuration, seconds, Model.IsSeekable) is { } target)
        {
            await SeekToAsync(target);
        }
    }

    private void Accelerator(VirtualKey key, VirtualKeyModifiers modifiers, Action action)
    {
        var accelerator = new KeyboardAccelerator { Key = key, Modifiers = modifiers };
        accelerator.Invoked += (_, args) =>
        {
            args.Handled = true;
            action();
        };
        RootGrid.KeyboardAccelerators.Add(accelerator);
    }

    private void OnPreviewKeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key != VirtualKey.Space)
        {
            return;
        }

        var focus = FocusManager.GetFocusedElement(RootGrid.XamlRoot) switch
        {
            TextBox or AutoSuggestBox => SpaceTarget.TextInput,
            ButtonBase or Slider => SpaceTarget.Control,
            _ => SpaceTarget.Other,
        };
        if (!ShellKeyboard.SpaceTogglesPlayback(focus))
        {
            return;
        }

        e.Handled = true;
        _ = TogglePauseAsync();
    }

    private async Task NudgeVolumeAsync(double delta)
    {
        if (_playback is not null && Model.HasPlayback && Model.CanAdjustSound())
        {
            await _playback.SetVolumeAsync(ShellKeyboard.NudgedVolume(Model.Volume, delta));
        }
    }
}
