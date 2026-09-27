import XCTest

@testable import GoosicSwift

/// "Songs lose their last seconds, and then nothing plays." With Automix on, YouTube Music faded
/// into a track of its own before the requested one ended; with it off, the page stops a
/// fraction of a second short and reports `paused`, never `ended`. The same cases as the
/// `goosic-shell-support` crate's `playback` tests.
final class TrackEndTests: XCTestCase {
    func testEndedIsTheEnd() {
        XCTAssertTrue(TrackEnd.hasFinished(state: "ended", position: 10, duration: 229, listenerPaused: false))
    }

    func testAPauseAtTheLastMomentIsTheEnd() {
        XCTAssertTrue(TrackEnd.hasFinished(state: "paused", position: 228.8, duration: 229, listenerPaused: false))
    }

    func testTheListenersOwnPauseIsNotTheEnd() {
        XCTAssertFalse(TrackEnd.hasFinished(state: "paused", position: 228.8, duration: 229, listenerPaused: true))
    }

    func testAPauseMidwayIsNotTheEnd() {
        XCTAssertFalse(TrackEnd.hasFinished(state: "paused", position: 120, duration: 229, listenerPaused: false))
        XCTAssertFalse(TrackEnd.hasFinished(state: "paused", position: 0, duration: 0, listenerPaused: false))
        XCTAssertFalse(TrackEnd.hasFinished(state: "paused", position: .nan, duration: 229, listenerPaused: false))
    }

    func testPlayingIsNeverTheEnd() {
        XCTAssertFalse(TrackEnd.hasFinished(state: "playing", position: 229, duration: 229, listenerPaused: false))
    }

    func testOnlyTheFirstEndOfANonAdvertisementAdvances() {
        func advance(_ state: String, ad: Bool = false, position: Double = 10, last: String? = nil,
                     listenerPaused: Bool = false) -> Bool {
            TrackEnd.shouldAdvance(
                state: state, isAdvertisement: ad, videoID: "v", position: position, duration: 229,
                lastEndedVideoID: last, listenerPaused: listenerPaused
            )
        }
        XCTAssertTrue(advance("ended"))
        XCTAssertTrue(advance("ended", last: "u"))
        XCTAssertFalse(advance("ended", last: "v"), "reported twice")
        XCTAssertFalse(advance("ended", ad: true), "an advertisement ended")
        XCTAssertFalse(advance("paused"))
        XCTAssertTrue(advance("paused", position: 228.8))
        XCTAssertFalse(advance("paused", position: 228.8, listenerPaused: true))
    }

    func testTheAutomixScriptOnlyTouchesTheAutomixSwitchAndSaysWhatItDid() {
        let script = OfficialBridge.automixOffScript
        XCTAssertTrue(script.contains("getElementById('automix')"))
        for answer in ["'absent'", "'off'", "'turned off'", "'still on'"] {
            XCTAssertTrue(script.contains(answer), answer)
        }
        XCTAssertFalse(script.contains("location"), "it never navigates")
    }
}
