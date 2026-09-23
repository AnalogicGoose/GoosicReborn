using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public sealed class StartRouteTests
{
    [Theory]
    [InlineData("home", "charts", "home")]
    [InlineData("library", "charts", "library")]
    [InlineData("liked", null, "liked")]
    [InlineData("last", "charts", "charts")]
    [InlineData("last", "moodsAndGenres", "moodsAndGenres")]
    public void TheChosenPageOpens(string startPage, string? lastRoute, string expected) =>
        Assert.Equal(expected, StartRoute.For(startPage, lastRoute));

    [Theory]
    [InlineData("last", "settings")]
    [InlineData("last", "downloads")]
    [InlineData("last", "somethingNewer")]
    [InlineData("last", null)]
    [InlineData(null, "charts")]
    [InlineData("nonsense", "charts")]
    public void AnythingElseOpensHome(string? startPage, string? lastRoute) =>
        Assert.Equal("home", StartRoute.For(startPage, lastRoute));

    [Fact]
    public void OnlyReopenablePagesAreRemembered()
    {
        Assert.True(StartRoute.IsRemembered("explore"));
        Assert.False(StartRoute.IsRemembered("settings"));
        Assert.False(StartRoute.IsRemembered("downloads"));
    }

    [Fact]
    public void AnUnknownPreferenceSelectsTheFirstChoice() =>
        Assert.Equal(0, StartRoute.IndexOf("nonsense"));
}
