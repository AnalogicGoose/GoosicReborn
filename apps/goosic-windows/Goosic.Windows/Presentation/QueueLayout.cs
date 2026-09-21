using System;
using System.Collections.Generic;
using System.Linq;

namespace Goosic.Windows.Presentation;

/// <summary>
/// The queue as the panel shows it: what is playing, then what plays after it. Tracks already
/// played stay in the model's queue, so Previous can reach them, but are not listed.
/// </summary>
public static class QueueLayout
{
    /// <summary>The entries after <paramref name="current"/>, or all of them when nothing is current.</summary>
    public static List<T> UpNext<T>(IReadOnlyList<T> queue, T? current) where T : class
    {
        var index = current is null ? -1 : IndexOf(queue, current);
        return queue.Skip(index + 1).ToList();
    }

    /// <summary>
    /// The whole queue after Up Next was reordered: everything through the current entry, in its
    /// old order, then Up Next in its new one.
    /// </summary>
    /// <remarks>
    /// Returns null when <paramref name="upNext"/> is not a permutation of what followed the
    /// current entry — a reorder that raced a queue change — so the caller rebuilds instead of
    /// losing or duplicating a track.
    /// </remarks>
    public static List<T>? Reordered<T>(IReadOnlyList<T> queue, T? current, IReadOnlyList<T> upNext) where T : class
    {
        var head = current is null ? 0 : IndexOf(queue, current) + 1;
        var before = queue.Skip(head).ToList();
        if (before.Count != upNext.Count || before.Any(entry => !upNext.Contains(entry)))
        {
            return null;
        }

        return queue.Take(head).Concat(upNext).ToList();
    }

    /// <summary>Where a keyboard move lands inside Up Next, or null when it cannot move.</summary>
    public static int? MoveTarget(int index, int count, int delta)
    {
        var target = index + delta;
        return index < 0 || index >= count || target < 0 || target >= count || delta == 0 ? null : target;
    }

    private static int IndexOf<T>(IReadOnlyList<T> queue, T entry) where T : class
    {
        for (var i = 0; i < queue.Count; i++)
        {
            if (ReferenceEquals(queue[i], entry))
            {
                return i;
            }
        }

        return -1;
    }
}

/// <summary>
/// What Clear removed, kept so Undo can put it back. Undo is only honoured while the track that
/// was playing when the queue was cleared is still the one playing: after that, "back where they
/// were" no longer has a meaning.
/// </summary>
public sealed class ClearedQueue<T> where T : class
{
    public ClearedQueue(T? current, IReadOnlyList<T> removed, DateTimeOffset clearedAt)
    {
        Current = current;
        Removed = removed;
        ClearedAt = clearedAt;
    }

    public static readonly TimeSpan UndoWindow = TimeSpan.FromSeconds(8);

    public T? Current { get; }

    public IReadOnlyList<T> Removed { get; }

    public DateTimeOffset ClearedAt { get; }

    public bool CanUndo(T? current, DateTimeOffset now) =>
        Removed.Count > 0 && ReferenceEquals(current, Current) && now - ClearedAt <= UndoWindow;

    /// <summary>
    /// The entries to put back after the current one: what was removed, minus anything queued
    /// again since, so Undo never duplicates a track.
    /// </summary>
    public List<T> ToRestore(IReadOnlyList<T> queue) =>
        Removed.Where(entry => !queue.Any(existing => ReferenceEquals(existing, entry))).ToList();
}
