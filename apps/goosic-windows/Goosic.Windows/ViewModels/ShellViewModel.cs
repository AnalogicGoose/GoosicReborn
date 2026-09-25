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
using Goosic.Windows.Presentation;
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
        new("liked", "Liked Music", "\uEB51"),
        new("history", "History", "\uE81C"),
        new("downloads", "Downloads", ""),
        new("settings", "Settings", "\uE713"),
    ];
}

public sealed class CardViewModel : INotifyPropertyChanged
{
    internal CardViewModel(CatalogItem item)
    {
        Title = item.Title;
        Subtitle = string.IsNullOrWhiteSpace(item.Subtitle) ? item.Artist ?? "" : item.Subtitle;
        Id = item.Id;
        Kind = item.Kind;
        VideoId = item.VideoId;
        Thumbnail = item.Thumbnail;
        ArtistId = item.ArtistId;
        AlbumId = item.AlbumId;
        Artist = item.Artist;
        Album = item.Album;
        CategoryBrush = CategoryColor(item.Color);
    }

    /// <summary>A mood or genre from Moods & genres, drawn as a coloured tile rather than a cover.</summary>
    public bool IsCategory => Kind == "category";

    /// <summary>An artist is a circle and anything else a rounded square, as every music app draws them.</summary>
    public Microsoft.UI.Xaml.CornerRadius RowArtworkCornerRadius => new(Kind == "artist" ? 24 : 4);

    public Microsoft.UI.Xaml.CornerRadius CardCornerRadius => new(Kind == "artist" ? CardShape.Size / 2 : 6);

    public Microsoft.UI.Xaml.Media.Brush CategoryBrush { get; }

    private static Microsoft.UI.Xaml.Media.SolidColorBrush CategoryColor(string? hex)
    {
        var color = Microsoft.UI.Colors.SlateGray;
        if (hex is { Length: 7 } && hex[0] == '#'
            && uint.TryParse(hex.AsSpan(1), System.Globalization.NumberStyles.HexNumber, null, out var rgb))
        {
            color = global::Windows.UI.Color.FromArgb(0xFF, (byte)(rgb >> 16), (byte)(rgb >> 8), (byte)rgb);
        }

        return new Microsoft.UI.Xaml.Media.SolidColorBrush(color);
    }

    public string Title { get; }
    public string Subtitle { get; }
    public string AccessibleName => string.IsNullOrWhiteSpace(Subtitle) ? Title : $"{Title}, {Subtitle}";
    internal string Id { get; }
    internal string Kind { get; }
    internal string? ArtistId { get; }
    internal string? AlbumId { get; }

    /// <summary>The catalog's artist and album, which the subtitle only describes.</summary>
    internal string? Artist { get; }
    internal string? Album { get; }

    /// <summary>
    /// What activating the card means, as one string the view can hand back.
    /// </summary>
    /// <remarks>
    /// A bare video id plays; anything else is an entity to open, encoded as kind, id and title
    /// joined by a unit separator -- a character no title or id carries, so the three parts come
    /// back apart exactly as they went in.
    /// </remarks>
    /// <summary>A music video: YouTube Music shows these as wide 16:9 cards, not square covers.</summary>
    public bool IsVideo => Kind == "video";

    /// <summary>The card's artwork width; its height is always the square card's.</summary>
    public double CardWidth => IsVideo ? Presentation.CardShape.VideoWidth : Presentation.CardShape.Size;

    /// <summary>Liked Music, which YouTube Music draws as a gradient with a thumbs-up rather than a cover.</summary>
    public bool IsLikedMusic => Id is "LM" or "VLLM";

    /// <summary>What shows until, or instead of, the artwork.</summary>
    public string PlaceholderGlyph => IsLikedMusic ? "" : "";

    public Microsoft.UI.Xaml.Visibility LikedMusicVisibility =>
        IsLikedMusic ? Microsoft.UI.Xaml.Visibility.Visible : Microsoft.UI.Xaml.Visibility.Collapsed;

    public string ActivationTag => VideoId is { Length: > 0 } video
        ? video
        : string.Join(ShellViewModel.KeySeparator, Kind, Id, Title);
    internal string? VideoId { get; }
    internal string? Thumbnail { get; }
    internal IReadOnlyList<CardViewModel> Context { get; set; } = [];

    /// <summary>The id for the view to hand back, or empty when the card is not a track.</summary>
    public string VideoIdOrEmpty => VideoId ?? "";

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
        ArtistId = item.ArtistId;
        AlbumId = item.AlbumId;
        Artist = item.Artist;
        Album = item.Album;
        EntryId = item.EntryId;
    }

    /// <summary>This occurrence in an account's playlist, when the personal reader supplied one.</summary>
    internal string? EntryId { get; }

    /// <summary>Promotes a playable shelf card without inventing metadata it did not carry.</summary>
    internal TrackViewModel(CardViewModel card)
    {
        Title = card.Title;
        Subtitle = card.Subtitle;
        Duration = "";
        VideoId = card.VideoId;
        Thumbnail = card.Thumbnail;
        Artwork = card.Artwork;
        ArtistId = card.ArtistId;
        AlbumId = card.AlbumId;
        Artist = card.Artist;
        Album = card.Album;
    }

    private TrackViewModel(TrackViewModel source)
    {
        Title = source.Title;
        Subtitle = source.Subtitle;
        Duration = source.Duration;
        VideoId = source.VideoId;
        Thumbnail = source.Thumbnail;
        Explicit = source.Explicit;
        ArtistId = source.ArtistId;
        AlbumId = source.AlbumId;
        Artist = source.Artist;
        Album = source.Album;
        Artwork = source.Artwork;
    }

    /// <summary>
    /// A separate queue entry for the same track.
    /// </summary>
    /// <remarks>
    /// The queue finds its place by reference. Queuing the row object itself would make a track
    /// added twice indistinguishable from itself, so Remove and "now playing" would land on
    /// whichever copy happened to come first.
    /// </remarks>
    internal TrackViewModel Clone() => new(this);

    internal string? ArtistId { get; }
    internal string? AlbumId { get; }
    internal string? Artist { get; }
    internal string? Album { get; }

    private bool _isCurrent;

    /// <summary>Whether this queue entry is the one the queue is on.</summary>
    public bool IsCurrent
    {
        get => _isCurrent;
        internal set
        {
            if (_isCurrent == value)
            {
                return;
            }

            _isCurrent = value;
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(IsCurrent)));
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(CurrentMarker)));
        }
    }

    public Microsoft.UI.Xaml.Visibility CurrentMarker =>
        IsCurrent ? Microsoft.UI.Xaml.Visibility.Visible : Microsoft.UI.Xaml.Visibility.Collapsed;

    private bool _isSelected;
    private bool _isEditing;

    /// <summary>Chosen while the listener edits their playlist; clicking the row toggles it instead of playing.</summary>
    public bool IsSelected
    {
        get => _isSelected;
        set
        {
            if (_isSelected == value)
            {
                return;
            }

            _isSelected = value;
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(IsSelected)));
            SelectionChanged?.Invoke();
        }
    }

    internal event Action? SelectionChanged;

    /// <summary>Whether the row shows its selection box, which it does only while its playlist is being edited.</summary>
    internal bool IsEditing
    {
        get => _isEditing;
        set
        {
            if (_isEditing == value)
            {
                return;
            }

            _isEditing = value;
            if (!value)
            {
                IsSelected = false;
            }

            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(SelectionVisibility)));
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(LeadingColumnVisibility)));
        }
    }

    public Microsoft.UI.Xaml.Visibility SelectionVisibility =>
        _isEditing ? Microsoft.UI.Xaml.Visibility.Visible : Microsoft.UI.Xaml.Visibility.Collapsed;

    /// <summary>An empty subtitle still takes a line, which would lift the title off centre.</summary>
    public Microsoft.UI.Xaml.Visibility SubtitleVisibility => string.IsNullOrWhiteSpace(Subtitle)
        ? Microsoft.UI.Xaml.Visibility.Collapsed
        : Microsoft.UI.Xaml.Visibility.Visible;

    private int _number;
    private bool _isNowPlaying;
    private bool _showsNumber;
    private bool _showsArtwork = true;

    /// <summary>The row's place on an ordered page, shown in place of a cover on an album.</summary>
    public string NumberText => _number > 0 ? _number.ToString(System.Globalization.CultureInfo.InvariantCulture) : "";

    /// <summary>Whether this row is the track the player has confirmed, wherever the row lives.</summary>
    public bool IsNowPlaying => _isNowPlaying;

    public Microsoft.UI.Xaml.Visibility NumberVisibility =>
        _showsNumber && !_isNowPlaying ? Microsoft.UI.Xaml.Visibility.Visible : Microsoft.UI.Xaml.Visibility.Collapsed;

    public Microsoft.UI.Xaml.Visibility PlayingVisibility =>
        _isNowPlaying ? Microsoft.UI.Xaml.Visibility.Visible : Microsoft.UI.Xaml.Visibility.Collapsed;

    public Microsoft.UI.Xaml.Visibility LeadingColumnVisibility =>
        !_isEditing && (_showsNumber || _isNowPlaying) ? Microsoft.UI.Xaml.Visibility.Visible : Microsoft.UI.Xaml.Visibility.Collapsed;

    public Microsoft.UI.Xaml.Visibility ArtworkVisibility =>
        _showsArtwork ? Microsoft.UI.Xaml.Visibility.Visible : Microsoft.UI.Xaml.Visibility.Collapsed;

    public Microsoft.UI.Xaml.Media.Brush TitleBrush => (Microsoft.UI.Xaml.Media.Brush)
        Microsoft.UI.Xaml.Application.Current.Resources[_isNowPlaying ? "AccentTextFillColorPrimaryBrush" : "TextFillColorPrimaryBrush"];

    /// <summary>Gives a row without a cover of its own the one its page shows, as an album's tracks need.</summary>
    internal void UseArtworkIfMissing(BitmapImage? artwork)
    {
        if (Artwork is null && string.IsNullOrEmpty(Thumbnail) && artwork is not null)
        {
            Artwork = artwork;
        }
    }

    /// <summary>Places the row on its page: its number, the page's layout, and whether it is playing.</summary>
    internal void Present(int number, bool showsNumber, bool showsArtwork, bool isNowPlaying)
    {
        if (_number == number && _showsNumber == showsNumber && _showsArtwork == showsArtwork
            && _isNowPlaying == isNowPlaying)
        {
            return;
        }

        _number = number;
        _showsNumber = showsNumber;
        _showsArtwork = showsArtwork;
        _isNowPlaying = isNowPlaying;
        foreach (var name in new[]
        {
            nameof(NumberText), nameof(IsNowPlaying), nameof(NumberVisibility), nameof(PlayingVisibility),
            nameof(LeadingColumnVisibility), nameof(ArtworkVisibility), nameof(TitleBrush), nameof(AccessibleName),
            nameof(AlbumColumnWidth), nameof(AlbumText),
        })
        {
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
        }
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    /// <summary>
    /// A playlist's rows name their album in a column of its own, as Spotify's and Apple Music's
    /// do; an album page's rows all share one, so it is left off there.
    /// </summary>
    public Microsoft.UI.Xaml.GridLength AlbumColumnWidth => _showsArtwork && !string.IsNullOrWhiteSpace(Album)
        ? new Microsoft.UI.Xaml.GridLength(2, Microsoft.UI.Xaml.GridUnitType.Star)
        : new Microsoft.UI.Xaml.GridLength(0);

    public string AlbumText => _showsArtwork ? Album ?? "" : "";

    public string Title { get; }
    public string Subtitle { get; }
    public string Duration { get; }
    public string AccessibleName => (_isNowPlaying ? "Now playing, " : "")
        + (string.IsNullOrWhiteSpace(Duration) ? $"{Title}, {Subtitle}" : $"{Title}, {Subtitle}, {Duration}")
        + (IsPlayable ? "" : ", unavailable");

    /// <summary>
    /// Whether the catalog gave this row something to play. A row without one stays listed, so an
    /// album keeps its numbering, but is dimmed and skipped by Play and Shuffle.
    /// </summary>
    public bool IsPlayable => !string.IsNullOrEmpty(VideoId) && !_refused;

    private bool _refused;

    /// <summary>Marks a track the official player would not play, so it shows as unavailable.</summary>
    internal void MarkRefused()
    {
        if (_refused)
        {
            return;
        }

        _refused = true;
        foreach (var name in new[] { nameof(IsPlayable), nameof(RowOpacity), nameof(UnavailableTip), nameof(AccessibleName) })
        {
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
        }
    }

    public double RowOpacity => IsPlayable ? 1 : 0.45;

    private static long s_nextOrdinal;

    /// <summary>The order rows arrived in, so a sorted list can go back to the playlist's own order.</summary>
    internal long Ordinal { get; } = System.Threading.Interlocked.Increment(ref s_nextOrdinal);

    private bool _filteredOut;

    /// <summary>Hidden by the page's filter box.</summary>
    internal bool FilteredOut
    {
        get => _filteredOut;
        set
        {
            if (_filteredOut != value)
            {
                _filteredOut = value;
                PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(FilterVisibility)));
            }
        }
    }

    public Microsoft.UI.Xaml.Visibility FilterVisibility =>
        _filteredOut ? Microsoft.UI.Xaml.Visibility.Collapsed : Microsoft.UI.Xaml.Visibility.Visible;

    /// <summary>Whether this row matches what was typed in the filter box.</summary>
    internal bool Matches(string text) =>
        text.Length == 0
        || Title.Contains(text, StringComparison.CurrentCultureIgnoreCase)
        || Subtitle.Contains(text, StringComparison.CurrentCultureIgnoreCase)
        || (Album?.Contains(text, StringComparison.CurrentCultureIgnoreCase) ?? false);

    public string? UnavailableTip => IsPlayable ? null : "This track isn’t available to play";
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
        foreach (var item in Items)
        {
            item.Context = Items;
        }

        IsList = shelf.Layout == "list";
        IsCategoryGrid = Items.Count > 0 && Items.All(item => item.IsCategory);
        Columns = IsList
            ? ShelfColumns.Split(Items, ShelfColumns.RowsPerColumn).Select(rows => new ShelfColumnViewModel(rows)).ToList()
            : [];
    }

    public string Title { get; }
    public ObservableCollection<CardViewModel> Items { get; }

    /// <summary>Song rows, shown as YouTube Music shows Quick picks: columns of rows, not cards.</summary>
    public bool IsList { get; }
    public bool IsCards => !IsList && !IsCategoryGrid;

    /// <summary>
    /// Moods & genres: every button at once in a wrapping grid, as YouTube Music and Apple Music
    /// lay out their genres, rather than twenty-seven tiles in a sideways row.
    /// </summary>
    public bool IsCategoryGrid { get; }

    /// <summary>Whether the shelf scrolls sideways, and so has arrows; a wrapping grid does not.</summary>
    public bool Scrolls => !IsCategoryGrid;

    /// <summary>The same cards as <see cref="Items"/>, in columns of four for a list shelf.</summary>
    public IReadOnlyList<ShelfColumnViewModel> Columns { get; }
}

/// <summary>One column of a list shelf.</summary>
public sealed class ShelfColumnViewModel(IReadOnlyList<CardViewModel> rows)
{
    public IReadOnlyList<CardViewModel> Rows { get; } = rows;
    public double Width => ShelfColumns.ColumnWidth;
}

/// <summary>One line shown in the lyrics panel, never synthesized by the shell.</summary>
/// <summary>One line of lyrics, and whether the music is on it.</summary>
public sealed class LyricLineViewModel : INotifyPropertyChanged
{
    private bool _isCurrent;

    internal LyricLineViewModel(string text, long atMilliseconds, bool synced)
    {
        Text = text;
        AtMilliseconds = atMilliseconds;
        Synced = synced;
    }

    /// <summary>Whether the document follows the song; unsynced lines have no current line.</summary>
    public bool Synced { get; }

    public event PropertyChangedEventHandler? PropertyChanged;

    public string Text { get; }
    public long AtMilliseconds { get; }

    public bool IsCurrent
    {
        get => _isCurrent;
        internal set
        {
            if (_isCurrent == value)
            {
                return;
            }

            _isCurrent = value;
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(IsCurrent)));
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Emphasis)));
        }
    }

    /// <summary>The current line at full strength, the rest receding, as the reference does.</summary>
    public double Emphasis => !Synced || IsCurrent ? 1.0 : 0.45;
}

/// <summary>What the window is showing, and how it asks the service to change it.</summary>
public sealed partial class ShellViewModel : INotifyPropertyChanged
{
    private readonly GoosicServiceClient _client;
    private readonly ArtworkLoader _artwork = new();
    private string _pageTitle = "Home";
    private string _pageSubtitle = "Live from YouTube Music, browsed as a guest";
    private BitmapImage? _pageArtwork;
    private int _pageArtworkVersion;
    private string _status = "";
    private string _accountInitials = "G";
    /// <summary>Separates the parts of a page key; it appears in no title, id or query.</summary>
    internal const string KeySeparator = "\u001f";

    /// <summary>
    /// What is on screen, as enough to put it back.
    /// </summary>
    /// <remarks>
    /// History used to hold bare route names, so going back to a search asked
    /// <c>catalog.browse</c> for a surface called "search". A key says which kind of page it was
    /// and carries what reloading it needs.
    /// </remarks>
    private string _currentRoute = "route" + KeySeparator + "home";
    private readonly Stack<string> _routeHistory = new();
    private int _pageRequestVersion;

    internal ShellViewModel(GoosicServiceClient client)
    {
        _client = client;
        WireTrackView();
        Tracks.CollectionChanged += (_, change) =>
        {
            OnPropertyChanged(nameof(HasTracks));
            // A page fills one row at a time; numbering only the new rows keeps a long list linear.
            if (change.Action == System.Collections.Specialized.NotifyCollectionChangedAction.Add
                && change.NewStartingIndex + (change.NewItems?.Count ?? 0) == Tracks.Count)
            {
                PresentTracks(change.NewStartingIndex);
            }
            else
            {
                PresentTracks();
            }
        };
        Queue.CollectionChanged += (_, _) =>
        {
            if (!_syncingUpNext)
            {
                QueueChanged();
            }
        };
        UpNext.CollectionChanged += OnUpNextChanged;
        Lyrics.CollectionChanged += (_, _) =>
        {
            OnPropertyChanged(nameof(HasLyrics));
            OnPropertyChanged(nameof(HasNoLyrics));
        };
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public IReadOnlyList<RouteEntry> Routes => RouteEntry.All;
    public ObservableCollection<ShelfViewModel> Shelves { get; } = [];
    public ObservableCollection<TrackViewModel> Tracks { get; } = [];
    /// <summary>The real catalog rows surrounding the user's current selection.</summary>
    public ObservableCollection<TrackViewModel> Queue { get; } = [];
    public ObservableCollection<LyricLineViewModel> Lyrics { get; } = [];
    public ObservableCollection<AccountViewModel> Accounts { get; } = [];
    private string _accountStatus = "Checking account…";
    private TrackViewModel? _pendingTrack;
    private TrackViewModel? _confirmedTrack;
    /// <summary>The id the page plays the pending track under, when it swapped in another version.</summary>
    private string? _pendingAlias;

    private bool IsPendingVideo(string? videoId) =>
        !string.IsNullOrEmpty(videoId)
        && (videoId == _pendingTrack?.VideoId || videoId == _pendingAlias);

    /// <summary>
    /// Whether the page's replacement for the requested track is the same song, and if so,
    /// accepts its reports as that track's.
    /// </summary>
    internal bool AcceptSubstitute(string requested, string actual, string pageTitle)
    {
        if (_pendingTrack is not { } pending || pending.VideoId != requested
            || !PlaybackOrder.IsSameSong(pending.Title, pageTitle))
        {
            return false;
        }

        _pendingAlias = actual;
        return true;
    }

    private PendingSeek? _pendingSeek;

    /// <summary>Shows a requested seek at once, and holds it until the player confirms it.</summary>
    internal void BeginSeek(double target)
    {
        _pendingSeek = new PendingSeek(target, DateTimeOffset.Now);
        PlaybackPosition = target;
    }

    /// <summary>The listener paused the current play; a pause near its end is theirs, not the end.</summary>
    private bool _listenerPaused;

    /// <summary>Records a pause or resume the listener asked for.</summary>
    internal void NoteListenerToggle() => _listenerPaused = IsPlaying;

    /// <summary>The current play has been heard playing, so an "ended" for it is real.</summary>
    private bool _endArmed;

    /// <summary>The current play's end has been acted on already.</summary>
    private bool _endHandled;
    private LyricsState _lyricsState = LyricsState.NothingPlaying;
    private int _lyricsRequestVersion;
    private PageState _pageState = PageState.Content;
    private double _playbackPosition;
    private double _playbackDuration;
    private double _volume = 1;
    private bool _isMuted;

    /// <summary>The filters `catalog.search` accepts, in the order they are offered.</summary>
    public IReadOnlyList<string> SearchFilters { get; } = ["all", "songs", "albums", "artists", "playlists"];

    public string PageTitle { get => _pageTitle; private set => Set(ref _pageTitle, value); }
    public string PageSubtitle
    {
        get => _pageSubtitle;
        private set
        {
            if (Set(ref _pageSubtitle, value))
            {
                OnPropertyChanged(nameof(PageMeta));
            }
        }
    }
    public BitmapImage? PageArtwork
    {
        get => _pageArtwork;
        private set
        {
            if (Set(ref _pageArtwork, value))
            {
                OnPropertyChanged(nameof(HasPageArtwork));
                if (value is not null)
                {
                    PresentTracks();
                }
            }
        }
    }
    public bool HasPageArtwork => PageArtwork is not null;
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

    private string _nowPlayingTitle = "Nothing playing";
    private string _nowPlayingSubtitle = "Choose a track to begin";
    private BitmapImage? _nowPlayingArtwork;

    public string NowPlayingTitle { get => _nowPlayingTitle; private set => Set(ref _nowPlayingTitle, value); }
    public string NowPlayingSubtitle { get => _nowPlayingSubtitle; private set => Set(ref _nowPlayingSubtitle, value); }
    public BitmapImage? NowPlayingArtwork { get => _nowPlayingArtwork; private set => Set(ref _nowPlayingArtwork, value); }
    /// <summary>What the lyrics panel says around, or instead of, the lines.</summary>
    public LyricsState LyricsState
    {
        get => _lyricsState;
        private set
        {
            if (Set(ref _lyricsState, value))
            {
                OnPropertyChanged(nameof(LyricsStatus));
                OnPropertyChanged(nameof(CanRetryLyrics));
                OnPropertyChanged(nameof(IsLyricsLoading));
                OnPropertyChanged(nameof(ShowsLyricsPlaceholder));
                OnPropertyChanged(nameof(HasLyricLines));
            }
        }
    }

    /// <summary>The one-line caption: where found lyrics came from, or why there are none.</summary>
    public string LyricsStatus => _lyricsState.HasLines ? _lyricsState.Message : _lyricsState.Title;
    public bool CanRetryLyrics => _lyricsState.CanRetry;
    public bool IsLyricsLoading => _lyricsState.IsLoading;
    public bool ShowsLyricsPlaceholder => _lyricsState.ShowsPlaceholder;
    public bool HasLyricLines => _lyricsState.HasLines;
    public string AccountStatus { get => _accountStatus; private set => Set(ref _accountStatus, value); }
    public double PlaybackPosition { get => _playbackPosition; private set => Set(ref _playbackPosition, value); }
    public double PlaybackDuration { get => _playbackDuration; private set => Set(ref _playbackDuration, value); }
    /// <summary>Seeking needs a real length, and is never offered during an advertisement.</summary>
    public bool IsSeekable => PlaybackDuration > 0 && !_isAdvertisement;

    private bool _isAdvertisement;

    /// <summary>Whether the official player is showing an advertisement right now.</summary>
    public bool IsAdvertisement
    {
        get => _isAdvertisement;
        private set
        {
            if (Set(ref _isAdvertisement, value))
            {
                OnPropertyChanged(nameof(IsSeekable));
                OnPropertyChanged(nameof(CanSkip));
            }
        }
    }

    /// <summary>
    /// Whether the listener may move to another track now, saying why not when they may not.
    /// </summary>
    /// <remarks>
    /// An advertisement is reported and played, never skipped: moving to another track during one
    /// would be skipping it by another name. An account change is quiescing the player, and a
    /// track started in the middle of it would belong to neither account.
    /// </remarks>
    internal bool CanChangeTrack()
    {
        if (IsAdvertisement)
        {
            ReportStatus("Track changes wait until the advertisement finishes.");
            return false;
        }

        if (IsAccountBusy)
        {
            ReportStatus("Playback waits while the account changes.");
            return false;
        }

        return true;
    }

    /// <summary>Whether volume or mute may change now; they stay as they are during advertisements.</summary>
    internal bool CanAdjustSound()
    {
        if (!IsAdvertisement)
        {
            return true;
        }

        ReportStatus("Volume and mute are unchanged during advertisements.");
        return false;
    }
    public double Volume
    {
        get => _volume;
        private set
        {
            if (Set(ref _volume, value))
            {
                OnPropertyChanged(nameof(VolumePercent));
                OnPropertyChanged(nameof(VolumeGlyph));
            }
        }
    }

    public bool IsMuted
    {
        get => _isMuted;
        private set
        {
            if (Set(ref _isMuted, value))
            {
                OnPropertyChanged(nameof(VolumeGlyph));
                OnPropertyChanged(nameof(MuteLabel));
            }
        }
    }

    /// <summary>The confirmed volume on the 0-100 scale the slider shows.</summary>
    public double VolumePercent => Math.Round(Volume * 100);

    /// <summary>Segoe Fluent Icons: muted, low, medium or high.</summary>
    public string VolumeGlyph => IsMuted || Volume <= 0 ? "\uE74F"
        : Volume < 0.34 ? "\uE993"
        : Volume < 0.67 ? "\uE994"
        : "\uE995";

    public string MuteLabel => IsMuted ? "Unmute" : "Mute";

    /// <summary>
    /// Set while the listener drags the position slider.
    /// </summary>
    /// <remarks>
    /// Samples arrive twice a second. Moving the slider under the listener's pointer to follow
    /// them would pull the thumb back to where the music is while they are choosing where it
    /// should go.
    /// </remarks>
    internal bool IsScrubbing { get; set; }

    /// <summary>
    /// Shows what the page confirmed.
    /// </summary>
    /// <remarks>
    /// Driven by accepted samples rather than by the request, so the bar says what is happening
    /// instead of what was asked for. An advertisement is named, because it is a marker to be
    /// reported and never something to hide or skip past.
    /// </remarks>
    /// <returns>Whether this is the first confirmed natural end for the current queue item.</returns>
    private bool _isPlaying;
    private bool _lyricsSynced;
    private int _currentLyric = -1;

    /// <summary>
    /// Raised when the page confirms a different track from the one it was confirming.
    /// </summary>
    /// <remarks>
    /// Confirmation, not selection, is what makes lyrics stale: a skip that Rust refuses, or a
    /// load that has not reached the page yet, must leave the lyrics of what is still playing.
    /// </remarks>
    internal event Action? ConfirmedTrackChanged;

    /// <summary>Raised with the index of the lyric line the music has reached.</summary>
    internal event Action<int>? CurrentLyricChanged;

    /// <summary>
    /// Whether the page last confirmed that it is playing.
    /// </summary>
    /// <remarks>
    /// Taken from an accepted sample, never from the click. The transport button offers the
    /// opposite of this, so it must not flip until the page has actually changed state -- a
    /// button that flipped on request would show "pause" over a player that refused to start.
    /// </remarks>
    public bool IsPlaying
    {
        get => _isPlaying;
        private set
        {
            if (Set(ref _isPlaying, value))
            {
                OnPropertyChanged(nameof(PlayPauseGlyph));
                OnPropertyChanged(nameof(PlayIconVisibility));
                OnPropertyChanged(nameof(PauseIconVisibility));
                OnPropertyChanged(nameof(PlayPauseLabel));
                PagePlaybackChanged();
            }
        }
    }

    /// <summary>Segoe Fluent Icons: Pause while playing, Play otherwise.</summary>
    public string PlayPauseGlyph => IsPlaying ? "\uE769" : "\uE768";

    public Microsoft.UI.Xaml.Visibility PlayIconVisibility =>
        IsPlaying ? Microsoft.UI.Xaml.Visibility.Collapsed : Microsoft.UI.Xaml.Visibility.Visible;

    public Microsoft.UI.Xaml.Visibility PauseIconVisibility =>
        IsPlaying ? Microsoft.UI.Xaml.Visibility.Visible : Microsoft.UI.Xaml.Visibility.Collapsed;

    public string PlayPauseLabel => IsPlaying ? "Pause" : "Play";

    /// <summary>
    /// Moves the highlight to the line the confirmed position has reached.
    /// </summary>
    /// <remarks>
    /// Only for synced lyrics. Plain lyrics carry no times, and highlighting one of them would
    /// claim a position the document never stated.
    /// </remarks>
    private void FollowLyrics(double seconds)
    {
        if (!_lyricsSynced || Lyrics.Count == 0)
        {
            return;
        }

        var at = (long)(seconds * 1000);
        var index = -1;
        for (var i = 0; i < Lyrics.Count; i++)
        {
            if (Lyrics[i].AtMilliseconds > at)
            {
                break;
            }

            index = i;
        }

        if (index == _currentLyric)
        {
            return;
        }

        if (_currentLyric >= 0 && _currentLyric < Lyrics.Count)
        {
            Lyrics[_currentLyric].IsCurrent = false;
        }

        _currentLyric = index;
        if (index >= 0)
        {
            Lyrics[index].IsCurrent = true;
            CurrentLyricChanged?.Invoke(index);
        }
    }

    internal bool ReportPlayback(BridgeEvent sample)
    {
        IsAdvertisement = sample.IsAdvertisement;
        var position = TimeSpan.FromSeconds(Math.Max(0, sample.CurrentTime));
        var total = TimeSpan.FromSeconds(Math.Max(0, sample.Duration));
        if (!IsScrubbing)
        {
            var (shown, settled) = SeekSettle.Show(Math.Max(0, sample.CurrentTime), _pendingSeek, DateTimeOffset.Now);
            PlaybackPosition = shown;
            if (settled)
            {
                _pendingSeek = null;
            }
        }

        PlaybackDuration = Math.Max(0, sample.Duration);
        ReportNowPlayingDetails(
            !sample.IsAdvertisement && IsPendingVideo(sample.VideoId) ? _pendingTrack : null,
            IsScrubbing ? PlaybackPosition : sample.CurrentTime,
            sample.Duration);
        Volume = Math.Clamp(sample.Volume, 0, 1);
        IsMuted = sample.Muted;
        IsPlaying = sample.State == "playing";
        OnPropertyChanged(nameof(IsSeekable));
        if (!sample.IsAdvertisement && _pendingTrack is { } pending && IsPendingVideo(sample.VideoId))
        {
            var changed = !ReferenceEquals(_confirmedTrack, pending);
            _confirmedTrack = pending;
            if (sample.State == "playing" && !_endHandled)
            {
                _endArmed = true;
                _listenerPaused = false;
            }

            if (changed)
            {
                Lyrics.Clear();
                _lyricsSynced = false;
                _currentLyric = -1;
                ConfirmedTrackChanged?.Invoke();
                AnnounceConfirmed(pending);
                PresentTracks();
            }

            FollowLyrics(sample.CurrentTime);
            NowPlayingArtwork = pending.Artwork;
            NowPlayingTitle = pending.Title;
            NowPlayingSubtitle = pending.Subtitle + " · " + Clock(position)
                + (total > TimeSpan.Zero ? " / " + Clock(total) : "");
            return MarkNaturalEnd(sample);
        }

        // A sample for a track the model did not ask for: an advertisement, or the moments before
        // the requested page takes over. Name the advertisement; otherwise say it is loading,
        // never the player's raw state word.
        if (sample.IsAdvertisement)
        {
            NowPlayingTitle = "Advertisement";
            NowPlayingSubtitle = total > TimeSpan.Zero ? Clock(position) + " / " + Clock(total) : Clock(position);
        }
        else if (_pendingTrack is { } waiting)
        {
            NowPlayingTitle = waiting.Title;
            NowPlayingSubtitle = "Loading\u2026";
        }

        return MarkNaturalEnd(sample);
    }

    /// <summary>
    /// Records that the requested track finished because the page moved past it.
    /// </summary>
    /// <returns>Whether this is the first end recorded for it, so the queue advances once.</returns>
    /// <summary>Songs the official player refused this session, by video id.</summary>
    private readonly HashSet<string> _refusedVideos = [];

    internal bool IsRefused(string? videoId) => videoId is not null && _refusedVideos.Contains(videoId);

    /// <summary>
    /// Records that the page would not play the requested track, marks it wherever it is shown,
    /// and says so; the queue then moves past it like a finished track.
    /// </summary>
    internal void ReportRefused(string videoId)
    {
        if (!IsPendingVideo(videoId) || _pendingTrack is not { } refused)
        {
            return;
        }

        _refusedVideos.Add(videoId);
        foreach (var row in Tracks.Concat(Queue).Where(row => row.VideoId == videoId))
        {
            row.MarkRefused();
        }

        ReportStatus($"“{refused.Title}” isn’t available to play here, so it was skipped.");
    }

    internal bool ConfirmEndedByPage(string videoId)
    {
        if (!IsPendingVideo(videoId) || _endHandled)
        {
            return false;
        }

        _endHandled = true;
        _endArmed = false;
        IsPlaying = false;
        return true;
    }

    /// <summary>
    /// Whether this sample is the current play's natural end, reported for the first time.
    /// </summary>
    /// <remarks>
    /// Only an end for the track being played counts, and only once it has been heard playing:
    /// the page keeps reporting "ended" for the previous song for a moment after the next is
    /// chosen, and acting on that skipped the next song entirely.
    /// </remarks>
    private bool MarkNaturalEnd(BridgeEvent sample)
    {
        if (sample.IsAdvertisement || _endHandled
            || !PlaybackOrder.IsFinished(sample.State, sample.CurrentTime, sample.Duration, _listenerPaused)
            || !PlaybackOrder.AcceptsEnd(sample.VideoId, IsPendingVideo(sample.VideoId) ? sample.VideoId : null, _endArmed))
        {
            return false;
        }

        _endHandled = true;
        _endArmed = false;
        return true;
    }

    /// <summary>Loads lyrics for the track the official renderer actually confirmed.</summary>
    internal async Task LoadLyricsAsync()
    {
        var track = _confirmedTrack;
        var request = ++_lyricsRequestVersion;
        Lyrics.Clear();
        _lyricsSynced = false;
        _currentLyric = -1;
        if (track is null)
        {
            LyricsState = LyricsState.NothingPlaying;
            return;
        }

        LyricsState = LyricsState.Loading;
        var lookup = LyricsLookup.For(track.Title, track.Artist, track.Subtitle, track.Album, track.Duration);
        var next = await RequestLyricsAsync(lookup, request).ConfigureAwait(true);
        // One quiet retry: a lookup that timed out usually succeeds a moment later, once LRCLIB
        // has fetched the song. Listeners had to close and reopen the panel to get that retry.
        if (next == LyricsState.Unavailable && request == _lyricsRequestVersion)
        {
            await Task.Delay(TimeSpan.FromSeconds(2)).ConfigureAwait(true);
            if (request == _lyricsRequestVersion)
            {
                next = await RequestLyricsAsync(lookup, request).ConfigureAwait(true);
            }
        }

        if (next is not null && request == _lyricsRequestVersion)
        {
            LyricsState = next;
        }
    }

    /// <summary>Asks for one track's lyrics and shows them if found.</summary>
    /// <returns>The panel's next state, or null when a newer request has taken over.</returns>
    private async Task<LyricsState?> RequestLyricsAsync(LyricsLookup lookup, int request)
    {
        try
        {
            var query = new JsonObject
            {
                ["title"] = lookup.Title,
                ["artist"] = lookup.Artist,
                ["album"] = lookup.Album,
            };
            if (lookup.DurationSeconds is { } seconds)
            {
                query["durationSeconds"] = seconds;
            }

            var answer = await _client.RequestAsync("lyrics.get", new JsonObject { ["lyrics"] = query })
                .ConfigureAwait(true);
            // A track change while this was on its way has already asked for its own lyrics.
            if (request != _lyricsRequestVersion)
            {
                return null;
            }

            var document = answer.Deserialize<LyricsResponsePayload>(ServiceProtocol.Json)?.Document;
            if (document is null || document.Lines.Count == 0)
            {
                return LyricsState.NotFound;
            }

            foreach (var line in document.Lines)
            {
                Lyrics.Add(new LyricLineViewModel(line.Text, line.AtMilliseconds, document.Synced));
            }

            _lyricsSynced = document.Synced;
            return LyricsState.Found(document.Synced, document.Source, document.Truncated);
        }
        catch (ServiceRefusedException refused) when (refused.Code == "lyricsNotFound")
        {
            return LyricsState.NotFound;
        }
        catch (Exception error)
        {
            BridgeLog.Write($"lyrics error {error.GetType().Name}: {error.Message}");
            return LyricsState.Unavailable;
        }
    }

    /// <summary>A position as minutes and seconds, counting minutes past the hour.</summary>
    private static string Clock(TimeSpan value) =>
        $"{(int)value.TotalMinutes:D2}:{value.Seconds:D2}";

    /// <summary>Says something on screen that did not come from the service.</summary>
    /// <summary>
    /// Says something about an action the listener just took, briefly, above the player.
    /// </summary>
    /// <remarks>
    /// Page-level notices, such as a clamped page, go to <see cref="Status"/> and stay with the
    /// page. An action's outcome is a toast: at the top of a scrolled page it would be missed,
    /// and left there it would outlive the action by pages.
    /// </remarks>
    internal void ReportStatus(string message)
    {
        Toast = message;
        if (message.Length > 0)
        {
            _ = ClearToastLaterAsync(++_toastVersion);
        }
    }

    private string _toast = "";
    private int _toastVersion;

    public string Toast
    {
        get => _toast;
        private set
        {
            if (Set(ref _toast, value))
            {
                OnPropertyChanged(nameof(HasToast));
            }
        }
    }

    public bool HasToast => _toast.Length > 0;

    internal void DismissToast()
    {
        _toastVersion++;
        Toast = "";
    }

    private async Task ClearToastLaterAsync(int version)
    {
        await Task.Delay(ToastLifetime).ConfigureAwait(true);
        if (version == _toastVersion)
        {
            Toast = "";
        }
    }

    private static readonly TimeSpan ToastLifetime = TimeSpan.FromSeconds(5);

    /// <summary>Greets the service, then loads the opening screen.</summary>
    internal async Task StartAsync()
    {
        BridgeLog.Write("startup: asking the service hello");
        try
        {
            await _client.RequestAsync("hello").ConfigureAwait(true);
            BridgeLog.Write("startup: hello answered");
            Status = "";
            await LoadSettingsAsync().ConfigureAwait(true);
        }
        catch (Exception error)
        {
            BridgeLog.Write($"service start failed: {error}");
            PageState = FailureState(error, "Goosic");
            return;
        }

        BridgeLog.Write("startup: reading accounts");
        await RefreshAccountsAsync().ConfigureAwait(true);
        BridgeLog.Write($"startup: {Accounts.Count} account(s), active {_activeAccount?.DisplayName ?? "none"}");
        var opening = StartRoute.For(StartPage, LastRoute);
        OpeningRouteChosen?.Invoke(opening);
        await LoadRouteAsync(opening).ConfigureAwait(true);
    }

    /// <summary>Raised with the page Goosic opens on, so the sidebar can highlight it.</summary>
    internal event Action<string>? OpeningRouteChosen;

    /// <summary>Loads one browse surface.</summary>
    internal async Task LoadRouteAsync(string route, bool rememberCurrentRoute = true)
    {
        var requestVersion = ++_pageRequestVersion;
        ClearPageArtwork();
        if (route == "settings")
        {
            ShowSettingsPage();
            return;
        }

        if (route == "downloads")
        {
            Remember("route" + KeySeparator + route, rememberCurrentRoute);
            Shelves.Clear();
            Tracks.Clear();
            NextCursor = null;
            AllTracksId = null;
            ForgetPersonalPage();
            PageTitle = "Downloads";
            PageSubtitle = "Tracks saved by a previous Goosic";
            ShowPageHeader = true;
            Status = "";
            PageState = PageState.Loading;
            int? count;
            try
            {
                var listed = await _client.RequestAsync("downloads.list").ConfigureAwait(true);
                count = listed?["downloads"] is JsonArray downloads ? downloads.Count : 0;
            }
            catch (ServiceUnavailableException error)
            {
                if (requestVersion == _pageRequestVersion)
                {
                    PageState = FailureState(error, PageTitle);
                }
                return;
            }
            catch (Exception error)
            {
                Describe(error);
                count = null;
            }

            if (requestVersion == _pageRequestVersion)
            {
                PageState = PageState.Downloads(count);
            }
            return;
        }

        Remember("route" + KeySeparator + route, rememberCurrentRoute);
        if (StartRoute.IsRemembered(route) && route != LastRoute)
        {
            // Rust keeps it, so "start on the page I was on last" survives a restart.
            LastRoute = route;
            _ = SaveAsync("lastRoute", route);
        }

        // Liked Music is an ordered list like a playlist; the other routes are shelves or results.
        PageKind = route == "liked" ? DetailKind.Playlist : DetailKind.Browse;
        SetPageTruncated(false);
        var entry = RouteEntry.All.FirstOrDefault(candidate => candidate.Route == route);
        PageTitle = entry?.Title ?? route;
        // Home is shelves under the title bar, with no heading of its own, as on macOS.
        ShowPageHeader = route != "home";
        Shelves.Clear();
        Tracks.Clear();
        NextCursor = null;
        AllTracksId = null;
        ForgetPersonalPage();
        Status = "";
        PageState = PageState.Loading;
        if (await TryLoadPersonalRouteAsync(route).ConfigureAwait(true))
        {
            return;
        }

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
            if (requestVersion != _pageRequestVersion)
            {
                return;
            }
            var page = answer.Deserialize<CatalogResponsePayload>(ServiceProtocol.Json)?.Page;
            if (page is null)
            {
                PageState = PageState.Failure(PageFailure.Other, null, PageTitle, NetworkAvailable());
                return;
            }

            PageSubtitle = string.IsNullOrWhiteSpace(page.Subtitle)
                ? "Live from YouTube Music, browsed as a guest"
                : page.Subtitle;

            NextCursor = page.NextCursor;
            AllTracksId = page.AllTracksId;
            foreach (var track in Listed(page.Tracks))
            {
                var row = new TrackViewModel(track);
                Tracks.Add(row);
                _ = row.LoadArtworkAsync(_artwork);
            }

            foreach (var shelf in ListedShelves(page.Shelves))
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
            PageState = Tracks.Count == 0 && Shelves.Count == 0
                ? PageState.Empty(PageSubject.Browse, PageTitle)
                : PageState.Content;
        }
        catch (Exception error) when (requestVersion == _pageRequestVersion)
        {
            PageState = FailureState(error, PageTitle);
        }
        catch (Exception error)
        {
            Describe(error);
        }
    }

    /// <summary>Returns to the prior native catalog route without involving the WebView.</summary>
    internal async Task GoBackAsync()
    {
        if (_routeHistory.Count == 0)
        {
            ReportStatus("There is no earlier page.");
            return;
        }

        var parts = _routeHistory.Pop().Split(KeySeparator);
        switch (parts[0])
        {
            case "search" when parts.Length == 3:
                await SearchAsync(parts[1], parts[2], remember: false).ConfigureAwait(true);
                break;
            case "entity" when parts.Length == 4:
                await OpenEntityAsync(parts[1], parts[2], parts[3], remember: false).ConfigureAwait(true);
                break;
            default:
                await LoadRouteAsync(parts.Length > 1 ? parts[1] : "home", rememberCurrentRoute: false)
                    .ConfigureAwait(true);
                break;
        }
    }

    /// <summary>Records the page being left, then makes <paramref name="key"/> current.</summary>
    private void Remember(string key, bool remember)
    {
        if (remember && !string.Equals(key, _currentRoute, StringComparison.Ordinal))
        {
            _routeHistory.Push(_currentRoute);
        }

        _currentRoute = key;
        PagePlaybackChanged();
    }

    /// <summary>Opens an album, playlist or artist.</summary>
    /// <remarks>
    /// Each kind has its own command. A kind this build does not know opens nothing rather than
    /// being guessed at: the protocol decodes unfamiliar kinds precisely so a newer service's row
    /// can be shown inert instead of failing the page.
    /// </remarks>
    internal async Task OpenEntityAsync(string kind, string id, string title, bool remember = true)
    {
        var requestVersion = ++_pageRequestVersion;
        var artworkVersion = ClearPageArtwork();
        var command = kind switch
        {
            "album" => "catalog.album",
            "playlist" => "catalog.playlist",
            "artist" => "catalog.artist",
            "category" => "catalog.category",
            _ => null,
        };
        if (command is null || id.Length == 0)
        {
            ReportStatus("That item cannot be opened.");
            return;
        }

        Remember(string.Join(KeySeparator, "entity", kind, id, title), remember);
        PageKind = DetailLayout.FromEntity(kind);
        SetPageTruncated(false);
        var hasEntityArtwork = SetEntityArtworkAsync(kind, id, artworkVersion);
        ShowPageHeader = true;
        PageTitle = title;
        PageSubtitle = kind switch
        {
            "album" => "Album",
            "playlist" => "Playlist",
            "category" => "Moods & genres",
            _ => "Artist",
        };
        Shelves.Clear();
        Tracks.Clear();
        NextCursor = null;
        AllTracksId = null;
        ForgetPersonalPage();
        Status = "";
        PageState = PageState.Loading;
        if (await TryOpenPersonalEntityAsync(kind, id, title).ConfigureAwait(true))
        {
            return;
        }

        try
        {
            var answer = await _client.RequestAsync(command, new JsonObject { ["catalogId"] = id })
                .ConfigureAwait(true);
            if (requestVersion != _pageRequestVersion)
            {
                return;
            }
            var page = answer.Deserialize<CatalogResponsePayload>(ServiceProtocol.Json)?.Page;
            if (page is null)
            {
                PageState = PageState.Failure(PageFailure.Other, null, title, NetworkAvailable());
                return;
            }

            if (!string.IsNullOrWhiteSpace(page.Title))
            {
                PageTitle = page.Title;
            }

            if (!string.IsNullOrWhiteSpace(page.Subtitle))
            {
                PageSubtitle = page.Subtitle;
            }

            NextCursor = page.NextCursor;
            AllTracksId = page.AllTracksId;
            foreach (var track in Listed(page.Tracks))
            {
                var row = new TrackViewModel(track);
                Tracks.Add(row);
                _ = row.LoadArtworkAsync(_artwork);
                if (PageArtwork is null && Tracks.Count == 1)
                {
                    _ = FallBackToTrackArtworkAsync(hasEntityArtwork, row, artworkVersion);
                }
            }

            foreach (var shelf in ListedShelves(page.Shelves))
            {
                var model = new ShelfViewModel(shelf);
                Shelves.Add(model);
                foreach (var card in model.Items)
                {
                    _ = card.LoadArtworkAsync(_artwork);
                }
            }

            Status = page.Truncated ? "This page was long, so only the first part is shown." : "";
            SetPageTruncated(page.Truncated);
            PageState = Tracks.Count == 0 && Shelves.Count == 0
                ? PageState.Empty(PageSubject.Entity, PageTitle)
                : PageState.Content;
        }
        catch (Exception error) when (requestVersion == _pageRequestVersion)
        {
            PageState = FailureState(error, title);
        }
        catch (Exception error)
        {
            Describe(error);
        }
    }

    /// <summary>Searches the catalog, and shows what came back as rows.</summary>
    internal async Task SearchAsync(string query, string filter, bool remember = true)
    {
        var trimmed = query.Trim();
        if (trimmed.Length == 0)
        {
            Status = "Type something to search the catalog.";
            return;
        }

        var requestVersion = ++_pageRequestVersion;
        ClearPageArtwork();

        Remember(string.Join(KeySeparator, "search", trimmed, filter), remember);
        ShowPageHeader = true;
        PageTitle = $"Results for “{trimmed}”";
        PageSubtitle = filter == "all" ? "Everything matching" : $"Matching {filter}";
        Shelves.Clear();
        Tracks.Clear();
        NextCursor = null;
        AllTracksId = null;
        ForgetPersonalPage();
        MarkSearchPage(trimmed);
        PageKind = DetailKind.Search;
        SetPageTruncated(false);
        Status = "";
        PageState = PageState.Loading;

        try
        {
            var payload = new JsonObject { ["query"] = trimmed, ["filter"] = filter };
            var answer = await _client.RequestAsync("catalog.search", payload).ConfigureAwait(true);
            if (requestVersion != _pageRequestVersion)
            {
                return;
            }
            var page = answer.Deserialize<CatalogResponsePayload>(ServiceProtocol.Json)?.Page;
            if (page is null)
            {
                PageState = PageState.Failure(PageFailure.Other, null, "Search", NetworkAvailable());
                return;
            }

            NextCursor = page.NextCursor;
            AllTracksId = page.AllTracksId;
            foreach (var track in Listed(page.Tracks))
            {
                var row = new TrackViewModel(track);
                Tracks.Add(row);
                _ = row.LoadArtworkAsync(_artwork);
            }

            foreach (var shelf in ListedShelves(page.Shelves))
            {
                var model = new ShelfViewModel(shelf);
                Shelves.Add(model);
                foreach (var card in model.Items)
                {
                    _ = card.LoadArtworkAsync(_artwork);
                }
            }

            Status = page.Truncated ? "This page was long, so only the first part is shown." : "";
            PageState = Tracks.Count == 0 && Shelves.Count == 0
                ? PageState.Empty(PageSubject.Search, trimmed)
                : PageState.Content;
        }
        catch (Exception error) when (requestVersion == _pageRequestVersion)
        {
            PageState = FailureState(error, "Search");
        }
        catch (Exception error)
        {
            Describe(error);
        }
    }

    /// <summary>Turns a transport or refusal into something worth reading on screen.</summary>
    private static string Describe(Exception error)
    {
        BridgeLog.Write($"ui error {error.GetType().Name}: {error.Message}");
        return error switch
        {
            ServiceUnavailableException => "Goosic’s playback service is unavailable. Reopen the app and try again.",
            TimeoutException => "That took too long. Check your connection and try again.",
            ServiceRefusedException => "That action is temporarily unavailable. Try again.",
            _ => "Something went wrong. Try again.",
        };
    }

    /// <summary>The page's state screen for a failed request.</summary>
    private static PageState FailureState(Exception error, string subject)
    {
        Describe(error);
        var (failure, code) = error switch
        {
            ServiceUnavailableException => (PageFailure.ServiceUnavailable, (string?)null),
            TimeoutException or TaskCanceledException => (PageFailure.Timeout, null),
            ServiceRefusedException refused => (PageFailure.Refused, refused.Code),
            _ => (PageFailure.Other, null),
        };
        return PageState.Failure(failure, code, subject, NetworkAvailable());
    }

    private static bool NetworkAvailable() =>
        System.Net.NetworkInformation.NetworkInterface.GetIsNetworkAvailable();

    /// <summary>What the page area shows instead of, or while waiting for, its content.</summary>
    public PageState PageState
    {
        get => _pageState;
        private set
        {
            if (Set(ref _pageState, value))
            {
                OnPropertyChanged(nameof(IsPageLoading));
                OnPropertyChanged(nameof(ShowsPageStatePanel));
            }
        }
    }

    public bool IsPageLoading => _pageState.Kind == PageStateKind.Loading;

    public bool ShowsPageStatePanel => _pageState.ShowsPanel;

    /// <summary>Loads the page on screen again, without adding it to history.</summary>
    internal async Task RetryPageAsync()
    {
        var parts = _currentRoute.Split(KeySeparator);
        switch (parts[0])
        {
            case "search" when parts.Length == 3:
                await SearchAsync(parts[1], parts[2], remember: false).ConfigureAwait(true);
                break;
            case "entity" when parts.Length == 4:
                await OpenEntityAsync(parts[1], parts[2], parts[3], remember: false).ConfigureAwait(true);
                break;
            default:
                await LoadRouteAsync(parts.Length > 1 ? parts[1] : "home", rememberCurrentRoute: false)
                    .ConfigureAwait(true);
                break;
        }
    }

    private int ClearPageArtwork()
    {
        PageArtwork = null;
        return ++_pageArtworkVersion;
    }

    /// <summary>Covers of albums, playlists and artists as the card that opened them showed them.</summary>
    /// <remarks>
    /// A catalog page carries no artwork of its own, so the hero would otherwise borrow its first
    /// track's, which for a playlist or an artist is a different picture. Kept per entity so Back
    /// shows the same cover.
    /// </remarks>
    private readonly Dictionary<string, string> _entityThumbnails = [];

    internal void RememberEntityThumbnail(string kind, string id, string? thumbnail)
    {
        if (!string.IsNullOrEmpty(thumbnail) && id.Length > 0)
        {
            _entityThumbnails[kind + KeySeparator + id] = thumbnail;
        }
    }

    private Task<bool> SetEntityArtworkAsync(string kind, string id, int version) =>
        _entityThumbnails.TryGetValue(kind + KeySeparator + id, out var thumbnail)
            ? SetPageArtworkFromAsync(thumbnail, version)
            : Task.FromResult(false);

    private async Task<bool> SetPageArtworkFromAsync(string thumbnail, int version)
    {
        var file = await _artwork.LocalFileAsync(thumbnail).ConfigureAwait(true);
        if (file is null || version != _pageArtworkVersion)
        {
            return false;
        }

        var image = new BitmapImage();
        using var stream = File.OpenRead(file);
        await image.SetSourceAsync(stream.AsRandomAccessStream());
        if (version == _pageArtworkVersion)
        {
            PageArtwork = image;
        }

        return true;
    }

    // ---- Detail layout ----------------------------------------------------------------------

    private DetailKind _pageKind = DetailKind.Browse;
    private bool _pageTruncated;

    public DetailKind PageKind
    {
        get => _pageKind;
        private set
        {
            if (Set(ref _pageKind, value))
            {
                OnPropertyChanged(nameof(PageHeroCornerRadius));
                PresentTracks();
            }
        }
    }

    public Microsoft.UI.Xaml.CornerRadius PageHeroCornerRadius => new(DetailLayout.HeroCornerRadius(_pageKind));

    /// <summary>The line under the title: the page's own subtitle, then its songs and running time.</summary>
    public string PageMeta => _pageKind is DetailKind.Album or DetailKind.Playlist
        ? DetailLayout.Summary(
            PageSubtitle,
            Tracks.Select(track => track.Duration).ToList(),
            // A list with more still to load is not the whole list: "100 songs" on a playlist of
            // three hundred says something false.
            _pageTruncated || !string.IsNullOrEmpty(_nextCursor))
        : PageSubtitle;

    private void SetPageTruncated(bool truncated)
    {
        _pageTruncated = truncated;
        OnPropertyChanged(nameof(PageMeta));
    }

    /// <summary>Numbers the rows and marks the one playing, following the page's layout.</summary>
    private void PresentTracks(int from = 0)
    {
        var numbered = DetailLayout.ShowsNumbers(_pageKind);
        var artwork = DetailLayout.ShowsRowArtwork(_pageKind);
        var playing = _confirmedTrack?.VideoId;
        for (var i = Math.Max(0, from); i < Tracks.Count; i++)
        {
            var row = Tracks[i];
            row.Present(i + 1, numbered, artwork, playing is not null && row.VideoId == playing);
            if (_pageKind == DetailKind.Album)
            {
                row.UseArtworkIfMissing(PageArtwork);
            }
        }

        OnPropertyChanged(nameof(PageMeta));
    }

    /// <summary>Uses the first track's cover only when the card that opened the page had none.</summary>
    private async Task FallBackToTrackArtworkAsync(Task<bool> entityArtwork, TrackViewModel row, int version)
    {
        if (!await entityArtwork.ConfigureAwait(true))
        {
            await SetPageArtworkAsync(row, version).ConfigureAwait(true);
        }
    }

    private async Task SetPageArtworkAsync(TrackViewModel row, int version)
    {
        await row.LoadArtworkAsync(_artwork).ConfigureAwait(true);
        if (version == _pageArtworkVersion && PageArtwork is null)
        {
            PageArtwork = row.Artwork;
        }
    }

    /// <summary>Whether transport controls have a selected or confirmed track to act on.</summary>
    public bool HasPlayback => _pendingTrack is not null;

    /// <summary>Whether Next and Previous are offered: there is playback, and no advertisement.</summary>
    public bool CanSkip => HasPlayback && !_isAdvertisement;

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
