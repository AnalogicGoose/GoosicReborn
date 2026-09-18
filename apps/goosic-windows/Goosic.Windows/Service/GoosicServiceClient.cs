using System;
using System.Collections.Concurrent;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Threading;
using System.Threading.Tasks;

namespace Goosic.Windows.Service;

/// <summary>
/// The private conversation with one <c>goosic-service</c> child process.
/// </summary>
/// <remarks>
/// The service is a single-client child reached through inherited stdio -- not a daemon, not a
/// socket, never shared. Its stdout carries one response per line and nothing else; anything it
/// wants to say to a human goes to stderr, which is why stderr is drained separately and never
/// parsed.
///
/// Requests are correlated by id because the service answers out of order: a catalog browse
/// waiting on a third-party host must not hold pause and seek behind it. That is the whole
/// reason this is a table of pending requests rather than a queue.
/// </remarks>
internal sealed class GoosicServiceClient : IAsyncDisposable
{
    private readonly Process? _process;
    private readonly StreamWriter? _input;
    private readonly FrameReader _frames = new();
    private readonly ConcurrentDictionary<string, TaskCompletionSource<ResponseEnvelope>> _pending = new();
    private readonly SemaphoreSlim _writeLock = new(1, 1);
    private readonly CancellationTokenSource _shutdown = new();
    private long _requestNumber;
    private volatile string? _closedReason;

    private GoosicServiceClient(Process? process, string? closedReason = null)
    {
        _process = process;
        _input = process?.StandardInput;
        _closedReason = closedReason;
    }

    /// <summary>
    /// A client that never had a service to talk to, and says so to everything asked of it.
    /// </summary>
    /// <remarks>
    /// Lets the window open and explain itself when the service binary is missing. The
    /// alternative -- no window, or a window bound to nothing -- shows an empty screen that
    /// reads as an empty catalog rather than as a shell that could not start its backend.
    /// </remarks>
    internal static GoosicServiceClient Unavailable(string reason) => new(null, reason);

    /// <summary>Why the conversation ended, or <c>null</c> while it is still going.</summary>
    internal string? ClosedReason => _closedReason;

    /// <summary>Starts the service and begins reading its answers.</summary>
    /// <param name="executablePath">
    /// The service binary. Always a full path: the shell ships the service beside itself, and
    /// resolving a bare name through <c>PATH</c> would let an unrelated program answer for the
    /// playback authority.
    /// </param>
    internal static GoosicServiceClient Start(string executablePath)
    {
        if (!File.Exists(executablePath))
        {
            throw new ServiceUnavailableException(
                $"goosic-service is not where the shell expects it: {executablePath}");
        }

        var info = new ProcessStartInfo(executablePath)
        {
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
            // The protocol is UTF-8. Without saying so these streams use the console's code
            // page, which decodes every non-ASCII byte wrongly -- an artist called Fernandez
            // arrives spelled with the bytes of its accent read as two Latin-1 characters.
            // Encoded without a BOM, because a byte-order mark inside a frame is not JSON.
            StandardOutputEncoding = new UTF8Encoding(encoderShouldEmitUTF8Identifier: false),
            StandardErrorEncoding = new UTF8Encoding(encoderShouldEmitUTF8Identifier: false),
            StandardInputEncoding = new UTF8Encoding(encoderShouldEmitUTF8Identifier: false),
        };

        // The service finds accounts and settings through APPDATA. A shell started from a
        // terminal without it would otherwise open a service that sees no accounts at all.
        if (string.IsNullOrEmpty(info.Environment["APPDATA"]))
        {
            info.Environment["APPDATA"] = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
        }

        Process process;
        try
        {
            process = Process.Start(info)
                ?? throw new ServiceUnavailableException("goosic-service did not start");
        }
        catch (Exception error) when (error is not ServiceUnavailableException)
        {
            throw new ServiceUnavailableException($"could not launch goosic-service: {error.Message}", error);
        }

        var client = new GoosicServiceClient(process);
        _ = client.ReadAnswersAsync();
        _ = client.DrainDiagnosticsAsync();
        return client;
    }

    /// <summary>Asks the service something and waits for the answer to that question.</summary>
    /// <exception cref="ServiceUnavailableException">The service died, or did not answer in time.</exception>
    /// <exception cref="ServiceRefusedException">The service answered and refused.</exception>
    internal async Task<JsonNode?> RequestAsync(
        string command,
        JsonObject? payload = null,
        CancellationToken cancellationToken = default)
    {
        if (_closedReason is { } closed)
        {
            throw new ServiceUnavailableException(closed);
        }

        var id = $"win-{Interlocked.Increment(ref _requestNumber)}";
        var envelope = new RequestEnvelope(ServiceProtocol.Version, id, command, payload ?? new JsonObject());
        var completion = new TaskCompletionSource<ResponseEnvelope>(TaskCreationOptions.RunContinuationsAsynchronously);
        _pending[id] = completion;

        try
        {
            await WriteAsync(envelope, cancellationToken).ConfigureAwait(false);

            using var deadline = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken, _shutdown.Token);
            deadline.CancelAfter(ServiceProtocol.TimeoutFor(command));

            ResponseEnvelope response;
            try
            {
                response = await completion.Task.WaitAsync(deadline.Token).ConfigureAwait(false);
            }
            catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
            {
                // A timeout is the absence of a response, so there is nothing to read and the
                // request is abandoned rather than retried. The transport stays up: a late
                // answer arrives for an id nobody is waiting on and is dropped, which is why
                // one slow catalog read must not take playback down with it.
                throw new ServiceUnavailableException($"goosic-service did not answer {command} in time");
            }

            if (!response.Ok)
            {
                var error = response.Error;
                throw new ServiceRefusedException(
                    error?.Code ?? "unknown",
                    error?.Message ?? "the service refused the request without saying why");
            }

            return response.Payload;
        }
        finally
        {
            _pending.TryRemove(id, out _);
        }
    }

    private async Task WriteAsync(RequestEnvelope envelope, CancellationToken cancellationToken)
    {
        if (_input is not { } input)
        {
            throw new ServiceUnavailableException(_closedReason ?? "goosic-service is not running");
        }

        var line = JsonSerializer.Serialize(envelope, ServiceProtocol.Json);
        await _writeLock.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            // One request per line, and the newline is what makes it a request rather than a
            // partial buffer the service is still waiting to complete.
            await input.WriteAsync(line.AsMemory(), cancellationToken).ConfigureAwait(false);
            await input.WriteAsync("\n".AsMemory(), cancellationToken).ConfigureAwait(false);
            await input.FlushAsync(cancellationToken).ConfigureAwait(false);
        }
        catch (Exception error)
        {
            Close($"could not reach goosic-service: {error.Message}");
            throw new ServiceUnavailableException($"could not reach goosic-service: {error.Message}", error);
        }
        finally
        {
            _writeLock.Release();
        }
    }

    private async Task ReadAnswersAsync()
    {
        if (_process is not { } process)
        {
            return;
        }

        var buffer = new char[8192];
        try
        {
            while (true)
            {
                var read = await process.StandardOutput.ReadAsync(buffer, _shutdown.Token).ConfigureAwait(false);
                if (read == 0)
                {
                    Close("goosic-service closed its output");
                    return;
                }

                foreach (var frame in _frames.Append(new string(buffer, 0, read)))
                {
                    Deliver(frame);
                }
            }
        }
        catch (OperationCanceledException)
        {
            // The shell is shutting down; nothing is waiting on an answer any more.
        }
        catch (Exception error)
        {
            Close($"goosic-service stopped answering: {error.Message}");
        }
    }

    private void Deliver(string frame)
    {
        ResponseEnvelope? response;
        try
        {
            response = JsonSerializer.Deserialize<ResponseEnvelope>(frame, ServiceProtocol.Json);
        }
        catch (JsonException)
        {
            // A frame that is not a response means the stream is no longer the protocol. There
            // is no way to tell which request it belonged to, so the conversation ends rather
            // than continuing against a stream nobody can interpret.
            Close("goosic-service sent something that is not a protocol response");
            return;
        }

        if (response is null || response.ProtocolVersion != ServiceProtocol.Version)
        {
            Close($"goosic-service speaks a protocol this shell does not: expected {ServiceProtocol.Version}");
            return;
        }

        // A response whose id nobody is waiting for is a late answer to an abandoned request.
        // Dropping it is correct; it is not an error and must not disturb the transport.
        if (_pending.TryRemove(response.RequestId, out var completion))
        {
            completion.TrySetResult(response);
        }
    }

    /// <summary>
    /// Reads stderr so the child never blocks on a full pipe.
    /// </summary>
    /// <remarks>
    /// Diagnostics are deliberately not parsed. The protocol lives on stdout, and treating
    /// stderr as data is how a diagnostic line becomes a command.
    /// </remarks>
    private async Task DrainDiagnosticsAsync()
    {
        if (_process is not { } process)
        {
            return;
        }

        try
        {
            while (await process.StandardError.ReadLineAsync(_shutdown.Token).ConfigureAwait(false) is { } line)
            {
                Debug.WriteLine($"[goosic-service] {line}");
            }
        }
        catch (OperationCanceledException)
        {
        }
        catch (Exception)
        {
            // Losing diagnostics is not worth ending the conversation over.
        }
    }

    private void Close(string reason)
    {
        _closedReason ??= reason;
        foreach (var id in _pending.Keys)
        {
            if (_pending.TryRemove(id, out var completion))
            {
                completion.TrySetException(new ServiceUnavailableException(reason));
            }
        }
    }

    public async ValueTask DisposeAsync()
    {
        Close("the shell closed the service");
        await _shutdown.CancelAsync().ConfigureAwait(false);

        if (_process is null || _input is null)
        {
            _shutdown.Dispose();
            _writeLock.Dispose();
            return;
        }

        try
        {
            // Closing stdin is how the service is asked to stop: it reads to end-of-input and
            // exits on its own, which lets it finish whatever it was writing.
            _input.Close();
            if (!_process.WaitForExit(2000))
            {
                _process.Kill(entireProcessTree: true);
            }
        }
        catch (Exception)
        {
            // The child is already gone, which is the outcome this was asking for.
        }

        _process.Dispose();
        _shutdown.Dispose();
        _writeLock.Dispose();
    }
}
