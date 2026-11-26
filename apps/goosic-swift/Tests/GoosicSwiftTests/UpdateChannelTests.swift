#if os(macOS)
import XCTest
@testable import GoosicSwift

/// The same cases as `only_experimental_reads_the_experimental_feed_channel` in
/// `goosic-shell-support`'s `updates` module, asked through the rules library.
final class UpdateChannelTests: XCTestCase {
    func testOnlyExperimentalReadsTheExperimentalFeedChannel() {
        XCTAssertEqual(SharedRules.updateChannels("stable"), [])
        XCTAssertEqual(SharedRules.updateChannels("experimental"), ["experimental"])
        XCTAssertEqual(SharedRules.updateChannels("nonsense"), [])
    }
}
#endif
