using System.Collections.Generic;
using System.Linq;
using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public class SidebarCustomizationTests
{
    [Theory]
    [InlineData("search")]
    [InlineData("home")]
    [InlineData("settings")]
    public void TheWayBackCanNeverBeHidden(string tag)
    {
        Assert.False(SidebarCustomization.IsHideable(tag));
        Assert.True(SidebarCustomization.IsShown(tag, [tag]));
        Assert.Empty(SidebarCustomization.SetShown([], tag, shown: false));
        Assert.Empty(SidebarCustomization.Sanitize(new[] { tag }));
    }

    [Fact]
    public void AHiddenDestinationIsGoneAndComesBack()
    {
        var hidden = SidebarCustomization.SetShown([], "charts", shown: false);
        Assert.False(SidebarCustomization.IsShown("charts", hidden.ToArray()));
        Assert.True(SidebarCustomization.IsShown("explore", hidden.ToArray()));
        var back = SidebarCustomization.SetShown(hidden, "charts", shown: true);
        Assert.True(SidebarCustomization.IsShown("charts", back.ToArray()));
    }

    [Fact]
    public void AHeadingDisappearsWithTheLastItemUnderIt()
    {
        string[] discover = ["explore", "charts", "moodsAndGenres", "newReleases"];
        Assert.False(SidebarCustomization.SectionHasItems("Discover", discover));
        Assert.True(SidebarCustomization.SectionHasItems("Discover", discover[1..]));
    }

    [Fact]
    public void AStoredListKeepsOnlyKnownNamesOnce() =>
        Assert.Equal(["charts"], SidebarCustomization.Sanitize(new[] { "charts", "charts", "nonsense", "home" }));

    [Fact]
    public void PinsAreKeptOnceInTheOrderTheyWerePinned()
    {
        var pins = SidebarCustomization.Pin([], new SidebarPin("playlist", "PL1", "One"));
        pins = SidebarCustomization.Pin(pins, new SidebarPin("album", "MP2", "Two"));
        pins = SidebarCustomization.Pin(pins, new SidebarPin("playlist", "PL1", "One again"));
        Assert.Equal(["PL1", "MP2"], pins.Select(pin => pin.Id));
        Assert.True(SidebarCustomization.IsPinned(pins, "album", "MP2"));
        Assert.False(SidebarCustomization.IsPinned(SidebarCustomization.Unpin(pins, "album", "MP2"), "album", "MP2"));
    }

    [Fact]
    public void ASongAndItsVideoAreOnePin()
    {
        var pins = SidebarCustomization.Pin([], new SidebarPin("song", "abc", "Song"));
        Assert.True(SidebarCustomization.IsPinned(pins, "video", "abc"));
        Assert.Single(SidebarCustomization.Pin(pins, new SidebarPin("video", "abc", "Video")));
    }

    [Fact]
    public void PinsStopAtTheLimit()
    {
        IReadOnlyList<SidebarPin> pins = [];
        for (var index = 0; index < SidebarCustomization.MaximumPins + 5; index++)
        {
            pins = SidebarCustomization.Pin(pins, new SidebarPin("playlist", $"PL{index}", "P"));
        }

        Assert.Equal(SidebarCustomization.MaximumPins, pins.Count);
    }

    [Theory]
    [InlineData(0, -1, "ABC")]
    [InlineData(0, 1, "BAC")]
    [InlineData(2, 1, "ABC")]
    [InlineData(2, -2, "CAB")]
    [InlineData(5, 1, "ABC")]
    public void MovingAPinStopsAtEitherEnd(int index, int offset, string expected)
    {
        IReadOnlyList<SidebarPin> pins = new[] { "A", "B", "C" }.Select(id => new SidebarPin("album", id, id)).ToArray();
        Assert.Equal(expected, string.Concat(SidebarCustomization.Move(pins, index, offset).Select(pin => pin.Id)));
    }

    [Fact]
    public void AStoredPinListDropsWhatCannotBeOpened()
    {
        var pins = SidebarCustomization.Sanitize(new SidebarPin?[]
        {
            new("playlist", "PL1", "One"), null, new("playlist", "", "No id"),
            new("episode", "E1", "Unknown kind"), new("playlist", "PL1", "Duplicate"),
        });
        Assert.Equal(["PL1"], pins.Select(pin => pin.Id));
    }

    [Fact]
    public void TheRemovedDownloadsPageIsNoLongerOffered()
    {
        Assert.False(SidebarCustomization.IsHideable("downloads"));
        Assert.Empty(SidebarCustomization.Sanitize(["downloads"]));
    }
}
