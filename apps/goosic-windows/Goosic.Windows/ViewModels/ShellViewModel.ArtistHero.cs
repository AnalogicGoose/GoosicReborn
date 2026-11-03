using System.Threading.Tasks;
using Goosic.Windows.Service;

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
    public bool HasArtistHero => _pageArtistHeader is not null;

    /// <summary>Whether the page opens with the usual heading; the hero takes its place.</summary>
    public bool ShowsStandardHeader => ShowPageHeader && _pageArtistHeader is null;

    /// <summary>The cached file for a picture, fetched first if need be.</summary>
    internal Task<string?> LocalArtworkAsync(string? remote) => _artwork.LocalFileAsync(remote);
}
