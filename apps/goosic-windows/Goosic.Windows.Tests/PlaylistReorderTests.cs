using System.Collections.Generic;
using System.Linq;
using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public sealed class PlaylistReorderTests
{
    private static readonly string[] Songs = ["a", "b", "c", "d", "e", "f"];

    private static (string Order, int Requests, string Replayed) Step(string selected, bool up)
    {
        var (order, moves) = PlaylistReorder.Step(Songs, song => selected.Contains(song), up);
        return (string.Concat(order), moves.Count, string.Concat(PlaylistReorder.Apply(Songs, moves)));
    }

    [Theory]
    [InlineData("c", "acbdef", 1)]
    [InlineData("cd", "acdbef", 1)]
    [InlineData("bd", "badcef", 2)]
    [InlineData("cf", "acbdfe", 2)]
    [InlineData("ab", "abcdef", 0)]
    [InlineData("abd", "abdcef", 1)]
    public void MovingUpShiftsEachBlockByOne(string selected, string expected, int requests)
    {
        var (order, count, replayed) = Step(selected, up: true);
        Assert.Equal(expected, order);
        Assert.Equal(requests, count);
        // What upstream ends up with, applying the requests in turn, is what the screen shows.
        Assert.Equal(expected, replayed);
    }

    [Theory]
    [InlineData("c", "abdcef", 1)]
    [InlineData("cd", "abecdf", 1)]
    [InlineData("be", "acbdfe", 2)]
    [InlineData("ef", "abcdef", 0)]
    [InlineData("a", "bacdef", 1)]
    public void MovingDownShiftsEachBlockByOne(string selected, string expected, int requests)
    {
        var (order, count, replayed) = Step(selected, up: false);
        Assert.Equal(expected, order);
        Assert.Equal(requests, count);
        Assert.Equal(expected, replayed);
    }

    [Fact]
    public void MovingTheLastSongUpNamesNoSuccessorOnlyWhenItGoesToTheEnd()
    {
        var (_, moves) = PlaylistReorder.Step(Songs, song => song == "f", up: true);
        var move = Assert.Single(moves);
        Assert.Equal("e", move.Moved);
        Assert.Null(move.Before);
    }

    [Fact]
    public void NothingSelectedMovesNothing()
    {
        var (order, moves) = PlaylistReorder.Step(Songs, _ => false, up: true);
        Assert.Equal(Songs, order);
        Assert.Empty(moves);
    }
}
