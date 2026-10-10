import XCTest
@testable import GoosicSwift

final class LyricsTimingTests: XCTestCase {
    private static var document: GoosicLyrics {
        GoosicLyrics(source: "test", synced: true, lines: [
            GoosicLyricsLine(words: [
                GoosicLyricsWord(atMs: 1000, endMs: 2000, text: "Hi "),
                GoosicLyricsWord(atMs: 2000, endMs: 4000, text: "all")
            ], atMs: 1000, text: "Hi all"),
            GoosicLyricsLine(atMs: 5000, text: "Next")
        ])
    }

    func testImportedTimingMatchesTextAndCannotOverlap() {
        var doc = Self.document
        XCTAssertTrue(LyricsTiming.valid(doc))
        doc.lines[0].words![1].atMs = 1500
        XCTAssertFalse(LyricsTiming.valid(doc))
        doc = Self.document; doc.lines[0].words![1].text = "wrong"
        XCTAssertFalse(LyricsTiming.valid(doc))
        doc = Self.document; doc.lines[0].words![1].endMs = 6000
        XCTAssertFalse(LyricsTiming.valid(doc))
    }

    func testOnlySuppliedWordTimingCanHighlightWords() {
        XCTAssertTrue(LyricsTiming.segments(GoosicLyricsLine(atMs: 1000, text: "Hi all")).isEmpty)
        XCTAssertEqual(LyricsTiming.segments(Self.document.lines[0]), Self.document.lines[0].words)
    }

    func testRealWordProgressAndStartOnlyTiming() {
        var word = GoosicLyricsWord(atMs: 1000, endMs: 2000, text: "Hi")
        XCTAssertEqual(LyricsTiming.progress(word, positionMs: 500), 0)
        XCTAssertEqual(LyricsTiming.progress(word, positionMs: 1500), 0.5)
        XCTAssertEqual(LyricsTiming.progress(word, positionMs: 2500), 1)
        XCTAssertEqual(LyricsTiming.progress(word, positionMs: .nan), 0)
        word.endMs = nil
        XCTAssertEqual(LyricsTiming.progress(word, positionMs: 999), 0)
        XCTAssertEqual(LyricsTiming.progress(word, positionMs: 1000), 1)
        var doc = Self.document
        doc.lines[0].words![0].endMs = nil
        XCTAssertTrue(LyricsTiming.valid(doc))
    }

    func testLRCLIBWordWireDecodesWithOptionalEnds() throws {
        let wire = Data(#"{"source":"LRCLIB","synced":true,"lines":[{"atMs":1000,"text":"Hi all","words":[{"atMs":1000,"text":"Hi "},{"atMs":2000,"endMs":3000,"text":"all"}]}],"truncated":false}"#.utf8)
        let doc = try JSONDecoder().decode(GoosicLyrics.self, from: wire)
        XCTAssertEqual(doc.lines[0].words?.count, 2)
        XCTAssertNil(doc.lines[0].words?[0].endMs)
        XCTAssertEqual(doc.lines[0].words?[1].endMs, 3000)
        XCTAssertTrue(LyricsTiming.valid(doc))
    }

    func testOldLineOnlyDocumentsRemainDecodable() throws {
        let json = Data(#"{"source":"old","synced":true,"lines":[{"atMs":1000,"text":"Hi"}]}"#.utf8)
        let doc = try JSONDecoder().decode(GoosicLyrics.self, from: json)
        XCTAssertNil(doc.lines[0].words)
        XCTAssertTrue(LyricsTiming.valid(doc))
    }

    func testImportInstallsOnlyValidDocumentForCurrentTrack() async {
        await MainActor.run {
            let model = GoosicAppModel(debugSidebarFixture: true)
            model.currentTrack = GoosicTrack(id: "t", title: "Track", subtitle: "", artist: "", artistID: nil,
                album: "", albumID: nil, duration: "3:00", videoID: "v", explicit: false)
            model.installImportedLyrics(Self.document)
            XCTAssertEqual(model.lyrics?.source, "Imported")
            var invalid = Self.document
            invalid.lines[0].words![1].atMs = 0
            model.installImportedLyrics(invalid)
            XCTAssertEqual(model.lyrics?.lines, Self.document.lines)
        }
    }
}
