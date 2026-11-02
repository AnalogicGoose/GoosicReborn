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
    private bool _sidebarExpanded;
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
        if (Model.CurrentSidebarDestination == tag || tag == "library" && Model.IsLibraryPage)
        {
            HighlightNavigation(Model.CurrentSidebarDestination);
            return;
        }

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
        if (Model.IsSettingsPage)
        {
            PlayPageEntrance();
        }
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
                item.IsChecked = railTag == tag;
            }
        }

        UpdatePlaylistSelection(SidebarPlaylists);
        UpdateSidebarIndicator();
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

    /// <summary>Every new page starts at its top, whichever path loaded it.</summary>
    private void OnPageLoadingChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs args)
    {
        if (args.PropertyName == nameof(ShellViewModel.IsSearchPage))
        {
            ApplyInsets();
        }
        else if (args.PropertyName == nameof(ShellViewModel.IsPageLoading) && Model.IsPageLoading)
        {
            _catalogArrivalIndex = 0;
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
        ApplyInsets();
    }

    private void SetSidebarExpanded(bool expanded)
    {
        var previousWidth = Sidebar.ActualWidth;
        // Move focus before its old control collapses; the renderer and model are untouched.
        if (expanded != _sidebarExpanded)
        {
            SidebarToggle.Focus(FocusState.Programmatic);
        }

        _sidebarExpanded = expanded;
        SidebarDismissLayer.Visibility = expanded ? Visibility.Visible : Visibility.Collapsed;
        var visible = expanded ? Visibility.Visible : Visibility.Collapsed;
        SidebarBrandName.Visibility = visible;
        SidebarSearchTab.Visibility = visible;
        SidebarNavigation.Visibility = visible;
        AccountDetails.Visibility = visible;
        SidebarRail.Visibility = expanded ? Visibility.Collapsed : Visibility.Visible;
        SidebarSearchButton.Visibility = expanded ? Visibility.Collapsed : Visibility.Visible;
        Sidebar.Width = expanded
            ? WindowLayout.SidebarWidth(_widthClass, RootGrid.ActualWidth)
            : WindowLayout.SidebarRailWidth;
        var label = expanded ? "Collapse sidebar" : "Expand sidebar";
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(SidebarToggle, label);
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetHelpText(SidebarToggle, "Ctrl+B");
        ToolTipService.SetToolTip(SidebarToggle, $"{label} (Ctrl+B)");
        if (expanded)
        {
            SidebarNavigation.ChangeView(null, 0, null, disableAnimation: true);
        }

        AnimateSidebarExpansion(previousWidth, expanded);
    }

    /// <summary>
    /// Expansion is temporary at every width; choosing a destination restores the rail.
    /// </summary>
    private void DismissOverlaySidebar()
    {
        if (_sidebarExpanded)
        {
            ToggleSidebar();
        }
    }

    private void OnDismissSidebar(object sender, TappedRoutedEventArgs e)
    {
        DismissOverlaySidebar();
        e.Handled = true;
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
                ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
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
        if (string.IsNullOrWhiteSpace(box.Text))
        {
            box.IsSuggestionListOpen = false;
            return;
        }

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

    private async void OnSearchSubmitButton(object sender, RoutedEventArgs e)
    {
        SearchQuery.IsSuggestionListOpen = false;
        await SubmitSearchAsync(SearchQuery.Text);
    }

    private void OnClearRecentSearches(object sender, RoutedEventArgs e)
    {
        ShellPreferences.RecentSearches = Array.Empty<string>();
        SearchQuery.ItemsSource = null;
        SearchQuery.IsSuggestionListOpen = false;
        UpdateRecentSearches();
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
        if (Model.IsSearchPage && sender is Selector { SelectedValue: string filter })
        {
            await Model.RefilterSearchAsync(filter);
            ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
            UpdateBackButton();
        }
    }

    private async void OnBack(object sender, RoutedEventArgs e) => await GoBackAsync();

    private async Task GoBackAsync()
    {
        DismissOverlaySidebar();
        await Model.GoBackAsync();
        if (Model.IsSettingsPage || Model.ShowsSearchLanding)
        {
            PlayPageEntrance();
        }
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
