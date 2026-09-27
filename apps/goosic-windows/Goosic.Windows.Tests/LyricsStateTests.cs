using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public class LyricsStateTests
{
    [Fact]
    public void LoadingShowsNeitherLinesNorPlaceholder()
    {
        Assert.True(LyricsState.Loading.IsLoading);
        Assert.False(LyricsState.Loading.HasLines);
        Assert.False(LyricsState.Loading.ShowsPlaceholder);
    }

    [Fact]
    public void OnlyAnUnavailableServiceOffersRetry()
    {
        Assert.True(LyricsState.Unavailable.CanRetry);
        Assert.False(LyricsState.NotFound.CanRetry);
        Assert.False(LyricsState.NothingPlaying.CanRetry);
    }

    [Theory]
    [InlineData(true, LyricsKind.Synced, "Synced · LRCLIB")]
    [InlineData(false, LyricsKind.Unsynced, "Not synced · LRCLIB")]
    public void FoundLyricsSayWhetherTheyFollowTheSong(bool synced, LyricsKind kind, string caption)
    {
        var state = LyricsState.Found(synced, "LRCLIB", truncated: false);
        Assert.Equal(kind, state.Kind);
        Assert.Equal(caption, state.Message);
        Assert.True(state.HasLines);
        Assert.Equal(synced, state.FollowsPlayback);
    }

    [Fact]
    public void AClampedDocumentSaysSo() =>
        Assert.EndsWith("Only the first part is shown", LyricsState.Found(true, "LRCLIB", truncated: true).Message);
}
