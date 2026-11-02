using System;
using System.Linq;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Threading.Tasks;
using Goosic.Windows.Service;

namespace Goosic.Windows.ViewModels;

public sealed partial class ShellViewModel
{
    internal int NavigationRevision => _pageRequestVersion;
    internal void CancelPendingNavigation() => ++_pageRequestVersion;
    private sealed record PreparedFeed(CatalogPage? Page, Exception? Error,
        (string BrowseId, string Title, string Shape)? PersonalSource);

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
