namespace Goosic.Windows.Presentation;

public enum LyricsKind { NothingPlaying, Loading, Synced, Unsynced, NotFound, Unavailable }

/// <summary>What the lyrics panel shows, beyond the lines themselves.</summary>
public sealed record LyricsState(LyricsKind Kind, string Title, string Message)
{
    public static LyricsState NothingPlaying { get; } =
        new(LyricsKind.NothingPlaying, "Nothing playing", "Lyrics appear here when a track starts.");

    public static LyricsState Loading { get; } = new(LyricsKind.Loading, "", "Looking up lyrics…");

    public static LyricsState NotFound { get; } =
        new(LyricsKind.NotFound, "No lyrics", "No lyrics were found for this track.");

    public static LyricsState Unavailable { get; } =
        new(LyricsKind.Unavailable, "Lyrics unavailable", "The lyrics service didn’t answer. Try again.");

    /// <summary>Lines are showing; the caption says where they came from and whether they follow the song.</summary>
    public static LyricsState Found(bool synced, string source, bool truncated)
    {
        var caption = synced ? $"Synced · {source}" : $"Not synced · {source}";
        return new(synced ? LyricsKind.Synced : LyricsKind.Unsynced, "",
            truncated ? caption + " · Only the first part is shown" : caption);
    }

    public bool HasLines => Kind is LyricsKind.Synced or LyricsKind.Unsynced;

    /// <summary>The centred message replaces the lines; a found document shows a caption instead.</summary>
    public bool ShowsPlaceholder => !HasLines && Kind != LyricsKind.Loading;

    public bool IsLoading => Kind == LyricsKind.Loading;

    public bool CanRetry => Kind == LyricsKind.Unavailable;

    /// <summary>Unsynced lines are all shown at full strength; there is no current line to emphasise.</summary>
    public bool FollowsPlayback => Kind == LyricsKind.Synced;
}
