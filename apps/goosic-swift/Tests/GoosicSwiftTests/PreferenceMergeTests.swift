import XCTest
@testable import GoosicSwift

final class PreferenceMergeTests: XCTestCase {
    func testPendingVolumeKeepsSubsequentShuffleAndRepeatChanges() async {
        let (shuffled, repeated) = await MainActor.run {
            let pending = GoosicPreferencesPatch(volume: 0.25)
            let shuffled = GoosicAppModel.merge(
                pending,
                GoosicPreferencesPatch(shuffle: true)
            )
            let repeated = GoosicAppModel.merge(
                shuffled,
                GoosicPreferencesPatch(volume: 0.5, repeatMode: "all")
            )
            return (shuffled, repeated)
        }

        XCTAssertEqual(shuffled.volume, 0.25)
        XCTAssertEqual(shuffled.shuffle, true)

        XCTAssertEqual(repeated.volume, 0.5, "A newer value should win for the same field.")
        XCTAssertEqual(repeated.shuffle, true)
        XCTAssertEqual(repeated.repeatMode, "all")
    }

    func testMergeRetainsEveryExistingFieldWhenTheUpdateDoesNotSetIt() async {
        let merged = await MainActor.run {
            let existing = GoosicPreferencesPatch(
                theme: "dark",
                volume: 0.4,
                muted: true,
                autoplay: false,
                lastRoute: "library",
                queueVisible: true,
                shuffle: true,
                repeatMode: "one"
            )
            return GoosicAppModel.merge(existing, GoosicPreferencesPatch(lastRoute: "home"))
        }

        XCTAssertEqual(merged.theme, "dark")
        XCTAssertEqual(merged.volume, 0.4)
        XCTAssertEqual(merged.muted, true)
        XCTAssertEqual(merged.autoplay, false)
        XCTAssertEqual(merged.lastRoute, "home")
        XCTAssertEqual(merged.queueVisible, true)
        XCTAssertEqual(merged.shuffle, true)
        XCTAssertEqual(merged.repeatMode, "one")
    }
}
