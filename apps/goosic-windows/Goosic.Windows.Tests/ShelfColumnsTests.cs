using System;
using System.Linq;
using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public sealed class ShelfColumnsTests
{
    [Fact]
    public void RowsFillEachColumnBeforeTheNext()
    {
        var columns = ShelfColumns.Split(Enumerable.Range(1, 10).ToList(), 4);
        Assert.Equal([[1, 2, 3, 4], [5, 6, 7, 8], [9, 10]], columns.Select(column => column.ToArray()).ToArray());
    }

    [Fact]
    public void AnEmptyShelfHasNoColumns() =>
        Assert.Empty(ShelfColumns.Split(Array.Empty<int>(), 4));

    [Fact]
    public void AColumnNeedsAtLeastOneRow() =>
        Assert.Throws<ArgumentOutOfRangeException>(() => ShelfColumns.Split(new[] { 1 }, 0));
}
