import XCTest
@testable import GoosicSwift

/// The same cases as `goosic-shell-support`'s `hero` tests; a change to one is a change to both.
final class ArtistHeroTests: XCTestCase {
    private func bitmap(width: Int, height: Int, pixel: (Int) -> [UInt8]) -> [UInt8] {
        (0..<(width * height)).flatMap { index in pixel(index / width) + [255] }
    }

    func testTheHeroFollowsTheBannerBetweenAFloorAndACeiling() {
        XCTAssertEqual(ArtistHero.height(forWidth: 1200), 500)
        XCTAssertEqual(ArtistHero.height(forWidth: 600), ArtistHero.minHeight)
        XCTAssertEqual(ArtistHero.height(forWidth: 2400), ArtistHero.maxHeight)
    }

    func testADarkBannerKeepsItsOwnColour() {
        let navy = bitmap(width: 4, height: 4) { _ in [20, 30, 60] }
        XCTAssertEqual(ArtistHero.tint(rgba: navy, width: 4, height: 4), [20, 30, 60])
    }

    func testABrightBannerIsDarkenedUntilWhiteTextReadsOverIt() {
        let white = bitmap(width: 4, height: 4) { _ in [255, 255, 255] }
        XCTAssertEqual(ArtistHero.tint(rgba: white, width: 4, height: 4), [118, 118, 118])
    }

    func testOnlyTheFootOfTheBannerIsSampled() {
        let banner = bitmap(width: 4, height: 8) { row in row < 6 ? [255, 255, 255] : [0, 0, 0] }
        XCTAssertEqual(ArtistHero.tint(rgba: banner, width: 4, height: 8), [0, 0, 0])
    }

    func testABitmapShorterThanItClaimsHasNoTint() {
        XCTAssertNil(ArtistHero.tint(rgba: [UInt8](repeating: 0, count: 12), width: 2, height: 2))
        XCTAssertNil(ArtistHero.tint(rgba: [], width: 0, height: 0))
    }
}
