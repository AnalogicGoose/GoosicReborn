using System;
using System.Text.Json.Nodes;
using System.Threading;
using System.Threading.Tasks;
using Goosic.Windows.ViewModels;
using Microsoft.UI.Dispatching;
using Windows.Media.Core;
using Windows.Media.Playback;
using Windows.Storage;

namespace Goosic.Windows.Service;

/// <summary>The native renderer for decoded files returned by <c>downloads.prepare</c>.</summary>
/// <remarks>
/// This host never opens a legacy WebM or reaches the network. Rust must grant the
/// <c>localDownloadedFile</c> lease before the decoded WAV is assigned to MediaPlayer, and every
/// state sample is reported back under that lease's generation and a strictly increasing sequence.
/// </remarks>
internal sealed class LocalPlaybackHost : IDisposable
{
    private readonly GoosicServiceClient _client;
    private readonly MediaPlayer _player = new();
    private readonly DispatcherQueueTimer _timer;
    private readonly SemaphoreSlim _playGate = new(1, 1);
    private string _videoId = "";
    private ulong _generation;
    private ulong _sequence;
    private long _request;
    private bool _ended;

    internal LocalPlaybackHost(GoosicServiceClient client, DispatcherQueue dispatcher)
    {
        _client = client;
        _player.CommandManager.IsEnabled = false;
        _timer = dispatcher.CreateTimer();
        _timer.Interval = TimeSpan.FromMilliseconds(250);
        _timer.Tick += (_, _) => Emit(CurrentState());
        _player.MediaEnded += (_, _) => dispatcher.TryEnqueue(() =>
        {
            if (_ended)
            {
                return;
            }

            _ended = true;
            _timer.Stop();
            Emit("ended");
        });
        _player.MediaFailed += (_, args) => dispatcher.TryEnqueue(() =>
        {
            _timer.Stop();
            Status?.Invoke("Windows could not play the decoded file: " + args.ErrorMessage);
        });
    }

    internal event Action<BridgeEvent>? Sampled;
    internal event Action<string>? Status;

    internal bool HasLoadedTrack => _player.Source is not null && _videoId.Length > 0;
    internal bool PreferredMuted => _player.IsMuted;

    internal async Task PlayAsync(DownloadedTrackViewModel track)
    {
        var request = Interlocked.Increment(ref _request);
        await _playGate.WaitAsync().ConfigureAwait(true);
        try
        {
            if (request != _request || !track.Available)
            {
                return;
            }

            Stop();
            var state = await _client.RequestAsync("state.get").ConfigureAwait(true);
            var owner = state?["state"]?["owner"]?.GetValue<string>() ?? "none";
            var generation = state?["state"]?["generation"]?.GetValue<ulong>() ?? 0;
            if (owner != "none")
            {
                var released = await _client.RequestAsync("playback.release", new JsonObject
                {
                    ["owner"] = owner,
                    ["generation"] = generation,
                }).ConfigureAwait(true);
                generation = released?["state"]?["generation"]?.GetValue<ulong>() ?? generation + 1;
            }

            var claim = await _client.RequestAsync("playback.claim", new JsonObject
            {
                ["owner"] = "localDownloadedFile",
                ["generation"] = generation,
            }).ConfigureAwait(true);
            _generation = claim?["state"]?["generation"]?.GetValue<ulong>() ?? generation + 1;

            var prepared = await _client.RequestAsync("downloads.prepare", new JsonObject
            {
                ["owner"] = "localDownloadedFile",
                ["generation"] = _generation,
                ["catalogId"] = track.VideoId,
            }).ConfigureAwait(true);
            var path = prepared?["localFile"]?.GetValue<string>();
            if (string.IsNullOrWhiteSpace(path) || !System.IO.File.Exists(path))
            {
                Status?.Invoke("Rust returned a decoded audio path that is not present on disk.");
                return;
            }

            _videoId = track.VideoId;
            _sequence = 0;
            _ended = false;
            var file = await StorageFile.GetFileFromPathAsync(path);
            _player.Source = MediaSource.CreateFromStorageFile(file);
            _player.Play();
            _timer.Start();
            Emit(CurrentState());
        }
        catch (ServiceRefusedException refused)
        {
            Status?.Invoke("Rust refused local playback: " + refused.Message);
        }
        catch (Exception error)
        {
            Status?.Invoke("Could not play the downloaded file: " + error.Message);
        }
        finally
        {
            _playGate.Release();
        }
    }

    internal void Stop()
    {
        _timer.Stop();
        _player.Pause();
        _player.Source = null;
        _videoId = "";
        _ended = false;
    }

    internal Task TogglePauseAsync()
    {
        if (!HasLoadedTrack)
        {
            Status?.Invoke("Choose a track to begin.");
        }
        else if (_player.PlaybackSession.PlaybackState == MediaPlaybackState.Playing)
        {
            _player.Pause();
            _timer.Stop();
            Emit("paused");
        }
        else
        {
            _player.Play();
            _timer.Start();
            Emit("playing");
        }

        return Task.CompletedTask;
    }

    internal Task SeekAsync(double seconds)
    {
        if (HasLoadedTrack && double.IsFinite(seconds) && seconds >= 0)
        {
            var duration = _player.PlaybackSession.NaturalDuration.TotalSeconds;
            _player.PlaybackSession.Position = TimeSpan.FromSeconds(duration > 0 ? Math.Min(seconds, duration) : seconds);
            Emit(CurrentState());
        }

        return Task.CompletedTask;
    }

    internal Task SetVolumeAsync(double volume)
    {
        if (double.IsFinite(volume))
        {
            _player.Volume = Math.Clamp(volume, 0, 1);
            if (_player.Volume > 0)
            {
                _player.IsMuted = false;
            }
            Emit(CurrentState());
        }

        return Task.CompletedTask;
    }

    internal Task ToggleMutedAsync()
    {
        _player.IsMuted = !_player.IsMuted;
        Emit(CurrentState());
        return Task.CompletedTask;
    }

    internal void RestoreVolume(double volume, bool muted)
    {
        if (double.IsFinite(volume))
        {
            _player.Volume = Math.Clamp(volume, 0, 1);
            _player.IsMuted = muted;
        }
    }

    private string CurrentState() => _player.PlaybackSession.PlaybackState switch
    {
        MediaPlaybackState.Playing => "playing",
        MediaPlaybackState.Buffering or MediaPlaybackState.Opening => "buffering",
        _ => "paused",
    };

    private void Emit(string state)
    {
        if (!HasLoadedTrack)
        {
            return;
        }

        var sample = new BridgeEvent
        {
            VideoId = _videoId,
            Sequence = ++_sequence,
            State = state,
            CurrentTime = Math.Max(0, _player.PlaybackSession.Position.TotalSeconds),
            Duration = Math.Max(0, _player.PlaybackSession.NaturalDuration.TotalSeconds),
            IsAdvertisement = false,
            Volume = _player.Volume,
            Muted = _player.IsMuted,
        };
        Sampled?.Invoke(sample);
        _ = ReportSampleAsync(sample);
    }

    private async Task ReportSampleAsync(BridgeEvent sample)
    {
        try
        {
            await _client.RequestAsync("playback.sample", new JsonObject
            {
                ["owner"] = "localDownloadedFile",
                ["generation"] = _generation,
                ["sequence"] = sample.Sequence,
            }).ConfigureAwait(true);
        }
        catch (ServiceRefusedException refused)
        {
            Status?.Invoke("Rust rejected a local playback sample: " + refused.Message);
        }
        catch (ServiceUnavailableException)
        {
            // The transport reports the lost service once; a periodic sample adds nothing.
        }
    }

    public void Dispose()
    {
        Stop();
        _player.Dispose();
        _playGate.Dispose();
    }
}
