using System.Threading.Tasks;
using Goosic.Windows.Service;
using System.Collections.Generic;
using System.Linq;
using Goosic.Windows.Presentation;

namespace Goosic.Windows.ViewModels;

/// <summary>
/// The artist page's hero: the header the catalog read from the artist's own page, which opens
/// that page with its banner instead of the usual heading.
/// </summary>
/// <remarks>
/// The window draws it; this only says whether there is one and what it holds. The header comes
/// with the first page of an artist and never from a shelf or a song, and is forgotten whenever
/// another page starts to open.
/// </remarks>
public sealed partial class ShellViewModel
{
    private ArtistHeader? _pageArtistHeader;

    private IEnumerable<CatalogShelf> ArtistContentShelves(IEnumerable<CatalogShelf> shelves)
    {
        foreach (var shelf in shelves)
        {
            var ids = Tracks.Select(track => track.VideoId).Where(id => !string.IsNullOrEmpty(id)).ToHashSet();
            if (HasArtistHero && shelf.Layout == "list"
                && shelf.Items.Any(item => ids.Contains(item.VideoId ?? item.Id)))
            {
                // A continuation can repeat top songs as a shelf. Keep every song in one list.
                foreach (var item in shelf.Items)
                {
                    if (ids.Add(item.VideoId ?? item.Id))
                    {
                        var row = new TrackViewModel(item);
                        Tracks.Add(row);
                        _ = row.LoadArtworkAsync(_artwork);
                    }
                }
                continue;
            }
            yield return shelf;
        }
    }

    /// <summary>The open artist's header, or <c>null</c> on any other page.</summary>
    internal ArtistHeader? PageArtistHeader
    {
        get => _pageArtistHeader;
        private set
        {
            if (Set(ref _pageArtistHeader, value))
            {
                OnPropertyChanged(nameof(HasArtistHero));
                OnPropertyChanged(nameof(ShowsStandardHeader));
            }
        }
    }

    /// <summary>Whether the page opens with the artist hero.</summary>
    public bool HasArtistHero => _pageArtistHeader is not null
        || (PageKind == DetailKind.Artist && IsPageLoading);

    /// <summary>Whether the page opens with the usual heading; the hero takes its place.</summary>
    public bool ShowsStandardHeader => ShowPageHeader && !HasArtistHero;

    /// <summary>The cached file for a picture, fetched first if need be.</summary>
    internal Task<string?> LocalArtworkAsync(string? remote) => _artwork.LocalFileAsync(remote);
}
