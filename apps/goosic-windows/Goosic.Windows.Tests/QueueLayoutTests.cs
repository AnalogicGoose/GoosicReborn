using System;
using System.Collections.Generic;
using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public class QueueLayoutTests
{
    private sealed class Entry(string name)
    {
        public override string ToString() => name;
    }

    private static readonly Entry A = new("a"), B = new("b"), C = new("c"), D = new("d");

    [Fact]
    public void UpNextIsWhatFollowsTheCurrentEntry()
    {
        Assert.Equal([C, D], QueueLayout.UpNext([A, B, C, D], B));
        Assert.Equal([A, B], QueueLayout.UpNext([A, B], null));
        Assert.Empty(QueueLayout.UpNext([A, B], B));
    }

    [Fact]
    public void AnEntryQueuedTwiceIsFoundByReference()
    {
        var twin = new Entry("b");
        Assert.Equal([B], QueueLayout.UpNext([A, twin, B], twin));
    }

    [Fact]
    public void ReorderingKeepsPlayedAndCurrentInPlace() =>
        Assert.Equal([A, B, D, C], QueueLayout.Reordered([A, B, C, D], B, [D, C]));

    [Fact]
    public void AReorderThatNoLongerMatchesIsRejected()
    {
        Assert.Null(QueueLayout.Reordered([A, B, C, D], B, [D]));
        Assert.Null(QueueLayout.Reordered([A, B, C, D], B, [D, A]));
    }

    [Theory]
    [InlineData(0, 3, -1, null)]
    [InlineData(2, 3, 1, null)]
    [InlineData(1, 3, -1, 0)]
    [InlineData(1, 3, 1, 2)]
    [InlineData(-1, 3, 1, null)]
    public void KeyboardMovesStayInsideUpNext(int index, int count, int delta, int? expected) =>
        Assert.Equal(expected, QueueLayout.MoveTarget(index, count, delta));
}

public class ClearedQueueTests
{
    private sealed class Entry;

    private static readonly DateTimeOffset Cleared = new(2026, 9, 16, 12, 0, 0, TimeSpan.Zero);

    [Fact]
    public void UndoIsOfferedWhileTheSameTrackPlaysInsideTheWindow()
    {
        var current = new Entry();
        var cleared = new ClearedQueue<Entry>(current, [new Entry()], Cleared);
        Assert.True(cleared.CanUndo(current, Cleared.AddSeconds(3)));
        Assert.False(cleared.CanUndo(current, Cleared + ClearedQueue<Entry>.UndoWindow + TimeSpan.FromSeconds(1)));
        Assert.False(cleared.CanUndo(new Entry(), Cleared.AddSeconds(1)));
    }

    [Fact]
    public void NothingRemovedMeansNothingToUndo() =>
        Assert.False(new ClearedQueue<Entry>(null, [], Cleared).CanUndo(null, Cleared));

    [Fact]
    public void UndoSkipsEntriesQueuedAgainSince()
    {
        var kept = new Entry();
        var requeued = new Entry();
        var cleared = new ClearedQueue<Entry>(null, [kept, requeued], Cleared);
        Assert.Equal([kept], cleared.ToRestore(new List<Entry> { requeued }));
    }
}
