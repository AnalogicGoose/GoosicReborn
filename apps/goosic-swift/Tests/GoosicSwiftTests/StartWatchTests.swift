@testable import GoosicSwift
import XCTest

final class StartWatchTests: XCTestCase {
    private func after(
        _ seconds: TimeInterval, heard: Bool = false, listener: Bool = false, nudged: Bool = false
    ) -> StartWatchAction {
        StartWatch.decide(sinceLoaded: seconds, heardPlaying: heard, listenerTookOver: listener, nudged: nudged)
    }

    func testAPageThatJustLoadedIsGivenTime() {
        XCTAssertEqual(after(1), .wait)
    }

    func testAPageStillSilentAfterAFewSecondsIsAskedToPlayOnce() {
        XCTAssertEqual(after(4), .nudge)
        XCTAssertEqual(after(6, nudged: true), .wait)
    }

    func testATrackThatNeverPlaysIsGivenUpOn() {
        XCTAssertEqual(after(20, nudged: true), .giveUp)
        XCTAssertEqual(after(45), .giveUp)
    }

    func testATrackThatHasPlayedIsNeverGivenUpOn() {
        XCTAssertEqual(after(60, heard: true), .done)
    }

    func testAPauseTheListenerChoseIsLeftAlone() {
        XCTAssertEqual(after(5, listener: true), .done)
        XCTAssertEqual(after(60, listener: true), .done)
    }

    func testLoadingShowsWhileWaitingForSoundButNeverOverAPause() {
        XCTAssertTrue(PlayerLoading.shows(awaitingStart: true, buffering: false, listenerPaused: false))
        XCTAssertTrue(PlayerLoading.shows(awaitingStart: false, buffering: true, listenerPaused: false))
        XCTAssertFalse(PlayerLoading.shows(awaitingStart: false, buffering: false, listenerPaused: false))
        XCTAssertFalse(PlayerLoading.shows(awaitingStart: true, buffering: true, listenerPaused: true))
    }

    func testPlayOutcomesKeepTheErrorName() {
        XCTAssertEqual(PlayOutcome("played"), .played)
        XCTAssertEqual(PlayOutcome("no-media"), .noMedia)
        XCTAssertEqual(PlayOutcome("superseded"), .superseded)
        XCTAssertEqual(PlayOutcome("pending"), .pending)
        XCTAssertEqual(PlayOutcome("failed:AbortError"), .failed("AbortError"))
        XCTAssertEqual(PlayOutcome("failed:"), .failed("Error"))
        XCTAssertEqual(PlayOutcome("nonsense"), .failed("Error"))
    }

    func testTheSharedScriptsReachTheShell() {
        XCTAssertTrue(OfficialBridge.listenerPlayScript.contains("const answerPrompt = true;"))
        XCTAssertTrue(OfficialBridge.hostPlayScript.contains("const answerPrompt = false;"))
        XCTAssertTrue(OfficialBridge.pauseScript.contains("media.pause()"))
        XCTAssertTrue(OfficialBridge.idlePromptScript.contains("ytmusic-you-there-renderer"))
    }
}
