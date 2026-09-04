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
