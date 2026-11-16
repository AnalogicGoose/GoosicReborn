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

    func testASuppliedEndDecidesTheSweepAndAStartOnlyWordRunsToTheNextWord() {
        let line = GoosicLyricsLine(words: [
            GoosicLyricsWord(atMs: 1000, endMs: 1400, text: "One "),
            GoosicLyricsWord(atMs: 2000, text: "two "),
            GoosicLyricsWord(atMs: 2600, text: "three "),
            GoosicLyricsWord(atMs: 2600, text: "same "),
            GoosicLyricsWord(atMs: 9000, text: "far "),
            GoosicLyricsWord(atMs: 20_000, text: "last")
        ], atMs: 1000, text: "One two three same far last")
        let sweeps = LyricsTiming.sweeps(line)
        XCTAssertEqual(sweeps[0], .init(startMs: 1000, endMs: 1400, supplied: true))
        XCTAssertEqual(sweeps[1], .init(startMs: 2000, endMs: 2600, supplied: false))
        // Words that start together cannot bound each other.
        XCTAssertEqual(sweeps[2].endMs, 2600 + LyricsTiming.settleMs)
        // A long wait for the next word is a rest, not a held note.
        XCTAssertEqual(sweeps[3].endMs, 2600 + LyricsTiming.longestInferredSweepMs)
        XCTAssertEqual(sweeps[5].endMs, 20_000 + LyricsTiming.lastWordSweepMs)
        XCTAssertEqual(sweeps[1].progress(1999), 0)
        XCTAssertEqual(sweeps[1].progress(2300), 0.5)
        XCTAssertEqual(sweeps[1].progress(2600), 1)
        XCTAssertEqual(sweeps[1].progress(.nan), 0)
        // The shared progress rule is untouched, and a line without words gets no sweep.
        XCTAssertEqual(LyricsTiming.progress(line.words![1], positionMs: 2000), 1)
        XCTAssertTrue(LyricsTiming.sweeps(GoosicLyricsLine(atMs: 1000, text: "One two")).isEmpty)
    }

    func testAWordRisesOnceAndStaysUp() {
        let sweep = LyricsTiming.Sweep(startMs: 1000, endMs: 1200, supplied: false)
        XCTAssertEqual(sweep.lift(900), 0)
        XCTAssertEqual(sweep.lift(1500), 0.75)
        XCTAssertEqual(sweep.lift(2000), 1)
        XCTAssertEqual(sweep.lift(60_000), 1)
    }

    func testOnlyAShortWordHeldForASecondIsEmphasised() {
        func sweep(_ duration: Double) -> LyricsTiming.Sweep {
            .init(startMs: 0, endMs: duration, supplied: true)
        }
        XCTAssertNil(LyricsTiming.held(sweep(999), characters: 4))
        XCTAssertNil(LyricsTiming.held(sweep(2000), characters: 1))
        XCTAssertNil(LyricsTiming.held(sweep(2000), characters: 8))
        let held = LyricsTiming.held(sweep(2000), characters: 4)!
        XCTAssertEqual(held.amount, 0.6, accuracy: 1e-9)
        XCTAssertLessThan(LyricsTiming.held(sweep(1000), characters: 4)!.amount, held.amount)
        XCTAssertLessThanOrEqual(LyricsTiming.held(sweep(60_000), characters: 4)!.amount, 1.2)
        XCTAssertLessThanOrEqual(LyricsTiming.held(sweep(60_000), characters: 4)!.glow, 0.8)
        // A letter is at rest before its turn and after it, and swells in between.
        let before = held.glyph(3, of: 4, sweep: sweep(2000), positionMs: 0)
        XCTAssertEqual(before.scale, 1)
        XCTAssertEqual(before.glow, 0)
        let peak = held.glyph(0, of: 4, sweep: sweep(2000), positionMs: 1000)
        XCTAssertEqual(peak.scale, 1.06, accuracy: 1e-9)
        XCTAssertEqual(peak.glow, held.glow, accuracy: 1e-9)
        XCTAssertEqual(held.glyph(0, of: 4, sweep: sweep(2000), positionMs: 9000).scale, 1, accuracy: 1e-9)
    }

    func testInterludesComeOnlyFromWhatTheLyricsMarkAsUnsung() {
        let lines = [
            GoosicLyricsLine(atMs: 6000, text: "One"),
            GoosicLyricsLine(atMs: 10_000, text: ""),
            GoosicLyricsLine(atMs: 15_000, text: "Two"),
            GoosicLyricsLine(atMs: 16_000, text: ""),
            GoosicLyricsLine(atMs: 17_000, text: "Three"),
            GoosicLyricsLine(atMs: 90_000, text: "")
        ]
        XCTAssertEqual(LyricsTiming.interlude(lines: lines, synced: true, index: nil),
                       .init(startMs: 0, endMs: 6000))
        XCTAssertEqual(LyricsTiming.interlude(lines: lines, synced: true, index: 1),
                       .init(startMs: 10_000, endMs: 15_000))
        // A sung line followed by a long wait is not an interlude, nor is a short empty line,
        // nor the last line, whose end nothing marks.
        XCTAssertNil(LyricsTiming.interlude(lines: lines, synced: true, index: 4))
        XCTAssertNil(LyricsTiming.interlude(lines: lines, synced: true, index: 3))
        XCTAssertNil(LyricsTiming.interlude(lines: lines, synced: true, index: 5))
        XCTAssertNil(LyricsTiming.interlude(lines: lines, synced: false, index: nil))
        XCTAssertNil(LyricsTiming.interlude(
            lines: [GoosicLyricsLine(atMs: 1000, text: "Soon")], synced: true, index: nil))
    }

    func testALineRestsOnlyAtItsSuppliedEnd() {
        let lines = [
            GoosicLyricsLine(atMs: 1000, endMs: 3000, text: "One"),
            GoosicLyricsLine(atMs: 9000, text: "Two"),
            GoosicLyricsLine(atMs: 12_000, endMs: 12_000, text: "Three")
        ]
        XCTAssertFalse(LyricsTiming.rests(lines: lines, index: 0, positionMs: 2999))
        XCTAssertTrue(LyricsTiming.rests(lines: lines, index: 0, positionMs: 3000))
        // Without an end the line stays current, and an end that is not after the start is no end.
        XCTAssertFalse(LyricsTiming.rests(lines: lines, index: 1, positionMs: 11_999))
        XCTAssertFalse(LyricsTiming.rests(lines: lines, index: 2, positionMs: 60_000))
        XCTAssertFalse(LyricsTiming.rests(lines: lines, index: nil, positionMs: 500))
        XCTAssertFalse(LyricsTiming.rests(lines: lines, index: 9, positionMs: 500))
        // A line that ends just before the next begins stays lit: no rest, and none scheduled.
        let close = [
            GoosicLyricsLine(atMs: 1000, endMs: 3000, text: "One"),
            GoosicLyricsLine(atMs: 3999, endMs: 6000, text: "Two")
        ]
        XCTAssertFalse(LyricsTiming.rests(lines: close, index: 0, positionMs: 3500))
        XCTAssertEqual(LyricsTiming.upcomingChanges(lines: close, synced: true, afterMs: 2500), [])
        XCTAssertEqual(LyricsTiming.upcomingChanges(lines: close, synced: true, afterMs: 3500), [3999])
        // The last line has nothing after it, so it rests at its end.
        XCTAssertTrue(LyricsTiming.rests(lines: close, index: 1, positionMs: 6000))
        // Starts and ends inside the interpolation limit are both scheduled, once each.
        XCTAssertEqual(LyricsTiming.upcomingChanges(lines: lines, synced: true, afterMs: 2500), [3000])
        XCTAssertEqual(LyricsTiming.upcomingChanges(lines: lines, synced: true, afterMs: 8500), [9000])
        XCTAssertTrue(LyricsTiming.upcomingChanges(lines: lines, synced: true, afterMs: 3000).isEmpty)
        XCTAssertTrue(LyricsTiming.upcomingChanges(lines: lines, synced: false, afterMs: 2500).isEmpty)
        // The rest after a supplied end is an interlude when it is long enough.
        XCTAssertEqual(LyricsTiming.interlude(lines: lines, synced: true, after: 0),
                       .init(startMs: 3000, endMs: 9000))
        XCTAssertNil(LyricsTiming.interlude(lines: lines, synced: true, after: 1))
        XCTAssertNil(LyricsTiming.interlude(lines: lines, synced: true, after: 2))
        // The line's end bounds its last word, which nothing else does.
        let line = GoosicLyricsLine(words: [GoosicLyricsWord(atMs: 1000, text: "One")],
                                    atMs: 1000, endMs: 1900, text: "One")
        XCTAssertEqual(LyricsTiming.sweeps(line)[0].endMs, 1900)
    }

    func testLineEndsDecodeAndStayOptional() throws {
        let wire = Data(#"{"source":"LRCLIB","synced":true,"lines":[{"atMs":1000,"endMs":2500,"text":"Hi"},{"atMs":3000,"text":"all"}]}"#.utf8)
        let doc = try JSONDecoder().decode(GoosicLyrics.self, from: wire)
        XCTAssertEqual(doc.lines[0].endMs, 2500)
        XCTAssertNil(doc.lines[1].endMs)
    }

    func testInterludeDotsFillInTurnAndFoldAwayBeforeTheNextLine() {
        let gap = LyricsTiming.Interlude(startMs: 0, endMs: 9000)
        XCTAssertEqual(gap.dotOpacity(0, positionMs: 0), 0.25)
        XCTAssertEqual(gap.dotOpacity(0, positionMs: 3000), 1)
        XCTAssertEqual(gap.dotOpacity(1, positionMs: 3000), 0.25)
        XCTAssertEqual(gap.dotOpacity(2, positionMs: 9000), 1)
        XCTAssertEqual(gap.scale(positionMs: 9000, animated: true), 0, accuracy: 1e-9)
        XCTAssertEqual(gap.scale(positionMs: 4000, animated: false), 1)
        XCTAssertEqual(gap.scale(positionMs: 9000, animated: false), 1)
    }

    func testDisplayPositionRunsAtMostOneSecondAheadAndFreezes() {
        XCTAssertEqual(LyricsTiming.displayPosition(confirmed: 10, elapsed: 0.4, frozen: false), 10.4)
        XCTAssertEqual(LyricsTiming.displayPosition(confirmed: 10, elapsed: 5, frozen: false), 11)
        XCTAssertEqual(LyricsTiming.displayPosition(confirmed: 10, elapsed: -3, frozen: false), 10)
        XCTAssertEqual(LyricsTiming.displayPosition(confirmed: 10, elapsed: 0.4, frozen: true), 10)
        XCTAssertEqual(LyricsTiming.displayPosition(confirmed: 10, elapsed: .nan, frozen: false), 10)
    }

    func testAConfirmedPositionIsFollowedWithoutSteppingToIt() {
        // Close by: the clock stays where it was and runs a little fast or slow.
        let behind = LyricsTiming.reanchor(predicted: 10, confirmed: 10.1, frozen: false)
        XCTAssertEqual(behind.base, 10)
        XCTAssertEqual(behind.rate, 1.06, accuracy: 1e-9)
        let ahead = LyricsTiming.reanchor(predicted: 10, confirmed: 9.9, frozen: false)
        XCTAssertEqual(ahead.base, 10)
        XCTAssertEqual(ahead.rate, 0.94, accuracy: 1e-9)
        // A second later 60% of the gap has closed, and the clock never ran backwards.
        XCTAssertEqual(LyricsTiming.displayPosition(
            confirmed: behind.base, elapsed: 1, frozen: false, rate: behind.rate), 11.06, accuracy: 1e-9)
        // A seek, a stall or a pause is taken at once.
        XCTAssertEqual(LyricsTiming.reanchor(predicted: 10, confirmed: 42, frozen: false).base, 42)
        XCTAssertEqual(LyricsTiming.reanchor(predicted: 10, confirmed: 42, frozen: false).rate, 1)
        XCTAssertEqual(LyricsTiming.reanchor(predicted: 10, confirmed: 10.1, frozen: true).base, 10.1)
        XCTAssertEqual(LyricsTiming.reanchor(predicted: .nan, confirmed: 5, frozen: false).base, 5)
    }

    func testOnlyLinesInsideTheInterpolationLimitAreScheduled() {
        let lines = [0, 4000, 4600, 5000, 5001, 9000].map {
            GoosicLyricsLine(atMs: Int64($0), text: "Line \($0)")
        }
        let starts = LyricsTiming.upcomingStarts(lines: lines, synced: true, afterMs: 4000)
        XCTAssertEqual(starts.map(\.index), [2, 3])
        XCTAssertEqual(starts.map(\.atMs), [4600, 5000])
        XCTAssertTrue(LyricsTiming.upcomingStarts(lines: lines, synced: false, afterMs: 4000).isEmpty)
    }

    func testALineAlreadyPresentedSurvivesAConfirmationJustShortOfIt() {
        let lines = [GoosicLyricsLine(atMs: 1000, text: "One"), GoosicLyricsLine(atMs: 5000, text: "Two")]
        XCTAssertTrue(LyricsTiming.keepsPresented(1, confirmed: 0, lines: lines, positionMs: 4900))
        XCTAssertTrue(LyricsTiming.keepsPresented(0, confirmed: nil, lines: lines, positionMs: 900))
        // A real step back, such as a seek, is honoured.
        XCTAssertFalse(LyricsTiming.keepsPresented(1, confirmed: 0, lines: lines, positionMs: 3000))
        XCTAssertFalse(LyricsTiming.keepsPresented(1, confirmed: nil, lines: lines, positionMs: 900))
        XCTAssertFalse(LyricsTiming.keepsPresented(1, confirmed: 1, lines: lines, positionMs: 5200))
        XCTAssertFalse(LyricsTiming.keepsPresented(7, confirmed: 6, lines: lines, positionMs: 4900))
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
