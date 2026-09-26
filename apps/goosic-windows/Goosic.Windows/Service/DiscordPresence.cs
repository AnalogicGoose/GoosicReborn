using System;
using System.Buffers.Binary;
using System.IO;
using System.IO.Pipes;
using System.Text;
using System.Text.Json.Nodes;
using System.Threading;
using System.Threading.Tasks;

namespace Goosic.Windows.Service;

/// <summary>What Discord shows while a song plays.</summary>
internal sealed record DiscordActivity(string Title, string Artist, string? Album, string? ArtworkUrl,
    DateTimeOffset? Started, DateTimeOffset? Ends);

/// <summary>
/// Shows the playing song on the listener's Discord profile, through the Discord app's local IPC
/// pipe (<c>\\.\pipe\discord-ipc-N</c>).
/// </summary>
/// <remarks>
/// <para>
/// Nothing leaves this machine from here: the Discord desktop app, already signed in, publishes
/// the status. No token, cookie, or account detail is read or sent — only the song's title,
/// artist, album, cover address and times, and only while the listener has this turned on.
/// </para>
/// <para>
/// Discord identifies a program by an application id registered on its developer portal; the
/// name shown ("Listening to Goosic") is that application's name. Until one is filled in below
/// the feature reports itself unavailable instead of connecting.
/// </para>
/// <para>
/// Updates are coalesced: the latest one wins and is sent at most every few seconds, because
/// Discord ignores a client that changes its status faster than about five times in twenty.
/// </para>
/// </remarks>
internal sealed class DiscordPresence : IDisposable
{
    /// <summary>The Goosic application on Discord's developer portal. Empty until it is registered.</summary>
    internal const string ApplicationId = "";

    internal static bool Available => ApplicationId.Length > 0;

    private static readonly TimeSpan MinimumInterval = TimeSpan.FromSeconds(4);
    private readonly SemaphoreSlim _wake = new(0);
    private readonly CancellationTokenSource _stop = new();
    private readonly object _gate = new();
    private (bool Pending, DiscordActivity? Activity) _latest;
    private NamedPipeClientStream? _pipe;

    internal DiscordPresence()
    {
        if (Available)
        {
            _ = Task.Run(RunAsync);
        }
    }

    /// <summary>Shows a song, or clears the status when <paramref name="activity"/> is null.</summary>
    internal void Update(DiscordActivity? activity)
    {
        lock (_gate)
        {
            _latest = (true, activity);
        }

        _wake.Release();
    }

    private async Task RunAsync()
    {
        var cancel = _stop.Token;
        while (!cancel.IsCancellationRequested)
        {
            try
            {
                await _wake.WaitAsync(cancel).ConfigureAwait(false);
                // Let a burst of changes (skip, skip, skip) settle into the one that stays.
                await Task.Delay(MinimumInterval, cancel).ConfigureAwait(false);
                DiscordActivity? activity;
                lock (_gate)
                {
                    if (!_latest.Pending)
                    {
                        continue;
                    }

                    activity = _latest.Activity;
                    _latest = (false, null);
                }

                while (_wake.CurrentCount > 0)
                {
                    await _wake.WaitAsync(cancel).ConfigureAwait(false);
                }

                await SendAsync(activity, cancel).ConfigureAwait(false);
            }
            catch (OperationCanceledException)
            {
                return;
            }
            catch (Exception error) when (error is IOException or TimeoutException or InvalidDataException)
            {
                // Discord is closed or restarted; the next update reconnects.
                BridgeLog.Write($"discord status not sent: {error.GetType().Name}");
                _pipe?.Dispose();
                _pipe = null;
            }
        }
    }

    private async Task SendAsync(DiscordActivity? activity, CancellationToken cancel)
    {
        var pipe = _pipe ?? await ConnectAsync(cancel).ConfigureAwait(false);
        if (pipe is null)
        {
            return;
        }

        var payload = new JsonObject
        {
            ["cmd"] = "SET_ACTIVITY",
            ["nonce"] = Guid.NewGuid().ToString(),
            ["args"] = new JsonObject
            {
                ["pid"] = Environment.ProcessId,
                ["activity"] = activity is null ? null : Describe(activity),
            },
        };
        await WriteFrameAsync(pipe, 1, payload, cancel).ConfigureAwait(false);
        await ReadFrameAsync(pipe, cancel).ConfigureAwait(false);
    }

    private static JsonObject Describe(DiscordActivity activity)
    {
        var description = new JsonObject
        {
            // 2 is "Listening to", the way music players appear.
            ["type"] = 2,
            ["details"] = Field(activity.Title),
            ["state"] = Field(activity.Artist.Length > 0 ? activity.Artist : "Goosic"),
        };
        if (activity.Started is { } started)
        {
            var timestamps = new JsonObject { ["start"] = started.ToUnixTimeMilliseconds() };
            if (activity.Ends is { } ends)
            {
                timestamps["end"] = ends.ToUnixTimeMilliseconds();
            }

            description["timestamps"] = timestamps;
        }

        if (activity.ArtworkUrl is { } artwork && artwork.StartsWith("https://", StringComparison.Ordinal))
        {
            description["assets"] = new JsonObject
            {
                ["large_image"] = artwork,
                ["large_text"] = Field(activity.Album ?? activity.Title),
            };
        }

        return description;
    }

    /// <summary>Discord refuses a text field shorter than two or longer than 128 characters.</summary>
    private static string Field(string text)
    {
        text = text.Trim();
        if (text.Length > 128)
        {
            text = text[..127] + "…";
        }

        return text.Length < 2 ? text.PadRight(2, '⠀') : text;
    }

    private async Task<NamedPipeClientStream?> ConnectAsync(CancellationToken cancel)
    {
        for (var index = 0; index < 10; index++)
        {
            var pipe = new NamedPipeClientStream(".", $"discord-ipc-{index}", PipeDirection.InOut, PipeOptions.Asynchronous);
            try
            {
                await pipe.ConnectAsync(200, cancel).ConfigureAwait(false);
                await WriteFrameAsync(pipe, 0, new JsonObject { ["v"] = 1, ["client_id"] = ApplicationId }, cancel)
                    .ConfigureAwait(false);
                await ReadFrameAsync(pipe, cancel).ConfigureAwait(false);
                _pipe = pipe;
                return pipe;
            }
            catch (Exception error) when (error is IOException or TimeoutException or InvalidDataException)
            {
                pipe.Dispose();
            }
        }

        return null;
    }

    private static async Task WriteFrameAsync(Stream pipe, int opcode, JsonObject body, CancellationToken cancel)
    {
        var json = Encoding.UTF8.GetBytes(body.ToJsonString());
        var frame = new byte[8 + json.Length];
        BinaryPrimitives.WriteInt32LittleEndian(frame, opcode);
        BinaryPrimitives.WriteInt32LittleEndian(frame.AsSpan(4), json.Length);
        json.CopyTo(frame, 8);
        await pipe.WriteAsync(frame, cancel).ConfigureAwait(false);
        await pipe.FlushAsync(cancel).ConfigureAwait(false);
    }

    private static async Task ReadFrameAsync(Stream pipe, CancellationToken cancel)
    {
        var header = new byte[8];
        await pipe.ReadExactlyAsync(header, cancel).ConfigureAwait(false);
        var length = BinaryPrimitives.ReadInt32LittleEndian(header.AsSpan(4));
        if (length is < 0 or > 64 * 1024)
        {
            throw new InvalidDataException("Discord sent an implausible frame.");
        }

        await pipe.ReadExactlyAsync(new byte[length], cancel).ConfigureAwait(false);
    }

    public void Dispose()
    {
        _stop.Cancel();
        _pipe?.Dispose();
    }
}
