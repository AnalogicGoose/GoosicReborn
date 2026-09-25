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
    // ---- Account ----------------------------------------------------------------------------

    private async void OnSignIn(object sender, RoutedEventArgs e)
    {
        AccountFlyout.Hide();
        await Model.SignInAsync();
    }

    private async void OnSignOut(object sender, RoutedEventArgs e)
    {
        AccountFlyout.Hide();
        if (await ConfirmAsync($"Sign out of {Model.AccountName}?",
                "Goosic forgets this account and clears its sign-in from this computer.", "Sign out"))
        {
            await Model.SignOutAsync();
        }
    }

    private async void OnUseAccount(object sender, RoutedEventArgs e)
    {
        AccountFlyout.Hide();
        if (sender is Button { Tag: string id })
        {
            await Model.SwitchAccountAsync(id);
        }
    }

    private async void OnUseGuest(object sender, RoutedEventArgs e)
    {
        AccountFlyout.Hide();
        await Model.SwitchAccountAsync(null);
    }

    private async void OnAccountRoute(object sender, RoutedEventArgs e)
    {
        AccountFlyout.Hide();
        if (sender is Button { Tag: string route })
        {
            DismissOverlaySidebar();
            await Model.LoadRouteAsync(route);
            ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
        }
    }

    private async void OnLibrarySection(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: LibrarySection section })
        {
            await Model.ShowLibrarySectionAsync(section);
        }
    }

    private async void OnSidebarPlaylist(object sender, RoutedEventArgs e)
    {
        if (sender is Button { Tag: string tag } && tag.Split(ShellViewModel.KeySeparator) is { Length: 3 } parts)
        {
            DismissOverlaySidebar();
            await OpenAsync(parts[0], parts[1], parts[2]);
        }
    }

    private async void OnNewPlaylist(object sender, RoutedEventArgs e)
    {
        await NewPlaylistAsync(null);
    }

    private async Task NewPlaylistAsync(string? videoId)
    {
        var answer = await PromptAsync("New playlist", "Title", "", "Create", privacy: true);
        if (answer is not { } chosen)
        {
            return;
        }

        var id = await Model.CreatePlaylistAsync(chosen.Text, chosen.Privacy, videoId);
        if (id is not null && videoId is null)
        {
            await OpenAsync("playlist", id, chosen.Text);
        }
    }

    private async void OnSavePagePlaylist(object sender, RoutedEventArgs e)
    {
        if (Model.PagePlaylistId is { } id)
        {
            await Model.SavePlaylistAsync(id, Model.PageTitle, saved: true);
        }
    }

    private async void OnFollowPageArtist(object sender, RoutedEventArgs e)
    {
        if (Model.PageArtistId is { } id)
        {
            await Model.FollowArtistAsync(id, Model.PageTitle, follow: !Model.IsPageArtistSubscribed);
        }
    }

    private async void OnRenamePagePlaylist(object sender, RoutedEventArgs e)
    {
        if (await PromptAsync("Rename playlist", "Title", Model.PageTitle, "Rename", privacy: false) is { } answer)
        {
            await Model.RenamePagePlaylistAsync(answer.Text);
        }
    }

    private async void OnPagePlaylistPrivacy(object sender, RoutedEventArgs e)
    {
        if (sender is MenuFlyoutItem { Tag: string privacy })
        {
            await Model.SetPagePlaylistPrivacyAsync(privacy);
        }
    }

    private async void OnDeletePagePlaylist(object sender, RoutedEventArgs e)
    {
        if (Model.PagePlaylistId is { } id)
        {
            await ConfirmDeleteAsync(id, Model.PageTitle);
        }
    }

    private async Task ConfirmDeleteAsync(string playlistId, string title)
    {
        if (await ConfirmAsync($"Delete “{title}”?",
                "The playlist is deleted from YouTube Music for good. This cannot be undone.", "Delete"))
        {
            await Model.DeletePlaylistAsync(playlistId, title);
        }
    }

    private async void OnLikeNowPlaying(object sender, RoutedEventArgs e)
    {
        // The button shows what YouTube Music accepted, not the click: a toggle flips itself,
        // and would stay lit over a like that was refused.
        await Model.ToggleNowPlayingRatingAsync("LIKE");
        if (sender is ToggleButton toggle)
        {
            toggle.IsChecked = Model.IsNowPlayingLiked;
        }
    }

    private async void OnDislikeNowPlaying(object sender, RoutedEventArgs e) =>
        await Model.ToggleNowPlayingRatingAsync("DISLIKE");
}
