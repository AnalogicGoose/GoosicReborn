#if os(macOS)
import AppKit
#if canImport(Sparkle)
import Sparkle
#endif

/// Sparkle is active only in a packaged app carrying a signed alpha feed and public key.
///
/// The feed holds stable items and experimental ones, which carry a `sparkle:channel`. Which
/// channels this copy may install from is `goosic-shell-support`'s `updates` rule, the one the
/// Windows updater asks too; Sparkle skips every item on a channel that is not allowed.
@MainActor
final class NativeMacUpdater: NSObject {
    static let shared = NativeMacUpdater()

    /// The listener's update channel, `stable` or `experimental`. The setting lives in the
    /// service, which answers after Sparkle has started and may already be checking; the last
    /// choice is kept here too, so the launch check reads the right channel.
    private(set) var channel = UserDefaults.standard.string(forKey: NativeMacUpdater.channelKey) ?? "stable"

    private static let channelKey = "GoosicUpdateChannel"

    #if canImport(Sparkle)
    private var controller: SPUStandardUpdaterController?
    #endif

    override private init() {
        super.init()
        #if canImport(Sparkle)
        controller = SPUStandardUpdaterController(
            startingUpdater: false,
            updaterDelegate: self,
            userDriverDelegate: nil
        )
        #endif
    }

    var isAvailable: Bool {
        #if canImport(Sparkle)
        let info = Bundle.main.infoDictionary ?? [:]
        return (info["SUFeedURL"] as? String)?.hasPrefix("https://") == true
            && (info["SUPublicEDKey"] as? String)?.isEmpty == false
        #else
        return false
        #endif
    }

    func start() {
        #if canImport(Sparkle)
        guard isAvailable else { return }
        controller?.startUpdater()
        #endif
    }

    func check() {
        #if canImport(Sparkle)
        guard isAvailable else { return }
        controller?.updater.checkForUpdates()
        #endif
    }

    /// Follows the listener's choice. Joining the experimental channel looks for its newest
    /// release straight away, quietly; leaving it never downgrades.
    func setChannel(_ channel: String) {
        guard channel != self.channel else { return }
        self.channel = channel
        UserDefaults.standard.set(channel, forKey: Self.channelKey)
        #if canImport(Sparkle)
        guard isAvailable, channel == "experimental", let updater = controller?.updater,
              updater.sessionInProgress == false else { return }
        updater.checkForUpdatesInBackground()
        #endif
    }
}

#if canImport(Sparkle)
extension NativeMacUpdater: @preconcurrency SPUUpdaterDelegate {
    func allowedChannels(for updater: SPUUpdater) -> Set<String> {
        Set(SharedRules.updateChannels(channel))
    }
}
#endif
#endif
