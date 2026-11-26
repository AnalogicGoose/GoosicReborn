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

/// Isolation is applied per test rather than to the class: swift-corelibs-xctest
/// discovers tests by casting the method to `(Self) -> () throws -> Void`, and a
/// `@MainActor`-isolated method does not carry that type, so an isolated class aborts
/// the whole run on Linux before any test executes.
final class ArtworkCacheBehaviourTests: XCTestCase {
    @MainActor
    private static func makeCache() -> (ArtworkCache, URL) {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("goosic-artwork-tests-\(UUID().uuidString)")
        return (ArtworkCache(directory: directory), directory)
    }

    func testAnEmptyOrMissingURLNeverProducesAFile() async {
        await MainActor.run {
            let (cache, directory) = Self.makeCache()
            defer { try? FileManager.default.removeItem(at: directory) }
            XCTAssertNil(cache.localFile(for: nil))
            XCTAssertNil(cache.localFile(for: ""))
        }
    }

    func testARefusedHostIsNotRetriedAndNeverResolves() async {
        await MainActor.run {
            let (cache, directory) = Self.makeCache()
            defer { try? FileManager.default.removeItem(at: directory) }
            XCTAssertNil(cache.localFile(for: "https://example.test/art.jpg"))
            XCTAssertNil(cache.localFile(for: "https://example.test/art.jpg"))
        }
    }

    func testAnAlreadyCachedFileIsReturnedWithoutAFetch() async throws {
        try await MainActor.run {
            // The cache indexes its directory once, when it opens, so that view layout never
            // touches the filesystem. A file already on disk is therefore written *before* the
            // cache is constructed — which is also the real sequence, since the previous run's
            // artwork is there before this run starts.
            let directory = FileManager.default.temporaryDirectory
                .appendingPathComponent("goosic-artwork-tests-\(UUID().uuidString)")
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            defer { try? FileManager.default.removeItem(at: directory) }
            let remote = "https://yt3.googleusercontent.com/already-there"
            let expected = directory.appendingPathComponent("\(ArtworkCache.cacheKey(for: remote)).img")
            try Data("pretend image".utf8).write(to: expected)
            let cache = ArtworkCache(directory: directory)

            XCTAssertEqual(cache.localFile(for: remote), expected)
        }
    }
}

final class RadioPageTests: XCTestCase {
    /// A radio page is tracks-only, unlike the shelf pages the browse routes return.
    func testARadioPageDecodesIntoPlayableTracksWithArtwork() throws {
        let wire = """
        {"protocolVersion":"0.5.0","requestId":"swift-1","ok":true,"payload":{"catalog":{\
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

    /// Same case as `an_artist_header_survives_the_page_view_and_its_continuation` in Rust.
    func testAnArtistHeaderSurvivesThePageViewAndItsContinuation() throws {
        let wire = """
        {"protocolVersion":"0.5.0","requestId":"swift-1","ok":true,"payload":{"catalog":{\
        "id":"UCartist","title":"Omar Courtz","artist":{\
        "banner":"https://lh3.googleusercontent.com/banner=w2880-h1200",\
        "audience":"28.9M monthly audience"}}}}
        """
        let response = try JSONDecoder().decode(GoosicResponse.self, from: Data(wire.utf8))
        let page = CatalogPageView(wire: try XCTUnwrap(response.payload?.catalog))
        let header = GoosicArtistHeader(
            banner: "https://lh3.googleusercontent.com/banner=w2880-h1200",
            audience: "28.9M monthly audience"
        )
        XCTAssertEqual(page.artist, header)
        let more = CatalogPageView(
            id: "continuation", title: "", subtitle: "", shelves: [], tracks: [],
            nextCursor: nil, allTracksID: nil, truncated: false
        )
        XCTAssertEqual(page.appending(more).artist, header)
    }

    func testARadioPageThatCameBackEmptyIsTreatedAsNothingToPlay() throws {
        let wire = """
        {"protocolVersion":"0.5.0","requestId":"swift-1","ok":true,\
        "payload":{"catalog":{"id":"radio:x","title":"Radio"}}}
        """
        let response = try JSONDecoder().decode(GoosicResponse.self, from: Data(wire.utf8))
        let page = CatalogPageView(wire: try XCTUnwrap(response.payload?.catalog))
        XCTAssertTrue(page.isEmpty)
        XCTAssertTrue(page.playableTracks.isEmpty)
    }
}

final class RepeatModeTests: XCTestCase {
    func testEveryModeRoundTripsThroughItsWireValue() {
        for mode in RepeatMode.allCases {
            XCTAssertEqual(RepeatMode(rawValue: mode.rawValue), mode)
        }
    }
}

/// Isolation is applied per test rather than to the class: swift-corelibs-xctest
/// discovers tests by casting the method to `(Self) -> () throws -> Void`, and a
/// `@MainActor`-isolated method does not carry that type, so an isolated class aborts
/// the whole run on Linux before any test executes.
final class LyricsTests: XCTestCase {
    func testDisplayDurationsBecomeSeconds() async {
        await MainActor.run {
            XCTAssertEqual(GoosicAppModel.durationSeconds("3:42"), 222)
            XCTAssertEqual(GoosicAppModel.durationSeconds("0:09"), 9)
            XCTAssertEqual(GoosicAppModel.durationSeconds("1:02:03"), 3_723)
        }
    }

    func testLookupPrefersTheMeasuredRecordingLength() async {
        await MainActor.run {
            XCTAssertEqual(GoosicAppModel.lyricsLookupDuration(measured: 251.6, catalog: "3:42"), 252)
            XCTAssertEqual(GoosicAppModel.lyricsLookupDuration(measured: 251.4, catalog: "3:42"), 251)
        }
    }

    func testLookupFallsBackToTheCatalogWhenNothingUsableWasMeasured() async {
        await MainActor.run {
            XCTAssertEqual(GoosicAppModel.lyricsLookupDuration(measured: nil, catalog: "3:42"), 222)
            XCTAssertEqual(GoosicAppModel.lyricsLookupDuration(measured: 0, catalog: "3:42"), 222)
            XCTAssertEqual(GoosicAppModel.lyricsLookupDuration(measured: .nan, catalog: "3:42"), 222)
            XCTAssertEqual(GoosicAppModel.lyricsLookupDuration(measured: .infinity, catalog: "3:42"), 222)
            XCTAssertNil(GoosicAppModel.lyricsLookupDuration(measured: nil, catalog: "bad"))
        }
    }

    func testAMalformedDurationIsRefusedRatherThanGuessed() async {
        await MainActor.run {
            // A wrong length would narrow the lyrics lookup to the wrong recording.
            XCTAssertNil(GoosicAppModel.durationSeconds(""))
            XCTAssertNil(GoosicAppModel.durationSeconds("222"))
            XCTAssertNil(GoosicAppModel.durationSeconds("a:b"))
            XCTAssertNil(GoosicAppModel.durationSeconds("1:2:3:4"))
        }
    }

    func testLyricsDecodeFromTheServiceWireShape() async throws {
        try await MainActor.run {
            let wire = """
            {"protocolVersion":"0.5.0","requestId":"swift-1","ok":true,"payload":{"lyrics":{\
            "source":"LRCLIB","synced":true,"lines":[\
            {"atMs":19160,"text":"When you were here before"},\
            {"atMs":24090,"text":"Couldn't look you in the eye"}]}}}
            """
            let response = try JSONDecoder().decode(GoosicResponse.self, from: Data(wire.utf8))
            let lyrics = try XCTUnwrap(response.payload?.lyrics)
            XCTAssertTrue(lyrics.synced)
            XCTAssertEqual(lyrics.source, "LRCLIB")
            XCTAssertEqual(lyrics.lines.count, 2)
            XCTAssertEqual(lyrics.lines[0].atMs, 19_160)
        }
    }

    func testRepeatedLyricLinesStillGetDistinctIdentities() async {
        await MainActor.run {
            // A chorus repeats its text; `ForEach` needs the ids to differ anyway.
            let first = GoosicLyricsLine(atMs: 1_000, text: "Chorus")
            let second = GoosicLyricsLine(atMs: 60_000, text: "Chorus")
            XCTAssertNotEqual(first.id, second.id)
        }
    }

    func testNothingIsHighlightedWithoutLyrics() async {
        await MainActor.run {
            XCTAssertNil(GoosicAppModel().activeLyricIndex)
        }
    }

    private static let timed = [
        GoosicLyricsLine(atMs: 0, text: "Zero"),
        GoosicLyricsLine(atMs: 10_000, text: "Ten"),
        GoosicLyricsLine(atMs: 20_000, text: "Twenty"),
    ]

    func testTheHighlightFollowsThePosition() async {
        await MainActor.run {
            // A local function does not inherit the closure's isolation.
            @MainActor func active(_ ms: Int64) -> Int? {
                GoosicAppModel.activeLyricIndex(lines: Self.timed, synced: true, positionMs: ms)
            }
            XCTAssertEqual(active(0), 0)
            XCTAssertEqual(active(9_999), 0)
            XCTAssertEqual(active(10_000), 1)
            XCTAssertEqual(active(25_000), 2)
        }
    }

    func testNothingIsHighlightedBeforeTheFirstLine() async {
        await MainActor.run {
            let late = [GoosicLyricsLine(atMs: 5_000, text: "Starts late")]
            XCTAssertNil(GoosicAppModel.activeLyricIndex(lines: late, synced: true, positionMs: 0))
            XCTAssertNil(GoosicAppModel.activeLyricIndex(lines: late, synced: true, positionMs: 4_999))
            XCTAssertEqual(
                GoosicAppModel.activeLyricIndex(lines: late, synced: true, positionMs: 5_000),
                0
            )
        }
    }

    func testUnsyncedLyricsNeverHighlight() async {
        await MainActor.run {
            let plain = [GoosicLyricsLine(atMs: -1, text: "Just words")]
            XCTAssertNil(GoosicAppModel.activeLyricIndex(lines: plain, synced: false, positionMs: 9_999))
        }
    }

    func testAnEmptyDocumentNeverHighlights() async {
        await MainActor.run {
            XCTAssertNil(GoosicAppModel.activeLyricIndex(lines: [], synced: true, positionMs: 1_000))
        }
    }
}

final class ThemeTests: XCTestCase {
    func testStoredValuesDecodeToTheirTheme() {
        XCTAssertEqual(GoosicTheme.named("system"), .system)
        XCTAssertEqual(GoosicTheme.named("light"), .light)
        XCTAssertEqual(GoosicTheme.named("dark"), .dark)
    }

    func testAStoredValueIsNormalizedBeforeItIsMatched() {
        // The value can come from a hand-edited file or a previous Goosic install.
        XCTAssertEqual(GoosicTheme.named("  Dark  "), .dark)
        XCTAssertEqual(GoosicTheme.named("LIGHT"), .light)
    }

    func testAnUnrecognizedValueFallsBackToFollowingTheSystem() {
        XCTAssertEqual(GoosicTheme.named("neon"), .system)
        XCTAssertEqual(GoosicTheme.named(""), .system)
    }

    func testEveryThemeRoundTripsThroughItsWireValue() {
        for theme in GoosicTheme.allCases {
            XCTAssertEqual(GoosicTheme.named(theme.rawValue), theme)
        }
    }

    func testSystemMeansNoPreferenceSoTheOSKeepsDeciding() {
        // A nil colour scheme is SwiftUI's "follow the system"; the app does not
        // reimplement light and dark.
        XCTAssertNil(GoosicTheme.system.colorScheme)
        XCTAssertNotNil(GoosicTheme.light.colorScheme)
        XCTAssertNotNil(GoosicTheme.dark.colorScheme)
    }
}
