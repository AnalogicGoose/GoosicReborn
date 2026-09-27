using System;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Net.Http;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading;
using System.Threading.Tasks;
using Goosic.Windows.Presentation;

namespace Goosic.Windows.Service;

/// <summary>A release newer than this build, and the verified way to install it.</summary>
internal sealed record AvailableUpdate(Version Version, string SetupName, string SetupUrl, string SumsUrl, string PageUrl);

/// <summary>
/// Finds, downloads, verifies, and starts the Setup of a newer Goosic from GitHub releases.
/// </summary>
/// <remarks>
/// <para>
/// This belongs to the Windows shell, not to the service: an update replaces the shell's own
/// files through the Setup program this platform ships, and nothing about it is a catalog read or
/// a playback decision. It sends no cookie, account header, or credential; the only requests are
/// anonymous reads of the public release listing and its files.
/// </para>
/// <para>
/// Setup is run only if its SHA-256 matches the release's <c>SHA256SUMS.txt</c>, and only when it
/// was downloaded from this repository's releases. That checks the file arrived intact; the
/// installer is unsigned, so it cannot vouch for who published the release.
/// </para>
/// </remarks>
internal static class AppUpdater
{
    private const string LatestRelease = "https://api.github.com/repos/AnalogicGoose/GoosicReborn/releases/latest";

    private static readonly HttpClient Http = CreateClient();

    /// <summary>
    /// This build's version, from the value Setup packaging stamps into it; null for a
    /// development build, which the project marks as 0.0.0.
    /// </summary>
    internal static Version? CurrentVersion { get; } = UpdateRules.ParseVersion(
        Assembly.GetEntryAssembly()?.GetCustomAttribute<AssemblyInformationalVersionAttribute>()?.InformationalVersion)
        is { } stamped && stamped > new Version(0, 0, 0) ? stamped : null;

    internal static string CurrentVersionText =>
        CurrentVersion is { } version ? UpdateRules.Format(version) : "development build";

    /// <summary>
    /// Whether this copy was installed by Setup, which is the only kind Setup can update. The
    /// portable ZIP and development builds have no uninstaller beside them.
    /// </summary>
    internal static bool CanInstall =>
        CurrentVersion is not null && File.Exists(Path.Combine(AppContext.BaseDirectory, "unins000.exe"));

    /// <summary>The newest release if it is newer than this build, else null.</summary>
    internal static async Task<AvailableUpdate?> CheckAsync(CancellationToken cancel)
    {
        using var response = await Http.GetAsync(LatestRelease, cancel).ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
        var release = await JsonSerializer.DeserializeAsync<Release>(
            await response.Content.ReadAsStreamAsync(cancel).ConfigureAwait(false), cancellationToken: cancel)
            .ConfigureAwait(false);
        var version = UpdateRules.ParseVersion(release?.Tag);
        if (release is null || release.Draft || release.Prerelease || !UpdateRules.IsNewer(CurrentVersion, version))
        {
            return null;
        }

        var architecture = RuntimeInformation.ProcessArchitecture == Architecture.X64 ? "x64" : "other";
        var setupName = UpdateRules.SetupAssetName(version!, architecture);
        var setup = release.Assets.FirstOrDefault(asset => asset.Name == setupName);
        var sums = release.Assets.FirstOrDefault(asset => asset.Name == "SHA256SUMS.txt");
        if (setupName is null || setup is null || sums is null
            || !UpdateRules.IsTrustedDownload(setup.Url) || !UpdateRules.IsTrustedDownload(sums.Url))
        {
            BridgeLog.Write($"update {release.Tag} has no verifiable Setup for this machine");
            return null;
        }

        return new AvailableUpdate(version!, setupName, setup.Url!, sums.Url!, release.PageUrl ?? "");
    }

    /// <summary>Downloads Setup and returns its path once its checksum matches the release's.</summary>
    internal static async Task<string> DownloadAsync(AvailableUpdate update, IProgress<double> progress,
        CancellationToken cancel)
    {
        var sums = await Http.GetStringAsync(update.SumsUrl, cancel).ConfigureAwait(false);
        var expected = UpdateRules.ChecksumFor(sums, update.SetupName)
            ?? throw new InvalidDataException("The release does not list a checksum for its Setup.");

        var folder = Path.Combine(Path.GetTempPath(), "Goosic-update");
        Directory.CreateDirectory(folder);
        var path = Path.Combine(folder, update.SetupName);
        using (var response = await Http.GetAsync(update.SetupUrl, HttpCompletionOption.ResponseHeadersRead, cancel)
            .ConfigureAwait(false))
        {
            response.EnsureSuccessStatusCode();
            var total = response.Content.Headers.ContentLength;
            await using var source = await response.Content.ReadAsStreamAsync(cancel).ConfigureAwait(false);
            await using var target = File.Create(path);
            var buffer = new byte[81920];
            long received = 0;
            int read;
            while ((read = await source.ReadAsync(buffer, cancel).ConfigureAwait(false)) > 0)
            {
                await target.WriteAsync(buffer.AsMemory(0, read), cancel).ConfigureAwait(false);
                received += read;
                if (total > 0)
                {
                    progress.Report((double)received / total.Value);
                }
            }
        }

        string actual;
        await using (var file = File.OpenRead(path))
        {
            actual = Convert.ToHexStringLower(await SHA256.HashDataAsync(file, cancel).ConfigureAwait(false));
        }

        if (actual != expected)
        {
            File.Delete(path);
            throw new InvalidDataException("The downloaded Setup did not match the release's checksum, so it was deleted.");
        }

        return path;
    }

    /// <summary>
    /// Starts Setup to replace this installation, and reopens Goosic when it finishes.
    /// </summary>
    /// <remarks>
    /// The caller exits straight afterwards so Setup can replace the running files; Setup's
    /// <c>/CLOSEAPPLICATIONS</c> closes anything still holding them. <c>/RELAUNCH=1</c> is read by
    /// Setup itself, which starts the new version once it has installed it.
    /// </remarks>
    internal static void StartSetup(string setupPath)
    {
        Process.Start(new ProcessStartInfo(setupPath)
        {
            Arguments = "/SILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS /RELAUNCH=1",
            UseShellExecute = true,
        });
    }

    private static HttpClient CreateClient()
    {
        var client = new HttpClient { Timeout = TimeSpan.FromMinutes(10) };
        // GitHub's API refuses requests without a User-Agent.
        client.DefaultRequestHeaders.UserAgent.ParseAdd($"Goosic/{CurrentVersionText.Replace(' ', '-')}");
        client.DefaultRequestHeaders.Accept.ParseAdd("application/vnd.github+json");
        return client;
    }

    private sealed class Release
    {
        [JsonPropertyName("tag_name")] public string? Tag { get; init; }
        [JsonPropertyName("html_url")] public string? PageUrl { get; init; }
        [JsonPropertyName("draft")] public bool Draft { get; init; }
        [JsonPropertyName("prerelease")] public bool Prerelease { get; init; }
        [JsonPropertyName("assets")] public Asset[] Assets { get; init; } = [];
    }

    private sealed class Asset
    {
        [JsonPropertyName("name")] public string? Name { get; init; }
        [JsonPropertyName("browser_download_url")] public string? Url { get; init; }
    }
}
