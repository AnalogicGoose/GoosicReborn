using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Net.Http;
using System.Threading;
using System.Threading.Tasks;

namespace Goosic.Windows.Service;

/// <summary>
/// Fetches and caches catalog artwork.
/// </summary>
/// <remarks>
/// Fetching and caching are platform work -- each shell has its own HTTP stack and cache
/// directory. The two decisions that are not are mirrored from
/// <c>goosic_shell_support::artwork</c>: which hosts a catalog response may point the shell at,
/// and how a remote URL becomes a file name that cannot escape its directory.
///
/// Artwork is public CDN content, so this must stay anonymous: no cookies, no account headers,
/// exactly like a catalog read. The handler is constructed with cookies disabled rather than
/// merely not setting any, so nothing can acquire them later.
/// </remarks>
internal sealed class ArtworkLoader
{
    /// <summary>Hosts YouTube Music serves artwork from, as <c>goosic_shell_support::artwork</c> lists them.</summary>
    /// <remarks><c>gstatic.com</c> holds the art of YouTube Music's own playlists, such as Liked Music.</remarks>
    private static readonly string[] AllowedHostSuffixes =
        ["googleusercontent.com", "ggpht.com", "ytimg.com", "youtube.com", "gstatic.com"];

    /// <summary>Artwork is small. Anything larger is not a thumbnail and is discarded.</summary>
    private const int MaxBytes = 4 * 1024 * 1024;

    /// <summary>So opening a dense screen cannot start hundreds of connections at once.</summary>
    private const int MaxConcurrentFetches = 6;

    private readonly HttpClient _http;
    private readonly SemaphoreSlim _slots = new(MaxConcurrentFetches, MaxConcurrentFetches);
    private readonly string _directory;
    private readonly ConcurrentDictionary<string, Task<string?>> _inFlight = new();
    private readonly HashSet<string> _onDisk = [];

    internal ArtworkLoader()
    {
        _directory = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "Goosic",
            "artwork");
        Directory.CreateDirectory(_directory);
        foreach (var file in Directory.EnumerateFiles(_directory, "*.img"))
        {
            _onDisk.Add(Path.GetFileName(file));
        }

        var handler = new HttpClientHandler
        {
            UseCookies = false,
            UseDefaultCredentials = false,
            AllowAutoRedirect = true,
        };
        _http = new HttpClient(handler) { Timeout = TimeSpan.FromSeconds(15) };
    }

    /// <summary>
    /// Whether this build is willing to fetch <paramref name="remote"/> at all.
    /// </summary>
    /// <remarks>
    /// The suffix is matched on a label boundary, so <c>evilgoogleusercontent.com</c> is not
    /// <c>googleusercontent.com</c>.
    /// </remarks>
    internal static bool IsAllowed(string remote)
    {
        if (!Uri.TryCreate(remote, UriKind.Absolute, out var url))
        {
            return false;
        }

        if (url.Scheme != Uri.UriSchemeHttps)
        {
            return false;
        }

        var host = url.Host.ToLowerInvariant();
        return AllowedHostSuffixes.Any(suffix =>
            host == suffix || host.EndsWith("." + suffix, StringComparison.Ordinal));
    }

    /// <summary>
    /// A stable, collision-resistant file name for a remote URL.
    /// </summary>
    /// <remarks>
    /// Two independent FNV-1a passes give 128 bits, the second over the reversed bytes so a
    /// transposition changes the key. The result is 32 hex digits and nothing else, so a path
    /// built from it cannot climb out of the cache directory whatever the URL contained. Kept
    /// identical to the Rust implementation so both name the same file.
    /// </remarks>
    internal static string CacheKey(string remote)
    {
        static ulong Fnv1a(IEnumerable<byte> bytes, ulong seed)
        {
            var hash = seed;
            foreach (var b in bytes)
            {
                hash = unchecked((hash ^ b) * 0x100_0000_01b3UL);
            }

            return hash;
        }

        var bytes = System.Text.Encoding.UTF8.GetBytes(remote);
        var low = Fnv1a(bytes, 0xcbf2_9ce4_8422_2325UL);
        var high = Fnv1a(((IEnumerable<byte>)bytes).Reverse(), 0x9dc5_bb15_8f2c_1e37UL);
        return low.ToString("x16", CultureInfo.InvariantCulture)
            + high.ToString("x16", CultureInfo.InvariantCulture);
    }

    /// <summary>
    /// The local file for <paramref name="remote"/>, fetching it if this is the first ask.
    /// </summary>
    /// <returns>The path, or <c>null</c> when the URL is refused or the fetch failed.</returns>
    internal Task<string?> LocalFileAsync(string? remote)
    {
        if (string.IsNullOrWhiteSpace(remote) || !IsAllowed(remote))
        {
            // A refused host leaves a blank card; saying so is how the next one is found.
            if (!string.IsNullOrWhiteSpace(remote))
            {
                BridgeLog.Write($"artwork refused: {BridgeLog.Describe(remote)}");
            }

            return Task.FromResult<string?>(null);
        }

        var name = CacheKey(remote) + ".img";
        var destination = Path.Combine(_directory, name);
        lock (_onDisk)
        {
            if (_onDisk.Contains(name))
            {
                return Task.FromResult<string?>(destination);
            }
        }

        // One fetch per URL however many cards ask for it: a shelf commonly repeats artwork, and
        // a dense screen would otherwise open the same connection several times over.
        return _inFlight.GetOrAdd(remote, key => FetchAsync(key, name, destination));
    }

    private async Task<string?> FetchAsync(string remote, string name, string destination)
    {
        await _slots.WaitAsync().ConfigureAwait(false);
        try
        {
            using var response = await _http.GetAsync(remote).ConfigureAwait(false);
            if (!response.IsSuccessStatusCode)
            {
                return null;
            }

            var bytes = await response.Content.ReadAsByteArrayAsync().ConfigureAwait(false);
            if (bytes.Length == 0 || bytes.Length > MaxBytes)
            {
                return null;
            }

            // Written beside the destination and moved onto it, so a half-written file can never
            // be picked up as a valid cache entry by a later read.
            var partial = destination + ".partial";
            await File.WriteAllBytesAsync(partial, bytes).ConfigureAwait(false);
            File.Move(partial, destination, overwrite: true);

            lock (_onDisk)
            {
                _onDisk.Add(name);
            }

            return destination;
        }
        catch (Exception)
        {
            // Artwork is decoration. A failure leaves the placeholder in place.
            return null;
        }
        finally
        {
            _slots.Release();
            _inFlight.TryRemove(remote, out _);
        }
    }
}
