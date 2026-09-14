using System;
using System.Collections.Generic;
using System.Text;

namespace Goosic.Windows.Service;

/// <summary>
/// Finds whole NDJSON frames in a stream that arrives in whatever sizes the pipe felt like.
/// </summary>
/// <remarks>
/// A pipe read is not a message. One read can carry half a frame, three frames, or a single
/// byte, so the reader keeps what it has not yet been able to terminate and returns only the
/// lines it can see the end of. This mirrors <c>goosic-shell-support</c>'s <c>FrameReader</c>,
/// whose behaviour the protocol fixtures pin down.
///
/// The size limit is the other half of the job: a desynchronised pipe does not usually announce
/// itself, it just never sends the newline the reader is waiting for, and a buffer that grows
/// without bound turns that into an out-of-memory crash instead of an error anyone can read.
/// </remarks>
internal sealed class FrameReader
{
    /// <summary>The largest frame accepted before the stream is treated as desynchronised.</summary>
    /// <remarks>Matches the Rust reader's limit, so both refuse the same stream.</remarks>
    internal const int MaxFrameBytes = 8 * 1024 * 1024;

    private readonly StringBuilder _pending = new();

    /// <summary>Adds what a read produced and returns every frame now complete.</summary>
    /// <exception cref="ServiceUnavailableException">The stream grew past <see cref="MaxFrameBytes"/>.</exception>
    internal IReadOnlyList<string> Append(string chunk)
    {
        _pending.Append(chunk);
        if (_pending.Length > MaxFrameBytes)
        {
            _pending.Clear();
            throw new ServiceUnavailableException(
                "the service sent a frame larger than the protocol allows; the stream is out of step");
        }

        var frames = new List<string>();
        var buffered = _pending.ToString();
        var start = 0;
        while (true)
        {
            var newline = buffered.IndexOf('\n', start);
            if (newline < 0)
            {
                break;
            }

            // A frame is the bytes before the newline. Trailing '\r' is tolerated so a stream
            // that has been through a Windows text layer somewhere is still readable.
            var frame = buffered[start..newline].TrimEnd('\r');
            if (frame.Length > 0)
            {
                frames.Add(frame);
            }

            start = newline + 1;
        }

        _pending.Clear();
        _pending.Append(buffered[start..]);
        return frames;
    }
}
