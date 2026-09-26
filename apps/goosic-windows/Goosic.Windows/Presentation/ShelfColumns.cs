using System;
using System.Collections.Generic;

namespace Goosic.Windows.Presentation;

/// <summary>How a list shelf is cut into the columns that scroll sideways.</summary>
public static class ShelfColumns
{
    /// <summary>YouTube Music shows Quick picks four rows deep.</summary>
    public const int RowsPerColumn = 4;

    /// <summary>Row width: three columns fill a typical window, and the fourth peeks in.</summary>
    public const double ColumnWidth = 380;

    /// <summary>Splits items into columns in reading order: down the first column, then the next.</summary>
    public static IReadOnlyList<IReadOnlyList<T>> Split<T>(IReadOnlyList<T> items, int rows)
    {
        ArgumentOutOfRangeException.ThrowIfLessThan(rows, 1);
        var columns = new List<IReadOnlyList<T>>();
        for (var start = 0; start < items.Count; start += rows)
        {
            var column = new List<T>(rows);
            for (var index = start; index < Math.Min(start + rows, items.Count); index++)
            {
                column.Add(items[index]);
            }

            columns.Add(column);
        }

        return columns;
    }
}
