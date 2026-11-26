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

    private static func swap(_ page: String, title: String, mayAdopt: Bool = true) -> OfficialPageSwap {
        OfficialPageSwap(requestedVideoID: "requested01", pageVideoID: page, pageTitle: title, mayAdopt: mayAdopt)
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

    func testAnAdoptedVersionIsNotSwappedAgain() {
        XCTAssertEqual(
            PageSwap.decide(Self.swap("thirdone001", title: "Hold On, We're Going Home", mayAdopt: false),
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

