using System.Threading.Tasks;
using Goosic.Windows.Presentation;

namespace Goosic.Windows.ViewModels;

public sealed partial class ShellViewModel
{
    private bool _isSearchPage;
    private string _searchQuery = "";
    private string _searchText = "";
    private string _searchFilter = "all";
    /// <summary>The page request a submitted search is waiting on; -1 when none has been sent.</summary>
    private int _searchRequestVersion = -1;

    /// <summary>The editable draft survives navigation separately from the submitted query.</summary>
    public string SearchText
    {
        get => _searchText;
        set
        {
            if (Set(ref _searchText, value))
            {
                OnPropertyChanged(nameof(CanSubmitSearch));
            }
        }
    }

    public bool CanSubmitSearch => !string.IsNullOrWhiteSpace(SearchText);

    public string SearchFilter { get => _searchFilter; private set => Set(ref _searchFilter, value); }

    public bool IsSearchPage
    {
        get => _isSearchPage;
        private set
        {
            if (Set(ref _isSearchPage, value))
            {
                OnPropertyChanged(nameof(ShowsSearchLanding));
            }
        }
    }

    public bool ShowsSearchLanding => IsSearchPage && _searchQuery.Length == 0;

    /// <summary>Search is a local navigation surface; opening it never starts playback.</summary>
    internal Task OpenSearchPageAsync(bool remember = true)
    {
        if (IsSearchPage)
        {
            return Task.CompletedTask;
        }

        if (_searchQuery.Length > 0)
        {
            return SearchAsync(_searchQuery, SearchFilter, remember, updateInput: false);
        }

        ShowSearchLanding(remember);
        return Task.CompletedTask;
    }

    internal void ShowSearchLanding(bool remember = false)
    {
        ++_pageRequestVersion;
        Remember(string.Join(KeySeparator, "search", "", SearchFilter), remember);
        ClearPageArtwork();
        Shelves.Clear();
        Tracks.Clear();
        NextCursor = null;
        AllTracksId = null;
        ForgetPersonalPage(keepSearchPage: true);
        MarkSearchPage("", SearchFilter, updateInput: false);
        PageKind = DetailKind.Search;
        SetPageTruncated(false);
        PageTitle = "Search";
        PageSubtitle = "Find songs, albums, artists and playlists";
        ShowPageHeader = false;
        Status = "";
        PageState = PageState.Content;
    }

    internal Task RefilterSearchAsync(string filter)
    {
        if (SearchFilter == filter)
        {
            return Task.CompletedTask;
        }

        SearchFilter = filter;
        if (_searchQuery.Length == 0)
        {
            Remember(string.Join(KeySeparator, "search", "", filter), remember: false);
            return Task.CompletedTask;
        }

        return SearchAsync(_searchQuery, filter);
    }

    private void MarkSearchPage(string query, string filter, bool updateInput)
    {
        _searchQuery = query;
        SearchFilter = filter;
        if (updateInput)
        {
            SearchText = query;
        }

        IsSearchPage = true;
        OnPropertyChanged(nameof(ShowsSearchLanding));
    }
}
