import XCTest
@testable import GoosicSwift

final class NowPlayingMeshTests: XCTestCase {
    private static func image(_ colors: [(UInt8, UInt8, UInt8, UInt8)], side: Int = 48) -> [UInt8] {
        // Columns cycle through `colors`, so a colour's share of the cover is its share of the list.
        var pixels: [UInt8] = []
        for _ in 0..<side {
            for x in 0..<side {
                let (r, g, b, a) = colors[(x / 2) % colors.count]
                pixels += [r, g, b, a]
            }
        }
        return pixels
    }

    func testASingleColourCoverRepeatsItsRealColourRatherThanInventingOthers() throws {
        let palette = try XCTUnwrap(NowPlayingMesh.palette(rgba: Self.image([(200, 20, 30, 255)]), width: 48, height: 48))
        XCTAssertEqual(palette.count, 5)
        for sample in palette {
            XCTAssertEqual(sample, MeshSample(red: 200, green: 20, blue: 30, weight: 1))
        }
    }

    func testTheMostCommonColourComesFirstAndWeightsAreRelativeToIt() throws {
        let red: (UInt8, UInt8, UInt8, UInt8) = (220, 10, 10, 255)
        let blue: (UInt8, UInt8, UInt8, UInt8) = (10, 10, 220, 255)
        let palette = try XCTUnwrap(NowPlayingMesh.palette(rgba: Self.image([red, red, red, blue]), width: 48, height: 48))
        XCTAssertEqual(palette[0].red, 220)
        XCTAssertEqual(palette[0].weight, 1)
        XCTAssertEqual(palette[1].blue, 220)
        XCTAssertEqual(palette[1].weight, 1.0 / 3.0, accuracy: 0.001)
    }

    func testTransparentPixelsAreIgnoredAndAnEmptyCoverHasNoPalette() {
        XCTAssertNil(NowPlayingMesh.palette(rgba: Self.image([(255, 255, 255, 0)]), width: 48, height: 48))
        XCTAssertNil(NowPlayingMesh.palette(rgba: [], width: 48, height: 48))
    }

    func testTheSameCoverAlwaysGetsTheSameArrangement() throws {
        let palette = try XCTUnwrap(NowPlayingMesh.palette(rgba: Self.image([(200, 20, 30, 255), (20, 20, 200, 255)]), width: 48, height: 48))
        let first = NowPlayingMesh.cells(for: palette)
        XCTAssertEqual(first, NowPlayingMesh.cells(for: palette))
        XCTAssertEqual(first.count, 36)
        for cell in first {
            XCTAssertTrue((0.2...0.8).contains(cell.x))
            XCTAssertTrue((0.2...0.8).contains(cell.y))
            XCTAssertTrue((1.05...1.5).contains(cell.scale))
            XCTAssertTrue(palette.contains(cell.color), "a cell only ever paints a sampled colour")
        }
    }

    func testAlternatingProgressRunsForwardThenBack() {
        XCTAssertEqual(NowPlayingMesh.alternatingProgress(time: 0, duration: 10, delay: 0), 0, accuracy: 1e-9)
        XCTAssertEqual(NowPlayingMesh.alternatingProgress(time: 5, duration: 10, delay: 0), 0.5, accuracy: 1e-9)
        XCTAssertEqual(NowPlayingMesh.alternatingProgress(time: 12.5, duration: 10, delay: 0), 0.75, accuracy: 1e-9)
        XCTAssertEqual(NowPlayingMesh.alternatingProgress(time: 0, duration: 10, delay: -2.5), 0.25, accuracy: 1e-9)
    }

    func testTheDriftStartsAndTurnsAtItsKeyframes() {
        // A -8 s delay means the 28 s drift begins 8 s in; its first keyframe is at time -8.
        let start = NowPlayingMesh.drift(at: -8)
        XCTAssertEqual(start.scale, 1.12, accuracy: 1e-9)
        XCTAssertEqual(start.degrees, -1.5, accuracy: 1e-9)
        let middle = NowPlayingMesh.drift(at: 6)
        XCTAssertEqual(middle.scale, 1.17, accuracy: 1e-9)
    }

    func testLargerCoversAreRequestedWithoutLosingTheirModifiers() {
        XCTAssertEqual(
            NowPlayingMesh.highResolutionVariant(of: "https://lh3.googleusercontent.com/abc=w120-h120-l90-rj"),
            "https://lh3.googleusercontent.com/abc=w1080-h1080-l90-rj"
        )
        XCTAssertEqual(
            NowPlayingMesh.highResolutionVariant(of: "https://yt3.ggpht.com/abc=s120-c-k-c0x00ffffff-no-rj"),
            "https://yt3.ggpht.com/abc=s1080-c-k-c0x00ffffff-no-rj"
        )
        XCTAssertEqual(
            NowPlayingMesh.highResolutionVariant(of: "https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg"),
            "https://i.ytimg.com/vi/dQw4w9WgXcQ/maxresdefault.jpg"
        )
        XCTAssertEqual(
            NowPlayingMesh.highResolutionVariant(of: "https://lh3.googleusercontent.com/abc"),
            "https://lh3.googleusercontent.com/abc=w1080-h1080-l90-rj"
        )
        XCTAssertNil(NowPlayingMesh.highResolutionVariant(of: "https://example.test/cover.png"))
    }
}
