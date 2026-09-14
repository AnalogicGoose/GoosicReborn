using System;
using System.Text.Json.Nodes;
using System.Threading.Tasks;
using Goosic.Windows.Service;

namespace Goosic.Windows.ViewModels;

/// <summary>
/// Preferences, the Settings page, and the now-playing details the full-screen player shows.
/// </summary>
/// <remarks>
/// Preferences are Rust's: <c>settings.get</c> and <c>settings.set</c> keep them in the same store
/// every shell reads, so a choice made here is the choice a macOS or Linux build of the same user
/// data opens with. Nothing here is a credential.
/// </remarks>
public sealed partial class ShellViewModel
{
    private bool _autoplay = true;
    private bool _artworkBackground = true;
    private bool _isSettingsPage;
    private bool _settingsLoaded;
    private string _nowPlayingArtist = "";
    private string? _nowPlayingThumbnail;
    private string _positionText = "0:00";
    private string _remainingText = "-0:00";

    /// <summary>Raised when the confirmed cover changes, with its thumbnail address.</summary>
    internal event Action<string?>? ArtworkChanged;

    public bool Autoplay { get => _autoplay; private set => Set(ref _autoplay, value); }

    public bool ArtworkBackground { get => _artworkBackground; private set => Set(ref _artworkBackground, value); }

    public bool IsSettingsPage { get => _isSettingsPage; private set => Set(ref _isSettingsPage, value); }

    public bool IsServiceConnected => string.IsNullOrEmpty(_status) || !_status.Contains("service", StringComparison.OrdinalIgnoreCase);

    public string NowPlayingArtist { get => _nowPlayingArtist; private set => Set(ref _nowPlayingArtist, value); }

    public string PositionText { get => _positionText; private set => Set(ref _positionText, value); }

    public string RemainingText { get => _remainingText; private set => Set(ref _remainingText, value); }

    internal string? NowPlayingThumbnail => _nowPlayingThumbnail;

    public bool HasLyrics => Lyrics.Count > 0;

    private bool _showPageHeader;

    /// <summary>Whether the page has a heading; Home does not.</summary>
    public bool ShowPageHeader { get => _showPageHeader; private set => Set(ref _showPageHeader, value); }

    /// <summary>What the sidebar's account row says under the name.</summary>
    public string ConnectionLabel => IsSignedIn ? "Connected" : "Guest";

    /// <summary>Whether there is an earlier page to go back to.</summary>
    internal bool CanGoBack => _routeHistory.Count > 0;

    /// <summary>The route on screen, or null for a search or an entity page.</summary>
    internal string? CurrentRouteName
    {
        get
        {
            var parts = _currentRoute.Split(KeySeparator);
            return parts[0] == "route" && parts.Length > 1 ? parts[1] : null;
        }
    }

    /// <summary>Opens one section of the library directly, as the sidebar's library items do.</summary>
    internal async Task OpenLibrarySectionAsync(string browseId)
    {
        _librarySection = browseId;
        await LoadRouteAsync("library").ConfigureAwait(true);
    }

    public bool HasNoLyrics => Lyrics.Count == 0;

    /// <summary>Reads preferences once, and applies the ones this shell honours.</summary>
    internal async Task LoadSettingsAsync()
    {
        try
        {
            var answer = await _client.RequestAsync("settings.get").ConfigureAwait(true);
            if (answer?["settings"] is not JsonObject settings)
            {
                return;
            }

            Autoplay = settings["autoplay"]?.GetValue<bool>() ?? true;
            ArtworkBackground = settings["artworkBackground"]?.GetValue<bool>() ?? true;
            IsShuffled = settings["shuffle"]?.GetValue<bool>() ?? false;
            Repeat = (settings["repeatMode"]?.GetValue<string>()) switch
            {
                "all" => RepeatMode.All,
                "one" => RepeatMode.One,
                _ => RepeatMode.Off,
            };
            _settingsLoaded = true;
        }
        catch (Exception error)
        {
            BridgeLog.Write($"settings unavailable: {error.Message}");
        }
    }

    /// <summary>Writes one preference; the snapshot Rust answers with is what stays shown.</summary>
    private async Task SaveAsync(string name, JsonNode value)
    {
        if (!_settingsLoaded)
        {
            return;
        }

        try
        {
            await _client.RequestAsync("settings.set", new JsonObject
            {
                ["preferences"] = new JsonObject { [name] = value },
            }).ConfigureAwait(true);
        }
        catch (Exception error)
        {
            ReportStatus("Could not save that preference: " + Describe(error));
        }
    }

    internal Task SetAutoplayAsync(bool value)
    {
        Autoplay = value;
        return SaveAsync("autoplay", value);
    }

    internal Task SetArtworkBackgroundAsync(bool value)
    {
        ArtworkBackground = value;
        return SaveAsync("artworkBackground", value);
    }

    private void SaveQueueModes()
    {
        _ = SaveAsync("shuffle", IsShuffled);
        _ = SaveAsync("repeatMode", Repeat switch { RepeatMode.All => "all", RepeatMode.One => "one", _ => "off" });
    }

    internal void ShowSettingsPage()
    {
        Remember("route" + KeySeparator + "settings", remember: true);
        Shelves.Clear();
        Tracks.Clear();
        NextCursor = null;
        ForgetPersonalPage();
        PageTitle = "Settings";
        ShowPageHeader = true;
        PageSubtitle = "Playback, appearance and accounts";
        Status = "";
        IsSettingsPage = true;
        OnPropertyChanged(nameof(IsServiceConnected));
    }

    /// <summary>Updates the times and artist line the full-screen player shows.</summary>
    private void ReportNowPlayingDetails(TrackViewModel? track, double position, double duration)
    {
        PositionText = Short(position);
        RemainingText = "-" + Short(Math.Max(0, duration - position));
        if (track is null)
        {
            return;
        }

        NowPlayingArtist = track.Subtitle;
        if (_nowPlayingThumbnail != track.Thumbnail)
        {
            _nowPlayingThumbnail = track.Thumbnail;
            ArtworkChanged?.Invoke(track.Thumbnail);
        }
    }

    private static string Short(double seconds)
    {
        var value = TimeSpan.FromSeconds(Math.Max(0, seconds));
        return value.TotalHours >= 1 ? value.ToString(@"h\:mm\:ss") : $"{(int)value.TotalMinutes}:{value.Seconds:D2}";
    }

    /// <summary>The cover on disk, larger when YouTube's servers have a larger one.</summary>
    internal async Task<string?> LargeArtworkFileAsync(string? thumbnail)
    {
        if (string.IsNullOrEmpty(thumbnail))
        {
            return null;
        }

        if (NowPlayingMesh.HighResolutionVariant(thumbnail) is { } larger
            && await _artwork.LocalFileAsync(larger).ConfigureAwait(true) is { } file)
        {
            return file;
        }

        return await _artwork.LocalFileAsync(thumbnail).ConfigureAwait(true);
    }

    internal Task<string?> ArtworkFileAsync(string? thumbnail) => _artwork.LocalFileAsync(thumbnail);
}
