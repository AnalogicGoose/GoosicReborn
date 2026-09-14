import XCTest
@testable import GoosicSwift

final class PlaybackLaunchIntentTests: XCTestCase {
    private static func track(_ id: String) -> GoosicTrack {
        GoosicTrack(id: id, title: id, subtitle: "", artist: "", artistID: nil,
                    album: "", albumID: nil, duration: "", videoID: id, explicit: false)
    }

    func testDiscoveryLaunchDoesNotCarryShelfHistory() {
        let song = Self.track("aaaaaaaaaaa")
        XCTAssertEqual(PlaybackLaunchIntent.resolve(track: song, explicitTracks: []), .station(song))
    }

    func testExplicitOrderAndRepeatedEntriesArePreserved() {
        let a = Self.track("aaaaaaaaaaa"), b = Self.track("bbbbbbbbbbb")
        XCTAssertEqual(PlaybackLaunchIntent.resolve(track: b, explicitTracks: [a, b, a]), .ordered(b, [a, b, a]))
    }

    func testStationRepliesAreBoundToRevisionAndAccount() {
        let personal = RadioRequestIdentity(revision: 8, accountID: "account-a")
        XCTAssertTrue(personal.usesPersonalCatalog)
        XCTAssertTrue(personal.accepts(revision: 8, accountID: "account-a"))
        XCTAssertFalse(personal.accepts(revision: 9, accountID: "account-a"))
        XCTAssertFalse(personal.accepts(revision: 8, accountID: "account-b"))
        XCTAssertFalse(personal.accepts(revision: 8, accountID: nil))
        let guest = RadioRequestIdentity(revision: 8, accountID: nil)
        XCTAssertFalse(guest.usesPersonalCatalog)
        XCTAssertFalse(guest.accepts(revision: 8, accountID: "account-a"))
    }
}

final class OfficialSampleSequenceTests: XCTestCase {
    func testSameLeaseContinuesAcrossPageLoads() {
        var sequence = OfficialSampleSequence()
        sequence.begin(generation: 9)
        XCTAssertEqual(sequence.issue(for: 9), 1)
        XCTAssertEqual(sequence.issue(for: 9), 2)
        sequence.begin(generation: 9)
        XCTAssertEqual(sequence.issue(for: 9), 3)
    }

    func testNewLeaseStartsAtOneAndOldLeaseCannotIssue() {
        var sequence = OfficialSampleSequence()
        sequence.begin(generation: 9)
        XCTAssertEqual(sequence.issue(for: 9), 1)
        sequence.begin(generation: 10)
        XCTAssertNil(sequence.issue(for: 9))
        XCTAssertEqual(sequence.issue(for: 10), 1)
    }
}

#if canImport(JavaScriptCore)
import JavaScriptCore

final class PlaybackJavaScriptTests: XCTestCase {
    private func context() throws -> JSContext {
        try XCTUnwrap(JSContext())
    }

    func testRadioUsesOnlyPanelRowsAndPanelContinuation() throws {
        let js = try context()
        let source = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("Sources/GoosicSwift/Resources/PersonalCatalog.js")
        js.evaluateScript(try String(contentsOf: source, encoding: .utf8))
        let answer = js.evaluateScript("""
        (() => {
          const row = id => ({ playlistPanelVideoRenderer: {videoId: id, title: {simpleText: id}} });
          const panel = {contents: [row('aaaaaaaaaaa'), row('bbbbbbbbbbb'),
            {playlistPanelVideoWrapperRenderer: {primaryRenderer: row('ccccccccccc')}},
            row('bbbbbbbbbbb')], continuations: [{nextContinuationData:{continuation:'owned'}}]};
          return JSON.stringify(GoosicPersonalCatalog.parseRadio({
            unrelated: {nextContinuationData:{continuation:'wrong'}},
            continuationContents: {playlistPanelContinuation: panel}
          }, 'aaaaaaaaaaa'));
        })()
        """)
        XCTAssertNil(js.exception)
        let data = try XCTUnwrap(answer?.toString()?.data(using: .utf8))
        let page = try JSONDecoder().decode(GoosicCatalogPage.self, from: data)
        XCTAssertEqual((page.tracks ?? []).map(\.id), ["bbbbbbbbbbb", "ccccccccccc"])
        XCTAssertEqual(page.nextCursor, "owned")
    }

    func testBootstrapGuardsFirstPlayReplacementResetAndAdHandoff() throws {
        let js = try context()
        js.evaluateScript("""
        var ad = false, writes = [], media = [], observer;
        var window = this;
        var document = { querySelector: () => ad ? {} : null,
          querySelectorAll: () => media, addEventListener: () => {} };
        class MutationObserver { constructor(fn) { observer = fn; } observe() {} }
        class HTMLMediaElement {
          constructor() { this.v = 1; this.m = false; }
          get volume() { return this.v; }
          set volume(v) { writes.push(['volume', v]); this.v = v; }
          get muted() { return this.m; }
          set muted(m) { writes.push(['muted', m]); this.m = m; }
          play() { writes.push(['play', this.v, this.m]); }
        }
        """)
        js.evaluateScript(OfficialVolumeBootstrap.script(volume: 0.2, muted: false))
        js.evaluateScript("var first = new HTMLMediaElement(); media.push(first); first.play();")
        XCTAssertEqual(js.evaluateScript("JSON.stringify(writes)")?.toString(),
                       "[[\"muted\",true],[\"volume\",0.2],[\"muted\",false],[\"play\",0.2,false]]")
        js.evaluateScript("first.volume = 1; first.muted = false;")
        XCTAssertEqual(js.evaluateScript("first.volume")?.toDouble(), 0.2)
        js.evaluateScript("var replacement = new HTMLMediaElement(); media.push(replacement); replacement.play();")
        XCTAssertEqual(js.evaluateScript("replacement.volume")?.toDouble(), 0.2)
        js.evaluateScript("ad = true; first.volume = 1; observer();")
        XCTAssertEqual(js.evaluateScript("first.volume")?.toDouble(), 1)
        js.evaluateScript("ad = false; observer(); first.play();")
        XCTAssertEqual(js.evaluateScript("first.volume")?.toDouble(), 0.2)
        js.evaluateScript("goosicSetVolumePreference(0.4, true); first.volume = 1; first.muted = false;")
        XCTAssertEqual(js.evaluateScript("first.volume")?.toDouble(), 0.4)
        XCTAssertEqual(js.evaluateScript("first.muted")?.toBool(), true)
        XCTAssertNil(js.exception)
    }
}
#endif
