#if os(macOS)
import XCTest
@testable import GoosicSwift

/// The same cases as `only_experimental_reads_the_experimental_feed_channel` in
/// `goosic-shell-support`'s `updates` module.
final class UpdateChannelTests: XCTestCase {
    func testOnlyExperimentalReadsTheExperimentalFeedChannel() {
        XCTAssertEqual(NativeMacUpdater.sparkleChannels(for: "stable"), [])
        XCTAssertEqual(NativeMacUpdater.sparkleChannels(for: "experimental"), ["experimental"])
        XCTAssertEqual(NativeMacUpdater.sparkleChannels(for: "nonsense"), [])
    }
}
#endif
