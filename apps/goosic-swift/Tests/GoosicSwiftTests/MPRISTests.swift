#if os(Linux)
import XCTest

@testable import GoosicSwift

/// D-Bus rejects a malformed object path by refusing the whole property, and a panel that asked
/// for `Metadata` then shows nothing at all with no error anyone sees. A video id is not a path,
/// so the conversion is worth pinning down.
final class MPRISTrackPathTests: XCTestCase {
    private func isValidObjectPath(_ path: String) -> Bool {
        guard path.hasPrefix("/"), !path.hasSuffix("/") || path == "/" else { return false }
        let segments = path.dropFirst().split(separator: "/", omittingEmptySubsequences: false)
        guard !segments.isEmpty else { return false }
        return segments.allSatisfy { segment in
            !segment.isEmpty && segment.allSatisfy { $0.isLetter || $0.isNumber || $0 == "_" }
        }
    }

    func testNothingPlayingReportsTheStandardNoTrackPath() {
        XCTAssertEqual(SystemMediaControls.trackPath(for: nil), SystemMediaControls.noTrackPath)
        XCTAssertEqual(SystemMediaControls.trackPath(for: ""), SystemMediaControls.noTrackPath)
        XCTAssertTrue(isValidObjectPath(SystemMediaControls.noTrackPath))
    }

    func testAnOrdinaryVideoIDSurvivesUnchanged() {
        let path = SystemMediaControls.trackPath(for: "dQw4w9WgXcQ")
        XCTAssertEqual(path, "/org/goosic/track/dQw4w9WgXcQ")
        XCTAssertTrue(isValidObjectPath(path))
    }

    func testCharactersAPathCannotHoldBecomeUnderscores() {
        // Video ids may carry `-` and `_`, and neither a dash nor a slash is legal in a path.
        for videoID in ["a-b_c-1234", "../../etc", "a b/c", "----------"] {
            let path = SystemMediaControls.trackPath(for: videoID)
            XCTAssertTrue(isValidObjectPath(path), "\(videoID) produced \(path)")
        }
    }

    func testTwoDifferentTracksDoNotShareOnePath() {
        XCTAssertNotEqual(
            SystemMediaControls.trackPath(for: "dQw4w9WgXcQ"),
            SystemMediaControls.trackPath(for: "oHg5SJYRHA0")
        )
    }
}
#endif
