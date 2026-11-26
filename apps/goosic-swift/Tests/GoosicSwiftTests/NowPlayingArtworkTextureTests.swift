#if os(macOS)
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers
import XCTest
@testable import GoosicSwift

final class NowPlayingArtworkTextureTests: XCTestCase {
    func testTexturePreservesSpatialColourFromWideArtwork() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".png")
        defer { try? FileManager.default.removeItem(at: url) }

        let cover = try Self.context(width: 256, height: 144)
        cover.setFillColor(red: 1, green: 0, blue: 0, alpha: 1)
        cover.fill(CGRect(x: 0, y: 0, width: 128, height: 144))
        cover.setFillColor(red: 0, green: 0, blue: 1, alpha: 1)
        cover.fill(CGRect(x: 128, y: 0, width: 128, height: 144))
        cover.setFillColor(red: 1, green: 1, blue: 1, alpha: 1)
        cover.fill(CGRect(x: 0, y: 96, width: 256, height: 48))
        let writer = try XCTUnwrap(CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil))
        CGImageDestinationAddImage(writer, try XCTUnwrap(cover.makeImage()), nil)
        XCTAssertTrue(CGImageDestinationFinalize(writer))

        let texture = try XCTUnwrap(NowPlayingArtworkTexture.make(from: url))
        XCTAssertEqual(texture.width, 128)
        XCTAssertEqual(texture.height, 128)

        let pixels = try Self.context(width: 128, height: 128)
        pixels.draw(texture, in: CGRect(x: 0, y: 0, width: 128, height: 128))
        let rgba = try XCTUnwrap(pixels.data).assumingMemoryBound(to: UInt8.self)
        let left = (64 * 128 + 12) * 4
        let right = (64 * 128 + 115) * 4
        XCTAssertGreaterThan(rgba[left], rgba[left + 2])
        XCTAssertGreaterThan(rgba[right + 2], rgba[right])
        XCTAssertEqual(rgba[left + 3], 255)
        XCTAssertEqual(rgba[right + 3], 255)
        let brightestChannel = (0..<(128 * 128)).map { pixel in
            let index = pixel * 4
            return max(rgba[index], rgba[index + 1], rgba[index + 2])
        }.max()
        XCTAssertEqual(brightestChannel, 204)
    }

    func testMissingArtworkLeavesNoTexture() {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        XCTAssertNil(NowPlayingArtworkTexture.make(from: url))
    }

    private static func context(width: Int, height: Int) throws -> CGContext {
        try XCTUnwrap(CGContext(data: nil, width: width, height: height, bitsPerComponent: 8,
                                  bytesPerRow: width * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
    }
}
#endif
