import XCTest

@testable import GoosicSwift

/// The same cases as the Windows shell's `SameSongTests`, so the two copies cannot drift.
final class PageSwapTests: XCTestCase {
    func testAnotherVersionOfTheSongMatches() {
        let pairs = [
            ("Hideaway", "Hideaway"),
            ("Hideaway", "Bella Poarch - Hideaway (Official Video)"),
            ("Swim", "SWIM [Official Audio]"),
            ("I Adore You (feat. Daecolm)", "I Adore You"),
            ("Don't Stop Me Now", "Dont Stop Me Now (Remastered 2011)"),
            ("Hold On, We\u{2019}re Going Home", "Hold On, We're Going Home (feat. Majid Jordan)"),
        ]
        for (requested, page) in pairs {
            XCTAssertTrue(PageSwap.isSameSong(requested: requested, page: page), "\(requested) / \(page)")
        }
    }

    func testADifferentSongDoesNot() {
        let pairs: [(String?, String?)] = [
            ("Down", "Downtown"),
            ("Hideaway", "Ribcage"),
            ("Hideaway", ""),
            ("", "Hideaway"),
            ("(Intro)", "Anything"),
            ("Hold On, We're Going Home", "Moment 4 Life (feat. Drake)"),
            (nil, "Hideaway"),
        ]
        for (requested, page) in pairs {
            XCTAssertFalse(PageSwap.isSameSong(requested: requested, page: page), "\(requested ?? "nil") / \(page ?? "nil")")
        }
    }

    func testOnlyTheRequestedTrackPlayingCountsAsHeard() {
        XCTAssertTrue(PageSwap.heardPlaying(state: "playing", duration: 230, isAdvertisement: false))
        XCTAssertFalse(PageSwap.heardPlaying(state: "playing", duration: 0, isAdvertisement: false))
        XCTAssertFalse(PageSwap.heardPlaying(state: "playing", duration: .nan, isAdvertisement: false))
        XCTAssertFalse(PageSwap.heardPlaying(state: "paused", duration: 230, isAdvertisement: false))
        XCTAssertFalse(PageSwap.heardPlaying(state: "playing", duration: 15, isAdvertisement: true))
    }

    private static func swap(_ page: String, title: String, adoptions: Int = 0) -> OfficialPageSwap {
        OfficialPageSwap(requestedVideoID: "requested01", pageVideoID: page, pageTitle: title, adoptions: adoptions)
    }

    func testTheSameSongUnderAnotherIDIsAdopted() {
        XCTAssertEqual(
            PageSwap.decide(Self.swap("musicvideo1", title: "Hold On, We're Going Home (feat. Majid Jordan)"),
                            loadedVideoID: "requested01", currentTitle: "Hold On, We\u{2019}re Going Home"),
            .adopt(videoID: "musicvideo1"))
    }

    func testAnotherSongIsARefusalNotTheNextRow() {
        XCTAssertEqual(
            PageSwap.decide(Self.swap("othersong01", title: "Moment 4 Life (feat. Drake)"),
                            loadedVideoID: "requested01", currentTitle: "Hold On, We're Going Home"),
            .refused)
        // An unreadable title cannot prove it is the same song.
        XCTAssertEqual(
            PageSwap.decide(Self.swap("othersong01", title: ""),
                            loadedVideoID: "requested01", currentTitle: "Hold On, We're Going Home"),
            .refused)
    }

    func testAnAdoptedVersionMayBeSwappedBackOnce() {
        // The page swapped the song for its video, then the video back for the song.
        XCTAssertEqual(
            PageSwap.decide(Self.swap("songversion", title: "Hold On, We're Going Home", adoptions: 1),
                            loadedVideoID: "requested01", currentTitle: "Hold On, We're Going Home"),
            .adopt(videoID: "songversion"))
    }

    func testAPageGoingRoundInCirclesIsARefusal() {
        XCTAssertEqual(
            PageSwap.decide(Self.swap("thirdone001", title: "Hold On, We're Going Home",
                                      adoptions: PageSwap.maxAdoptions),
                            loadedVideoID: "requested01", currentTitle: "Hold On, We're Going Home"),
            .refused)
    }

    func testAReportForAnEarlierLoadIsIgnored() {
        XCTAssertEqual(
            PageSwap.decide(Self.swap("othersong01", title: "Anything"),
                            loadedVideoID: "nextsong001", currentTitle: "Next song"),
            .ignore)
    }
}

/// Refused songs are skipped where the Windows shell skips them: when a list starts and when
/// radio adds recommendations. The chosen row always stays.
final class RefusedTrackTests: XCTestCase {
    private static func track(_ id: String) -> GoosicTrack {
        GoosicTrack(id: id, title: id, subtitle: "", artist: "", artistID: nil, album: "",
                    albumID: nil, duration: "3:00", videoID: id, explicit: false, thumbnail: nil)
    }

    func testAStartingListLeavesOutRefusedSongsButKeepsTheChosenOne() async {
        let list = ["songa000001", "refused0001", "songb000001", "refused0002"].map(Self.track)
        let refused: Set<String> = ["refused0001", "refused0002"]
        await MainActor.run {
            XCTAssertEqual(
                GoosicAppModel.startableTracks(list, chosen: list[0], refused: refused).map(\.videoID),
                ["songa000001", "songb000001"])
            XCTAssertEqual(
                GoosicAppModel.startableTracks(list, chosen: list[1], refused: refused).map(\.videoID),
                ["songa000001", "refused0001", "songb000001"])
            XCTAssertEqual(GoosicAppModel.startableTracks(list, chosen: list[0], refused: []), list)
        }
    }

    func testRadioNeverAddsARefusedSong() async {
        let candidates = ["refused0001", "songa000001"].map(Self.track)
        await MainActor.run {
            let fresh = GoosicAppModel.freshRadioTracks(candidates, excluding: [], refused: ["refused0001"])
            XCTAssertEqual(fresh.map(\.videoID), ["songa000001"])
        }
    }
}
