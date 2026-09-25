using System;
using System.Collections.Generic;
using System.IO;
using System.Threading.Tasks;
using Goosic.Windows.Service;
using Goosic.Windows.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Windows.ApplicationModel.DataTransfer;
using Windows.Media;
using Windows.System;

namespace Goosic.Windows;

public sealed partial class MainWindow : Window
{
    // ---- Context menus ----------------------------------------------------------------------

    private void OnItemMore(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement element && BuildMenu(element.DataContext) is { } menu)
        {
            menu.ShowAt(element, new FlyoutShowOptions { Placement = FlyoutPlacementMode.BottomEdgeAlignedRight });
        }
    }

    // ---- Track row overflow -------------------------------------------------------------------

    private void OnTrackRowPointerEntered(object sender, PointerRoutedEventArgs e) => ShowRowMore(sender, true);

    private void OnTrackRowPointerExited(object sender, PointerRoutedEventArgs e) => ShowRowMore(sender, false);

    /// <summary>
    /// Keeps More visible while focus is anywhere in the row. Focus events bubble, so moving from
    /// the row to its More button arrives here too.
    /// </summary>
    private void OnTrackRowFocusChanged(object sender, RoutedEventArgs e)
    {
        if (sender is Button row)
        {
            var focused = FocusManager.GetFocusedElement(RootGrid.XamlRoot) as DependencyObject;
            ShowRowMore(row, focused is not null && (ReferenceEquals(focused, row) || IsDescendant(focused, row)));
        }
    }

    private static void ShowRowMore(object sender, bool visible)
    {
        if (sender is DependencyObject row && FindMoreButton(row) is { } more)
        {
            more.Opacity = visible || more.FocusState != FocusState.Unfocused ? 1 : 0;
        }
    }

    private static Button? FindMoreButton(DependencyObject root)
    {
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(root); i++)
        {
            var child = VisualTreeHelper.GetChild(root, i);
            if (child is Button { Tag: "more" } more)
            {
                return more;
            }

            if (FindMoreButton(child) is { } found)
            {
                return found;
            }
        }

        return null;
    }

    private static bool IsDescendant(DependencyObject node, DependencyObject ancestor)
    {
        for (var current = VisualTreeHelper.GetParent(node); current is not null; current = VisualTreeHelper.GetParent(current))
        {
            if (ReferenceEquals(current, ancestor))
            {
                return true;
            }
        }

        return false;
    }

    private void OnItemContextRequested(UIElement sender, ContextRequestedEventArgs args)
    {
        if (sender is not FrameworkElement element || BuildMenu(element.DataContext) is not { } menu)
        {
            return;
        }

        args.Handled = true;
        var options = new FlyoutShowOptions();
        if (args.TryGetPosition(element, out var point))
        {
            options.Position = point;
        }

        menu.ShowAt(element, options);
    }

    /// <summary>
    /// The menu for a row or a card, offering only what that item can do.
    /// </summary>
    /// <remarks>
    /// Library, likes and playlists belong to a signed-in account and are left out until the
    /// account profile exists, rather than shown as items that would fail.
    /// </remarks>
    private MenuFlyout? BuildMenu(object? item)
    {
        var menu = new MenuFlyout();
        switch (item)
        {
            case TrackViewModel track when Model.IsQueueEntry(track):
                Add(menu, "Play", "", async () => await PlayEntryAsync(Model.JumpTo(track)));
                Add(menu, "Start radio", "", async () => await PlayEntryAsync(Model.StartStation(track)));
                menu.Items.Add(new MenuFlyoutSeparator());
                AddNavigation(menu, track.ArtistId, track.AlbumId, track.Subtitle, track.Title);
                Add(menu, "Copy link", "", () => CopyLink(ShellViewModel.LinkFor(track)));
                menu.Items.Add(new MenuFlyoutSeparator());
                Add(menu, "Remove from queue", "", () => Model.RemoveFromQueue(track));
                break;

            case TrackViewModel track when !string.IsNullOrEmpty(track.VideoId):
                Add(menu, "Play", "\uE768", async () => await PlayEntryAsync(Model.PlayFromPage(track)));
                Add(menu, "Start radio", "\uEC05", async () => await PlayEntryAsync(Model.StartStation(track)));
                menu.Items.Add(new MenuFlyoutSeparator());
                Add(menu, "Play next", "\uE7AC", () => Model.Enqueue(track, next: true));
                Add(menu, "Add to queue", "\uE710", () => Model.Enqueue(track, next: false));
                AddAccountTrackItems(menu, track.VideoId, track.Title);
                if (Model.IsOwnedPlaylistPage && !string.IsNullOrEmpty(track.EntryId))
                {
                    Add(menu, "Remove from playlist", "\uE74D", async () => await Model.RemoveFromPagePlaylistAsync(track));
                }

                menu.Items.Add(new MenuFlyoutSeparator());
                AddNavigation(menu, track.ArtistId, track.AlbumId, track.Subtitle, track.Title);
                Add(menu, "Copy link", "", () => CopyLink(ShellViewModel.LinkFor(track)));
                break;

            case CardViewModel card when !string.IsNullOrEmpty(card.VideoId):
                Add(menu, "Play", "", async () => await PlayEntryAsync(Model.PlayFromShelf(card)));
                Add(menu, "Start radio", "", async () => await PlayEntryAsync(Model.StartStation(card)));
                menu.Items.Add(new MenuFlyoutSeparator());
                Add(menu, "Play next", "\uE7AC", () => Model.Enqueue(card, next: true));
                Add(menu, "Add to queue", "\uE710", () => Model.Enqueue(card, next: false));
                AddAccountTrackItems(menu, card.VideoId, card.Title);
                menu.Items.Add(new MenuFlyoutSeparator());
                AddNavigation(menu, card.ArtistId, card.AlbumId, card.Subtitle, card.Title);
                Add(menu, "Copy link", "", () => CopyLink(ShellViewModel.LinkFor(card)));
                break;

            case CardViewModel card when card.Kind is "album" or "playlist" or "artist":
                Add(menu, "Play", "", async () => await PlayEntryAsync(await Model.PlayEntityAsync(card.Kind, card.Id, shuffle: false)));
                Add(menu, "Shuffle", "", async () => await PlayEntryAsync(await Model.PlayEntityAsync(card.Kind, card.Id, shuffle: true)));
                menu.Items.Add(new MenuFlyoutSeparator());
                Add(menu, card.Kind == "artist" ? "Go to artist" : card.Kind == "album" ? "Go to album" : "Open playlist",
                    "\uE8A7", async () =>
                    {
                        Model.RememberEntityThumbnail(card.Kind, card.Id, card.Thumbnail);
                        await OpenAsync(card.Kind, card.Id, card.Title);
                    });
                if (Model.IsSignedIn && card.Kind == "playlist")
                {
                    if (Model.OwnsPlaylist(card.Id))
                    {
                        Add(menu, "Delete playlist…", "\uE74D", async () => await ConfirmDeleteAsync(card.Id, card.Title));
                    }
                    else
                    {
                        Add(menu, "Save to library", "\uE8F4", async () => await Model.SavePlaylistAsync(card.Id, card.Title, saved: true));
                        Add(menu, "Remove from library", "\uE8F5", async () => await Model.SavePlaylistAsync(card.Id, card.Title, saved: false));
                    }
                }
                else if (Model.IsSignedIn && card.Kind == "artist")
                {
                    Add(menu, "Subscribe", "\uE8FA", async () => await Model.FollowArtistAsync(card.Id, card.Title, follow: true));
                    Add(menu, "Unsubscribe", "\uE8F8", async () => await Model.FollowArtistAsync(card.Id, card.Title, follow: false));
                }
                Add(menu, "Copy link", "", () => CopyLink(ShellViewModel.LinkFor(card)));
                break;

            default:
                return null;
        }

        return menu;
    }

    /// <summary>
    /// The player bar's menu, for the song that is playing.
    /// </summary>
    /// <remarks>
    /// The playing song is a queue entry, and a queue entry's own menu is about the queue: it had
    /// no like, dislike or "Save to playlist", which is what someone reaching for the player's
    /// menu most often wants. This one offers what a song offers anywhere else, minus playing it.
    /// </remarks>
    private MenuFlyout BuildNowPlayingMenu(TrackViewModel track)
    {
        var menu = new MenuFlyout();
        Add(menu, "Full-screen player", "", () => SetFullPlayerOpen(true));
        Add(menu, "Start radio", "", async () => await PlayEntryAsync(Model.StartStation(track)));
        Add(menu, "Mini player", "", () => SetMiniPlayer(true));
        menu.Items.Add(BuildSleepTimerMenu());
        AddAccountTrackItems(menu, track.VideoId, track.Title);
        menu.Items.Add(new MenuFlyoutSeparator());
        AddNavigation(menu, track.ArtistId, track.AlbumId, track.Subtitle, track.Title);
        Add(menu, "Copy link", "", () => CopyLink(ShellViewModel.LinkFor(track)));
        return menu;
    }

    /// <summary>Like, dislike and save to playlist, offered only while an account is active.</summary>
    private void AddAccountTrackItems(MenuFlyout menu, string? videoId, string title)
    {
        if (!Model.IsSignedIn || string.IsNullOrEmpty(videoId))
        {
            return;
        }

        menu.Items.Add(new MenuFlyoutSeparator());
        var rating = Model.RatingOf(videoId);
        Add(menu, rating == "LIKE" ? "Remove from Liked Music" : "Like", "\uEB51",
            async () => await Model.RateAsync(videoId, title, rating == "LIKE" ? "INDIFFERENT" : "LIKE"));
        Add(menu, rating == "DISLIKE" ? "Remove dislike" : "Dislike", "\uE8E0",
            async () => await Model.RateAsync(videoId, title, rating == "DISLIKE" ? "INDIFFERENT" : "DISLIKE"));

        var save = new MenuFlyoutSubItem { Text = "Save to playlist", Icon = new FontIcon { Glyph = "\uE8F4" } };
        var create = new MenuFlyoutItem { Text = "New playlist…", Icon = new FontIcon { Glyph = "\uE710" } };
        create.Click += async (_, _) => await NewPlaylistAsync(videoId);
        save.Items.Add(create);
        if (Model.UserPlaylists.Count > 0)
        {
            save.Items.Add(new MenuFlyoutSeparator());
        }

        foreach (var playlist in Model.UserPlaylists)
        {
            var item = new MenuFlyoutItem { Text = playlist.Title };
            item.Click += async (_, _) => await Model.AddToPlaylistAsync(playlist, videoId, title);
            save.Items.Add(item);
        }

        menu.Items.Add(save);
    }

    private void AddNavigation(MenuFlyout menu, string? artistId, string? albumId, string artist, string title)
    {
        if (!string.IsNullOrEmpty(artistId))
        {
            Add(menu, "Go to artist", "", async () => await OpenAsync("artist", artistId, artist));
        }

        if (!string.IsNullOrEmpty(albumId))
        {
            Add(menu, "Go to album", "", async () => await OpenAsync("album", albumId, title));
        }
    }

    private static void Add(MenuFlyout menu, string text, string glyph, Action action)
    {
        var item = new MenuFlyoutItem { Text = text, Icon = new FontIcon { Glyph = glyph } };
        item.Click += (_, _) => action();
        menu.Items.Add(item);
    }

    private void CopyLink(string link)
    {
        var package = new DataPackage();
        package.SetText(link);
        Clipboard.SetContent(package);
        Model.ReportStatus("Link copied.");
    }
}
