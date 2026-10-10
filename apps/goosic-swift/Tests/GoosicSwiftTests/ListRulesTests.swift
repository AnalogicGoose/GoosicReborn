import XCTest

@testable import GoosicSwift

/// The list, queue and player rules the macOS shell shares with the Windows one
/// (`Presentation/ListRules.cs` there). Both are held to the same cases.
final class ListRulesTests: XCTestCase {
    private func track(
        _ id: String, title: String = "", artist: String = "", album: String = "",
        duration: String = "", explicit: Bool = false
    ) -> GoosicTrack {
        GoosicTrack(
            id: id, title: title.isEmpty ? id : title, subtitle: "", artist: artist, artistID: nil,
            album: album, albumID: nil, duration: duration, videoID: id, explicit: explicit
        )
    }

    func testCustomOrderIsThePlaylistsOwnOrder() {
        let rows = [track("b"), track("a"), track("c")]
        XCTAssertEqual(TrackSortOrder.custom.sorted(rows).map(\.id), ["b", "a", "c"])
    }

    func testSortingByTitleIgnoresCaseAndKeepsTiesInPlaylistOrder() {
        let rows = [track("1", title: "beta"), track("2", title: "Alpha"), track("3", title: "alpha")]
        XCTAssertEqual(TrackSortOrder.title.sorted(rows).map(\.id), ["2", "3", "1"])
    }

    func testAnUnknownLengthSortsLast() {
        let rows = [track("x", duration: ""), track("long", duration: "4:10"), track("short", duration: "2:01")]
        XCTAssertEqual(TrackSortOrder.duration.sorted(rows).map(\.id), ["short", "long", "x"])
    }

    func testFindMatchesTitleArtistOrAlbum() {
        let row = track("v", title: "Afterglow", artist: "Signal Fires", album: "Night Windows")
        XCTAssertTrue(TrackSortOrder.matches(row, "glow"))
        XCTAssertTrue(TrackSortOrder.matches(row, "signal"))
        XCTAssertTrue(TrackSortOrder.matches(row, "WINDOWS"))
        XCTAssertTrue(TrackSortOrder.matches(row, "   "))
        XCTAssertFalse(TrackSortOrder.matches(row, "dawn"))
    }

    func testRecentSearchesPutTheNewestFirstWithoutACopyAndStopAtEight() {
        let earlier = ["one", "Two", "three"]
        XCTAssertEqual(RecentSearches.remember(earlier, " two "), ["two", "one", "three"])
        let full = (1...8).map { "q\($0)" }
        XCTAssertEqual(RecentSearches.remember(full, "new").count, RecentSearches.limit)
        XCTAssertEqual(RecentSearches.remember(full, "new").first, "new")
        XCTAssertEqual(RecentSearches.remember(earlier, "   "), earlier)
    }

    func testSuggestionsNarrowAsYouTypeAndNeverRepeatTheBox() {
        let recent = ["daft punk", "punk rock", "jazz"]
        XCTAssertEqual(RecentSearches.suggest(recent, ""), recent)
        XCTAssertEqual(RecentSearches.suggest(recent, "punk"), ["daft punk", "punk rock"])
        XCTAssertEqual(RecentSearches.suggest(recent, "Jazz"), [])
    }

    func testUpNextSaysWhereTheQueueCameFrom() {
        XCTAssertEqual(PlayerText.upNext(source: "Liked Music", remaining: 99, findingMore: false),
                       "From Liked Music · 99 songs")
        XCTAssertEqual(PlayerText.upNext(source: nil, remaining: 1, findingMore: false), "1 song")
        XCTAssertEqual(PlayerText.upNext(source: "Song radio", remaining: 0, findingMore: true),
                       "From Song radio · Finding songs like this…")
        XCTAssertEqual(PlayerText.upNext(source: "", remaining: 0, findingMore: false), "Nothing after this")
    }

    func testAnUnfinishedListDoesNotCountItselfAsComplete() {
        XCTAssertEqual(PlayerText.songCount(100, hasMore: true), "100+ songs")
        XCTAssertEqual(PlayerText.songCount(100, hasMore: false), "100 songs")
        XCTAssertEqual(PlayerText.songCount(1, hasMore: false), "1 song")
    }

    func testTheSleepTimerLabelRoundsUp() {
        XCTAssertEqual(PlayerText.sleepTimer(remaining: nil, endOfSong: false), "Sleep timer")
        XCTAssertEqual(PlayerText.sleepTimer(remaining: 20, endOfSong: false), "Sleep timer: 1 min left")
        XCTAssertEqual(PlayerText.sleepTimer(remaining: 61, endOfSong: false), "Sleep timer: 2 min left")
        XCTAssertEqual(PlayerText.sleepTimer(remaining: 61, endOfSong: true), "Sleep timer: end of song")
    }

    func testMovingQueueRowsFollowsTheListGesture() {
        let rows = ["a", "b", "c", "d"]
        XCTAssertEqual(GoosicAppModel.moving(rows, from: [0], to: 3), ["b", "c", "a", "d"])
        XCTAssertEqual(GoosicAppModel.moving(rows, from: [3], to: 0), ["d", "a", "b", "c"])
        XCTAssertEqual(GoosicAppModel.moving(rows, from: [1, 2], to: 4), ["a", "d", "b", "c"])
        XCTAssertNil(GoosicAppModel.moving(rows, from: [9], to: 0))
    }

    func testLikedMusicIsOnePageWhicheverIdOpensIt() {
        XCTAssertEqual(GoosicEntityReference.playlist("VLLM").normalized, .likedMusic)
        XCTAssertTrue(GoosicEntityReference.playlist("LM").isLikedMusic)
        XCTAssertFalse(GoosicEntityReference.playlist("PL1").isLikedMusic)
    }

    func testCategoryColoursAreReadOnlyFromHex() {
        let color = categoryColor("#FF8000")
        XCTAssertEqual(color?.red, 1)
        XCTAssertEqual(color?.blue, 0)
        XCTAssertNil(categoryColor("orange"))
        XCTAssertNil(categoryColor(nil))
    }

    func testMoodsBecomeTilesAndLibrariesBecomeAGrid() {
        let mood = GoosicCatalogItem(kind: .category, id: "b|p", title: "Chill", subtitle: "", color: "#112233")
        let shelf = GoosicShelf(id: "m", title: "Moods", cards: [GoosicCard(catalog: mood)])
        XCTAssertEqual(ShelfPresentation.preferred(for: .route(.moodsAndGenres), shelf: shelf), .tiles)
        XCTAssertEqual(GoosicCard(catalog: mood).action, .show(.category("b|p")))

        let album = GoosicCatalogItem(kind: .album, id: "MPRE", title: "A", subtitle: "")
        let library = GoosicShelf(id: "l", title: "", cards: [GoosicCard(catalog: album)])
        XCTAssertEqual(ShelfPresentation.preferred(for: .library("Albums"), shelf: library), .grid)
        XCTAssertEqual(ShelfPresentation.preferred(for: .route(.home), shelf: library), .cards)
    }

    func testAShelfYouTubeMusicMarksAsAListIsDrawnAsColumnsOfRows() {
        let song = GoosicCatalogItem(kind: .song, id: "v", title: "S", subtitle: "", videoId: "v")
        let wire = GoosicCatalogPage(
            id: "home", title: "Home",
            shelves: [GoosicCatalogShelf(id: "q", title: "Quick picks", items: [song], layout: "list")]
        )
        let page = CatalogPageView(wire: wire)
        guard case .columns(let rows) = ShelfPresentation.preferred(for: .route(.home), shelf: page.shelves[0]) else {
            return XCTFail("a list shelf on Home should be columns of rows")
        }
        XCTAssertEqual(rows.map(\.videoID), ["v"])
    }

    func testAPlaylistCardForLikedMusicOpensLikedMusic() {
        let card = GoosicCard(catalog: GoosicCatalogItem(kind: .playlist, id: "VLLM", title: "Liked Music", subtitle: ""))
        XCTAssertEqual(card.action, .show(.likedMusic))
    }

    func testThePageStoreKeepsPagesPerScopeWithoutTheirCursor() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let store = CatalogPageStore(root: root)
        let page = GoosicCatalogPage(id: "p", title: "Mix", tracks: [], thumbnail: "https://x", nextCursor: "c1")
        store.save(page, for: .playlist("PL/../1"), scope: "account-a")

        let loaded = try XCTUnwrap(store.load(.playlist("PL/../1"), scope: "account-a"))
        XCTAssertEqual(loaded.title, "Mix")
        XCTAssertEqual(loaded.thumbnail, "https://x")
        XCTAssertNil(loaded.nextCursor)
        XCTAssertNil(store.load(.playlist("PL/../1"), scope: "account-b"))
        XCTAssertNil(CatalogPageStore.name(for: .search(query: "q", filter: "all")))

        store.removeScope("account-a")
        XCTAssertNil(store.load(.playlist("PL/../1"), scope: "account-a"))
    }

    func testAnEndedSessionIsRecognisedFromTheReadersMarker() {
        struct Scripted: LocalizedError { let errorDescription: String? }
        XCTAssertTrue(PersonalSessionExpired.matches(PersonalSessionExpired()))
        XCTAssertTrue(PersonalSessionExpired.matches(Scripted(errorDescription: "GOOSIC_SIGNED_OUT: ended")))
        XCTAssertFalse(PersonalSessionExpired.matches(Scripted(errorDescription: "HTTP 500")))
    }

    func testTheHeaderSplitsItsSubtitleIntoBylineAndFacts() {
        let album = CollectionHeaderLines(subtitle: "Album • Metro Boomin • 2023", kind: "Album")
        XCTAssertEqual(album.byline, "Metro Boomin")
        XCTAssertEqual(album.facts, ["2023"])
        let liked = CollectionHeaderLines(subtitle: "Liked Music · Auto playlist • 2026", kind: "Liked Music")
        XCTAssertNil(liked.byline)
        XCTAssertEqual(liked.facts, ["2026"])
        XCTAssertEqual(CollectionHeaderLines(subtitle: "", kind: "Playlist"), CollectionHeaderLines(subtitle: " • ", kind: "Playlist"))
    }
}
