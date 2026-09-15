using System;
using System.Collections.ObjectModel;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading.Tasks;
using Goosic.Windows.Service;

namespace Goosic.Windows.ViewModels;

/// <summary>One finalized file already known to the Rust download library.</summary>
public sealed record DownloadedTrackViewModel(
    [property: JsonPropertyName("videoId")] string VideoId,
    [property: JsonPropertyName("title")] string Title,
    [property: JsonPropertyName("artist")] string Artist,
    [property: JsonPropertyName("bytes")] ulong Bytes,
    [property: JsonPropertyName("available")] bool Available,
    [property: JsonPropertyName("imported")] bool Imported)
{
    public string Detail => $"{Artist} · {FormatBytes(Bytes)}"
        + (Available ? "" : " · File missing");

    public string PlayLabel => Available ? "Play" : "Unavailable";

    private static string FormatBytes(ulong bytes) => bytes switch
    {
        >= 1_073_741_824 => $"{bytes / 1_073_741_824.0:F1} GB",
        >= 1_048_576 => $"{bytes / 1_048_576.0:F1} MB",
        >= 1_024 => $"{bytes / 1_024.0:F1} KB",
        _ => $"{bytes} B",
    };
}

internal sealed record DownloadsPayload(
    [property: JsonPropertyName("downloads")] DownloadedTrackViewModel[]? Downloads,
    [property: JsonPropertyName("message")] string? Message);

public sealed partial class ShellViewModel
{
    private bool _isDownloadsPage;

    public ObservableCollection<DownloadedTrackViewModel> Downloads { get; } = [];

    public bool IsDownloadsPage
    {
        get => _isDownloadsPage;
        private set => Set(ref _isDownloadsPage, value);
    }

    internal async Task LoadDownloadsAsync()
    {
        Status = "Reading downloaded files…";
        try
        {
            var answer = await _client.RequestAsync("downloads.list").ConfigureAwait(true);
            ApplyDownloads(answer.Deserialize<DownloadsPayload>(ServiceProtocol.Json));
            Status = Downloads.Count == 0
                ? "No imported downloads yet. Goosic only imports finalized files from a previous installation."
                : "";
        }
        catch (Exception error)
        {
            Status = "Could not read downloaded files: " + Describe(error);
        }
    }

    /// <summary>Imports finalized legacy files without modifying or deleting the originals.</summary>
    internal async Task ImportLegacyDownloadsAsync()
    {
        Status = "Reading finalized files from the previous Goosic…";
        try
        {
            var answer = await _client.RequestAsync("downloads.importLegacy").ConfigureAwait(true);
            var payload = answer.Deserialize<DownloadsPayload>(ServiceProtocol.Json);
            ApplyDownloads(payload);
            Status = payload?.Message ?? "Imported downloaded files from the previous Goosic.";
        }
        catch (Exception error)
        {
            Status = "Could not import downloaded files: " + Describe(error);
        }
    }

    internal TrackViewModel? SelectDownloaded(DownloadedTrackViewModel download)
    {
        if (!download.Available)
        {
            Status = "This downloaded file is missing from disk. Refresh Downloads to recheck it.";
            return null;
        }

        return StartQueue([new TrackViewModel(download)], null);
    }

    private void ApplyDownloads(DownloadsPayload? payload)
    {
        Downloads.Clear();
        foreach (var download in payload?.Downloads ?? [])
        {
            Downloads.Add(download);
        }
    }
}
