using System;
using System.Collections.Generic;
using System.Linq;

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

    /// <summary>
    /// Whether a shelf that arrived with the next part of a page continues the last one shown.
    /// </summary>
    /// <remarks>
    /// A filtered search continues one list: its next rows come back under the shelf already on
    /// screen, same id and same title, and belong at the end of it rather than under a second
    /// heading. Both must match, because browse continuations number their shelves from zero
    /// again and can reuse an id for a shelf that is not the same one.
    /// </remarks>
    public static bool Continues(string shownId, string shownTitle, string id, string title) =>
        shownId.Length > 0 && shownId == id && shownTitle == title;

    /// <summary>The arriving items whose ids are not already shown, each once, in the order they came.</summary>
    public static IReadOnlyList<T> Unseen<T>(IEnumerable<string> shown, IEnumerable<T> arriving, Func<T, string> id)
    {
        var seen = new HashSet<string>(shown);
        return arriving.Where(item => seen.Add(id(item))).ToList();
    }
}
