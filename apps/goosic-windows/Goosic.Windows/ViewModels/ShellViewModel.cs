using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Linq;
using System.Runtime.CompilerServices;
using System.Text.Json;
using System.IO;
using System.Text.Json.Nodes;
using System.Threading.Tasks;
using Goosic.Windows.Service;
using Microsoft.UI.Xaml.Media.Imaging;

namespace Goosic.Windows.ViewModels;

/// <summary>One entry in the rail and the named sidebar.</summary>
/// <remarks>
/// The glyphs are Segoe Fluent Icons code points, so they follow the system font rather than
/// shipping an icon set the shell would then have to theme itself.
/// </remarks>
public sealed record RouteEntry(string Route, string Title, string Glyph)
{
    public static IReadOnlyList<RouteEntry> All { get; } =
    [
        new("home", "Home", ""),
        new("explore", "Explore", ""),
        new("charts", "Charts", ""),
        new("moodsAndGenres", "Moods & genres", ""),
        new("newReleases", "New releases", ""),
        new("library", "Library", ""),
        new("downloads", "Downloads", ""),
    ];
}

public sealed class CardViewModel : INotifyPropertyChanged
{
    internal CardViewModel(CatalogItem item)
    {
        Title = item.Title;
        Subtitle = string.IsNullOrWhiteSpace(item.Subtitle) ? item.Artist ?? "" : item.Subtitle;
        Id = item.Id;
        VideoId = item.VideoId;
        Thumbnail = item.Thumbnail;
    }

    public string Title { get; }
    public string Subtitle { get; }
    internal string Id { get; }
    internal string? VideoId { get; }
    internal string? Thumbnail { get; }

    public event PropertyChangedEventHandler? PropertyChanged;

    /// <summary>
    /// The artwork, once it has been fetched.
    /// </summary>
    /// <remarks>
    /// Left null until <see cref="ArtworkLoader"/> has the bytes on disk. Binding the control to
    /// the remote URL instead would let a catalog response point the shell at an arbitrary host,
    /// which the allow-list exists to prevent -- so the view only ever sees a local file.
    /// </remarks>
    public BitmapImage? Artwork
    {
        get => _artwork;
        private set
        {
            _artwork = value;
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Artwork)));
        }
    }

    private BitmapImage? _artwork;

    /// <summary>Asks the loader for this card's cover and shows it if one arrives.</summary>
    internal async Task LoadArtworkAsync(ArtworkLoader loader)
    {
        var file = await loader.LocalFileAsync(Thumbnail).ConfigureAwait(true);
        if (file is null)
        {
            return;
        }

        var image = new BitmapImage();
        using var stream = File.OpenRead(file);
        await image.SetSourceAsync(stream.AsRandomAccessStream());
        Artwork = image;
    }
}

/// <summary>One track row: a search result, or an ordered list on a detail page.</summary>
public sealed class TrackViewModel : INotifyPropertyChanged
{
    internal TrackViewModel(CatalogItem item)
    {
        Title = item.Title;
        Subtitle = string.IsNullOrWhiteSpace(item.Subtitle) ? item.Artist ?? "" : item.Subtitle;
        Duration = item.Duration ?? "";
        VideoId = item.VideoId;
        Thumbnail = item.Thumbnail;
        Explicit = item.Explicit;
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public string Title { get; }
    public string Subtitle { get; }
    public string Duration { get; }
    public bool Explicit { get; }
    internal string? VideoId { get; }
    internal string? Thumbnail { get; }

    /// <summary>The id for the view to hand back, or empty when the row cannot be played.</summary>
    public string VideoIdOrEmpty => VideoId ?? "";

    public BitmapImage? Artwork
    {
        get => _artwork;
        private set
        {
            _artwork = value;
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Artwork)));
        }
    }

    private BitmapImage? _artwork;

    internal async Task LoadArtworkAsync(ArtworkLoader loader)
    {
        var file = await loader.LocalFileAsync(Thumbnail).ConfigureAwait(true);
        if (file is null)
        {
            return;
        }

        var image = new BitmapImage();
        using var stream = File.OpenRead(file);
        await image.SetSourceAsync(stream.AsRandomAccessStream());
        Artwork = image;
    }
}

public sealed class ShelfViewModel
{
    internal ShelfViewModel(CatalogShelf shelf)
    {
        Title = shelf.Title;
        Items = new ObservableCollection<CardViewModel>(shelf.Items.Select(item => new CardViewModel(item)));
    }

    public string Title { get; }
    public ObservableCollection<CardViewModel> Items { get; }
}

/// <summary>What the window is showing, and how it asks the service to change it.</summary>
public sealed class ShellViewModel : INotifyPropertyChanged
{
    private readonly GoosicServiceClient _client;
    private readonly ArtworkLoader _artwork = new();
    private string _pageTitle = "Home";
    private string _pageSubtitle = "Live from YouTube Music, browsed as a guest";
    private string _status = "";
    private string _accountInitials = "G";

    internal ShellViewModel(GoosicServiceClient client)
    {
        _client = client;
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public IReadOnlyList<RouteEntry> Routes => RouteEntry.All;
    public ObservableCollection<ShelfViewModel> Shelves { get; } = [];
    public ObservableCollection<TrackViewModel> Tracks { get; } = [];

    /// <summary>The filters `catalog.search` accepts, in the order they are offered.</summary>
    public IReadOnlyList<string> SearchFilters { get; } = ["all", "songs", "albums", "artists", "playlists"];

    public string PageTitle { get => _pageTitle; private set => Set(ref _pageTitle, value); }
    public string PageSubtitle { get => _pageSubtitle; private set => Set(ref _pageSubtitle, value); }
    public string AccountInitials { get => _accountInitials; private set => Set(ref _accountInitials, value); }

    public string Status
    {
        get => _status;
        private set
        {
            if (Set(ref _status, value))
            {
                OnPropertyChanged(nameof(HasStatus));
            }
        }
    }

    public bool HasStatus => !string.IsNullOrEmpty(_status);

    /// <summary>Says something on screen that did not come from the service.</summary>
    internal void ReportStatus(string message) => Status = message;

    /// <summary>Greets the service, then loads the opening screen.</summary>
    internal async Task StartAsync()
    {
        try
        {
            await _client.RequestAsync("hello").ConfigureAwait(true);
            Status = "";
        }
        catch (Exception error)
        {
            Status = Describe(error);
            return;
        }

        await LoadRouteAsync("home").ConfigureAwait(true);
    }

    /// <summary>Loads one browse surface.</summary>
    internal async Task LoadRouteAsync(string route)
    {
        var entry = RouteEntry.All.FirstOrDefault(candidate => candidate.Route == route);
        PageTitle = entry?.Title ?? route;
        Shelves.Clear();
        Tracks.Clear();
        Status = $"Loading {PageTitle.ToLowerInvariant()}…";

        try
        {
            // `catalog.browse` reads the surface from `catalogId`, and uses `query` as the title
            // it echoes back on the page.
            var payload = new JsonObject
            {
                ["catalogId"] = route,
                ["query"] = PageTitle,
            };
            var answer = await _client.RequestAsync("catalog.browse", payload).ConfigureAwait(true);
            var page = answer.Deserialize<CatalogResponsePayload>(ServiceProtocol.Json)?.Page;
            if (page is null)
            {
                Status = "The service answered without a page.";
                return;
            }

            PageSubtitle = string.IsNullOrWhiteSpace(page.Subtitle)
                ? "Live from YouTube Music, browsed as a guest"
                : page.Subtitle;

            foreach (var track in page.Tracks)
            {
                var row = new TrackViewModel(track);
                Tracks.Add(row);
                _ = row.LoadArtworkAsync(_artwork);
            }

            foreach (var shelf in page.Shelves)
            {
                var model = new ShelfViewModel(shelf);
                Shelves.Add(model);
                foreach (var card in model.Items)
                {
                    // Not awaited: a page should render immediately and fill in as covers
                    // arrive, rather than waiting on the slowest thumbnail.
                    _ = card.LoadArtworkAsync(_artwork);
                }
            }

            // A clamped page says so rather than presenting a partial list as complete.
            Status = page.Truncated
                ? "This page was long, so only the first part is shown."
                : "";
        }
        catch (Exception error)
        {
            Status = Describe(error);
        }
    }

    /// <summary>Searches the catalog, and shows what came back as rows.</summary>
    internal async Task SearchAsync(string query, string filter)
    {
        var trimmed = query.Trim();
        if (trimmed.Length == 0)
        {
            return;
        }

        PageTitle = $"Results for “{trimmed}”";
        PageSubtitle = filter == "all" ? "Everything matching" : $"Matching {filter}";
        Shelves.Clear();
        Tracks.Clear();
        Status = "Searching…";

        try
        {
            var payload = new JsonObject { ["query"] = trimmed, ["filter"] = filter };
            var answer = await _client.RequestAsync("catalog.search", payload).ConfigureAwait(true);
            var page = answer.Deserialize<CatalogResponsePayload>(ServiceProtocol.Json)?.Page;
            if (page is null)
            {
                Status = "The service answered without a page.";
                return;
            }

            foreach (var track in page.Tracks)
            {
                var row = new TrackViewModel(track);
                Tracks.Add(row);
                _ = row.LoadArtworkAsync(_artwork);
            }

            foreach (var shelf in page.Shelves)
            {
                var model = new ShelfViewModel(shelf);
                Shelves.Add(model);
                foreach (var card in model.Items)
                {
                    _ = card.LoadArtworkAsync(_artwork);
                }
            }

            Status = Tracks.Count == 0 && Shelves.Count == 0
                ? $"Nothing matched “{trimmed}”."
                : page.Truncated ? "This page was long, so only the first part is shown." : "";
        }
        catch (Exception error)
        {
            Status = Describe(error);
        }
    }

    /// <summary>Turns a transport or refusal into something worth reading on screen.</summary>
    private static string Describe(Exception error) => error switch
    {
        ServiceRefusedException refused => refused.Message,
        ServiceUnavailableException unavailable => unavailable.Message,
        _ => error.Message,
    };

    private bool Set<T>(ref T field, T value, [CallerMemberName] string? name = null)
    {
        if (EqualityComparer<T>.Default.Equals(field, value))
        {
            return false;
        }

        field = value;
        OnPropertyChanged(name);
        return true;
    }

    private void OnPropertyChanged(string? name) =>
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
}
