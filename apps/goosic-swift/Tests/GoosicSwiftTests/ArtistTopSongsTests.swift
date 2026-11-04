import XCTest

@testable import GoosicSwift

/// An artist answer can carry its top songs as `tracks` and again as a list shelf. The page shows
/// them once, as Windows does.
final class ArtistTopSongsTests: XCTestCase {
    private func song(_ id: String) -> GoosicTrack {
        GoosicTrack(
            id: id, title: id, subtitle: "", artist: "", artistID: nil, album: "", albumID: nil,
            duration: "", videoID: id, explicit: false
        )
    }

    private func songs(_ id: String, _ ids: [String], isList: Bool = true) -> GoosicShelf {
        GoosicShelf(
            id: id, title: id,
            cards: ids.map { GoosicCard(id: $0, title: $0, subtitle: "", action: .play(song($0))) },
            isList: isList
        )
    }

    private func albums(_ id: String) -> GoosicShelf {
        GoosicShelf(id: id, title: id, cards: [
            GoosicCard(id: "MPRE\(id)", title: id, subtitle: "", action: .show(.album("MPRE\(id)")))
        ])
    }

    private func page(
        tracks: [String], shelves: [GoosicShelf], artist: GoosicArtistHeader? = GoosicArtistHeader()
    ) -> CatalogPageView {
        CatalogPageView(
            id: "UCartist", title: "Signal Fires", subtitle: "", shelves: shelves,
            tracks: tracks.map(song), nextCursor: nil, allTracksID: nil, truncated: false,
            artist: artist
        )
    }

    func testARepeatedTopSongShelfJoinsTheListOnce() {
        let view = page(tracks: ["a", "b"], shelves: [songs("Top songs", ["b", "c"]), albums("Albums")])
        XCTAssertEqual(view.listedTracks.map(\.id), ["a", "b", "c"])
        XCTAssertEqual(view.listedShelves.map(\.id), ["Albums"])
        XCTAssertEqual(view.playableTracks.map(\.id), ["a", "b", "c"])
    }

    func testUnrelatedSongShelvesAndCarouselsStay() {
        let view = page(tracks: ["a"], shelves: [songs("Singles", ["x", "y"]), songs("Videos", ["a"], isList: false)])
        XCTAssertEqual(view.listedTracks.map(\.id), ["a"])
        XCTAssertEqual(view.listedShelves.map(\.id), ["Singles", "Videos"])
    }

    func testOtherPagesAreUnchanged() {
        let view = page(tracks: ["a"], shelves: [songs("Top songs", ["a", "b"])], artist: nil)
        XCTAssertEqual(view.listedTracks.map(\.id), ["a"])
        XCTAssertEqual(view.listedShelves.map(\.id), ["Top songs"])
    }

    func testAContinuationThatRepeatsTheSongsIsMergedToo() {
        let first = page(tracks: ["a", "b"], shelves: [])
        let next = page(tracks: [], shelves: [songs("Top songs", ["a", "b", "c"])], artist: nil)
        let joined = first.appending(next)
        XCTAssertEqual(joined.listedTracks.map(\.id), ["a", "b", "c"])
        XCTAssertTrue(joined.listedShelves.isEmpty)
    }
}
