using System;
using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public sealed class UpdateRulesTests
{
    [Theory]
    [InlineData("v0.2.0", "0.2.0")]
    [InlineData("0.10.3", "0.10.3")]
    [InlineData("V1.0.0", "1.0.0")]
    [InlineData("0.1.0+8c4cb97", "0.1.0")]
    public void ReleaseTagsAndBuildVersionsParse(string text, string expected) =>
        Assert.Equal(Version.Parse(expected), UpdateRules.ParseVersion(text));

    [Theory]
    [InlineData(null)]
    [InlineData("")]
    [InlineData("v0.2.0-beta.1")]
    [InlineData("0.2")]
    [InlineData("latest")]
    [InlineData("1.2.3.4")]
    public void PreReleasesAndNonsenseAreNotVersions(string? text) =>
        Assert.Null(UpdateRules.ParseVersion(text));

    [Fact]
    public void OnlyAStrictlyNewerReleaseIsOffered()
    {
        var current = new Version(0, 1, 0);
        Assert.True(UpdateRules.IsNewer(current, new Version(0, 1, 1)));
        Assert.True(UpdateRules.IsNewer(current, new Version(0, 10, 0)));
        Assert.False(UpdateRules.IsNewer(current, new Version(0, 1, 0)));
        Assert.False(UpdateRules.IsNewer(current, new Version(0, 0, 9)));
        Assert.False(UpdateRules.IsNewer(null, new Version(9, 0, 0)));
        Assert.False(UpdateRules.IsNewer(current, null));
    }

    [Fact]
    public void SetupIsPublishedForX64Only()
    {
        Assert.Equal("Goosic-0.2.0-windows-x64-setup.exe", UpdateRules.SetupAssetName(new Version(0, 2, 0), "x64"));
        Assert.Null(UpdateRules.SetupAssetName(new Version(0, 2, 0), "arm64"));
    }

    private const string Sums =
        "3313fa36c1b946ed8f366e57757af0dba46dd6c6a0891b70e8522e34867d32af  Goosic-0.1.0-windows-x64-setup.exe\n"
        + "CEA4DBAD9AC9027329E5FAB7B16AEA6D7DCDC825CE91E7E3CDE894CA6197B215 *Goosic-0.1.0-windows-x64.zip\r\n"
        + "b18f26bf00dc670957a3693a16b093ca73dc6f6e401785302e9433a75906f423  Goosic-0.1.0-windows-x64/goosic-service.exe\n";

    [Fact]
    public void TheChecksumIsReadForExactlyTheNamedFile()
    {
        Assert.Equal("3313fa36c1b946ed8f366e57757af0dba46dd6c6a0891b70e8522e34867d32af",
            UpdateRules.ChecksumFor(Sums, "Goosic-0.1.0-windows-x64-setup.exe"));
        Assert.Equal("cea4dbad9ac9027329e5fab7b16aea6d7dcdc825ce91e7e3cde894ca6197b215",
            UpdateRules.ChecksumFor(Sums, "Goosic-0.1.0-windows-x64.zip"));
        Assert.Null(UpdateRules.ChecksumFor(Sums, "goosic-service.exe"));
        Assert.Null(UpdateRules.ChecksumFor(Sums, "Goosic-0.1.0-windows-x64-setup"));
        Assert.Null(UpdateRules.ChecksumFor("not-a-hash  Goosic-0.1.0-windows-x64-setup.exe", "Goosic-0.1.0-windows-x64-setup.exe"));
    }

    [Theory]
    [InlineData("https://github.com/AnalogicGoose/GoosicReborn/releases/download/v0.2.0/Goosic-0.2.0-windows-x64-setup.exe", true)]
    [InlineData("http://github.com/AnalogicGoose/GoosicReborn/releases/download/v0.2.0/x.exe", false)]
    [InlineData("https://github.com/someone-else/GoosicReborn/releases/download/v0.2.0/x.exe", false)]
    [InlineData("https://github.com.evil.example/AnalogicGoose/GoosicReborn/releases/download/v0.2.0/x.exe", false)]
    [InlineData("https://example.com/AnalogicGoose/GoosicReborn/releases/download/v0.2.0/x.exe", false)]
    [InlineData(null, false)]
    public void OnlyThisRepositorysReleaseDownloadsAreTrusted(string? url, bool trusted) =>
        Assert.Equal(trusted, UpdateRules.IsTrustedDownload(url));
}
