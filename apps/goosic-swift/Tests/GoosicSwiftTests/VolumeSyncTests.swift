import XCTest

@testable import GoosicSwift

/// "Volume keeps going back to 100." The official player is a web page with a volume of its own
/// and reports one on every event; the shell pushed the stored preference into it and then
/// believed the next report — which still carried the page's own value, because the push had not
/// taken effect yet.
final class VolumeSyncTests: XCTestCase {
    func testAReportIsIgnoredWhileAPushIsStillOutstanding() {
        // The preference is 0.3, the page still says 1.0. Believing it is the bug.
        XCTAssertEqual(
            VolumeSync.reconcile(reported: 1.0, current: 0.3, requested: 0.3),
            .ignore
        )
    }

    func testThePushIsDoneOnceTheRendererEchoesIt() {
        XCTAssertEqual(
            VolumeSync.reconcile(reported: 0.3, current: 0.3, requested: 0.3),
            .settled
        )
    }

    /// Floats survive a round trip through JavaScript. Exact equality would leave a push
    /// outstanding forever over a difference nobody can hear, and the shell would stop following
    /// the player entirely.
    func testAnInaudibleDifferenceCountsAsAnEcho() {
        XCTAssertEqual(
            VolumeSync.reconcile(reported: 0.3001, current: 0.3, requested: 0.3),
            .settled
        )
    }

    /// With nothing outstanding, a change really did come from the player — somebody moved the
    /// slider inside the page — so it is followed.
    func testAChangeMadeInThePlayerIsAdopted() {
        XCTAssertEqual(
            VolumeSync.reconcile(reported: 0.8, current: 0.3, requested: nil),
            .adopt(0.8)
        )
    }

    func testAReportThatMatchesWhatIsAlreadyStoredIsNotNews() {
        XCTAssertEqual(
            VolumeSync.reconcile(reported: 0.3, current: 0.3, requested: nil),
            .settled
        )
    }

    func testAnAdvertisementHandOffReappliesTheSavedPreferenceToReplacementMedia() {
        XCTAssertTrue(
            VolumeSync.shouldReapplyPreference(wasAdvertisement: true, isAdvertisement: false)
        )
        XCTAssertFalse(
            VolumeSync.shouldReapplyPreference(wasAdvertisement: false, isAdvertisement: false)
        )
        XCTAssertFalse(
            VolumeSync.shouldReapplyPreference(wasAdvertisement: true, isAdvertisement: true)
        )
    }

    func testAnUnexpectedOfficialPlayerVolumeReappliesRatherThanOverwritingThePreference() {
        XCTAssertEqual(
            VolumeSync.reconcileOfficialRenderer(reported: 1.0, preferred: 0.3, requested: nil),
            .reapply(0.3)
        )
        XCTAssertEqual(
            VolumeSync.reconcileOfficialRenderer(reported: 1.0, preferred: 0.3, requested: 0.3),
            .waitingForEcho
        )
        XCTAssertEqual(
            VolumeSync.reconcileOfficialRenderer(reported: 0.3, preferred: 0.3, requested: 0.3),
            .settled
        )
    }

    /// The value is carried out rather than assigned inside, so the caller cannot adopt a volume
    /// without deciding whether to persist it — which is the half of this bug that made the
    /// preference on disk disagree with the volume in the app.
    func testAdoptionCarriesTheValueSoItCanBeWrittenDown() {
        guard case .adopt(let value) = VolumeSync.reconcile(
            reported: 0.55, current: 1.0, requested: nil
        ) else {
            return XCTFail("expected an adoption")
        }
        XCTAssertEqual(value, 0.55, accuracy: 0.0001)
    }
}

/// A shelf of songs on Home was collapsing into a text list, because the rule that draws rows
/// asks only whether every card is playable — a test written for search, where it is right.
final class ShelfPresentationTests: XCTestCase {
    private func songShelf() -> GoosicShelf {
        let track = GoosicTrack(
            id: "v1", title: "Song", subtitle: "", artist: "A", artistID: nil,
            album: "", albumID: nil, duration: "3:00", videoID: "v1",
            explicit: false, thumbnail: nil
        )
        return GoosicShelf(
            id: "s", title: "Quick picks",
            cards: [GoosicCard(id: "v1", title: "Song", subtitle: "", action: .play(track), thumbnail: nil)]
        )
    }

    private func albumShelf() -> GoosicShelf {
        GoosicShelf(
            id: "s", title: "New releases",
            cards: [GoosicCard(id: "a1", title: "Album", subtitle: "", action: .show(.album("a1")), thumbnail: nil)]
        )
    }

    func testAnAllSongsShelfOnHomeStaysArtwork() {
        XCTAssertEqual(ShelfPresentation.preferred(for: .route(.home), shelf: songShelf()), .cards)
    }

    func testTheSameShelfInSearchResultsIsALisit() {
        guard case .rows = ShelfPresentation.preferred(
            for: .search(query: "q", filter: "songs"), shelf: songShelf()
        ) else {
            return XCTFail("search results read better as rows")
        }
    }

    func testAShelfOfAlbumsIsNeverRows() {
        XCTAssertEqual(
            ShelfPresentation.preferred(for: .search(query: "q", filter: "all"), shelf: albumShelf()),
            .cards
        )
    }

    /// The placeholder drawn before a page arrives has to match the layout that replaces it, or
    /// it becomes the first of two jumps rather than a stand-in for one.
    func testTheSkeletonMatchesTheLayoutTheKeyWillProduce() {
        XCTAssertTrue(CatalogKey.playlist("PL1").expectsTrackList)
        XCTAssertTrue(CatalogKey.album("a1").expectsTrackList)
        XCTAssertFalse(CatalogKey.route(.home).expectsTrackList)
        XCTAssertFalse(CatalogKey.library("Playlists").expectsTrackList)
        XCTAssertFalse(CatalogKey.artist("UC1").expectsTrackList)
    }
}
