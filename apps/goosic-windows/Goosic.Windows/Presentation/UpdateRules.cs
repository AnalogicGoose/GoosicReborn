using System;
using System.Globalization;
using System.Linq;

namespace Goosic.Windows.Presentation;

/// <summary>
/// The decisions the updater makes about a GitHub release, kept apart from the network so they
/// can be tested: which version it is, whether it is newer, which file to install, and what that
/// file's checksum must be.
/// </summary>
public static class UpdateRules
{
    /// <summary>The fourth field of a release that is not a beta: above every beta of its version.</summary>
    public const int ReleaseRank = 99;

    /// <summary>
    /// Reads "v0.2.0", "0.2.0" or "v0.4.7-beta.2". The beta number goes in the fourth field, and
    /// a release that is not a beta gets <see cref="ReleaseRank"/>, so 0.4.7 beta 2 orders before
    /// 0.4.7 and after 0.4.6, and the four fields read as the macOS build number (40702). Any
    /// other suffix is not a release this updater installs. The Windows copy of
    /// <c>updates::parse_release_version</c> in <c>goosic-shell-support</c>, held to its cases.
    /// </summary>
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

        // Build metadata ("+abc123") is not part of the version.
        var plus = core.IndexOf('+');
        if (plus >= 0)
        {
            core = core[..plus];
        }

        var beta = ReleaseRank;
        var dash = core.IndexOf('-');
        if (dash >= 0)
        {
            var suffix = core[(dash + 1)..];
            core = core[..dash];
            if (!suffix.StartsWith("beta.", StringComparison.Ordinal)
                || !IsDigits(suffix[5..])
                || !int.TryParse(suffix[5..], NumberStyles.None, CultureInfo.InvariantCulture, out beta)
                || beta < 1 || beta >= ReleaseRank)
            {
                return null;
            }
        }

        var parts = core.Split('.');
        if (parts.Length != 3 || !Array.TrueForAll(parts, IsDigits))
        {
            return null;
        }

        return int.TryParse(parts[0], NumberStyles.None, CultureInfo.InvariantCulture, out var major)
            && int.TryParse(parts[1], NumberStyles.None, CultureInfo.InvariantCulture, out var minor)
            && int.TryParse(parts[2], NumberStyles.None, CultureInfo.InvariantCulture, out var patch)
            ? new Version(major, minor, patch, beta)
            : null;
    }

    /// <summary>Only a strictly newer release is offered; the same or an older one never is.</summary>
    public static bool IsNewer(Version? current, Version? candidate) =>
        current is not null && candidate is not null && candidate > current;

    /// <summary>The Setup file a release carries for this machine, or null where none is published.</summary>
    public static string? SetupAssetName(Version version, string architecture) =>
        architecture == "x64" ? $"Goosic-{Spelled(version)}-windows-x64-setup.exe" : null;

    /// <summary>What a person reads: "0.4.7" or "0.4.7 beta 2".</summary>
    public static string Format(Version version) =>
        IsBeta(version)
            ? string.Create(CultureInfo.InvariantCulture, $"{version.Major}.{version.Minor}.{version.Build} beta {version.Revision}")
            : string.Create(CultureInfo.InvariantCulture, $"{version.Major}.{version.Minor}.{version.Build}");

    /// <summary>The version as a tag or file name spells it: "0.4.7" or "0.4.7-beta.2".</summary>
    public static string Spelled(Version version) =>
        IsBeta(version)
            ? string.Create(CultureInfo.InvariantCulture, $"{version.Major}.{version.Minor}.{version.Build}-beta.{version.Revision}")
            : string.Create(CultureInfo.InvariantCulture, $"{version.Major}.{version.Minor}.{version.Build}");

    private static bool IsBeta(Version version) => version.Revision is > 0 and < ReleaseRank;

    private static bool IsDigits(string text) => text.Length > 0 && text.All(char.IsAsciiDigit);

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
