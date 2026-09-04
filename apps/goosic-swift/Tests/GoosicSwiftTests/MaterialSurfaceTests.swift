import XCTest

#if os(macOS)
import AppKit
#endif

@testable import GoosicSwift

final class MaterialSurfaceTests: XCTestCase {
    func testMacOSVersionSelection() {
        XCTAssertEqual(
            MaterialSurfacePlatformResolver.resolve(platform: .macOS, majorVersion: 26),
            .macOSLiquidGlass
        )
        XCTAssertEqual(
            MaterialSurfacePlatformResolver.resolve(platform: .macOS, majorVersion: 30),
            .macOSLiquidGlass
        )
        XCTAssertEqual(
            MaterialSurfacePlatformResolver.resolve(platform: .macOS, majorVersion: 14),
            .macOSVisualEffect
        )
        XCTAssertEqual(
            MaterialSurfacePlatformResolver.resolve(platform: .macOS, majorVersion: 25),
            .macOSVisualEffect
        )
    }

    func testUnsupportedVersionsAndPlatformsUseSafeFallbacks() {
        XCTAssertEqual(
            MaterialSurfacePlatformResolver.resolve(platform: .macOS, majorVersion: 13),
            .staticFallback
        )
        XCTAssertEqual(
            MaterialSurfacePlatformResolver.resolve(platform: .macOS, majorVersion: nil),
            .staticFallback
        )
        XCTAssertEqual(
            MaterialSurfacePlatformResolver.resolve(platform: .windows, majorVersion: 11),
            .winUIBackdrop
        )
        XCTAssertEqual(
            MaterialSurfacePlatformResolver.resolve(platform: .other, majorVersion: 99),
            .staticFallback
        )
    }

    func testEverySurfaceKindIsRepresented() {
        XCTAssertEqual(
            MaterialSurfaceKind.allCases,
            [.sidebar, .queue, .nowPlaying]
        )
    }

    #if os(macOS)
    func testAppKitHostUsesTheRuntimeMacOSBackendAndPassesThroughEvents() {
        let host = MaterialSurfaceHostView(frame: .zero)
        host.apply(kind: .sidebar)
        let runtimeVersion = ProcessInfo.processInfo.operatingSystemVersion.majorVersion
        let expected = MaterialSurfacePlatformResolver.resolve(platform: .macOS, majorVersion: runtimeVersion)

        XCTAssertEqual(host.selectedBackend, expected)
        XCTAssertNil(host.hitTest(NSPoint.zero))
    }
    #endif
}
