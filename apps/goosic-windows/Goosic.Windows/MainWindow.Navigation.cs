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
    // ---- Navigation -------------------------------------------------------------------------

    /// <summary>
    /// A sidebar item: a route, or a library section written as <c>library:browseId</c>.
    /// </summary>
    private async void OnNavigate(object sender, RoutedEventArgs e)
    {
        if (sender is Button { Tag: string tag })
        {
            await NavigateAsync(tag);
        }
    }

    private async Task NavigateAsync(string tag)
    {
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
        var selected = (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["GoosicSidebarSelectedBrush"];
        foreach (var child in SidebarItems.Children)
        {
            if (child is Button { Tag: string itemTag } item)
            {
                item.Background = itemTag == tag ? selected : new Microsoft.UI.Xaml.Media.SolidColorBrush(Microsoft.UI.Colors.Transparent);
            }
        }
    }

    /// <summary>Every new page starts at its top, whichever path loaded it.</summary>
    private void OnPageLoadingChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs args)
    {
        if (args.PropertyName == nameof(ShellViewModel.IsPageLoading) && Model.IsPageLoading)
        {
            ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
        }
        else if (args.PropertyName == nameof(ShellViewModel.IsPageLoading))
        {
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

    /// <summary>Hides or shows the sidebar; the content takes the whole width while it is hidden.</summary>
    private void OnToggleSidebar(object sender, RoutedEventArgs e) => ToggleSidebar();

    private void ToggleSidebar()
    {
        Sidebar.Visibility = Sidebar.Visibility == Visibility.Visible ? Visibility.Collapsed : Visibility.Visible;
        ApplyInsets();
    }

    /// <summary>
    /// Closes the sidebar after a choice where it covers the page, so the choice is visible.
    /// </summary>
    private void DismissOverlaySidebar()
    {
        if (WindowLayout.ClosesSidebarAfterNavigation(_widthClass) && Sidebar.Visibility == Visibility.Visible)
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

    private void OpenSearch()
    {
        if (Sidebar.Visibility != Visibility.Visible)
        {
            ToggleSidebar();
        }

        SidebarSearch.Focus(FocusState.Programmatic);
    }

    private const int RecentSearchLimit = 8;

    /// <summary>
    /// Offers recent searches as the box opens and as it is typed in, as Spotify's and Apple
    /// Music's search does. They are kept on this computer and never sent anywhere.
    /// </summary>
    private void OnSearchTextChanged(AutoSuggestBox sender, AutoSuggestBoxTextChangedEventArgs args)
    {
        if (args.Reason == AutoSuggestionBoxTextChangeReason.UserInput)
        {
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
        var typed = box.Text.Trim();
        var matches = new List<string>();
        foreach (var recent in ShellPreferences.RecentSearches)
        {
            // What is already in the box is not a suggestion.
            if (string.Equals(recent, typed, StringComparison.CurrentCultureIgnoreCase))
            {
                continue;
            }

            if (typed.Length == 0 || recent.Contains(typed, StringComparison.CurrentCultureIgnoreCase))
            {
                matches.Add(recent);
            }
        }

        box.ItemsSource = matches;
        box.IsSuggestionListOpen = matches.Count > 0;
    }

    private static void RememberSearch(string query)
    {
        var trimmed = query.Trim();
        if (trimmed.Length == 0)
        {
            return;
        }

        var recent = new List<string> { trimmed };
        foreach (var earlier in ShellPreferences.RecentSearches)
        {
            if (!string.Equals(earlier, trimmed, StringComparison.CurrentCultureIgnoreCase) && recent.Count < RecentSearchLimit)
            {
                recent.Add(earlier);
            }
        }

        ShellPreferences.RecentSearches = recent;
    }

    private async void OnSearchSubmitted(AutoSuggestBox sender, AutoSuggestBoxQuerySubmittedEventArgs args)
    {
        var query = args.ChosenSuggestion as string ?? args.QueryText;
        sender.Text = query;
        sender.IsSuggestionListOpen = false;
        RememberSearch(query);
        HighlightNavigation(null);
        DismissOverlaySidebar();
        await Model.SearchAsync(query, "all");
        ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
        UpdateBackButton();
    }

    private async void OnBack(object sender, RoutedEventArgs e) => await GoBackAsync();

    private async Task GoBackAsync()
    {
        await Model.GoBackAsync();
        HighlightNavigation(Model.CurrentRouteName);
        ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
        UpdateBackButton();
    }

    private async Task OpenAsync(string kind, string id, string title)
    {
        HighlightNavigation(null);
        await Model.OpenEntityAsync(kind, id, title);
        ContentScroller.ChangeView(null, 0, null, disableAnimation: true);
        UpdateBackButton();
    }
}
