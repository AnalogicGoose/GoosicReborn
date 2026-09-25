using System;
using System.Collections.Generic;
using System.IO;
using System.Threading.Tasks;
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
    // ---- System media controls --------------------------------------------------------------

    private void WireSystemMediaControls()
    {
        try
        {
            _media = new SystemMediaControls(DispatcherQueue);
        }
        catch (Exception error)
        {
            // The overlay is a convenience; a machine without it still plays.
            BridgeLog.Write($"media controls error {error.GetType().Name}: {error.Message}");
            Model.ReportDetail("Windows media controls are unavailable on this device.");
            return;
        }

        _media.ButtonPressed += button =>
        {
            switch (button)
            {
                case SystemMediaTransportControlsButton.Play:
                case SystemMediaTransportControlsButton.Pause:
                    _ = TogglePauseAsync();
                    break;
                case SystemMediaTransportControlsButton.Next:
                    _ = AdvanceAsync(forward: true, natural: false);
                    break;
                case SystemMediaTransportControlsButton.Previous:
                    _ = PreviousAsync();
                    break;
            }
        };
        _media.SeekRequested += seconds =>
        {
            if (Model.IsSeekable)
            {
                _ = SeekToAsync(Math.Clamp(seconds, 0, Model.PlaybackDuration));
            }
        };
        Model.NowPlayingChanged += async track =>
        {
            var artwork = await Model.ArtworkFileAsync(track);
            if (_media is not null && track.VideoId is { } videoId)
            {
                await _media.ShowTrackAsync(videoId, track.Title, track.Subtitle, artwork);
            }
        };
    }
}
