import XCTest
@testable import GoosicSwift

/// The queue's rules are `QueueEngine`'s and are tested in Rust. These check the Mac's side of
/// the channel: that commands reach the engine, and that the model draws what it answers with
/// the shell's own track values, duplicates included.
final class QueueEditingTests: XCTestCase {
    private static func track(_ id: String) -> GoosicTrack {
        GoosicTrack(id: id, title: id, subtitle: "", artist: "", artistID: nil,
                    album: "", albumID: nil, duration: "3:00", videoID: id, explicit: false)
    }

    func testClearAndUndoPreserveThePlayingOccurrenceAndPosition() async {
        await MainActor.run {
            let model = GoosicAppModel(debugSidebarFixture: true)
            let repeated = Self.track("same")
            model.adoptFixtureQueue([repeated, repeated, Self.track("next")], current: 1)
            model.currentTrack = repeated
            let original = model.queue
            XCTAssertTrue(model.canClearUpcoming)
            model.clearUpcoming()
            XCTAssertEqual(model.queue.tracks, [repeated, repeated])
            XCTAssertEqual(model.queue.currentIndex, 1)
            XCTAssertEqual(model.currentTrack, repeated)
            XCTAssertTrue(model.canUndoClear)
            model.undoClearUpcoming()
            XCTAssertEqual(model.queue, original)
            XCTAssertFalse(model.canUndoClear)
        }
    }

    func testAQueueEditInvalidatesUndo() async {
        await MainActor.run {
            let model = GoosicAppModel(debugSidebarFixture: true)
            let rows = [Self.track("past"), Self.track("current"), Self.track("next")]
            model.adoptFixtureQueue(rows, current: 1)
            model.currentTrack = rows[1]
            model.clearUpcoming()
            model.addToQueue(Self.track("added"))
            XCTAssertFalse(model.canUndoClear)
            let edited = model.queue
            model.undoClearUpcoming()
            XCTAssertEqual(model.queue, edited)
            XCTAssertEqual(model.queue.tracks.map(\.videoID), ["past", "current", "added"])
        }
    }

    func testRowsAreRemovedAndMovedByEntryNotByValue() async {
        await MainActor.run {
            let model = GoosicAppModel(debugSidebarFixture: true)
            let a = Self.track("a"), b = Self.track("b")
            model.adoptFixtureQueue([a, b, a, b], current: 1)
            model.removeFromQueue(at: 1)
            XCTAssertEqual(model.queue.tracks.count, 4, "the playing entry stays")
            model.removeFromQueue(at: 2)
            XCTAssertEqual(model.queue.tracks.map(\.videoID), ["a", "b", "b"])
            XCTAssertEqual(model.queue.currentIndex, 1)
            model.moveQueueItems(from: IndexSet(integer: 1), to: 3)
            XCTAssertEqual(model.queue.tracks.map(\.videoID), ["a", "b", "b"])
            XCTAssertEqual(model.queue.currentIndex, 2, "the playing entry moved with its row")
        }
    }
}

final class SharedQueueTests: XCTestCase {
    private static func track(_ id: String) -> GoosicTrack {
        GoosicTrack(id: id, title: "Song \(id)", subtitle: "", artist: "", artistID: nil,
                    album: "", albumID: nil, duration: "", videoID: id, explicit: false)
    }

    func testASearchResultStartsAStationFromThatSongAlone() {
        let queue = SharedQueue()
        let tracks = ["dumbai", "dubai", "other"].map(Self.track)
        let outcome = queue.handle(.launch(
            tracks: tracks, start: 0, kind: SharedRules.launchKind(for: .search),
            sourceTitle: "Songs", sourceKey: nil, shuffle: nil))
        XCTAssertEqual(outcome.snapshot.entries.count, 1)
        XCTAssertEqual(outcome.added.map(\.input), [0])
        XCTAssertEqual(outcome.snapshot.sourceTitle, "Song dumbai radio")
        guard case .play = outcome.effects.first else {
            return XCTFail("the chosen song plays at once: \(outcome.effects)")
        }
        guard case .fetchRadio(_, let seed, nil, nil) = outcome.effects.last else {
            return XCTFail("recommendations are asked for: \(outcome.effects)")
        }
        XCTAssertEqual(seed, "dumbai")
    }

    func testAnAlbumPlaysInOrderAndShuffleIsSavedThroughAnEffect() {
        let queue = SharedQueue()
        let outcome = queue.handle(.launch(
            tracks: ["a", "b", "c"].map(Self.track), start: 1,
            kind: SharedRules.launchKind(for: .album), sourceTitle: "Album", sourceKey: "k",
            shuffle: nil))
        XCTAssertEqual(outcome.snapshot.current, 1)
        XCTAssertEqual(outcome.snapshot.upNextText, "From Album · 1 song")
        let toggled = queue.handle(.toggleShuffle)
        XCTAssertEqual(toggled.effects, [.savePreferences(shuffle: true, repeatMode: .off)])
    }

    func testSurfacesMapPagesToLaunchKinds() {
        XCTAssertEqual(SharedRules.launchKind(for: LaunchSurface(page: .search(query: "x", filter: ""))), .station)
        XCTAssertEqual(SharedRules.launchKind(for: LaunchSurface(page: .album("a"))), .ordered)
        XCTAssertEqual(SharedRules.launchKind(for: LaunchSurface(page: .playlist("LM"))), .ordered)
        XCTAssertEqual(LaunchSurface(page: .playlist("LM")), .likedMusic)
        XCTAssertEqual(SharedRules.launchKind(for: LaunchSurface(page: .route(.home))), .station)
        XCTAssertEqual(SharedRules.launchKind(for: LaunchSurface(page: nil)), .station)
    }
}
