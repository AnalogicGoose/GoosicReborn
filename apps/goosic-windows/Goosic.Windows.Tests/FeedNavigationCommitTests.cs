using System.Threading.Tasks;
using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public class FeedNavigationCommitTests
{
    [Fact]
    public async Task ASlowEarlierReplyCannotReplaceTheNewerDestination()
    {
        var slow = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var revision = 1;
        var commits = 0;
        async Task CompleteSlow()
        {
            await slow.Task;
            if (FeedNavigationCommitRules.CanCommit(1, revision, "guest", "guest", false)) commits++;
        }

        var pending = CompleteSlow();
        revision = 2;
        Assert.True(FeedNavigationCommitRules.CanCommit(2, revision, "guest", "guest", false));
        slow.SetResult();
        await pending;
        Assert.Equal(0, commits);
    }

    [Theory]
    [InlineData("account-a", "account-b", false)]
    [InlineData("account-a", "account-a", true)]
    public void AccountChangesBlockPreparedPersonalContent(string prepared, string active, bool busy) =>
        Assert.False(FeedNavigationCommitRules.CanCommit(3, 3, prepared, active, busy));

    [Fact]
    public void TheAccountOperationsOwnLoadStillLands() =>
        Assert.True(FeedNavigationCommitRules.CanCommit(4, 4, "account-b", "account-b", true, accountBusyAtStart: true));
}
