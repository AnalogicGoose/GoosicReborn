using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public sealed class LyricsLookupTests
{
    [Fact]
    public void TheCatalogArtistIsUsedRatherThanTheShelfSubtitle()
    {
        var lookup = LyricsLookup.For("Save Your Tears", "The Weeknd", "Song • The Weeknd", "After Hours", "3:35");
        Assert.Equal(new LyricsLookup("Save Your Tears", "The Weeknd", "After Hours", 215), lookup);
    }

    [Fact]
    public void ADescriptiveSubtitleIsNeverSentAsTheArtist()
    {
        Assert.Equal("", LyricsLookup.For("Party In The U.S.A.", null, "Ecoficient • 356K views", null, null).Artist);
        Assert.Equal("", LyricsLookup.For("Giant", "", "Calvin Harris · Giant", null, null).Artist);
    }

    [Fact]
    public void ASubtitleThatIsOnlyANameStandsInForAMissingArtist()
    {
        Assert.Equal("Kanye West", LyricsLookup.For("Homecoming", null, "Kanye West", null, null).Artist);
    }

    [Theory]
    [InlineData("3:35", 215)]
    [InlineData("1:02:03", 3723)]
    [InlineData("0:07", 7)]
    public void DisplayDurationsBecomeSeconds(string text, int seconds) =>
        Assert.Equal(seconds, LyricsLookup.Seconds(text));

    [Theory]
    [InlineData(null)]
    [InlineData("")]
    [InlineData("215")]
    [InlineData("3:3x")]
    [InlineData("-1:30")]
    [InlineData("1:2:3:4")]
    public void AnythingElseIsNoDuration(string? text) => Assert.Null(LyricsLookup.Seconds(text));
}
