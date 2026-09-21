using System;
using System.IO;

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
/// </remarks>
internal static class BridgeLog
{
    private const long MaxBytes = 512 * 1024;
    private static readonly object Gate = new();

    private static readonly string LogPath = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "Goosic",
        "logs",
        "bridge.log");

    internal static string Location => LogPath;

    internal static void Write(string message)
    {
        try
        {
            lock (Gate)
            {
                Directory.CreateDirectory(Path.GetDirectoryName(LogPath)!);

                // Bounded: a page that reports several times a second would otherwise grow this
                // without limit across a long listening session.
                if (File.Exists(LogPath) && new FileInfo(LogPath).Length > MaxBytes)
                {
                    File.Delete(LogPath);
                }

                File.AppendAllText(LogPath, $"{DateTime.Now:HH:mm:ss.fff} {message}{Environment.NewLine}");
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
