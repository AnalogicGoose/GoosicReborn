namespace Goosic.Windows.Presentation;

/// <summary>Only the latest request in the same idle account context may replace the canvas.</summary>
public static class FeedNavigationCommitRules
{
    public static bool CanCommit(int requestRevision, int currentRevision, string requestScope,
        string currentScope, bool accountBusy) =>
        requestRevision == currentRevision && requestScope == currentScope && !accountBusy;
}
