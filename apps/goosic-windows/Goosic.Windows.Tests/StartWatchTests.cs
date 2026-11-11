using System;
using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public class StartWatchTests
{
    private static StartWatchAction After(double seconds, bool heard = false, bool listener = false, bool nudged = false) =>
        StartWatch.Decide(TimeSpan.FromSeconds(seconds), heard, listener, nudged);

    [Fact]
    public void A_page_that_just_loaded_is_given_time()
    {
        Assert.Equal(StartWatchAction.Wait, After(1));
    }

    [Fact]
    public void A_page_still_silent_after_a_few_seconds_is_asked_to_play_once()
    {
        Assert.Equal(StartWatchAction.Nudge, After(4));
        Assert.Equal(StartWatchAction.Wait, After(6, nudged: true));
    }

    [Fact]
    public void A_track_that_never_plays_is_given_up_on()
    {
        Assert.Equal(StartWatchAction.GiveUp, After(20, nudged: true));
        Assert.Equal(StartWatchAction.GiveUp, After(45));
    }

    [Fact]
    public void A_track_that_has_played_is_never_given_up_on()
    {
        Assert.Equal(StartWatchAction.Done, After(60, heard: true));
    }

    [Fact]
    public void A_pause_the_listener_chose_is_left_alone()
    {
        Assert.Equal(StartWatchAction.Done, After(5, listener: true));
        Assert.Equal(StartWatchAction.Done, After(60, listener: true));
    }
}
