using System;
using System.Collections.Specialized;
using System.Linq;
using System.Text.Json.Nodes;
using System.Threading.Tasks;
using Goosic.Windows.Presentation;
using Goosic.Windows.Service;

namespace Goosic.Windows.ViewModels;

/// <summary>
/// Editing a playlist the account owns: choosing songs, removing them, and moving them.
/// </summary>
/// <remarks>
/// Every change goes to YouTube Music through the personal reader, in the account's own profile,
/// one entry at a time and by the entry's own id, since a playlist can hold the same song twice.
/// The screen changes first and is reloaded from YouTube Music if a request is refused, so what
/// it shows never stays different from the playlist itself.
/// </remarks>
public sealed partial class ShellViewModel
{
    private bool _isEditingPlaylist;
    private bool _editBusy;

    /// <summary>Whether the playlist on screen is being edited: rows select instead of playing.</summary>
    public bool IsEditingPlaylist { get => _isEditingPlaylist; private set => Set(ref _isEditingPlaylist, value); }

    public int SelectedCount => IsEditingPlaylist ? Tracks.Count(track => track.IsSelected) : 0;

    public bool HasSelection => SelectedCount > 0;

    public bool CanEditSelection => HasSelection && !_editBusy;

    public string SelectionSummary => SelectedCount switch
    {
        0 => "Choose songs to move or remove",
        1 => "1 song selected",
        var count => $"{count} songs selected",
    };

    internal void BeginPlaylistEdit()
    {
        if (!IsOwnedPlaylistPage || IsEditingPlaylist)
        {
            return;
        }

        IsEditingPlaylist = true;
        Tracks.CollectionChanged += OnEditedTracksChanged;
        foreach (var track in Tracks)
        {
            Watch(track);
        }

        SelectionChangedForEdit();
    }

    internal void EndPlaylistEdit()
    {
        if (!IsEditingPlaylist)
        {
            return;
        }

        Tracks.CollectionChanged -= OnEditedTracksChanged;
        foreach (var track in Tracks)
        {
            track.SelectionChanged -= SelectionChangedForEdit;
            track.IsEditing = false;
        }

        IsEditingPlaylist = false;
        SelectionChangedForEdit();
    }

    internal void SelectAllForEdit(bool selected)
    {
        foreach (var track in Tracks)
        {
            track.IsSelected = selected;
        }
    }

    /// <summary>A row that arrives while editing, from the next part of a long playlist, can be chosen too.</summary>
    private void OnEditedTracksChanged(object? sender, NotifyCollectionChangedEventArgs e)
    {
        foreach (var track in e.NewItems?.OfType<TrackViewModel>() ?? [])
        {
            Watch(track);
        }

        SelectionChangedForEdit();
    }

    private void Watch(TrackViewModel track)
    {
        track.SelectionChanged -= SelectionChangedForEdit;
        track.SelectionChanged += SelectionChangedForEdit;
        track.IsEditing = true;
    }

    private void SelectionChangedForEdit()
    {
        OnPropertyChanged(nameof(SelectedCount));
        OnPropertyChanged(nameof(HasSelection));
        OnPropertyChanged(nameof(CanEditSelection));
        OnPropertyChanged(nameof(SelectionSummary));
    }

    private void SetEditBusy(bool busy)
    {
        _editBusy = busy;
        OnPropertyChanged(nameof(CanEditSelection));
    }

    /// <summary>Removes every chosen song from the playlist.</summary>
    internal async Task RemoveSelectedAsync()
    {
        if (_pagePlaylistId is not { } playlist || Personal is null || _editBusy)
        {
            return;
        }

        var chosen = Tracks.Where(track => track.IsSelected).ToList();
        if (chosen.Any(track => string.IsNullOrEmpty(track.EntryId) || string.IsNullOrEmpty(track.VideoId)))
        {
            ReportStatus("Some of those songs can't be removed here. Reload the playlist and try again.");
            return;
        }

        SetEditBusy(true);
        var removed = 0;
        try
        {
            foreach (var track in chosen)
            {
                await Personal.MutateAsync("removeFromPlaylist", new JsonObject
                {
                    ["playlistId"] = playlist,
                    ["videoId"] = track.VideoId,
                    ["setVideoId"] = track.EntryId,
                }).ConfigureAwait(true);
                Tracks.Remove(track);
                removed++;
            }

            ReportStatus(removed == 1 ? "Removed 1 song from this playlist." : $"Removed {removed} songs from this playlist.");
        }
        catch (Exception error)
        {
            BridgeLog.Write($"playlist remove stopped after {removed}: {error.Message}");
            ReportStatus($"YouTube Music stopped after removing {removed} of {chosen.Count}: {Describe(error)}");
        }
        finally
        {
            PresentTracks();
            SetEditBusy(false);
            SelectionChangedForEdit();
        }
    }

    /// <summary>Moves the chosen songs one place up or down, keeping their order among themselves.</summary>
    internal async Task MoveSelectedAsync(bool up)
    {
        if (_pagePlaylistId is not { } playlist || Personal is null || _editBusy || !HasSelection)
        {
            return;
        }

        // A move names its neighbour by entry id, so every row taking part needs one.
        if (Tracks.Any(track => string.IsNullOrEmpty(track.EntryId)))
        {
            ReportStatus("This playlist can't be reordered here. Reload it and try again.");
            return;
        }

        var (order, moves) = PlaylistReorder.Step(Tracks.ToList(), track => track.IsSelected, up);
        if (moves.Count == 0)
        {
            return;
        }

        SetEditBusy(true);
        for (var index = 0; index < order.Count; index++)
        {
            var from = Tracks.IndexOf(order[index]);
            if (from != index)
            {
                Tracks.Move(from, index);
            }
        }

        PresentTracks();
        try
        {
            foreach (var move in moves)
            {
                var arguments = new JsonObject { ["playlistId"] = playlist, ["setVideoId"] = move.Moved.EntryId };
                if (move.Before is { } successor)
                {
                    arguments["successorSetVideoId"] = successor.EntryId;
                }

                await Personal.MutateAsync("movePlaylistItem", arguments).ConfigureAwait(true);
            }
        }
        catch (Exception error)
        {
            BridgeLog.Write($"playlist move failed: {error.Message}");
            ReportStatus("YouTube Music didn't accept that move, so the playlist was reloaded: " + Describe(error));
            EndPlaylistEdit();
            await RetryPageAsync().ConfigureAwait(true);
        }
        finally
        {
            SetEditBusy(false);
        }
    }
}
