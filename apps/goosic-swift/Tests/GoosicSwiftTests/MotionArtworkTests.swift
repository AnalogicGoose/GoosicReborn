#if os(macOS)
import XCTest
@testable import GoosicSwift

final class MotionArtworkTests: XCTestCase {
    private static let base = URL(string: "https://mvod.itunes.apple.com/example/master.m3u8")!
    private static var candidate: [String: Any] { ["trackName": "Example", "artistName": "Artist", "collectionName": "Album", "collectionId": 123, "trackTimeMillis": 200000] }

    func testLiveAppleArtworkWhenExplicitlyEnabled() async throws {
        guard ProcessInfo.processInfo.environment["GOOSIC_TEST_MOTION_LIVE"] == "1" else {
            throw XCTSkip("Opt-in Apple network check")
        }
        let file = try await ExperimentalMotionResolver.shared.resolve(
            MotionArtworkQuery(title: "Blinding Lights", artist: "The Weeknd", album: "After Hours (Deluxe)", seconds: 200))
        let url = try XCTUnwrap(file, "Expected a known album to resolve an animated cover")
        defer { try? FileManager.default.removeItem(at: url) }
        XCTAssertGreaterThan(try Data(contentsOf: url).count, 0)
    }

    func testTransportPolicyMatchesRustCases() {
        for raw in ["https://itunes.apple.com/search", "https://music.apple.com/us/album/123", "https://mvod.itunes.apple.com/cover.mp4"] {
            XCTAssertTrue(MotionArtworkPolicy.allows(URL(string: raw)!))
        }
        for raw in ["http://music.apple.com/x", "https://music.apple.com.evil/x", "https://user@music.apple.com/x", "https://music.apple.com:444/x", "file:///tmp/x"] {
            XCTAssertFalse(MotionArtworkPolicy.allows(URL(string: raw)!))
        }
        XCTAssertTrue(MotionArtworkPolicy.durationMatches(200, 205))
        for duration in [0, 206, Double.nan, Double.infinity] {
            XCTAssertFalse(MotionArtworkPolicy.durationMatches(200, duration))
        }
    }

    func testMatchingRejectsOtherEditionsAndAmbiguousAlbums() {
        let query = MotionArtworkQuery(title: "Example", artist: "Artist", album: "Album", seconds: 200)
        XCTAssertEqual(MotionArtworkParsing.candidates([Self.candidate], query: query).count, 1)
        for key in ["artistName", "trackName", "collectionName"] {
            var other = Self.candidate; other[key] = "Different"
            XCTAssertTrue(MotionArtworkParsing.candidates([other], query: query).isEmpty)
        }
        var remix = Self.candidate; remix["trackTimeMillis"] = 230000
        XCTAssertTrue(MotionArtworkParsing.candidates([remix], query: query).isEmpty)
        var compilation = Self.candidate; compilation["collectionName"] = "Compilation"; compilation["collectionId"] = 456
        let noAlbum = MotionArtworkQuery(title: "Example", artist: "Artist", album: "", seconds: 200)
        XCTAssertTrue(MotionArtworkParsing.candidates([Self.candidate, compilation], query: noAlbum).isEmpty)
    }

    func testPageRequiresCorrectAlbumHeaderAndAllowedVideoHost() throws {
        let item: [String: Any] = ["id": "album-detail-header - 123", "title": "Album", "subtitleLinks": [["title": "Artist"]],
            "videoArtwork": ["dictionary": ["motionDetailSquare": ["video": Self.base.absoluteString]]]]
        let payload: [String: Any] = ["data": [["data": ["sections": [["items": [item]]]]]]]
        let data = try JSONSerialization.data(withJSONObject: payload)
        let page = "<script id=\"serialized-server-data\">\(String(decoding: data, as: UTF8.self))</script>"
        XCTAssertEqual(MotionArtworkParsing.motionURL(page: page, candidate: Self.candidate), Self.base)
        XCTAssertNil(MotionArtworkParsing.motionURL(page: page.replacingOccurrences(of: "Artist", with: "Other"), candidate: Self.candidate))
        XCTAssertNil(MotionArtworkParsing.motionURL(page: page.replacingOccurrences(of: "mvod.itunes.apple.com", with: "evil.example"), candidate: Self.candidate))
        XCTAssertNil(MotionArtworkParsing.motionURL(page: "<script>bad</script>", candidate: Self.candidate))
    }

    func testManifestRejectsEncryptedUnboundedAndExternalResources() {
        let good = "#EXTM3U\n#EXT-X-MAP:URI=\"cover.mp4\",BYTERANGE=\"100@0\"\n#EXTINF:10,\n#EXT-X-BYTERANGE:200@100\ncover.mp4\n#EXT-X-ENDLIST"
        XCTAssertEqual(MotionArtworkParsing.movie(good, base: Self.base)?.lastPathComponent, "cover.mp4")
        for bad in [good.replacingOccurrences(of: "#EXT-X-ENDLIST", with: ""),
                    good + "\n#EXT-X-KEY:METHOD=AES-128,URI=\"key\"",
                    good.replacingOccurrences(of: "#EXTINF:10,", with: "#EXTINF:61,"),
                    good.replacingOccurrences(of: "cover.mp4", with: "https://evil.example/cover.mp4"),
                    good + "\nother.mp4", good.replacingOccurrences(of: "#EXTINF:10,", with: "#EXTINF:")] {
            XCTAssertNil(MotionArtworkParsing.movie(bad, base: Self.base))
        }
    }

    func testVariantsRejectAudioCodecsHDRLargeAndExternalVideos() {
        let good = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=2000000,CODECS=\"avc1.64001F\",RESOLUTION=512x512\ncover.m3u8\n"
        XCTAssertEqual(MotionArtworkParsing.variants(good, base: Self.base).count, 1)
        for bad in [good.replacingOccurrences(of: "avc1", with: "mp4a"),
                    good.replacingOccurrences(of: "512x512", with: "2160x2160"),
                    good.replacingOccurrences(of: "RESOLUTION=", with: "VIDEO-RANGE=PQ,RESOLUTION="),
                    good.replacingOccurrences(of: "cover.m3u8", with: "https://evil.example/cover.m3u8")] {
            XCTAssertTrue(MotionArtworkParsing.variants(bad, base: Self.base).isEmpty)
        }
    }
}
#endif
