import XCTest

@testable import GoosicSwift

final class SystemMediaProjectionTests: XCTestCase {
    private let track = GoosicTrack(
        id: "track",
        title: "Afterglow",
        subtitle: "Song",
        artist: "Signal Fires",
        artistID: nil,
        album: "Night Windows",
        albumID: nil,
        duration: "3:42",
        videoID: "video",
        explicit: false,
        thumbnail: "https://example.test/art.jpg"
    )

    private func snapshot(
        owner: GoosicOwner = .officialWebView,
        paused: Bool = false,
        advertisement: Bool = false,
        queue: Bool = true,
        transition: PlaybackTransition = .idle,
        time: Double = 42,
        duration: Double = 222,
        ready: Bool = true
    ) -> SystemMediaPlaybackSnapshot {
        SystemMediaPlaybackSnapshot(
            track: track,
            currentTime: time,
            duration: duration,
            isPaused: paused,
            owner: owner,
            isAdvertisement: advertisement,
            hasQueue: queue,
            transition: transition,
            volume: 1,
            isMuted: false,
            isReady: ready
        )
    }

    func testProjectionUsesConfirmedPositionAndTrackMetadata() {
        let projection = SystemMediaNowPlayingProjection.make(from: snapshot())
        XCTAssertEqual(projection.title, "Afterglow")
        XCTAssertEqual(projection.artist, "Signal Fires")
        XCTAssertEqual(projection.album, "Night Windows")
        XCTAssertEqual(projection.artworkURL?.absoluteString, "https://example.test/art.jpg")
        XCTAssertEqual(projection.elapsedTime, 42)
        XCTAssertEqual(projection.duration, 222)
        XCTAssertEqual(projection.playbackRate, 1)
        XCTAssertEqual(projection.playbackState, .playing)
        XCTAssertTrue(projection.isActive)
    }

    func testProjectionClampsImpossibleConfirmedTimesAndStopsWhenReleased() {
        let clamped = SystemMediaNowPlayingProjection.make(from: snapshot(time: 999, duration: 30))
        XCTAssertEqual(clamped.elapsedTime, 30)
        XCTAssertEqual(clamped.duration, 30)

        let released = SystemMediaNowPlayingProjection.make(from: snapshot(owner: .none))
        XCTAssertFalse(released.isActive)
        XCTAssertNil(released.title)
        XCTAssertEqual(released.playbackState, .stopped)
        XCTAssertEqual(released.playbackRate, 0)
    }

    func testProjectionAndCommandsStayClearedBeforeTheFirstConfirmedSample() {
        let loading = SystemMediaNowPlayingProjection.make(from: snapshot(ready: false))
        XCTAssertFalse(loading.isActive)
        XCTAssertNil(loading.title)
        XCTAssertEqual(loading.playbackState, .stopped)

        let availability = SystemMediaCommandAvailability.make(from: snapshot(ready: false))
        XCTAssertFalse(availability.play)
        XCTAssertFalse(availability.pause)
        XCTAssertFalse(availability.togglePlayPause)
        XCTAssertFalse(availability.next)
        XCTAssertFalse(availability.previous)
        XCTAssertFalse(availability.changePosition)
        XCTAssertFalse(availability.stop)
        XCTAssertFalse(availability.changeVolume)
    }

    func testAdvertisementsRemainPauseableButDisableContentAndVolumeCommands() {
        let availability = SystemMediaCommandAvailability.make(from: snapshot(paused: true, advertisement: true))
        XCTAssertTrue(availability.play)
        XCTAssertTrue(availability.pause == false)
        XCTAssertTrue(availability.togglePlayPause)
        XCTAssertFalse(availability.next)
        XCTAssertFalse(availability.previous)
        XCTAssertFalse(availability.changePosition)
        XCTAssertTrue(availability.stop)
        XCTAssertFalse(availability.changeVolume)

        let playingAd = SystemMediaCommandAvailability.make(from: snapshot(advertisement: true))
        XCTAssertFalse(playingAd.play)
        XCTAssertTrue(playingAd.pause)
        XCTAssertTrue(playingAd.stop)
    }

    func testNoCommandsAreAvailableDuringAPlaybackTransition() {
        let availability = SystemMediaCommandAvailability.make(from: snapshot(transition: .claiming))
        XCTAssertFalse(availability.play)
        XCTAssertFalse(availability.pause)
        XCTAssertFalse(availability.togglePlayPause)
        XCTAssertFalse(availability.next)
        XCTAssertFalse(availability.changePosition)
        XCTAssertFalse(availability.stop)
    }

    #if os(macOS)
    func testEmbeddedPageMediaSessionGuardClearsMetadataAndHandlers() {
        let script = OfficialPlaybackHost.mediaSessionGuardScript
        XCTAssertTrue(script.contains("session.metadata = null"))
        XCTAssertTrue(script.contains("session.playbackState = 'none'"))
        XCTAssertTrue(script.contains("Object.defineProperty(session, 'setActionHandler'"))
        XCTAssertTrue(script.contains("setActionHandler(action, null)"))
    }
    #endif
}
