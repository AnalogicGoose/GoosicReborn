using System;
using System.Collections.Generic;
using System.Linq;

namespace Goosic.Windows.Presentation;

/// <summary>Moves the selected entries of a playlist one place up or down.</summary>
/// <remarks>
/// Upstream moves one entry per request, placing it before a named entry, or at the end when none
/// is named. Moving a block of selected songs up is the same as moving the one song above it to
/// just after the block, so each contiguous block costs a single request however long it is,
/// and the songs keep their order within it.
/// </remarks>
public static class PlaylistReorder
{
    /// <summary>One upstream move: put <c>Moved</c> before <c>Before</c>, or at the end when that is null.</summary>
    public readonly record struct Move<T>(T Moved, T? Before) where T : class;

    public static (List<T> Order, List<Move<T>> Moves) Step<T>(IReadOnlyList<T> order, Func<T, bool> isSelected, bool up)
        where T : class
    {
        var list = order.ToList();
        var moves = new List<Move<T>>();
        if (up)
        {
            var index = 0;
            while (index < list.Count)
            {
                if (!isSelected(list[index]))
                {
                    index++;
                    continue;
                }

                var start = index;
                while (index < list.Count && isSelected(list[index]))
                {
                    index++;
                }

                var end = index - 1;
                if (start == 0)
                {
                    continue;
                }

                // The song above the block drops to just below it.
                var displaced = list[start - 1];
                var successor = end + 1 < list.Count ? list[end + 1] : null;
                list.RemoveAt(start - 1);
                list.Insert(end, displaced);
                moves.Add(new Move<T>(displaced, successor));
            }
        }
        else
        {
            var index = list.Count - 1;
            while (index >= 0)
            {
                if (!isSelected(list[index]))
                {
                    index--;
                    continue;
                }

                var end = index;
                while (index >= 0 && isSelected(list[index]))
                {
                    index--;
                }

                var start = index + 1;
                if (end == list.Count - 1)
                {
                    continue;
                }

                // The song below the block rises to just above it.
                var displaced = list[end + 1];
                var successor = list[start];
                list.RemoveAt(end + 1);
                list.Insert(start, displaced);
                moves.Add(new Move<T>(displaced, successor));
            }
        }

        return (list, moves);
    }

    /// <summary>Replays moves the way upstream applies them, one at a time.</summary>
    public static List<T> Apply<T>(IReadOnlyList<T> order, IEnumerable<Move<T>> moves) where T : class
    {
        var list = order.ToList();
        foreach (var move in moves)
        {
            list.Remove(move.Moved);
            var at = move.Before is null ? list.Count : list.IndexOf(move.Before);
            list.Insert(at, move.Moved);
        }

        return list;
    }
}
