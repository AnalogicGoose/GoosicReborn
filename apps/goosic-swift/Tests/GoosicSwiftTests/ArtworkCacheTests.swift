import XCTest

@testable import GoosicSwift

final class ArtworkHostPolicyTests: XCTestCase {
    private func allowed(_ string: String) -> Bool {
        guard let url = URL(string: string) else { return false }
        return ArtworkCache.isAllowed(url)
    }

    func testArtworkHostsYouTubeMusicActuallyServesAreAllowed() {
        XCTAssertTrue(allowed("https://yt3.googleusercontent.com/abc"))
        XCTAssertTrue(allowed("https://lh3.googleusercontent.com/abc"))
        XCTAssertTrue(allowed("https://i.ytimg.com/vi/abc/hq.jpg"))
        XCTAssertTrue(allowed("https://yt3.ggpht.com/abc"))
    }

    func testPlaintextIsRefusedSoArtworkIsNeverFetchedInTheClear() {
        XCTAssertFalse(allowed("http://yt3.googleusercontent.com/abc"))
    }

    func testAnUnrelatedHostIsRefusedRatherThanFetched() {
        // A catalog response must not be able to point the shell at an arbitrary server.
        XCTAssertFalse(allowed("https://example.test/art.jpg"))
        XCTAssertFalse(allowed("https://localhost/art.jpg"))
        XCTAssertFalse(allowed("https://169.254.169.254/latest/meta-data"))
    }

    func testSuffixLookalikesDoNotSlipThrough() {
        // The check is on a label boundary, not a bare string suffix.
        XCTAssertFalse(allowed("https://evilgoogleusercontent.com/art.jpg"))
        XCTAssertFalse(allowed("https://notytimg.com/art.jpg"))
        XCTAssertFalse(allowed("https://googleusercontent.com.evil.test/art.jpg"))
    }

    func testTheBareAllowedDomainItselfIsAccepted() {
        XCTAssertTrue(allowed("https://googleusercontent.com/art.jpg"))
    }

    func testAFileURLIsNotTreatedAsRemoteArtwork() {
        XCTAssertFalse(allowed("file:///etc/passwd"))
    }
}

final class ArtworkCacheKeyTests: XCTestCase {
    func testTheKeyIsStableForTheSameURL() {
        let url = "https://yt3.googleusercontent.com/abc"
        XCTAssertEqual(ArtworkCache.cacheKey(for: url), ArtworkCache.cacheKey(for: url))
    }

    func testDifferentURLsGetDifferentKeys() {
        XCTAssertNotEqual(
            ArtworkCache.cacheKey(for: "https://yt3.googleusercontent.com/a"),
            ArtworkCache.cacheKey(for: "https://yt3.googleusercontent.com/b")
        )
    }

    func testNearIdenticalURLsDoNotCollide() {
        // Two passes over the bytes, one reversed, so a transposition changes the key.
        XCTAssertNotEqual(
            ArtworkCache.cacheKey(for: "https://yt3.googleusercontent.com/ab"),
            ArtworkCache.cacheKey(for: "https://yt3.googleusercontent.com/ba")
        )
    }

    func testTheKeyIsAFixedLengthFilenameSafeString() {
        let key = ArtworkCache.cacheKey(for: "https://yt3.googleusercontent.com/a?x=1&y=/../..")
        XCTAssertEqual(key.count, 32)
        XCTAssertTrue(key.allSatisfy { $0.isHexDigit })
        // A cache path built from this can never escape its directory.
        XCTAssertFalse(key.contains("/"))
        XCTAssertFalse(key.contains("."))
    }
}

@MainActor
final class ArtworkCacheBehaviourTests: XCTestCase {
    private func makeCache() -> (ArtworkCache, URL) {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("goosic-artwork-tests-\(UUID().uuidString)")
        return (ArtworkCache(directory: directory), directory)
    }

    func testAnEmptyOrMissingURLNeverProducesAFile() {
        let (cache, directory) = makeCache()
        defer { try? FileManager.default.removeItem(at: directory) }
        XCTAssertNil(cache.localFile(for: nil))
        XCTAssertNil(cache.localFile(for: ""))
    }

    func testARefusedHostIsNotRetriedAndNeverResolves() {
        let (cache, directory) = makeCache()
        defer { try? FileManager.default.removeItem(at: directory) }
        XCTAssertNil(cache.localFile(for: "https://example.test/art.jpg"))
        XCTAssertNil(cache.localFile(for: "https://example.test/art.jpg"))
    }

    func testAnAlreadyCachedFileIsReturnedWithoutAFetch() throws {
        let (cache, directory) = makeCache()
        defer { try? FileManager.default.removeItem(at: directory) }
        let remote = "https://yt3.googleusercontent.com/already-there"
        let expected = directory.appendingPathComponent("\(ArtworkCache.cacheKey(for: remote)).img")
        try Data("pretend image".utf8).write(to: expected)

        XCTAssertEqual(cache.localFile(for: remote), expected)
    }
}

final class RadioPageTests: XCTestCase {
    /// A radio page is tracks-only, unlike the shelf pages the browse routes return.
    func testARadioPageDecodesIntoPlayableTracksWithArtwork() throws {
        let wire = """
        {"protocolVersion":"0.3.0","requestId":"swift-1","ok":true,"payload":{"catalog":{\
        "id":"radio:JhulBGMA7G4","title":"Radio","tracks":[\
        {"kind":"song","id":"qXI87eMP-bs","title":"Face to Face","subtitle":"Daft Punk",\
        "artist":"Daft Punk","duration":"4:01","videoId":"qXI87eMP-bs",\
        "thumbnail":"https://yt3.googleusercontent.com/a"},\
        {"kind":"song","id":"mllzzUjMezU","title":"Shooting Stars","subtitle":"Bag Raiders",\
        "artist":"Bag Raiders","duration":"3:56","videoId":"mllzzUjMezU"}]}}}
        """
        let response = try JSONDecoder().decode(GoosicResponse.self, from: Data(wire.utf8))
        let page = CatalogPageView(wire: try XCTUnwrap(response.payload?.catalog))

        XCTAssertEqual(page.id, "radio:JhulBGMA7G4")
        XCTAssertTrue(page.shelves.isEmpty)
        XCTAssertEqual(page.tracks.count, 2)
        XCTAssertEqual(page.playableTracks.map(\.videoID), ["qXI87eMP-bs", "mllzzUjMezU"])
        XCTAssertEqual(page.tracks[0].thumbnail, "https://yt3.googleusercontent.com/a")
        XCTAssertNil(page.tracks[1].thumbnail, "artwork is optional on a queue row")
    }

    func testARadioPageThatCameBackEmptyIsTreatedAsNothingToPlay() throws {
        let wire = """
        {"protocolVersion":"0.3.0","requestId":"swift-1","ok":true,\
        "payload":{"catalog":{"id":"radio:x","title":"Radio"}}}
        """
        let response = try JSONDecoder().decode(GoosicResponse.self, from: Data(wire.utf8))
        let page = CatalogPageView(wire: try XCTUnwrap(response.payload?.catalog))
        XCTAssertTrue(page.isEmpty)
        XCTAssertTrue(page.playableTracks.isEmpty)
    }
}

final class RepeatModeTests: XCTestCase {
    func testTheCycleVisitsEveryModeAndReturns() {
        XCTAssertEqual(RepeatMode.off.next, .all)
        XCTAssertEqual(RepeatMode.all.next, .one)
        XCTAssertEqual(RepeatMode.one.next, .off)
    }

    func testEveryModeRoundTripsThroughItsWireValue() {
        for mode in RepeatMode.allCases {
            XCTAssertEqual(RepeatMode(rawValue: mode.rawValue), mode)
        }
    }
}

@MainActor
final class QueueAdvanceTests: XCTestCase {
    private func model(tracks: Int) -> GoosicAppModel {
        let model = GoosicAppModel()
        model.queue = GoosicQueue(
            tracks: (0..<tracks).map { index in
                GoosicTrack(
                    id: "v\(index)",
                    title: "Track \(index)",
                    subtitle: "",
                    artist: "",
                    artistID: nil,
                    album: "",
                    albumID: nil,
                    duration: "",
                    videoID: "v\(index)",
                    explicit: false,
                    thumbnail: nil
                )
            },
            currentIndex: 0
        )
        return model
    }

    func testAnEmptyQueueHasNowhereToGo() {
        XCTAssertNil(model(tracks: 0).indexAfter(0, wrapping: true))
    }

    func testInOrderPlaybackWalksForward() {
        let model = model(tracks: 3)
        XCTAssertEqual(model.indexAfter(0, wrapping: false), 1)
        XCTAssertEqual(model.indexAfter(1, wrapping: false), 2)
    }

    func testTheEndOfAQueueStopsSoRadioCanTakeOver() {
        // `wrapping: false` is the natural end of a track.
        XCTAssertNil(model(tracks: 3).indexAfter(2, wrapping: false))
    }

    func testPressingNextAtTheEndWrapsEvenWithRepeatOff() {
        // A deliberate Next should move rather than do nothing.
        XCTAssertEqual(model(tracks: 3).indexAfter(2, wrapping: true), 0)
    }

    func testRepeatAllWrapsAtTheNaturalEnd() {
        let model = model(tracks: 3)
        model.cycleRepeatMode()
        XCTAssertEqual(model.repeatMode, .all)
        XCTAssertEqual(model.indexAfter(2, wrapping: false), 0)
    }

    func testRepeatOneStaysOnTheSameTrack() {
        let model = model(tracks: 3)
        model.cycleRepeatMode()
        model.cycleRepeatMode()
        XCTAssertEqual(model.repeatMode, .one)
        XCTAssertEqual(model.indexAfter(1, wrapping: false), 1)
        XCTAssertEqual(model.indexAfter(1, wrapping: true), 1)
    }

    func testShuffleNeverPicksTheTrackItIsAlreadyOn() {
        let model = model(tracks: 4)
        model.toggleShuffle()
        XCTAssertTrue(model.shuffle)
        for _ in 0..<200 {
            XCTAssertNotEqual(model.indexAfter(2, wrapping: false), 2)
        }
    }

    func testShuffleOverASingleTrackStopsAtTheEndRatherThanLooping() {
        let model = model(tracks: 1)
        model.toggleShuffle()
        XCTAssertNil(model.indexAfter(0, wrapping: false))
        XCTAssertEqual(model.indexAfter(0, wrapping: true), 0)
    }
}
