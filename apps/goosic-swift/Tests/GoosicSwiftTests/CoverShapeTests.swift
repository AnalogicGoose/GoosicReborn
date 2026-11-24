import XCTest
@testable import GoosicSwift

/// The same cases as the `cover_aspect` and `fit_cover` tests in `goosic-shell-support` and the
/// Windows `CoverShapeTests`, so a cover takes one shape on every platform.
final class CoverShapeTests: XCTestCase {
    func testCoverAspectFollowsThePictureWithinBounds() {
        XCTAssertEqual(CoverShape.aspect(pixelWidth: 544, pixelHeight: 544), 1)
        XCTAssertEqual(CoverShape.aspect(pixelWidth: 1280, pixelHeight: 720), 16.0 / 9.0, accuracy: 1e-9)
        XCTAssertEqual(CoverShape.aspect(pixelWidth: 480, pixelHeight: 360), 4.0 / 3.0, accuracy: 1e-9)
        XCTAssertEqual(CoverShape.aspect(pixelWidth: 3000, pixelHeight: 300), CoverShape.maxAspect)
        XCTAssertEqual(CoverShape.aspect(pixelWidth: 100, pixelHeight: 1000), CoverShape.minAspect)
        XCTAssertEqual(CoverShape.aspect(pixelWidth: 0, pixelHeight: 720), 1)
        XCTAssertEqual(CoverShape.aspect(pixelWidth: .nan, pixelHeight: 720), 1)
    }

    func testACoverFitsItsSlotOnTheLongerSide() {
        XCTAssertTrue(CoverShape.fit(aspect: 1, maxWidth: 520, maxHeight: 520) == (520, 520))
        let wide = CoverShape.fit(aspect: 16.0 / 9.0, maxWidth: 520, maxHeight: 520)
        XCTAssertEqual(wide.width, 520)
        XCTAssertEqual(wide.height, 292.5, accuracy: 1e-9)
        let tall = CoverShape.fit(aspect: 9.0 / 16.0, maxWidth: 520, maxHeight: 520)
        XCTAssertEqual(tall.width, 292.5, accuracy: 1e-9)
        XCTAssertEqual(tall.height, 520)
        XCTAssertTrue(CoverShape.fit(aspect: 1, maxWidth: 0, maxHeight: 0) == (0, 0))
    }
}
