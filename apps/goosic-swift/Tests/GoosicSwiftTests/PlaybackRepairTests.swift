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

    func testActiveMediaPrefersPausedContentOverAnEndedAdvertisement() throws {
        let js = try context()
        js.evaluateScript("""
        var candidates = [
          {id:'ad', paused:true, ended:true, readyState:4},
          {id:'content', paused:true, ended:false, readyState:4}
        ];
        var document = {querySelectorAll: () => candidates};
        """)
        XCTAssertEqual(js.evaluateScript(OfficialBridge.activeMediaElementScript)?.forProperty("id")?.toString(), "content")
        js.evaluateScript("candidates.unshift({id:'playing', paused:false, ended:false, readyState:4});")
        XCTAssertEqual(js.evaluateScript(OfficialBridge.activeMediaElementScript)?.forProperty("id")?.toString(), "playing")
        js.evaluateScript("candidates = [];")
        XCTAssertTrue(js.evaluateScript(OfficialBridge.activeMediaElementScript)?.isNull == true)
        XCTAssertNil(js.exception)
    }

    func testObserverDoesNotTreatRetainedAdTextAsAnActiveAdvertisement() throws {
        let js = try context()
        js.evaluateScript("""
        var activeAd = false, events = [], poll;
        var window = this;
        window.location = {search:'?v=requested'};
        class URLSearchParams { get() { return 'requested'; } }
        window.webkit = {messageHandlers:{goosicBridge:{postMessage:event => events.push(event)}}};
        window.setInterval = callback => { poll = callback; };
        var media = {paused:false, ended:false, readyState:4, currentTime:10, duration:180,
          volume:0.2, muted:false, addEventListener:() => {}};
        var document = {documentElement:{}, querySelectorAll:() => [media],
          querySelector:selector => activeAd || selector.includes('.ytp-ad-text') ? {} : null};
        class MutationObserver { observe() {} }
        """)
        js.evaluateScript(OfficialBridge.observerScript(token: "token", generation: 1, videoID: "requested"))
        XCTAssertFalse(js.evaluateScript("events[events.length - 1].isAdvertisement")?.toBool() == true)
        js.evaluateScript("activeAd = true; poll();")
        XCTAssertTrue(js.evaluateScript("events[events.length - 1].isAdvertisement")?.toBool() == true)
        js.evaluateScript("activeAd = false; poll();")
        XCTAssertFalse(js.evaluateScript("events[events.length - 1].isAdvertisement")?.toBool() == true)
        XCTAssertNil(js.exception)
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
                       "[[\"volume\",0.2],[\"play\",0.2,false]]")
        // A level change while playing touches only the level: no mute, which was audible.
        js.evaluateScript("writes = []; goosicSetVolumePreference(0.3, false); goosicSetVolumePreference(0.2, false);")
        XCTAssertEqual(js.evaluateScript("JSON.stringify(writes)")?.toString(),
                       "[[\"volume\",0.3],[\"volume\",0.2]]")
        js.evaluateScript("first.volume = 1; first.muted = false;")
        XCTAssertEqual(js.evaluateScript("first.volume")?.toDouble(), 0.2)
        js.evaluateScript("var replacement = new HTMLMediaElement(); media.push(replacement); replacement.play();")
        XCTAssertEqual(js.evaluateScript("replacement.volume")?.toDouble(), 0.2)
        js.evaluateScript("ad = true; first.volume = 1; observer();")
        XCTAssertEqual(js.evaluateScript("first.volume")?.toDouble(), 0.2)
        js.evaluateScript("goosicSetVolumePreference(0.25, true);")
        XCTAssertEqual(js.evaluateScript("first.volume")?.toDouble(), 0.25)
        XCTAssertEqual(js.evaluateScript("replacement.volume")?.toDouble(), 0.25)
        XCTAssertEqual(js.evaluateScript("first.muted")?.toBool(), true)
        js.evaluateScript("ad = false; observer(); first.play();")
        XCTAssertEqual(js.evaluateScript("first.volume")?.toDouble(), 0.25)
        js.evaluateScript("goosicSetVolumePreference(0.4, true); first.volume = 1; first.muted = false;")
        XCTAssertEqual(js.evaluateScript("first.volume")?.toDouble(), 0.4)
        XCTAssertEqual(js.evaluateScript("first.muted")?.toBool(), true)
        XCTAssertNil(js.exception)
    }
}
#endif
