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
    private bool _sidebarExpanded = true;
    private string? _navigationTag = "home";
    // ---- Navigation -------------------------------------------------------------------------

    /// <summary>
    /// A sidebar item: a route, or a library section written as <c>library:browseId</c>.
    /// </summary>
    private async void OnNavigate(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { Tag: string tag })
        {
            await NavigateAsync(tag);
        }
    }

    private async Task NavigateAsync(string tag)
    {
        if (tag == "search")
        {
            await OpenSearchAsync();
            return;
        }

        HighlightNavigation(tag);
        DismissOverlaySidebar();
        if (tag.StartsWith("library:", StringComparison.Ordinal))
        {
            await Model.OpenLibrarySectionAsync(tag["library:".Length..]);
        }
        else
        {
            await Model.LoadRouteAsync(tag);
        }

        ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
        UpdateBackButton();
    }

    /// <summary>Marks the sidebar item for the page on screen, as the macOS sidebar selects its row.</summary>
    private void HighlightNavigation(string? tag)
    {
        _navigationTag = tag;
        SidebarSearchTab.IsChecked = tag == "search";
        SidebarSearchButton.IsChecked = tag == "search";
        foreach (var child in SidebarItems.Children)
        {
            if (child is ToggleButton { Tag: string itemTag } item)
            {
                item.IsChecked = itemTag == tag;
            }
        }

        foreach (var child in SidebarRailItems.Children)
        {
            if (child is ToggleButton { Tag: string railTag } item)
            {
                item.IsChecked = railTag == tag || railTag == SidebarGroup(tag);
            }
        }

        UpdatePlaylistSelection(SidebarPlaylists);
    }

    private void OnSidebarPlaylistLoaded(object sender, RoutedEventArgs e)
    {
        if (sender is ToggleButton item)
        {
            item.IsChecked = IsCurrentSidebarPlaylist(item);
        }
    }

    private bool IsCurrentSidebarPlaylist(ToggleButton item) =>
        item.DataContext is PlaylistSummaryViewModel playlist
        && Model.CurrentRouteName is null
        && Model.PagePlaylistId is { } id
        && string.Equals(playlist.Id.StartsWith("VL", StringComparison.Ordinal) ? playlist.Id[2..] : playlist.Id,
            id, StringComparison.Ordinal);

    private void UpdatePlaylistSelection(DependencyObject parent)
    {
        for (var index = 0; index < VisualTreeHelper.GetChildrenCount(parent); index++)
        {
            var child = VisualTreeHelper.GetChild(parent, index);
            if (child is ToggleButton item)
            {
                item.IsChecked = IsCurrentSidebarPlaylist(item);
            }
            else
            {
                UpdatePlaylistSelection(child);
            }
        }
    }

    private static string? SidebarGroup(string? tag) => tag switch
    {
        "explore" or "charts" or "moodsAndGenres" or "newReleases" => "discover",
        "library" or "liked" or "history" or "downloads" => "library",
        "library:FEmusic_liked_playlists" => "playlists",
        not null when tag.StartsWith("library:", StringComparison.Ordinal) => "library",
        _ => null,
    };

    /// <summary>Every new page starts at its top, whichever path loaded it.</summary>
    private void OnPageLoadingChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs args)
    {
        if (args.PropertyName == nameof(ShellViewModel.IsSearchPage))
        {
            ApplyInsets();
        }
        else if (args.PropertyName == nameof(ShellViewModel.IsPageLoading) && Model.IsPageLoading)
        {
            ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
        }
        else if (args.PropertyName == nameof(ShellViewModel.IsPageLoading))
        {
            HighlightNavigation(Model.CurrentSidebarDestination);
            PlayPageEntrance();
        }
    }

    /// <summary>Opens every song behind an artist's top few, as a playlist.</summary>
    private async void OnShowAllTracks(object sender, RoutedEventArgs e)
    {
        if (Model.AllTracksId is not { } id)
        {
            return;
        }

        await Model.OpenEntityAsync("playlist", id, $"{Model.PageTitle}: songs");
        ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
        UpdateBackButton();
    }

    private void UpdateBackButton() =>
        BackButton.Visibility = Model.CanGoBack ? Visibility.Visible : Visibility.Collapsed;

    /// <summary>Switches between the compact rail and the expanded navigation panel.</summary>
    private void OnToggleSidebar(object sender, RoutedEventArgs e) => ToggleSidebar();

    private void ToggleSidebar()
    {
        SetSidebarExpanded(!_sidebarExpanded);
        GlidePlayerOnce();
        ApplyInsets();
    }

    private void SetSidebarExpanded(bool expanded)
    {
        // Move focus before its old control collapses; the renderer and model are untouched.
        if (expanded != _sidebarExpanded)
        {
            SidebarToggle.Focus(FocusState.Programmatic);
        }

        _sidebarExpanded = expanded;
        var visible = expanded ? Visibility.Visible : Visibility.Collapsed;
        SidebarBrandName.Visibility = visible;
        SidebarSearchTab.Visibility = visible;
        SidebarNavigation.Visibility = visible;
        AccountDetails.Visibility = visible;
        SidebarRail.Visibility = expanded ? Visibility.Collapsed : Visibility.Visible;
        SidebarRailSurface.Visibility = SidebarRail.Visibility;
        SidebarSearchButton.Visibility = expanded ? Visibility.Collapsed : Visibility.Visible;
        Sidebar.Width = expanded
            ? WindowLayout.SidebarWidth(_widthClass, RootGrid.ActualWidth)
            : WindowLayout.SidebarRailWidth;
        var label = expanded ? "Collapse sidebar" : "Expand sidebar";
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(SidebarToggle, label);
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetHelpText(SidebarToggle, "Ctrl+B");
        ToolTipService.SetToolTip(SidebarToggle, $"{label} (Ctrl+B)");
    }

    private void OnExpandSidebarGroup(object sender, RoutedEventArgs e)
    {
        if (sender is not FrameworkElement { Tag: string group })
        {
            return;
        }

        if (!_sidebarExpanded)
        {
            ToggleSidebar();
        }

        HighlightNavigation(_navigationTag);
        // Expansion must be measured before revealing a row that was outside the short viewport.
        SidebarNavigation.UpdateLayout();
        var firstTag = group switch
        {
            "discover" => "explore",
            "library" => "liked",
            _ => "library:FEmusic_liked_playlists",
        };
        foreach (var child in SidebarItems.Children)
        {
            if (child is ToggleButton { Tag: string tag } item && tag == firstTag)
            {
                item.Focus(FocusState.Programmatic);
                item.StartBringIntoView(new BringIntoViewOptions { AnimationDesired = false });
                break;
            }
        }
    }

    /// <summary>
    /// Closes the sidebar after a choice where it covers the page, so the choice is visible.
    /// </summary>
    private void DismissOverlaySidebar()
    {
        if (WindowLayout.ClosesSidebarAfterNavigation(_widthClass) && _sidebarExpanded)
        {
            ToggleSidebar();
        }
    }

    private async void OnRetryPage(object sender, RoutedEventArgs e)
    {
        PageRetryButton.IsEnabled = false;
        await Model.RetryPageAsync();
        PageRetryButton.IsEnabled = true;
    }

    private void OpenSearch() => _ = OpenSearchAsync();

    private async Task OpenSearchAsync()
    {
        UpdateRecentSearches();
        var alreadyOpen = Model.IsSearchPage;
        var navigation = Model.OpenSearchPageAsync();
        HighlightNavigation("search");
        DismissOverlaySidebar();
        ApplyInsets();
        SearchChrome.UpdateLayout();
        SearchQuery.Focus(FocusState.Programmatic);
        if (!alreadyOpen)
        {
            ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
        }

        await navigation;
        UpdateBackButton();
    }

    /// <summary>
    /// Offers recent searches as the box opens and as it is typed in, as Spotify's and Apple
    /// Music's search does. They are kept on this computer and never sent anywhere.
    /// </summary>
    private void OnSearchTextChanged(AutoSuggestBox sender, AutoSuggestBoxTextChangedEventArgs args)
    {
        if (args.Reason == AutoSuggestionBoxTextChangeReason.UserInput)
        {
            Model.SearchText = sender.Text;
            if (string.IsNullOrWhiteSpace(sender.Text) && Model.IsSearchPage)
            {
                Model.ShowSearchLanding();
                UpdateRecentSearches();
                UpdateBackButton();
            }

            ShowRecentSearches(sender);
        }
    }

    private void OnSearchGotFocus(object sender, RoutedEventArgs e)
    {
        if (sender is AutoSuggestBox box && box.Text.Length == 0)
        {
            ShowRecentSearches(box);
        }
    }

    private static void ShowRecentSearches(AutoSuggestBox box)
    {
        var matches = RecentSearches.Suggest(ShellPreferences.RecentSearches, box.Text);
        box.ItemsSource = matches;
        box.IsSuggestionListOpen = matches.Count > 0;
    }

    private void UpdateRecentSearches()
    {
        var recent = ShellPreferences.RecentSearches;
        RecentSearchItems.ItemsSource = recent;
        RecentSearchSection.Visibility = recent.Count > 0 ? Visibility.Visible : Visibility.Collapsed;
    }

    private async void OnSearchSubmitted(AutoSuggestBox sender, AutoSuggestBoxQuerySubmittedEventArgs args)
    {
        var query = args.ChosenSuggestion as string ?? args.QueryText;
        sender.Text = query;
        sender.IsSuggestionListOpen = false;
        await SubmitSearchAsync(query);
    }

    private async Task SubmitSearchAsync(string query)
    {
        if (!string.IsNullOrWhiteSpace(query))
        {
            ShellPreferences.RecentSearches = RecentSearches.Remember(ShellPreferences.RecentSearches, query);
        }

        UpdateRecentSearches();
        HighlightNavigation("search");
        await Model.SearchAsync(query, Model.SearchFilter);
        ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
        UpdateBackButton();
    }

    private async void OnRecentSearch(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { Tag: string query })
        {
            await SubmitSearchAsync(query);
        }
    }

    private async void OnSearchCategoryChanged(object sender, SelectionChangedEventArgs e)
    {
        if (Model.IsSearchPage && sender is ComboBox { SelectedValue: string filter })
        {
            await Model.RefilterSearchAsync(filter);
            ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
            UpdateBackButton();
        }
    }

    private async void OnBack(object sender, RoutedEventArgs e) => await GoBackAsync();

    private async Task GoBackAsync()
    {
        await Model.GoBackAsync();
        HighlightNavigation(Model.CurrentSidebarDestination);
        ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
        UpdateBackButton();
    }

    private async Task OpenAsync(string kind, string id, string title)
    {
        HighlightNavigation(null);
        await Model.OpenEntityAsync(kind, id, title);
        // Some entities open a route of their own, such as Liked Music; its sidebar row lights up.
        HighlightNavigation(Model.CurrentSidebarDestination);
        ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
        UpdateBackButton();
    }
}
