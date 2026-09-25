using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public class SidePanelStateTests
{
    [Fact]
    public void OpeningOnePanelReplacesTheOther()
    {
        var state = new SidePanelState();
        Assert.Equal(SidePanelContent.Lyrics, state.Toggle(SidePanelContent.Lyrics, available: true));
        Assert.Equal(SidePanelContent.Queue, state.Toggle(SidePanelContent.Queue, available: true));
    }

    [Fact]
    public void TogglingTheOpenPanelClosesIt()
    {
        var state = new SidePanelState();
        state.Toggle(SidePanelContent.Queue, true);
        Assert.Equal(SidePanelContent.None, state.Toggle(SidePanelContent.Queue, true));
        Assert.False(state.IsOpen);
    }

    [Fact]
    public void NothingToShowCannotOpen()
    {
        var state = new SidePanelState();
        Assert.Equal(SidePanelContent.None, state.Toggle(SidePanelContent.Lyrics, available: false));
    }

    [Fact]
    public void AnUnavailablePanelLeavesTheOtherAlone()
    {
        var state = new SidePanelState();
        state.Toggle(SidePanelContent.Lyrics, true);
        Assert.Equal(SidePanelContent.Lyrics, state.Toggle(SidePanelContent.Queue, available: false));
    }

    [Fact]
    public void AShowingPanelClosesWhenItsContentIsGone()
    {
        var state = new SidePanelState();
        state.Toggle(SidePanelContent.Queue, true);
        Assert.Equal(SidePanelContent.None, state.Toggle(SidePanelContent.Queue, available: false));
    }

    [Theory]
    [InlineData(SidePanelContent.Lyrics)]
    [InlineData(SidePanelContent.Queue)]
    public void ClosingReturnsFocusToTheToggleThatOpenedIt(SidePanelContent content)
    {
        var state = new SidePanelState();
        state.Toggle(content, true);
        Assert.Equal(content, state.Close());
        Assert.False(state.IsOpen);
    }

    [Fact]
    public void ClosingWhenAlreadyClosedStillNamesAFocusTarget()
    {
        var state = new SidePanelState();
        state.Toggle(SidePanelContent.Lyrics, true);
        state.Toggle(SidePanelContent.Lyrics, true);
        Assert.Equal(SidePanelContent.Lyrics, state.Close());
        Assert.Equal(SidePanelContent.Queue, new SidePanelState().Close());
    }
}
