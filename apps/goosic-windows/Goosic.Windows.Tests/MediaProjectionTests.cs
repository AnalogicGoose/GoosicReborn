using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

/// <summary>
/// The system media overlay asks <c>goosic-shell-support</c> which buttons work, as the Mac's Now
/// Playing does, and the shared rules for artwork are answered there too.
/// </summary>
public class MediaProjectionTests
{
    private static PlaybackRules.MediaInput Playing(bool advertisement, bool hasQueue) => new()
    {
        HasTrack = true,
        IsReady = true,
        HasOwner = true,
        CurrentTime = 30,
        Duration = 200,
        IsPaused = false,
        IsAdvertisement = advertisement,
        HasQueue = hasQueue,
        IsIdle = true,
    };

    [Fact]
    public void ContentWithAQueueCanBeSkippedAndScrubbed()
    {
        var shown = PlaybackRules.ProjectMedia(Playing(advertisement: false, hasQueue: true));
        Assert.True(shown.IsActive);
        Assert.Equal(2, shown.PlaybackState);
        Assert.True(shown.Next && shown.Previous && shown.ChangePosition && shown.Pause);
        Assert.False(shown.Play);
    }

    [Fact]
    public void AnAdvertisementCanBePausedButNotSkippedOrScrubbed()
    {
        var shown = PlaybackRules.ProjectMedia(Playing(advertisement: true, hasQueue: true));
        Assert.True(shown.Pause);
        Assert.False(shown.Next || shown.Previous || shown.ChangePosition);
    }

    [Fact]
    public void NextNeedsAQueue()
    {
        var shown = PlaybackRules.ProjectMedia(Playing(advertisement: false, hasQueue: false));
        Assert.False(shown.Next || shown.Previous);
        Assert.True(shown.ChangePosition);
    }

    [Fact]
    public void ArtworkComesOnlyFromYouTubeMusicImageHosts()
    {
        Assert.True(PlaybackRules.ArtworkIsAllowed("https://lh3.googleusercontent.com/a.jpg"));
        Assert.False(PlaybackRules.ArtworkIsAllowed("https://evilgoogleusercontent.com/a.jpg"));
        Assert.False(PlaybackRules.ArtworkIsAllowed("http://lh3.googleusercontent.com/a.jpg"));
        Assert.Matches("^[0-9a-f]{32}$", PlaybackRules.ArtworkCacheKey("https://lh3.googleusercontent.com/a.jpg"));
    }
}
