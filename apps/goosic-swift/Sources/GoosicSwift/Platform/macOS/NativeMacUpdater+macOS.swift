#if os(macOS) && !GOOSIC_PORTABLE
import Foundation
#if canImport(Sparkle)
import Sparkle
#endif

/// Sparkle is active only in a packaged app carrying a signed alpha feed and public key.
@MainActor
final class NativeMacUpdater {
    static let shared = NativeMacUpdater()

    #if canImport(Sparkle)
    private let controller = SPUStandardUpdaterController(
        startingUpdater: false,
        updaterDelegate: nil,
        userDriverDelegate: nil
    )
    #endif

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
        controller.startUpdater()
        #endif
    }

    func check() {
        #if canImport(Sparkle)
        guard isAvailable else { return }
        controller.updater.checkForUpdates()
        #endif
    }
}
#endif
