using System;
using System.IO;
using System.Text;
using System.Threading;
using System.Threading.Channels;
using System.Threading.Tasks;

namespace Goosic.Windows.Service;

/// <summary>
/// A plain-text record of what the official player did, for diagnosing playback.
/// </summary>
/// <remarks>
/// Playback crosses four places that cannot see each other -- the shell, WebView2, the page and
/// Rust -- and a failure anywhere looks the same from the window: nothing plays. This writes one
/// line per step so the break can be read rather than guessed at.
///
/// It records navigation hosts and paths, bridge verdicts and page capability probes. It never
/// records cookies, headers, query strings or message bodies beyond a short prefix, because the
/// log sits in the user's profile and must not become the place credentials leak to.
///
/// Lines are written by one background task. The callers are almost all on the UI thread, and a
/// playing page reports several times a second: opening, appending and closing the file for each
/// line there was disk work in the middle of every frame the player drew.
/// </remarks>
internal static class BridgeLog
{
    private const long MaxBytes = 512 * 1024;

    private static readonly string LogPath = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "Goosic",
        "logs",
        "bridge.log");

    private static readonly Channel<string> Lines = Channel.CreateUnbounded<string>(
        new UnboundedChannelOptions { SingleReader = true });

    private static readonly object FileGate = new();
    private static long _pending;
    private static int _started;

    internal static string Location => LogPath;

    /// <summary>The file the log was moved to when it last filled, kept so a failure survives it.</summary>
    internal static string PreviousLocation => LogPath + ".1";

    internal static void Write(string message)
    {
        // Stamped here, not when it reaches the disk, so the order of events stays readable.
        Interlocked.Increment(ref _pending);
        Lines.Writer.TryWrite($"{DateTime.Now:HH:mm:ss.fff} {message}{Environment.NewLine}");
        if (Interlocked.Exchange(ref _started, 1) == 0)
        {
            _ = Task.Run(DrainAsync);
        }
    }

    /// <summary>
    /// Waits, briefly, until everything written so far is on disk.
    /// </summary>
    /// <remarks>
    /// For the moments just before the process may end: the line that says why is the one that
    /// must not be lost in the queue.
    /// </remarks>
    internal static void Flush()
    {
        var deadline = Environment.TickCount64 + 1000;
        while (Interlocked.Read(ref _pending) > 0 && Environment.TickCount64 < deadline)
        {
            Thread.Sleep(5);
        }
    }

    private static async Task DrainAsync()
    {
        var batch = new StringBuilder();
        while (await Lines.Reader.WaitToReadAsync().ConfigureAwait(false))
        {
            var count = 0;
            while (Lines.Reader.TryRead(out var line))
            {
                batch.Append(line);
                count++;
            }

            Append(batch.ToString());
            batch.Clear();
            Interlocked.Add(ref _pending, -count);
        }
    }

    private static void Append(string text)
    {
        try
        {
            lock (FileGate)
            {
                Directory.CreateDirectory(Path.GetDirectoryName(LogPath)!);

                // Bounded: a long listening session would otherwise grow this without limit. The
                // full file is kept as the previous one rather than deleted, because the evidence
                // of a failed play used to be wiped by a few minutes of the next song.
                if (File.Exists(LogPath) && new FileInfo(LogPath).Length > MaxBytes)
                {
                    File.Move(LogPath, PreviousLocation, overwrite: true);
                }

                File.AppendAllText(LogPath, text);
            }
        }
        catch (IOException)
        {
            // Diagnostics must never be the reason playback fails.
        }
        catch (UnauthorizedAccessException)
        {
        }
    }

    /// <summary>A URL reduced to scheme, host and path.</summary>
    internal static string Describe(string url) =>
        Uri.TryCreate(url, UriKind.Absolute, out var parsed)
            ? $"{parsed.Scheme}://{parsed.Host}{parsed.AbsolutePath}"
            : "(unparseable url)";
}
