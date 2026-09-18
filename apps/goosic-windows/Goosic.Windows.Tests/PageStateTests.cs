using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public class PageStateTests
{
    [Fact]
    public void LoadingShowsTheSkeletonNotThePanel()
    {
        Assert.False(PageState.Loading.ShowsPanel);
        Assert.False(PageState.Content.ShowsPanel);
    }

    [Fact]
    public void AStoppedServiceCannotBeRetriedFromThePage()
    {
        var state = PageState.Failure(PageFailure.ServiceUnavailable, null, "Home", networkAvailable: true);
        Assert.Equal(PageStateKind.ServiceUnavailable, state.Kind);
        Assert.False(state.CanRetry);
    }

    [Theory]
    [InlineData(PageFailure.Timeout, null)]
    [InlineData(PageFailure.Refused, "catalogUnavailable")]
    public void NoNetworkReadsAsOffline(PageFailure failure, string? code)
    {
        var state = PageState.Failure(failure, code, "Home", networkAvailable: false);
        Assert.Equal(PageStateKind.Offline, state.Kind);
        Assert.True(state.CanRetry);
    }

    [Theory]
    [InlineData("catalogUnavailable", "Catalog unreachable")]
    [InlineData("catalogUpstreamError", "Catalog rejected the request")]
    [InlineData("catalogDecodeError", "Unreadable catalog response")]
    [InlineData("somethingNew", "Could not load")]
    public void RefusalsUseTheSharedTitles(string code, string title)
    {
        var state = PageState.Failure(PageFailure.Refused, code, "Home", networkAvailable: true);
        Assert.Equal(title, state.Title);
        Assert.True(state.CanRetry);
    }

    [Fact]
    public void AnEmptyCatalogIsEmptyNotAFailure() =>
        Assert.Equal(PageStateKind.Empty,
            PageState.Failure(PageFailure.Refused, "catalogEmpty", "Charts", true).Kind);

    [Fact]
    public void AnEmptySearchNamesTheQuery()
    {
        var state = PageState.Empty(PageSubject.Search, "lofi");
        Assert.Contains("“lofi”", state.Message);
        Assert.False(state.CanRetry);
    }

    [Fact]
    public void SignedOutOffersSignInNotRetry()
    {
        Assert.True(PageState.SignInRequired.CanSignIn);
        Assert.False(PageState.SignInRequired.CanRetry);
    }

    [Fact]
    public void DownloadsDistinguishUnreadableEmptyAndUnplayable()
    {
        Assert.Equal(PageStateKind.Failed, PageState.Downloads(null).Kind);
        Assert.True(PageState.Downloads(null).CanRetry);
        Assert.Equal(PageStateKind.Empty, PageState.Downloads(0).Kind);
        Assert.False(PageState.Downloads(0).CanRetry);

        var some = PageState.Downloads(3);
        Assert.Equal(PageStateKind.NotSupported, some.Kind);
        Assert.False(some.CanRetry);
        Assert.StartsWith("3 downloaded tracks are", some.Message);
        Assert.StartsWith("1 downloaded track is", PageState.Downloads(1).Message);
    }
}
