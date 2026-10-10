#if os(macOS) && GOOSIC_PREVIEW_NO_WEBKIT
import Foundation

@MainActor
final class AccountLoginHost {
    var onCompleted: ((AccountLoginResult, AccountLoginHost) -> Void)?
    var onCancelled: (() -> Void)?
    func start() { onCancelled?() }
    func start(reusing accountId: UUID, profileId: UUID) { onCancelled?() }
    func close() {}
    /// No staging store is ever created off macOS - `start()` cancels immediately - so promoting
    /// and discarding are both no-ops rather than unreachable paths.
    func commitPromotion() {}
    func discardStaging() {}
}
#endif
