namespace Goosic.Windows.Presentation;

public enum SidePanelContent { None, Lyrics, Queue }

/// <summary>
/// Lyrics and Playing Next share the right edge, so at most one is open and opening one closes the
/// other. This holds that rule; <c>MainWindow</c> only mirrors the result onto its controls.
/// </summary>
public sealed class SidePanelState
{
    public SidePanelContent Content { get; private set; }

    public bool IsOpen => Content != SidePanelContent.None;

    /// <summary>
    /// The toggle that opened the panel most recently, so closing it can hand focus back there
    /// rather than dropping it on the window.
    /// </summary>
    public SidePanelContent LastOpened { get; private set; }

    /// <summary>Opens <paramref name="content"/>, or closes it when it is already showing.</summary>
    /// <param name="available">Whether there is anything to show: no playback means no lyrics.</param>
    /// <returns>The content now showing.</returns>
    public SidePanelContent Toggle(SidePanelContent content, bool available)
    {
        if (content == SidePanelContent.None)
        {
            return Content;
        }

        if (!available)
        {
            // Nothing to show: it cannot open, and if it was showing, what it showed is gone.
            if (Content == content)
            {
                Content = SidePanelContent.None;
            }

            return Content;
        }

        Content = Content == content ? SidePanelContent.None : content;
        if (Content != SidePanelContent.None)
        {
            LastOpened = Content;
        }

        return Content;
    }

    /// <summary>Closes the panel and says which toggle should receive focus.</summary>
    public SidePanelContent Close()
    {
        var focus = Content == SidePanelContent.None ? LastOpened : Content;
        Content = SidePanelContent.None;
        return focus == SidePanelContent.None ? SidePanelContent.Queue : focus;
    }
}
