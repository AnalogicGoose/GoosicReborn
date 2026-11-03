using System;
using System.Linq;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Threading.Tasks;
using Goosic.Windows.Presentation;
using Goosic.Windows.Service;

namespace Goosic.Windows.ViewModels;

public sealed partial class ShellViewModel
{
    /// <summary>How long a destination may take before the old page gives way to the loading state.</summary>
    private static readonly TimeSpan SlowNavigationDelay = TimeSpan.FromMilliseconds(350);

    internal int NavigationRevision => _pageRequestVersion;

    /// <summary>The request that made the page on screen current and may still be filling it.</summary>
    private int _currentPageRequestVersion = -1;

    /// <summary>
    /// Settles choosing the destination already on screen, cancelling a later navigation when the
    /// page is kept. Returns false when the destination has to load again.
    /// </summary>
    internal bool KeepCurrentPageOnReselect()
    {
        switch (FeedNavigationCommitRules.Reselect(PageClearedForPendingNavigation, IsPageLoading,
                    _pageRequestVersion, _currentPageRequestVersion))
        {
            case ReselectOutcome.Reload:
                return false;
            case ReselectOutcome.CancelPending:
                ++_pageRequestVersion;
                break;
        }

        return true;
    }

    /// <summary>
    /// Cancels a navigation in flight unless it is the search being refocused, so returning to
    /// Search drops a late Home or Library answer but never the query that is still loading.
    /// </summary>
    internal void CancelPendingNavigationExceptSearch()
    {
        if (_pageRequestVersion != _searchRequestVersion)
        {
            ++_pageRequestVersion;
        }
    }
    private sealed record PreparedFeed(CatalogPage? Page, Exception? Error,
        (string BrowseId, string Title, string Shape)? PersonalSource);

    /// <summary>
    /// The page on screen was cleared for a navigation that has not committed yet. Until one does,
    /// the current destination's content is gone, so choosing it again must reload it.
    /// </summary>
    internal bool PageClearedForPendingNavigation { get; private set; }

    /// <summary>Replaces the old page with the destination's loading state while its answer is late.</summary>
    private void ShowNavigationPending(string route)
    {
        PageClearedForPendingNavigation = true;
        ClearPageArtwork();
        Shelves.Clear();
        Tracks.Clear();
        NextCursor = null;
        PageTitle = RouteEntry.All.FirstOrDefault(entry => entry.Route == route)?.Title ?? route;
        PageSubtitle = "";
        ShowPageHeader = route != "home";
        Status = "";
        PageState = global::Goosic.Windows.Presentation.PageState.Loading;
    }

    /// <summary>Fetch without changing route, visible collections, title, queue or playback.</summary>
    private async Task<PreparedFeed?> PrepareFeedAsync(string route, string? librarySection)
    {
        var personalSource = PersonalRouteSource(route, librarySection);
        if (personalSource is not null && (!IsSignedIn || Personal is null)) return null;
        try
        {
            if (personalSource is { } source)
            {
                var personalPage = await Personal!.BrowseAsync(source.BrowseId, source.Title, null, source.Shape)
                    .ConfigureAwait(true);
                return new PreparedFeed(personalPage, null, source);
            }

            var title = RouteEntry.All.FirstOrDefault(entry => entry.Route == route)?.Title ?? route;
            var payload = new JsonObject { ["catalogId"] = route, ["query"] = title };
            var answer = await _client.RequestAsync("catalog.browse", payload).ConfigureAwait(true);
            var page = answer.Deserialize<CatalogResponsePayload>(ServiceProtocol.Json)?.Page
                ?? throw new InvalidOperationException("Catalog response did not contain a page.");
            return new PreparedFeed(page, null, null);
        }
        catch (Exception error)
        {
            return new PreparedFeed(null, error, personalSource);
        }
    }
}
