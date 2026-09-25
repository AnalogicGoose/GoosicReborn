using System;
using System.Linq;
using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public sealed class ListRulesTests
{
    private sealed record Row(string Title, string Subtitle, string? Artist, string? Album, string Duration, long Ordinal)
        : ISortableTrack;

    private static readonly Row[] Playlist =
    [
        new("Golden", "Harry Styles", "Harry Styles", "Fine Line", "3:29", 1),
        new("Call Back", "Don Toliver", "Don Toliver", "OCTANE", "2:04", 2),
        new("Lights Up", "Harry Styles", "Harry Styles", "Fine Line", "2:53", 3),
        new("Untimed", "Someone", "Someone", null, "", 4),
    ];

    [Fact]
    public void Custom_order_is_the_order_the_rows_arrived_in()
    {
        var shuffled = Playlist.Reverse();
        Assert.Equal([1, 2, 3, 4], TrackOrder.Sort(shuffled, "custom").Select(row => row.Ordinal));
    }

    [Fact]
    public void Sorting_by_title_ignores_case_and_ties_keep_the_playlist_order()
    {
        Assert.Equal(["Call Back", "Golden", "Lights Up", "Untimed"], TrackOrder.Sort(Playlist, "title").Select(row => row.Title));
        // Both Harry Styles songs stay in their playlist order.
        Assert.Equal(["Call Back", "Golden", "Lights Up", "Untimed"], TrackOrder.Sort(Playlist, "artist").Select(row => row.Title));
    }

    [Fact]
    public void A_song_with_no_length_sorts_after_every_timed_song()
    {
        Assert.Equal(["Call Back", "Lights Up", "Golden", "Untimed"], TrackOrder.Sort(Playlist, "duration").Select(row => row.Title));
    }

    [Fact]
    public void An_unknown_sort_is_the_playlist_order()
    {
        Assert.Equal([1, 2, 3, 4], TrackOrder.Sort(Playlist, "nonsense").Select(row => row.Ordinal));
        Assert.Equal("Custom order", TrackOrder.Label("nonsense"));
    }

    [Fact]
    public void Find_matches_title_artist_line_and_album_without_case()
    {
        Assert.True(TrackOrder.Matches(Playlist[0], "harry"));
        Assert.True(TrackOrder.Matches(Playlist[1], "octane"));
        Assert.True(TrackOrder.Matches(Playlist[2], "LIGHTS"));
        Assert.False(TrackOrder.Matches(Playlist[1], "harry"));
        Assert.True(TrackOrder.Matches(Playlist[3], "   "));
    }

    [Fact]
    public void A_search_is_remembered_first_once_and_the_list_stays_short()
    {
        var earlier = Enumerable.Range(1, RecentSearches.Limit).Select(i => $"query {i}").ToList();
        var remembered = RecentSearches.Remember(earlier, "  QUERY 3 ");
        Assert.Equal("QUERY 3", remembered[0]);
        Assert.Single(remembered, item => item.Equals("query 3", StringComparison.OrdinalIgnoreCase));
        Assert.Equal(RecentSearches.Limit, remembered.Count);
        Assert.Equal(earlier.Take(RecentSearches.Limit), RecentSearches.Remember(earlier, "   "));
    }

    [Fact]
    public void Suggestions_narrow_as_you_type_and_never_repeat_the_box()
    {
        string[] recent = ["bad bunny", "bad omens", "daft punk"];
        Assert.Equal(recent, RecentSearches.Suggest(recent, ""));
        Assert.Equal(["bad bunny", "bad omens"], RecentSearches.Suggest(recent, "BAD"));
        Assert.DoesNotContain("bad bunny", RecentSearches.Suggest(recent, "bad bunny"));
    }

    [Fact]
    public void Up_next_names_its_source_and_counts_songs()
    {
        Assert.Equal("From Liked Music · 99 songs", PlayerText.UpNext("Liked Music", 99, findingMore: false));
        Assert.Equal("1 song", PlayerText.UpNext("", 1, findingMore: false));
        Assert.Equal("From Golden radio · Finding songs like this…", PlayerText.UpNext("Golden radio", 0, findingMore: true));
        Assert.Equal("Nothing after this", PlayerText.UpNext("", 0, findingMore: false));
    }

    [Fact]
    public void The_sleep_timer_label_rounds_the_last_minute_up()
    {
        Assert.Equal("Sleep timer", PlayerText.SleepTimer(null, endOfSong: false));
        Assert.Equal("Sleep timer: end of song", PlayerText.SleepTimer(null, endOfSong: true));
        Assert.Equal("Sleep timer: 15 min left", PlayerText.SleepTimer(TimeSpan.FromMinutes(14.2), endOfSong: false));
        Assert.Equal("Sleep timer: 1 min left", PlayerText.SleepTimer(TimeSpan.FromSeconds(5), endOfSong: false));
    }

    [Fact]
    public void Liked_music_is_recognised_by_either_id_and_only_as_a_playlist()
    {
        Assert.True(CatalogRules.IsLikedMusic("playlist", "LM"));
        Assert.True(CatalogRules.IsLikedMusic("playlist", "VLLM"));
        Assert.False(CatalogRules.IsLikedMusic("album", "VLLM"));
        Assert.False(CatalogRules.IsLikedMusic("playlist", "PLxyz"));
    }

    [Fact]
    public void Library_artists_are_the_artists_of_its_songs_not_its_subscriptions()
    {
        Assert.Equal("FEmusic_library_corpus_track_artists", CatalogRules.LibraryArtists);
        Assert.Equal("FEmusic_library_corpus_artists", CatalogRules.LibrarySubscriptions);
    }

    [Fact]
    public void A_category_colour_is_read_only_from_hash_and_six_hex_digits()
    {
        Assert.Equal(((byte)0xA4, (byte)0xC5, (byte)0xFF), CatalogRules.ParseColor("#A4C5FF"));
        Assert.Null(CatalogRules.ParseColor(null));
        Assert.Null(CatalogRules.ParseColor("A4C5FF"));
        Assert.Null(CatalogRules.ParseColor("#A4C5FG"));
        Assert.Null(CatalogRules.ParseColor("#FFF"));
    }
}
