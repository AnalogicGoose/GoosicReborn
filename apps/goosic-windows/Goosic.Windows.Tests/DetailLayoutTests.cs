using System;
using System.Linq;
using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public class DetailLayoutTests
{
    [Theory]
    [InlineData("album", DetailKind.Album)]
    [InlineData("playlist", DetailKind.Playlist)]
    [InlineData("artist", DetailKind.Artist)]
    [InlineData("podcast", DetailKind.Browse)]
    public void EntityKindsMapToLayouts(string kind, DetailKind expected) =>
        Assert.Equal(expected, DetailLayout.FromEntity(kind));

    [Fact]
    public void AlbumsNumberRowsAndDropTheRepeatedCover()
    {
        Assert.True(DetailLayout.ShowsNumbers(DetailKind.Album));
        Assert.False(DetailLayout.ShowsRowArtwork(DetailKind.Album));
    }

    [Fact]
    public void SearchResultsKeepCoversAndHaveNoOrder()
    {
        Assert.False(DetailLayout.ShowsNumbers(DetailKind.Search));
        Assert.True(DetailLayout.ShowsRowArtwork(DetailKind.Search));
    }

    [Fact]
    public void OnlyArtistsAreRound()
    {
        Assert.Equal(74, DetailLayout.HeroCornerRadius(DetailKind.Artist));
        Assert.Equal(10, DetailLayout.HeroCornerRadius(DetailKind.Album));
    }

    [Theory]
    [InlineData("3:45", 225)]
    [InlineData("1:02:03", 3723)]
    [InlineData("0:07", 7)]
    public void ReadsCatalogDurations(string text, int seconds) =>
        Assert.Equal(TimeSpan.FromSeconds(seconds), DetailLayout.ParseDuration(text));

    [Theory]
    [InlineData("")]
    [InlineData("LIVE")]
    [InlineData("3")]
    [InlineData("1:2:3:4")]
    [InlineData("-1:30")]
    public void UnknownDurationsAreNotGuessed(string text) =>
        Assert.Null(DetailLayout.ParseDuration(text));

    [Fact]
    public void SummaryAddsCountAndRunningTime() =>
        Assert.Equal("Album · Nonpoint · 3 songs · 11 min",
            DetailLayout.Summary("Album · Nonpoint", ["3:30", "4:00", "3:30"], truncated: false));

    [Fact]
    public void ASingleSongIsSingular() =>
        Assert.Equal("Single · 1 song · 3 min", DetailLayout.Summary("Single", ["3:05"], false));

    [Fact]
    public void AClampedPageNeverClaimsItsCountOrLength() =>
        Assert.Equal("Playlist · 100+ songs",
            DetailLayout.Summary("Playlist", Enumerable.Repeat("3:00", 100).ToList(), truncated: true));

    [Fact]
    public void AMissingDurationDropsTheTotal() =>
        Assert.Equal("Playlist · 2 songs", DetailLayout.Summary("Playlist", ["3:00", ""], false));

    [Fact]
    public void NoTracksMeansJustTheSubtitle() =>
        Assert.Equal("Artist", DetailLayout.Summary("  Artist ", [], false));

    [Theory]
    [InlineData(59, "59 min")]
    [InlineData(60, "1 hr")]
    [InlineData(135, "2 hr 15 min")]
    [InlineData(0.4, "1 min")]
    public void TotalsReadNaturally(double minutes, string expected) =>
        Assert.Equal(expected, DetailLayout.FormatTotal(TimeSpan.FromMinutes(minutes)));
}
