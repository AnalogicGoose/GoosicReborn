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

    /// The listener's update channel, `stable` or `experimental`. The authoritative setting lives
    /// in `goosic-service`, so startup waits for `syncPersistedChannel(_:)` before starting Sparkle
    /// and keeps the last confirmed value here only as a fallback if the service cannot be reached.
    private(set) var channel: String

    private static let channelKey = "GoosicUpdateChannel"
    private let defaults: UserDefaults
    private var hasStarted = false

    #if canImport(Sparkle)
    private var controller: SPUStandardUpdaterController?
    #endif

    init(defaults: UserDefaults = .standard, createController: Bool = true) {
        self.defaults = defaults
        self.channel = defaults.string(forKey: Self.channelKey) ?? "stable"
        super.init()
        #if canImport(Sparkle)
        if createController {
            controller = SPUStandardUpdaterController(
                startingUpdater: false,
                updaterDelegate: self,
                userDriverDelegate: nil
            )
        }
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
        guard !hasStarted else { return }
        #if canImport(Sparkle)
        guard isAvailable else { return }
        hasStarted = true
        controller?.startUpdater()
        #endif
    }

    func check() {
        #if canImport(Sparkle)
        guard isAvailable else { return }
        if !hasStarted { start() }
        controller?.updater.checkForUpdates()
        #endif
    }

    /// Applies the authoritative channel read from `goosic-service` and starts Sparkle once it is
    /// known, so launch checks never run on a stale cached channel or miss a first-launch upgrade.
    func syncPersistedChannel(_ channel: String) {
        let changed = channel != self.channel
        self.channel = channel
        defaults.set(channel, forKey: Self.channelKey)
        if !hasStarted {
            start()
        } else if changed {
            checkExperimentalIfIdle(channel)
        }
    }

    /// Follows the listener's choice. Joining the experimental channel looks for its newest
    /// release straight away, quietly; leaving it never downgrades.
    func setChannel(_ channel: String) {
        guard channel != self.channel else { return }
        self.channel = channel
        defaults.set(channel, forKey: Self.channelKey)
        if !hasStarted {
            start()
        } else {
            checkExperimentalIfIdle(channel)
        }
    }

    private func checkExperimentalIfIdle(_ channel: String) {
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
