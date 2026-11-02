using System;
using System.Linq;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Threading.Tasks;
using Goosic.Windows.Service;

namespace Goosic.Windows.ViewModels;

public sealed partial class ShellViewModel
{
    /// <summary>How long a destination may take before the old page gives way to the loading state.</summary>
    private static readonly TimeSpan SlowNavigationDelay = TimeSpan.FromMilliseconds(350);

    internal int NavigationRevision => _pageRequestVersion;
    internal void CancelPendingNavigation() => ++_pageRequestVersion;
    private sealed record PreparedFeed(CatalogPage? Page, Exception? Error,
        (string BrowseId, string Title, string Shape)? PersonalSource);

    /// <summary>Replaces the old page with the destination's loading state while its answer is late.</summary>
    private void ShowNavigationPending(string route)
    {
        ClearPageArtwork();
        Shelves.Clear();
        Tracks.Clear();
        NextCursor = null;
        PageTitle = RouteEntry.All.FirstOrDefault(entry => entry.Route == route)?.Title ?? route;
        PageSubtitle = "";
        ShowPageHeader = route != "home";
        Status = "";
        PageState = PageState.Loading;
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
