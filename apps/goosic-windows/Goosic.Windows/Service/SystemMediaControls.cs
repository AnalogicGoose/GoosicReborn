using System;
using System.Threading.Tasks;
using Microsoft.UI.Dispatching;
using Windows.Media;
using Windows.Media.Playback;
using Windows.Storage;
using Windows.Storage.Streams;

namespace Goosic.Windows.Service;

/// <summary>
/// The Windows media overlay: the flyout over the volume keys, the lock screen, and the
/// keyboard's media keys.
/// </summary>
/// <remarks>
/// <para>
/// This is the Windows counterpart of Now Playing on macOS and MPRIS on Linux, and like them it
/// decides nothing. It shows what the page confirmed and forwards a button press to the same
/// handler the transport uses, so a key on the keyboard cannot ask for anything the window's own
/// button could not.
/// </para>
/// <para>
/// An unplayed <see cref="MediaPlayer"/> is used only as the owner of the transport controls. Its
/// command manager is switched off, so Windows does not wire the buttons to a player that has
/// nothing loaded. The page's own media session is neutralised by the guard script, which is
/// what keeps the overlay from showing two sessions for one track.
/// </para>
/// </remarks>
internal sealed class SystemMediaControls : IDisposable
{
    private readonly MediaPlayer _owner = new();
    private readonly SystemMediaTransportControls _controls;
    private readonly DispatcherQueue _dispatcher;
    private string? _videoId;
    private DateTime _lastTimeline = DateTime.MinValue;

    internal SystemMediaControls(DispatcherQueue dispatcher)
    {
        _dispatcher = dispatcher;
        _owner.CommandManager.IsEnabled = false;
        _controls = _owner.SystemMediaTransportControls;
        _controls.IsEnabled = true;
        _controls.IsPlayEnabled = true;
        _controls.IsPauseEnabled = true;
        _controls.IsNextEnabled = true;
        _controls.IsPreviousEnabled = true;
        _controls.PlaybackStatus = MediaPlaybackStatus.Closed;

        // Raised on a thread-pool thread; every consumer touches the view model.
        _controls.ButtonPressed += (_, args) =>
            _dispatcher.TryEnqueue(() => ButtonPressed?.Invoke(args.Button));
        _controls.PlaybackPositionChangeRequested += (_, args) =>
            _dispatcher.TryEnqueue(() => SeekRequested?.Invoke(args.RequestedPlaybackPosition.TotalSeconds));
    }

    internal event Action<SystemMediaTransportControlsButton>? ButtonPressed;

    internal event Action<double>? SeekRequested;

    /// <summary>Shows a newly confirmed track.</summary>
    internal async Task ShowTrackAsync(string videoId, string title, string artist, string? artworkFile)
    {
        _videoId = videoId;
        var updater = _controls.DisplayUpdater;
        updater.ClearAll();
        updater.Type = MediaPlaybackType.Music;
        updater.MusicProperties.Title = title;
        updater.MusicProperties.Artist = artist;
        if (artworkFile is not null)
        {
            try
            {
                var file = await StorageFile.GetFileFromPathAsync(artworkFile);
                if (_videoId != videoId)
                {
                    return;
                }

                updater.Thumbnail = RandomAccessStreamReference.CreateFromFile(file);
            }
            catch (Exception)
            {
                // A cover that cannot be read leaves the overlay without one, not without a title.
            }
        }

        updater.Update();
    }

    /// <summary>Reflects an accepted sample: state, and at most once a second, position.</summary>
    internal void ReportSample(BridgeEvent sample)
    {
        // Track changes are refused during an advertisement, so the overlay does not offer them;
        // a button that is shown and then refused reads as a broken button.
        var trackChangesAllowed = !sample.IsAdvertisement;
        if (_controls.IsNextEnabled != trackChangesAllowed)
        {
            _controls.IsNextEnabled = trackChangesAllowed;
            _controls.IsPreviousEnabled = trackChangesAllowed;
        }

        _controls.PlaybackStatus = sample.State switch
        {
            "playing" => MediaPlaybackStatus.Playing,
            "paused" => MediaPlaybackStatus.Paused,
            "ended" => MediaPlaybackStatus.Stopped,
            "buffering" or "loading" => MediaPlaybackStatus.Changing,
            _ => _controls.PlaybackStatus,
        };

        if (DateTime.UtcNow - _lastTimeline < TimeSpan.FromSeconds(1) || sample.Duration <= 0)
        {
            return;
        }

        _lastTimeline = DateTime.UtcNow;
        var end = TimeSpan.FromSeconds(sample.Duration);
        _controls.UpdateTimelineProperties(new SystemMediaTransportControlsTimelineProperties
        {
            StartTime = TimeSpan.Zero,
            MinSeekTime = TimeSpan.Zero,
            EndTime = end,
            MaxSeekTime = sample.IsAdvertisement ? TimeSpan.Zero : end,
            Position = TimeSpan.FromSeconds(Math.Clamp(sample.CurrentTime, 0, sample.Duration)),
        });
    }

    public void Dispose()
    {
        _controls.IsEnabled = false;
        _owner.Dispose();
    }
}
