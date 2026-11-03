namespace Goosic.Windows.Presentation;

/// <summary>Only the latest request in the same idle account context may replace the canvas.</summary>
/// <remarks>
/// A load the account operation started itself, such as Home after a switch, began while the
/// account was busy and must still land; only a request that an account change overtook is
/// dropped, because that operation shows its own page afterwards.
/// </remarks>
public static class FeedNavigationCommitRules
{
    public static bool CanCommit(int requestRevision, int currentRevision, string requestScope,
        string currentScope, bool accountBusy, bool accountBusyAtStart = false) =>
        requestRevision == currentRevision && requestScope == currentScope
        && !(accountBusy && !accountBusyAtStart);

    /// <summary>What choosing the destination already on screen does.</summary>
    /// <remarks>
    /// The page is kept only when something will leave it complete. A page cleared for a
    /// navigation has nothing left to keep, and a page still loading whose own request a later
    /// navigation overtook has nothing left to finish it; both load again. Otherwise a later
    /// navigation is cancelled, but never the request still filling the page: that request is the
    /// destination chosen again.
    /// </remarks>
    public static ReselectOutcome Reselect(bool pageCleared, bool pageLoading, int currentRevision,
        int currentPageRevision) =>
        pageCleared ? ReselectOutcome.Reload
        : currentRevision == currentPageRevision ? ReselectOutcome.Keep
        : pageLoading ? ReselectOutcome.Reload
        : ReselectOutcome.CancelPending;
}

public enum ReselectOutcome
{
    /// <summary>The latest request is the page's own; leave it to finish.</summary>
    Keep,
    /// <summary>Drop the later navigation and keep the complete page on screen.</summary>
    CancelPending,
    /// <summary>Load the destination again.</summary>
    Reload,
}
