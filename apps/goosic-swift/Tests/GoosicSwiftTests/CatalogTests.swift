import XCTest

@testable import GoosicSwift

/// Covers the pure catalog-to-display conversions. Anything requiring a window, a service
/// process, or the network belongs in the Rust live tests instead.
final class CatalogConversionTests: XCTestCase {
    private func item(
        kind: GoosicCatalogKind,
        id: String,
        title: String,
        subtitle: String = "",
        artist: String? = nil,
        album: String? = nil,
        duration: String? = nil,
        videoId: String? = nil,
        explicit: Bool? = nil
    ) -> GoosicCatalogItem {
        GoosicCatalogItem(
            kind: kind,
            id: id,
            title: title,
            subtitle: subtitle,
            artist: artist,
            artistId: nil,
            album: album,
            albumId: nil,
            duration: duration,
            thumbnail: nil,
            videoId: videoId,
            explicit: explicit
        )
    }

    func testOnlyRowsWithAVideoIdBecomeTracks() {
        let song = item(kind: .song, id: "abc", title: "Afterglow", videoId: "abc")
        let album = item(kind: .album, id: "MPRE1", title: "Night Windows")
        XCTAssertNotNil(GoosicTrack(catalog: song))
        XCTAssertNil(GoosicTrack(catalog: album))
    }

    func testAnEmptyVideoIdIsNotTreatedAsPlayable() {
        let broken = item(kind: .song, id: "x", title: "Ghost", videoId: "")
        XCTAssertNil(GoosicTrack(catalog: broken))
    }

    func testCardActionsFollowTheRowKind() {
        XCTAssertEqual(
            GoosicCard(catalog: item(kind: .album, id: "MPRE1", title: "Night Windows")).action,
            .show(.album("MPRE1"))
        )
        XCTAssertEqual(
            GoosicCard(catalog: item(kind: .artist, id: "UC1", title: "Signal Fires")).action,
            .show(.artist("UC1"))
        )
        XCTAssertEqual(
            GoosicCard(catalog: item(kind: .playlist, id: "VLPL1", title: "Focus")).action,
            .show(.playlist("VLPL1"))
        )
    }

    func testAnUnknownKindStaysVisibleButInert() {
        let card = GoosicCard(catalog: item(kind: .unknown, id: "?", title: "Something new"))
        XCTAssertNil(card.action)
        XCTAssertEqual(card.title, "Something new")
    }

    func testASongCardCarriesItsPlayableTrack() {
        let card = GoosicCard(catalog: item(kind: .song, id: "abc", title: "Afterglow", videoId: "abc"))
        guard case .play(let track)? = card.action else {
            return XCTFail("a song card should be playable")
        }
        XCTAssertEqual(track.videoID, "abc")
    }

    func testDuplicateShelfAndCardIdsAreDisambiguated() {
        let repeated = item(kind: .album, id: "MPRE1", title: "Night Windows")
        let page = GoosicCatalogPage(
            id: "search:x",
            title: "x",
            subtitle: nil,
            shelves: [
                GoosicCatalogShelf(id: "shelf", title: "One", items: [repeated, repeated]),
                GoosicCatalogShelf(id: "shelf", title: "Two", items: [repeated]),
            ],
            tracks: nil,
            thumbnail: nil,
            truncated: nil
        )
        let view = CatalogPageView(wire: page)
        XCTAssertEqual(view.shelves.count, 2)
        XCTAssertNotEqual(view.shelves[0].id, view.shelves[1].id)
        XCTAssertEqual(view.shelves[0].cards.count, 1, "duplicate card ids collapse to the first")
    }

    func testAShelfOfSongsExposesATrackListAndAMixedShelfDoesNot() {
        let song = item(kind: .song, id: "a", title: "A", videoId: "a")
        let other = item(kind: .song, id: "b", title: "B", videoId: "b")
        let album = item(kind: .album, id: "MPRE1", title: "Album")
        let songs = GoosicShelf(id: "s", title: "Songs", cards: [song, other].map(GoosicCard.init(catalog:)))
        let mixed = GoosicShelf(id: "m", title: "Mixed", cards: [song, album].map(GoosicCard.init(catalog:)))
        XCTAssertEqual(songs.trackList?.count, 2)
        XCTAssertNil(mixed.trackList)
    }

    func testPageViewKeepsOnlyPlayableTracks() {
        let page = GoosicCatalogPage(
            id: "MPRE1",
            title: "Night Windows",
            subtitle: "Signal Fires",
            shelves: nil,
            tracks: [
                item(kind: .song, id: "a", title: "A", videoId: "a"),
                item(kind: .album, id: "MPRE2", title: "Not a track"),
            ],
            thumbnail: nil,
            truncated: true
        )
        let view = CatalogPageView(wire: page)
        XCTAssertEqual(view.tracks.count, 1)
        XCTAssertTrue(view.truncated)
        XCTAssertFalse(view.isEmpty)
    }

    func testPlayableTracksSpanTracksAndSongShelves() {
        let page = GoosicCatalogPage(
            id: "artist",
            title: "Signal Fires",
            subtitle: nil,
            shelves: [GoosicCatalogShelf(
                id: "s",
                title: "Songs",
                items: [item(kind: .song, id: "b", title: "B", videoId: "b")]
            )],
            tracks: [item(kind: .song, id: "a", title: "A", videoId: "a")],
            thumbnail: nil,
            truncated: nil
        )
        XCTAssertEqual(CatalogPageView(wire: page).playableTracks.map(\.id), ["a", "b"])
    }
}

final class TrackPresentationTests: XCTestCase {
    private func track(subtitle: String, artist: String = "", album: String = "", duration: String = "") -> GoosicTrack {
        GoosicTrack(
            id: "v",
            title: "T",
            subtitle: subtitle,
            artist: artist,
            artistID: nil,
            album: album,
            albumID: nil,
            duration: duration,
            videoID: "v",
            explicit: false
        )
    }

    func testResolvedArtistAndAlbumWinOverTheUpstreamDescriptor() {
        let row = track(subtitle: "Song • Signal Fires • Night Windows • 3:42", artist: "Signal Fires", album: "Night Windows")
        XCTAssertEqual(row.secondaryText, "Signal Fires · Night Windows")
    }

    func testTheDescriptorDropsWhatTheRowAlreadyShows() {
        // Upstream gives this shape when it resolves no artist link for the row.
        XCTAssertEqual(track(subtitle: "Song • 5:21", duration: "5:21").secondaryText, "")
        XCTAssertEqual(track(subtitle: "Video • 4:00", duration: "4:00").secondaryText, "")
    }

    func testUnrecognizedDescriptorPartsSurvive() {
        XCTAssertEqual(
            track(subtitle: "Song • 993K plays", duration: "5:21").secondaryText,
            "993K plays"
        )
    }
}

final class CatalogRoutingTests: XCTestCase {
    func testOnlyCatalogBackedRoutesResolveToABrowseRoute() {
        XCTAssertEqual(GoosicRoute.home.catalogRoute, "home")
        XCTAssertEqual(GoosicRoute.moodsAndGenres.catalogRoute, "moodsAndGenres")
        XCTAssertNil(GoosicRoute.search.catalogRoute)
        XCTAssertNil(GoosicRoute.library.catalogRoute)
        XCTAssertNil(GoosicRoute.downloads.catalogRoute)
        XCTAssertNil(GoosicRoute.settings.catalogRoute)
    }

    func testSearchFiltersMapToProtocolNames() {
        XCTAssertEqual(CatalogSearchFilter.all.protocolName, "all")
        XCTAssertEqual(CatalogSearchFilter.songs.protocolName, "songs")
        XCTAssertEqual(CatalogSearchFilter.playlists.protocolName, "playlists")
    }

    func testEntityKeysAreDistinctPerKind() {
        XCTAssertNotEqual(CatalogKey.entity(.album("x")), CatalogKey.entity(.artist("x")))
        XCTAssertNotEqual(CatalogKey.entity(.album("x")), CatalogKey.entity(.playlist("x")))
    }

    func testSearchKeysSeparateFilters() {
        XCTAssertNotEqual(
            CatalogKey.search(query: "a", filter: "all"),
            CatalogKey.search(query: "a", filter: "songs")
        )
    }

    func testFailureTextExplainsKnownServiceCodes() {
        XCTAssertEqual(catalogFailureText(code: "catalogEmpty", message: "", subject: "“x”").title, "No results")
        XCTAssertEqual(
            catalogFailureText(code: "catalogUnavailable", message: "", subject: "home").title,
            "Catalog unreachable"
        )
        XCTAssertEqual(
            catalogFailureText(code: "somethingNew", message: "boom", subject: "home").detail,
            "boom"
        )
    }
}

final class ProtocolDecodingTests: XCTestCase {
    func testACatalogPageDecodesFromTheServiceWireShape() throws {
        let wire = """
        {"protocolVersion":"0.2.0","requestId":"swift-1","ok":true,"payload":{"catalog":{\
        "id":"search:x","title":"x","shelves":[{"id":"all-songs","title":"Songs","items":[{\
        "kind":"song","id":"abc","title":"Afterglow","subtitle":"Song • 3:42",\
        "duration":"3:42","videoId":"abc","explicit":true}]}]}}}
        """
        let response = try JSONDecoder().decode(GoosicResponse.self, from: Data(wire.utf8))
        let page = try XCTUnwrap(response.payload?.catalog)
        let view = CatalogPageView(wire: page)
        XCTAssertEqual(view.shelves.first?.title, "Songs")
        let track = try XCTUnwrap(view.shelves.first?.trackList?.first)
        XCTAssertEqual(track.videoID, "abc")
        XCTAssertTrue(track.explicit)
    }

    func testAnUnknownKindFromANewerServiceDoesNotFailThePage() throws {
        let wire = """
        {"kind":"podcast","id":"p","title":"A show","subtitle":"Podcast"}
        """
        let item = try JSONDecoder().decode(GoosicCatalogItem.self, from: Data(wire.utf8))
        XCTAssertEqual(item.kind, .unknown)
    }
}

@MainActor
final class TransportFormattingTests: XCTestCase {
    func testTimeTextCoversMinutesAndHours() {
        XCTAssertEqual(GoosicAppModel.timeText(0), "0:00")
        XCTAssertEqual(GoosicAppModel.timeText(9), "0:09")
        XCTAssertEqual(GoosicAppModel.timeText(222), "3:42")
        XCTAssertEqual(GoosicAppModel.timeText(3_723), "1:02:03")
    }

    func testTimeTextRefusesToRenderNonsense() {
        XCTAssertEqual(GoosicAppModel.timeText(-1), "0:00")
        XCTAssertEqual(GoosicAppModel.timeText(.nan), "0:00")
        XCTAssertEqual(GoosicAppModel.timeText(.infinity), "0:00")
    }

    func testNothingIsSeekableBeforeTheHostReportsADuration() {
        let model = GoosicAppModel()
        XCTAssertFalse(model.isSeekable)
        XCTAssertEqual(model.durationText, "--:--")
        XCTAssertEqual(model.displayedPosition, 0)
    }

    func testNowPlayingSubtitleFallsBackWhenNothingIsPlaying() {
        XCTAssertEqual(GoosicAppModel().nowPlayingSubtitle, "Choose a track to begin")
    }
}

final class DetailSubtitleTests: XCTestCase {
    func testTheKindIsNotRepeatedWhenUpstreamAlreadyLeadsWithIt() {
        XCTAssertEqual(detailSubtitle(kindLabel: "Playlist", pageSubtitle: "Playlist • 2026"), "Playlist • 2026")
        XCTAssertEqual(detailSubtitle(kindLabel: "Album", pageSubtitle: "album • Signal Fires"), "album • Signal Fires")
    }

    func testTheKindIsAddedWhenUpstreamOmitsIt() {
        XCTAssertEqual(detailSubtitle(kindLabel: "Album", pageSubtitle: "Signal Fires"), "Album · Signal Fires")
    }

    func testAnEmptySubtitleFallsBackToTheKind() {
        XCTAssertEqual(detailSubtitle(kindLabel: "Artist", pageSubtitle: "   "), "Artist")
    }
}
