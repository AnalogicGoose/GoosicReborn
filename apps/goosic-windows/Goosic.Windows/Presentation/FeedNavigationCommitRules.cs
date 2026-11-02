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
}
