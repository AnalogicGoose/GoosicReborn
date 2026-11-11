using System;
using System.Collections.Generic;
using System.IO;
using System.Threading.Tasks;
using Goosic.Windows.Presentation;
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
    /// A song's menu has the macOS shell's order and names, so a song offers the same actions in
    /// the same places on both: playing, then the account's like and dislike, then where the song
    /// comes from, then playlists and its link. Library, likes and playlists belong to a signed-in
    /// account and are left out until the account profile exists, rather than shown as items that
    /// would fail.
    /// </remarks>
    private MenuFlyout? BuildMenu(object? item)
    {
        var menu = new MenuFlyout();
        switch (item)
        {
            case TrackViewModel track when Model.IsQueueEntry(track):
                Add(menu, "Play", MenuGlyphs.Play, async () => await PlayEntryAsync(Model.JumpTo(track)));
                Add(menu, "Start Radio", MenuGlyphs.Radio, async () => await PlayEntryAsync(Model.StartStation(track)));
                Add(menu, "Remove from Queue", MenuGlyphs.RemoveFromQueue, () => Model.RemoveFromQueue(track));
                AddAccountTrackItems(menu, track.VideoId, track.Title);
                AddNavigation(menu, track.ArtistId, track.AlbumId, track.Artist, track.Album);
                AddPlaylistItems(menu, track.VideoId, track.Title);
                AddLink(menu, ShellViewModel.LinkFor(track));
                break;

            case TrackViewModel track when !string.IsNullOrEmpty(track.VideoId):
                Add(menu, "Play", MenuGlyphs.Play, async () => await PlayEntryAsync(Model.PlayFromPage(track)));
                Add(menu, "Play Next", MenuGlyphs.PlayNext, () => Model.Enqueue(track, next: true));
                Add(menu, "Add to Queue", MenuGlyphs.AddToQueue, () => Model.Enqueue(track, next: false));
                Add(menu, "Start Radio", MenuGlyphs.Radio, async () => await PlayEntryAsync(Model.StartStation(track)));
                AddAccountTrackItems(menu, track.VideoId, track.Title);
                AddNavigation(menu, track.ArtistId, track.AlbumId, track.Artist, track.Album);
                AddPlaylistItems(menu, track.VideoId, track.Title);
                AddPinItem(menu, new SidebarPin("song", track.VideoId!, track.Title, track.Artist ?? "", track.Thumbnail));
                if (Model.IsSignedIn && Model.IsOwnedPlaylistPage && !string.IsNullOrEmpty(track.EntryId))
                {
                    Add(menu, "Remove from Playlist", MenuGlyphs.Delete, async () => await Model.RemoveFromPagePlaylistAsync(track));
                }

                AddLink(menu, ShellViewModel.LinkFor(track));
                break;

            case CardViewModel card when !string.IsNullOrEmpty(card.VideoId):
                Add(menu, "Play", MenuGlyphs.Play, async () => await PlayEntryAsync(Model.PlayFromShelf(card)));
                Add(menu, "Play Next", MenuGlyphs.PlayNext, () => Model.Enqueue(card, next: true));
                Add(menu, "Add to Queue", MenuGlyphs.AddToQueue, () => Model.Enqueue(card, next: false));
                Add(menu, "Start Radio", MenuGlyphs.Radio, async () => await PlayEntryAsync(Model.StartStation(card)));
                AddAccountTrackItems(menu, card.VideoId, card.Title);
                AddNavigation(menu, card.ArtistId, card.AlbumId, card.Artist, card.Album);
                AddPlaylistItems(menu, card.VideoId, card.Title);
                AddPinItem(menu, new SidebarPin("song", card.VideoId!, card.Title, card.Subtitle, card.Thumbnail));
                AddLink(menu, ShellViewModel.LinkFor(card));
                break;

            case CardViewModel card when card.Kind is "album" or "playlist" or "artist":
                Add(menu, "Open", MenuGlyphs.Open, async () =>
                {
                    Model.RememberEntityThumbnail(card.Kind, card.Id, card.Thumbnail);
                    await OpenAsync(card.Kind, card.Id, card.Title);
                });
                Add(menu, "Play", MenuGlyphs.Play, async () => await PlayEntryAsync(await Model.PlayEntityAsync(card.Kind, card.Id, shuffle: false)));
                Add(menu, "Shuffle", MenuGlyphs.Shuffle, async () => await PlayEntryAsync(await Model.PlayEntityAsync(card.Kind, card.Id, shuffle: true)));
                AddPinItem(menu, new SidebarPin(card.Kind, card.Id, card.Title, card.Subtitle, card.Thumbnail));
                if (Model.IsSignedIn && card.Kind == "playlist")
                {
                    menu.Items.Add(new MenuFlyoutSeparator());
                    if (Model.OwnsPlaylist(card.Id))
                    {
                        Add(menu, "Delete Playlist…", MenuGlyphs.Delete, async () => await ConfirmDeleteAsync(card.Id, card.Title));
                    }
                    else
                    {
                        Add(menu, "Add to Library", MenuGlyphs.AddToLibrary, async () => await Model.SavePlaylistAsync(card.Id, card.Title, saved: true));
                        Add(menu, "Remove from Library", MenuGlyphs.RemoveFromLibrary, async () => await Model.SavePlaylistAsync(card.Id, card.Title, saved: false));
                    }
                }
                else if (Model.IsSignedIn && card.Kind == "artist")
                {
                    menu.Items.Add(new MenuFlyoutSeparator());
                    Add(menu, "Subscribe", MenuGlyphs.Subscribe, async () => await Model.FollowArtistAsync(card.Id, card.Title, follow: true));
                    Add(menu, "Unsubscribe", MenuGlyphs.Unsubscribe, async () => await Model.FollowArtistAsync(card.Id, card.Title, follow: false));
                }

                AddLink(menu, ShellViewModel.LinkFor(card));
                break;

            default:
                return null;
        }

        return menu;
    }

    /// <summary>
    /// The menu for the song that is playing: the player bar's More, and the full player's.
    /// </summary>
    /// <remarks>
    /// The playing song is a queue entry, and a queue entry's own menu is about the queue: it had
    /// no like, dislike or "Add to Playlist", which is what someone reaching for the player's
    /// menu most often wants. This one offers what a song offers anywhere else, minus playing it.
    /// The full player's leaves out the ways into the full player and the bar's panels, which it
    /// already shows or covers.
    /// </remarks>
    private MenuFlyout BuildNowPlayingMenu(TrackViewModel track, bool inFullPlayer = false)
    {
        var menu = new MenuFlyout();
        Add(menu, "Start Radio", MenuGlyphs.Radio, async () => await PlayEntryAsync(Model.StartStation(track)));
        if (!inFullPlayer)
        {
            Add(menu, "Full-Screen Player", MenuGlyphs.FullScreen, () => SetFullPlayerOpen(true));
            Add(menu, "Mini Player", MenuGlyphs.MiniPlayer, () => SetMiniPlayer(true));
            Add(menu, "Lyrics", MenuGlyphs.Lyrics, () => OnToggleLyrics(this, new RoutedEventArgs()));
            Add(menu, "Playing Next", MenuGlyphs.Queue, () => OnToggleQueue(this, new RoutedEventArgs()));
        }

        Add(menu, Model.MuteLabel, Model.VolumeGlyph, () => OnToggleMuted(this, new RoutedEventArgs()));
        menu.Items.Add(BuildSleepTimerMenu());
        AddAccountTrackItems(menu, track.VideoId, track.Title);
        AddNavigation(menu, track.ArtistId, track.AlbumId, track.Artist, track.Album);
        AddPlaylistItems(menu, track.VideoId, track.Title);
        AddLink(menu, ShellViewModel.LinkFor(track));
        return menu;
    }

    /// <summary>Like and dislike, offered only while an account is active.</summary>
    private void AddAccountTrackItems(MenuFlyout menu, string? videoId, string title)
    {
        if (!Model.IsSignedIn || string.IsNullOrEmpty(videoId))
        {
            return;
        }

        menu.Items.Add(new MenuFlyoutSeparator());
        var rating = Model.RatingOf(videoId);
        Add(menu, rating == "LIKE" ? "Remove from Liked Music" : "Like", rating == "LIKE" ? MenuGlyphs.Unlike : MenuGlyphs.Like,
            async () => await Model.RateAsync(videoId, title, rating == "LIKE" ? "INDIFFERENT" : "LIKE"));
        Add(menu, rating == "DISLIKE" ? "Remove Dislike" : "Dislike", MenuGlyphs.Dislike,
            async () => await Model.RateAsync(videoId, title, rating == "DISLIKE" ? "INDIFFERENT" : "DISLIKE"));
    }

    /// <summary>Add to Playlist, offered only while an account is active.</summary>
    private void AddPlaylistItems(MenuFlyout menu, string? videoId, string title)
    {
        if (!Model.IsSignedIn || string.IsNullOrEmpty(videoId))
        {
            return;
        }

        menu.Items.Add(new MenuFlyoutSeparator());
        var save = new MenuFlyoutSubItem { Text = "Add to Playlist", Icon = Icon(MenuGlyphs.AddToPlaylist) };
        var create = new MenuFlyoutItem { Text = "New Playlist…", Icon = Icon(MenuGlyphs.New) };
        create.Click += async (_, _) => await NewPlaylistAsync(videoId);
        save.Items.Add(create);
        if (Model.UserPlaylists.Count > 0)
        {
            save.Items.Add(new MenuFlyoutSeparator());
        }

        foreach (var playlist in Model.UserPlaylists)
        {
            var item = new MenuFlyoutItem { Text = playlist.Title, Icon = Icon(MenuGlyphs.Playlist) };
            item.Click += async (_, _) => await Model.AddToPlaylistAsync(playlist, videoId, title);
            save.Items.Add(item);
        }

        menu.Items.Add(save);
    }

    /// <summary>
    /// Go to Artist and Go to Album. Both stay whenever the song names an artist or album and are
    /// only dimmed when the catalog gave no link, so the menu keeps its shape from song to song.
    /// </summary>
    private void AddNavigation(MenuFlyout menu, string? artistId, string? albumId, string? artist, string? album)
    {
        if (string.IsNullOrEmpty(artistId) && string.IsNullOrEmpty(albumId)
            && string.IsNullOrEmpty(artist) && string.IsNullOrEmpty(album))
        {
            return;
        }

        menu.Items.Add(new MenuFlyoutSeparator());
        var toArtist = Add(menu, "Go to Artist", MenuGlyphs.Artist, async () => await OpenAsync("artist", artistId!, artist ?? ""));
        toArtist.IsEnabled = !string.IsNullOrEmpty(artistId);
        var toAlbum = Add(menu, "Go to Album", MenuGlyphs.Album, async () => await OpenAsync("album", albumId!, album ?? ""));
        toAlbum.IsEnabled = !string.IsNullOrEmpty(albumId);
    }

    private void AddLink(MenuFlyout menu, string link)
    {
        menu.Items.Add(new MenuFlyoutSeparator());
        Add(menu, "Copy Link", MenuGlyphs.Link, () => CopyLink(link));
    }

    /// <summary>
    /// The Segoe Fluent Icons for menu items, named for what they stand for. Every item carries
    /// one, as every item in the macOS shell's menus carries a symbol: a menu where some rows have
    /// an icon and some do not reads as ragged.
    /// </summary>
    private static class MenuGlyphs
    {
        public const string Play = "\uE768";
        public const string PlayNext = "\uE7AC";
        public const string AddToQueue = "\uE710";
        public const string Radio = "\uEC05";
        public const string RemoveFromQueue = "\uE711";
        public const string Shuffle = "\uE8B1";
        public const string Like = "\uEB51";
        public const string Unlike = "\uEB52";
        public const string Dislike = "\uE8E0";
        public const string Artist = "\uE77B";
        public const string Album = "\uE93C";
        public const string AddToPlaylist = "\uE8F4";
        public const string Playlist = "\uE90B";
        public const string New = "\uE710";
        public const string Delete = "\uE74D";
        public const string Link = "\uE71B";
        public const string Open = "\uE8A7";
        public const string AddToLibrary = "\uE8F4";
        public const string RemoveFromLibrary = "\uE8F5";
        public const string Subscribe = "\uE8FA";
        public const string Unsubscribe = "\uE8F8";
        public const string FullScreen = "\uE740";
        public const string MiniPlayer = "\uEE49";
        public const string Lyrics = "\uE90A";
        public const string Queue = "\uE8FD";
    }

    private static FontIcon Icon(string glyph) => new() { Glyph = glyph };

    private static MenuFlyoutItem Add(MenuFlyout menu, string text, string glyph, Action action)
    {
        var item = new MenuFlyoutItem { Text = text, Icon = Icon(glyph) };
        item.Click += (_, _) => action();
        menu.Items.Add(item);
        return item;
    }

    private void CopyLink(string link)
    {
        var package = new DataPackage();
        package.SetText(link);
        Clipboard.SetContent(package);
        Model.ReportStatus("Link copied.");
    }
}
