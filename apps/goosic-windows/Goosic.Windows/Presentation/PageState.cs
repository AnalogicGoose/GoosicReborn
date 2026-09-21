namespace Goosic.Windows.Presentation;

public enum PageStateKind
{
    /// <summary>The page has something to show; no state screen.</summary>
    Content,
    Loading,
    Empty,
    /// <summary>The network is down, so nothing the catalog says can arrive.</summary>
    Offline,
    /// <summary>The service process is gone; nothing works until the app is reopened.</summary>
    ServiceUnavailable,
    /// <summary>The request was answered with a failure that trying again may clear.</summary>
    Failed,
    /// <summary>The page cannot exist on this platform yet. Retrying would not help.</summary>
    NotSupported,
    /// <summary>The page needs an account.</summary>
    SignInRequired,
}

/// <summary>How a request failed, as far as the page is concerned.</summary>
public enum PageFailure
{
    ServiceUnavailable,
    Timeout,
    /// <summary>A refusal from the service, carrying its code.</summary>
    Refused,
    Other,
}

/// <summary>What the page area shows in place of content, and whether it offers Retry.</summary>
public sealed record PageState(PageStateKind Kind, string Title, string Message, string Glyph)
{
    public static PageState Content { get; } = new(PageStateKind.Content, "", "", "");

    public static PageState Loading { get; } = new(PageStateKind.Loading, "", "", "");

    public bool ShowsPanel => Kind is not (PageStateKind.Content or PageStateKind.Loading);

    /// <summary>
    /// Retry is offered only after a failure. An empty answer was a real answer, and a button
    /// that asks the same question again would suggest otherwise.
    /// </summary>
    public bool CanRetry => Kind is PageStateKind.Failed or PageStateKind.Offline;

    public bool CanSignIn => Kind == PageStateKind.SignInRequired;

    /// <summary>The empty screen for a page, worded for what the page is.</summary>
    /// <param name="subject">What was asked for: a search's query, or the page's title.</param>
    public static PageState Empty(PageSubject subject, string name) => subject switch
    {
        PageSubject.Search => new(PageStateKind.Empty, "No results",
            $"Nothing matched “{name}”. Check the spelling or try fewer words.", Glyphs.Search),
        PageSubject.Library => new(PageStateKind.Empty, "Nothing here yet",
            "Music you save, like or play on this account will appear here.", Glyphs.Library),
        _ => new(PageStateKind.Empty, "Nothing to show",
            $"{name} came back empty.", Glyphs.Music),
    };

    public static PageState SignInRequired { get; } = new(PageStateKind.SignInRequired, "Sign in to see this",
        "Sign in with your Google account to see your library, liked music and history.", Glyphs.Account);

    /// <summary>
    /// Downloads made by the previous Goosic are imported and decoded by the service, but the
    /// Windows shell has no local playback host to play them through yet.
    /// </summary>
    public static PageState DownloadsNotSupported(int trackCount) => new(PageStateKind.NotSupported,
        "Downloads aren’t playable here yet",
        (trackCount == 1 ? "1 downloaded track is" : $"{trackCount} downloaded tracks are")
            + " safe and unchanged. Playing them needs a local player the Windows app doesn’t have yet.",
        Glyphs.Download);

    public static PageState DownloadsEmpty { get; } = new(PageStateKind.Empty, "No downloads",
        "Tracks saved by a previous Goosic appear here once they’ve been imported.", Glyphs.Download);

    /// <summary>The download index could not be opened; the files themselves are untouched.</summary>
    public static PageState DownloadsUnavailable { get; } = new(PageStateKind.Failed, "Downloads unavailable",
        "Goosic couldn’t open its list of downloads. Your files haven’t been changed. Try again.", Glyphs.Download);

    /// <summary>What the Downloads page shows for a <c>downloads.list</c> answer.</summary>
    /// <param name="trackCount">The tracks listed, or null when the list could not be read.</param>
    public static PageState Downloads(int? trackCount) => trackCount switch
    {
        null => DownloadsUnavailable,
        0 => DownloadsEmpty,
        var count => DownloadsNotSupported(count.Value),
    };

    /// <summary>
    /// The failure screen. Titles follow <c>failure_text</c> in <c>goosic-shell-support</c>, so
    /// every shell names a failure the same way; until the FFI exports that rule, a change to
    /// its wording is a change here too.
    /// </summary>
    /// <param name="networkAvailable">
    /// Whether the machine reports a network. A catalog that cannot be reached on a machine with no
    /// network is shown as offline, which tells the person where to look.
    /// </param>
    public static PageState Failure(PageFailure failure, string? code, string subject, bool networkAvailable)
    {
        if (failure == PageFailure.ServiceUnavailable)
        {
            return new(PageStateKind.ServiceUnavailable, "Service not connected",
                "Goosic’s playback service stopped. Reopen the app to start it again.", Glyphs.Warning);
        }

        if (!networkAvailable && failure is PageFailure.Timeout or PageFailure.Refused)
        {
            return new(PageStateKind.Offline, "You’re offline",
                "Connect to the internet, then try again.", Glyphs.Offline);
        }

        if (failure == PageFailure.Timeout)
        {
            return new(PageStateKind.Failed, "Taking too long",
                "YouTube Music didn’t answer in time. Check your connection and try again.", Glyphs.Offline);
        }

        return code switch
        {
            "catalogEmpty" => new(PageStateKind.Empty, "No results",
                $"{subject} returned nothing to show.", Glyphs.Music),
            "catalogUnavailable" => new(PageStateKind.Failed, "Catalog unreachable",
                "Could not reach YouTube Music. Check your connection and try again.", Glyphs.Offline),
            "catalogUpstreamError" => new(PageStateKind.Failed, "Catalog rejected the request",
                "YouTube Music turned this request down. Try again in a moment.", Glyphs.Warning),
            "catalogDecodeError" => new(PageStateKind.Failed, "Unreadable catalog response",
                "YouTube Music answered in a shape this build does not understand.", Glyphs.Warning),
            _ => new(PageStateKind.Failed, "Could not load",
                $"{subject} is temporarily unavailable. Try again.", Glyphs.Warning),
        };
    }

    /// <summary>Segoe Fluent Icons code points.</summary>
    public static class Glyphs
    {
        public const string Search = "\uE721";
        public const string Library = "\uE8F1";
        public const string Music = "\uEC4F";
        public const string Account = "\uE77B";
        public const string Download = "\uE896";
        public const string Warning = "\uE7BA";
        public const string Offline = "\uF384";
    }
}

public enum PageSubject { Browse, Search, Library, Entity }
