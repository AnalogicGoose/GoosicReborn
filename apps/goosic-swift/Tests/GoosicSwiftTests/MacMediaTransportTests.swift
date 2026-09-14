#if os(macOS)
import XCTest
@testable import GoosicSwift

final class MacMediaTransportTests: XCTestCase {
    func testPauseSupersedesPlayWhileMediaIsLoading() {
        var request = MacMediaTransportRequest()
        request.request(paused: false)
        let playRevision = request.revision
        request.request(paused: true)
        request.complete(revision: playRevision)
        XCTAssertEqual(request.paused, true)
        request.complete(revision: request.revision)
        XCTAssertNil(request.paused)
    }

    func testOldDocumentCompletionCannotConsumeNewTrackRequest() {
        var request = MacMediaTransportRequest()
        request.request(paused: false)
        let oldRevision = request.revision
        request.reset()
        XCTAssertNil(request.paused)
        request.request(paused: false)
        request.complete(revision: oldRevision)
        XCTAssertEqual(request.paused, false)
    }

    func testReleasingPlaybackDiscardsPendingCommand() {
        var request = MacMediaTransportRequest()
        request.request(paused: false)
        request.reset()
        XCTAssertNil(request.paused)
    }
}
#endif
