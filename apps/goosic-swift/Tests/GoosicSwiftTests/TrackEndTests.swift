import XCTest

@testable import GoosicSwift

final class AdPlaybackRecoveryTests: XCTestCase {
    func testAdHandoffWaitsForContentAndResumesOnlyOnce() {
        var recovery = AdPlaybackRecovery()
        XCTAssertFalse(recovery.observe(advertisement: true, state: "playing", listenerPaused: false))
        XCTAssertFalse(recovery.observe(advertisement: true, state: "ended", listenerPaused: false))
        XCTAssertFalse(recovery.observe(advertisement: false, state: "ended", listenerPaused: false))
        XCTAssertTrue(recovery.awaitingContent)
        XCTAssertTrue(recovery.observe(advertisement: false, state: "paused", listenerPaused: false))
        XCTAssertFalse(recovery.observe(advertisement: false, state: "paused", listenerPaused: false))
        XCTAssertFalse(recovery.observe(advertisement: false, state: "playing", listenerPaused: false))
        XCTAssertFalse(recovery.awaitingContent)
        XCTAssertFalse(recovery.observe(advertisement: false, state: "paused", listenerPaused: false))
        XCTAssertFalse(recovery.observe(advertisement: true, state: "paused", listenerPaused: true))
        XCTAssertFalse(recovery.observe(advertisement: false, state: "paused", listenerPaused: true))
        XCTAssertTrue(recovery.awaitingContent)
    }
}

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

    func testOnlyTheFirstEndOfAHeardPlayAdvances() {
        var guardian = EndGuard()
        guardian.begin()
        XCTAssertFalse(guardian.observe(advertisement: false, state: "ended", position: 229, duration: 229,
                                        listenerPaused: false).advance, "never heard")
        _ = guardian.observe(advertisement: false, state: "playing", position: 10, duration: 229, listenerPaused: false)
        XCTAssertFalse(guardian.observe(advertisement: false, state: "paused", position: 228.8, duration: 229,
                                        listenerPaused: true).advance, "the listener paused")
        XCTAssertTrue(guardian.observe(advertisement: false, state: "paused", position: 228.8, duration: 229,
                                       listenerPaused: false).advance)
        XCTAssertFalse(guardian.observe(advertisement: false, state: "ended", position: 229, duration: 229,
                                        listenerPaused: false).advance, "reported twice")
        XCTAssertFalse(guardian.pageEnded())
    }

    func testContentAnAdvertisementLeftPausedIsResumedNotEnded() {
        var guardian = EndGuard()
        _ = guardian.observe(advertisement: false, state: "playing", position: 1, duration: 229, listenerPaused: false)
        XCTAssertFalse(guardian.observe(advertisement: true, state: "ended", position: 30, duration: 30,
                                        listenerPaused: false).advance)
        let after = guardian.observe(advertisement: false, state: "paused", position: 229, duration: 229,
                                     listenerPaused: false)
        XCTAssertTrue(after.resume)
        XCTAssertFalse(after.advance)
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
