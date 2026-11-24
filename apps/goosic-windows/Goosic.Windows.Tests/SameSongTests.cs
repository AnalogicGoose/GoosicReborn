using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public class SameSongTests
{
    [Theory]
    [InlineData("Hideaway", "Hideaway")]
    [InlineData("Hideaway", "Bella Poarch - Hideaway (Official Video)")]
    [InlineData("Swim", "SWIM [Official Audio]")]
    [InlineData("I Adore You (feat. Daecolm)", "I Adore You")]
    [InlineData("Don't Stop Me Now", "Dont Stop Me Now (Remastered 2011)")]
    public void AnotherVersionOfTheSongMatches(string requested, string page) =>
        Assert.True(PlaybackOrder.IsSameSong(requested, page));

    [Theory]
    [InlineData("Down", "Downtown")]
    [InlineData("Hideaway", "Ribcage")]
    [InlineData("Hideaway", "")]
    [InlineData("", "Hideaway")]
    [InlineData("(Intro)", "Anything")]
    public void ADifferentSongDoesNot(string requested, string page) =>
        Assert.False(PlaybackOrder.IsSameSong(requested, page));

    [Fact]
    public void AnAdoptedVersionMayBeSwappedBackOnceButNotForEver()
    {
        Assert.True(PlaybackOrder.MayAdopt(0));
        // The page swapped the song for its video, then the video back for the song.
        Assert.True(PlaybackOrder.MayAdopt(1));
        Assert.False(PlaybackOrder.MayAdopt(PlaybackOrder.MaxAdoptions));
    }

    [Fact]
    public void TheSongVersionScriptOnlyPressesSongAndSaysWhatItDid()
    {
        var script = PlaybackOrder.SongVersionScript;
        Assert.Contains("ytmusic-av-toggle", script);
        Assert.Contains(".song-button", script);
        Assert.DoesNotContain("video-button", script);
        Assert.DoesNotContain("location", script);
        foreach (var answer in new[] { "song", "switched", "no song version" })
        {
            Assert.True(PlaybackOrder.SongVersionSettled(answer), answer);
        }

        foreach (var answer in new[] { "absent", "pending", "", null })
        {
            Assert.False(PlaybackOrder.SongVersionSettled(answer), answer);
        }
    }
}
