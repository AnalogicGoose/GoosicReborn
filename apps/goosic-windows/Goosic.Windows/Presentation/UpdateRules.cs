using System;
using System.Globalization;

namespace Goosic.Windows.Presentation;

/// <summary>
/// The decisions the updater makes about a GitHub release, kept apart from the network so they
/// can be tested: which version it is, whether it is newer, which file to install, and what that
/// file's checksum must be.
/// </summary>
public static class UpdateRules
{
    /// <summary>Reads "v0.2.0" or "0.2.0". A pre-release or anything else is not an update.</summary>
    public static Version? ParseVersion(string? text)
    {
        if (string.IsNullOrWhiteSpace(text))
        {
            return null;
        }

        var core = text.Trim();
        if (core.StartsWith('v') || core.StartsWith('V'))
        {
            core = core[1..];
        }

        // Build metadata ("+abc123") is not part of the version; a pre-release ("-beta") is
        // not a release this updater installs.
        var plus = core.IndexOf('+');
        if (plus >= 0)
        {
            core = core[..plus];
        }

        if (core.Contains('-') || core.Split('.').Length != 3)
        {
            return null;
        }

        return Version.TryParse(core, out var version) && version.Major >= 0 ? version : null;
    }

    /// <summary>Only a strictly newer release is offered; the same or an older one never is.</summary>
    public static bool IsNewer(Version? current, Version? candidate) =>
        current is not null && candidate is not null && candidate > current;

    /// <summary>The Setup file a release carries for this machine, or null where none is published.</summary>
    public static string? SetupAssetName(Version version, string architecture) =>
        architecture == "x64" ? $"Goosic-{Format(version)}-windows-x64-setup.exe" : null;

    public static string Format(Version version) =>
        string.Create(CultureInfo.InvariantCulture, $"{version.Major}.{version.Minor}.{version.Build}");

    /// <summary>
    /// The SHA-256 a <c>SHA256SUMS.txt</c> lists for one file, lower-cased, or null if it lists none.
    /// </summary>
    /// <remarks>Each line is a 64-digit hash, whitespace, and a file name; a binary-mode "*" before the name is accepted.</remarks>
    public static string? ChecksumFor(string sums, string fileName)
    {
        foreach (var raw in sums.Split('\n'))
        {
            var line = raw.Trim();
            var space = line.IndexOfAny([' ', '\t']);
            if (space != 64)
            {
                continue;
            }

            var name = line[space..].TrimStart().TrimStart('*');
            var hash = line[..64];
            if (name == fileName && IsHex(hash))
            {
                return hash.ToLowerInvariant();
            }
        }

        return null;
    }

    /// <summary>
    /// Only HTTPS from GitHub itself. The release's own listing names the download, and this is
    /// what stops a changed listing from sending Setup somewhere else.
    /// </summary>
    public static bool IsTrustedDownload(string? url) =>
        Uri.TryCreate(url, UriKind.Absolute, out var uri)
        && uri.Scheme == Uri.UriSchemeHttps
        && uri.Host.Equals("github.com", StringComparison.OrdinalIgnoreCase)
        && uri.AbsolutePath.StartsWith("/AnalogicGoose/GoosicReborn/releases/download/", StringComparison.Ordinal);

    private static bool IsHex(string text)
    {
        foreach (var c in text)
        {
            if (!Uri.IsHexDigit(c))
            {
                return false;
            }
        }

        return true;
    }
}
