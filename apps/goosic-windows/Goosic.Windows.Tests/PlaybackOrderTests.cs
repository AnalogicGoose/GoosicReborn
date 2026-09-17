using System;
using System.Collections.Generic;
using System.Linq;
using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public class PlaybackOrderTests
{
    [Theory]
    [InlineData(5, 2, true, 3)]
    [InlineData(5, 2, false, 1)]
    public void MovesToTheNeighbour(int count, int index, bool forward, int expected) =>
        Assert.Equal(new MoveDecision(MoveKind.Play, expected),
            PlaybackOrder.Move(count, index, forward, natural: false, QueueRepeat.Off));

    [Theory]
    [InlineData(QueueRepeat.Off)]
    [InlineData(QueueRepeat.One)]
    public void PreviousOnTheFirstSongRestartsItInsteadOfWrapping(QueueRepeat repeat) =>
        Assert.Equal(MoveKind.Restart, PlaybackOrder.Move(5, 0, forward: false, natural: false, repeat).Kind);

    [Fact]
    public void NextOnTheLastSongAsksForMoreInsteadOfStartingOver()
    {
        Assert.Equal(MoveKind.NeedMore, PlaybackOrder.Move(5, 4, forward: true, natural: false, QueueRepeat.Off).Kind);
        Assert.Equal(MoveKind.NeedMore, PlaybackOrder.Move(5, 4, forward: true, natural: true, QueueRepeat.Off).Kind);
    }

    [Fact]
    public void RepeatAllWrapsBothWays()
    {
        Assert.Equal(new MoveDecision(MoveKind.Play, 0), PlaybackOrder.Move(5, 4, true, false, QueueRepeat.All));
        Assert.Equal(new MoveDecision(MoveKind.Play, 4), PlaybackOrder.Move(5, 0, false, false, QueueRepeat.All));
    }

    [Fact]
    public void RepeatOneOnlyHoldsANaturalEnd()
    {
        Assert.Equal(MoveKind.Restart, PlaybackOrder.Move(5, 2, true, natural: true, QueueRepeat.One).Kind);
        Assert.Equal(new MoveDecision(MoveKind.Play, 3), PlaybackOrder.Move(5, 2, true, natural: false, QueueRepeat.One));
    }

    [Fact]
    public void ACurrentSongMissingFromTheQueueNeverPicksAStrangerOne()
    {
        Assert.Equal(MoveKind.Restart, PlaybackOrder.Move(5, -1, forward: false, natural: false, QueueRepeat.All).Kind);
        Assert.Equal(MoveKind.NeedMore, PlaybackOrder.Move(5, -1, forward: true, natural: false, QueueRepeat.All).Kind);
        Assert.Equal(MoveKind.NeedMore, PlaybackOrder.Move(0, 0, forward: true, natural: true, QueueRepeat.Off).Kind);
    }

    [Fact]
    public void RecommendationsSkipQueuedPlayedAndRepeatedSongs()
    {
        var fresh = PlaybackOrder.FreshRecommendations(
            new[] { "a", "b", "b", "c", "", "d" }, id => id, new string?[] { "a", null, "c" });
        Assert.Equal(new[] { "b", "d" }, fresh);
    }

    [Fact]
    public void AShuffledQueueStartsWithTheChosenSongAndKeepsEveryOther()
    {
        var items = Enumerable.Range(0, 20).Select(i => new Song(i)).ToList();
        var chosen = items[7];
        var order = PlaybackOrder.ShuffledStartingWith(items, chosen, new Random(4));
        Assert.Same(chosen, order[0]);
        Assert.Equal(items.Count, order.Count);
        Assert.Equal(items.OrderBy(s => s.N), order.OrderBy(s => s.N));
    }

    [Theory]
    [InlineData(DetailKind.Album, LaunchKind.Ordered)]
    [InlineData(DetailKind.Playlist, LaunchKind.Ordered)]
    [InlineData(DetailKind.Search, LaunchKind.Station)]
    [InlineData(DetailKind.Browse, LaunchKind.Station)]
    [InlineData(DetailKind.Artist, LaunchKind.Station)]
    public void OnlyChosenListsPlayInOrder(DetailKind page, LaunchKind expected) =>
        Assert.Equal(expected, PlaybackOrder.LaunchFor(page));

    [Theory]
    [InlineData("abc", "abc", true, true)]
    [InlineData("abc", "abc", false, false)]
    [InlineData("old", "new", true, false)]
    [InlineData(null, "new", true, false)]
    public void OnlyTheCurrentPlaysHeardEndCounts(string? ended, string current, bool armed, bool expected) =>
        Assert.Equal(expected, PlaybackOrder.AcceptsEnd(ended, current, armed));

    private sealed record Song(int N);
}

public class RadioStationTests
{
    [Fact]
    public void AStationLoadsUntilItsCursorRunsOut()
    {
        var station = new RadioStation("seed", "account");
        Assert.True(station.CanLoadMore);
        var paged = station.After("next");
        Assert.True(paged.CanLoadMore);
        Assert.False(paged.After(null).CanLoadMore);
    }

    [Fact]
    public void ARepeatedCursorEndsTheStationInsteadOfLooping() =>
        Assert.False(new RadioStation("seed", null, "same", Loaded: true).After("same").CanLoadMore);

    [Fact]
    public void AnAccountsCursorIsOnlyUsedWithThatAccount()
    {
        var personal = new RadioStation("seed", "a");
        Assert.True(personal.UsableWith("a"));
        Assert.False(personal.UsableWith("b"));
        Assert.False(personal.UsableWith(null));
        Assert.True(new RadioStation("seed", null).UsableWith("anyone"));
    }
}

public class TrackEndTests
{
    [Theory]
    [InlineData("ended", 10, 229, false, true)]
    [InlineData("paused", 228.8, 229, false, true)]
    [InlineData("paused", 228.8, 229, true, false)]
    [InlineData("paused", 120, 229, false, false)]
    [InlineData("paused", 0, 0, false, false)]
    [InlineData("playing", 229, 229, false, false)]
    public void APauseAtTheLastMomentIsTheEndUnlessTheListenerPaused(
        string state, double position, double duration, bool listenerPaused, bool expected) =>
        Assert.Equal(expected, PlaybackOrder.IsFinished(state, position, duration, listenerPaused));
}
