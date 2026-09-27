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
}
