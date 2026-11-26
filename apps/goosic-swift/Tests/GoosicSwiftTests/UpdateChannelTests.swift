#if os(macOS)
import Foundation
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

    func testEarlyManualChecksWaitForChannelOrFallback() async {
        await MainActor.run {
            let suite = "com.goosic.tests.update-channel." + UUID().uuidString
            let defaults = UserDefaults(suiteName: suite)!
            defer { defaults.removePersistentDomain(forName: suite) }
            for useFallback in [false, true] {
                defaults.set("experimental", forKey: "GoosicUpdateChannel")
                let updater = NativeMacUpdater(defaults: defaults, createController: false)
                updater.check()
                updater.check()
                XCTAssertTrue(updater.manualCheckPending)
                if useFallback {
                    updater.start()
                } else {
                    updater.syncPersistedChannel("stable")
                }
                XCTAssertEqual(updater.channel, useFallback ? "experimental" : "stable")
                XCTAssertFalse(updater.manualCheckPending)
                updater.check()
                XCTAssertFalse(updater.manualCheckPending)
            }
        }
    }

    func testPersistedChannelReconcilesStaleCachedExperimentalBackToStable() async {
        let suiteName = "com.goosic.tests.update-channel.\(UUID().uuidString)"
        let (initialChannel, reconciledChannel, storedChannel) = await MainActor.run { () -> (String, String, String?) in
            let defaults = UserDefaults(suiteName: suiteName)!
            defaults.set("experimental", forKey: "GoosicUpdateChannel")
            let updater = NativeMacUpdater(defaults: defaults, createController: false)
            let initial = updater.channel
            updater.syncPersistedChannel("stable")
            let reconciled = updater.channel
            let stored = defaults.string(forKey: "GoosicUpdateChannel")
            defaults.removePersistentDomain(forName: suiteName)
            return (initial, reconciled, stored)
        }
        XCTAssertEqual(initialChannel, "experimental")
        XCTAssertEqual(reconciledChannel, "stable")
        XCTAssertEqual(storedChannel, "stable")
    }

    func testPersistedExperimentalAppliesOnFirstLaunchWithoutCachedDefault() async {
        let suiteName = "com.goosic.tests.update-channel.\(UUID().uuidString)"
        let (initialChannel, syncedChannel, storedChannel) = await MainActor.run { () -> (String, String, String?) in
            let defaults = UserDefaults(suiteName: suiteName)!
            defaults.removePersistentDomain(forName: suiteName)
            let updater = NativeMacUpdater(defaults: defaults, createController: false)
            let initial = updater.channel
            updater.syncPersistedChannel("experimental")
            let synced = updater.channel
            let stored = defaults.string(forKey: "GoosicUpdateChannel")
            defaults.removePersistentDomain(forName: suiteName)
            return (initial, synced, stored)
        }
        XCTAssertEqual(initialChannel, "stable")
        XCTAssertEqual(syncedChannel, "experimental")
        XCTAssertEqual(storedChannel, "experimental")
    }
}
#endif
