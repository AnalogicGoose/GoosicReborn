using System;
using System.Collections.Generic;
using System.Collections.Specialized;
using System.Linq;
using Goosic.Windows.Presentation;

namespace Goosic.Windows.ViewModels;

/// <summary>One way to order a list page, as the Sort menu offers it.</summary>
public sealed record TrackSortChoice(string Label, string Key);

public sealed partial class ShellViewModel
{
    // ---- Sort and filter --------------------------------------------------------------------
    //
    // Spotify and Apple Music let a playlist be sorted by title, artist, album or length, and
    // searched in place. Both are views over the rows already loaded: nothing is written back to
    // the playlist, and "Custom order" returns to the order the playlist itself has.

    public IReadOnlyList<TrackSortChoice> TrackSortChoices { get; } =
        TrackOrder.Choices.Select(choice => new TrackSortChoice(choice.Label, choice.Key)).ToList();

    private string _trackSort = "custom";
    private string _trackFilter = "";
    private bool _sortingTracks;

    /// <summary>Sorting and filtering are offered on lists of songs someone put together.</summary>
    public bool CanSortTracks => HasTracks && _pageKind is DetailKind.Playlist && !IsEditingPlaylist;

    public string TrackSortLabel => "Sort: " + TrackOrder.Label(_trackSort);

    public string TrackFilter
    {
        get => _trackFilter;
        set
        {
            var text = value?.Trim() ?? "";
            if (text == _trackFilter)
            {
                return;
            }

            _trackFilter = text;
            OnPropertyChanged(nameof(TrackFilter));
            ApplyTrackFilter();
        }
    }

    /// <summary>Whether a sort or filter is hiding the list's own order, so editing waits.</summary>
    internal bool IsTrackViewCustomized => _trackSort != "custom" || _trackFilter.Length > 0;

    private void WireTrackView()
    {
        Tracks.CollectionChanged += (_, args) =>
        {
            if (_sortingTracks)
            {
                return;
            }

            if (args.Action == NotifyCollectionChangedAction.Reset)
            {
                // A new page starts in its own order, unfiltered.
                _trackSort = "custom";
                _trackFilter = "";
                OnPropertyChanged(nameof(TrackFilter));
                OnPropertyChanged(nameof(TrackSortLabel));
            }
            else if (args.Action == NotifyCollectionChangedAction.Add && args.NewItems is { } added)
            {
                foreach (TrackViewModel row in added)
                {
                    row.FilteredOut = !row.Matches(_trackFilter);
                }
            }

            OnPropertyChanged(nameof(CanSortTracks));
        };
    }

    internal void SortTracks(string key)
    {
        _trackSort = key;
        OnPropertyChanged(nameof(TrackSortLabel));
        ReorderTracks();
    }

    /// <summary>Puts the loaded rows in the chosen order, moving only the rows out of place.</summary>
    private void ReorderTracks()
    {
        var target = TrackOrder.Sort(Tracks, _trackSort);
        _sortingTracks = true;
        try
        {
            for (var i = 0; i < target.Count; i++)
            {
                var from = Tracks.IndexOf(target[i]);
                if (from != i)
                {
                    Tracks.Move(from, i);
                }
            }
        }
        finally
        {
            _sortingTracks = false;
        }

        PresentTracks();
    }

    private void ApplyTrackFilter()
    {
        foreach (var row in Tracks)
        {
            row.FilteredOut = !row.Matches(_trackFilter);
        }
    }

    /// <summary>Rows a page's Play button plays: the ones on screen, in the order on screen.</summary>
    internal IEnumerable<TrackViewModel> VisibleTracks => Tracks.Where(row => !row.FilteredOut);
}
