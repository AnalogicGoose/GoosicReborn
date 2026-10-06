import XCTest
@testable import GoosicSwift

final class QueueEditingTests: XCTestCase {
    private static func track(_ id: String) -> GoosicTrack {
        GoosicTrack(id: id, title: id, subtitle: "", artist: "", artistID: nil,
                    album: "", albumID: nil, duration: "3:00", videoID: id, explicit: false)
    }

    func testClearingKeepsHistoryAndTheCurrentEntry() {
        XCTAssertEqual(QueueEditing.retainedCount(count: 4, currentIndex: 1), 2)
        XCTAssertEqual(QueueEditing.retainedCount(count: 4, currentIndex: 3), 4)
        XCTAssertNil(QueueEditing.retainedCount(count: 0, currentIndex: 0))
        XCTAssertNil(QueueEditing.retainedCount(count: 4, currentIndex: 4))
    }

    func testUndoExpiresOrIsInvalidatedByAnotherRevision() {
        XCTAssertTrue(QueueEditing.canUndo(expectedRevision: 7, revision: 7, deadline: 20, now: 19))
        XCTAssertFalse(QueueEditing.canUndo(expectedRevision: 7, revision: 8, deadline: 20, now: 19))
        XCTAssertFalse(QueueEditing.canUndo(expectedRevision: 7, revision: 7, deadline: 20, now: 20))
        XCTAssertFalse(QueueEditing.canUndo(expectedRevision: 7, revision: 7, deadline: 20, now: .nan))
        XCTAssertFalse(QueueEditing.canUndo(expectedRevision: 7, revision: 7, deadline: .infinity, now: 19))
    }

    func testClearAndUndoPreserveThePlayingOccurrenceAndPosition() async {
        await MainActor.run {
            let model = GoosicAppModel(debugSidebarFixture: true)
            let repeated = Self.track("same")
            model.queue = GoosicQueue(tracks: [repeated, repeated, Self.track("next")], currentIndex: 1)
            model.currentTrack = repeated
            let original = model.queue
            model.clearUpcoming()
            XCTAssertEqual(model.queue.tracks, [repeated, repeated])
            XCTAssertEqual(model.queue.currentIndex, 1)
            XCTAssertEqual(model.currentTrack, repeated)
            XCTAssertTrue(model.canUndoClear)
            model.undoClearUpcoming()
            XCTAssertEqual(model.queue, original)
            XCTAssertEqual(model.currentTrack, repeated)
            XCTAssertFalse(model.canUndoClear)
        }
    }

    func testAQueueEditOrCurrentEntryChangeInvalidatesUndo() async {
        await MainActor.run {
            let model = GoosicAppModel(debugSidebarFixture: true)
            let rows = [Self.track("past"), Self.track("current"), Self.track("next")]
            model.queue = GoosicQueue(tracks: rows, currentIndex: 1)
            model.currentTrack = rows[1]
            model.clearUpcoming()
            model.addToQueue(Self.track("added"))
            XCTAssertFalse(model.canUndoClear)
            let edited = model.queue
            model.undoClearUpcoming()
            XCTAssertEqual(model.queue, edited)

            model.clearUpcoming()
            XCTAssertTrue(model.canUndoClear)
            model.queue.currentIndex = 0
            XCTAssertFalse(model.canUndoClear)
        }
    }
}
