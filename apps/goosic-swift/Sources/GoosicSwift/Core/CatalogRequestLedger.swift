import Foundation

/// Decides which catalog answers may still be applied.
///
/// `CatalogKey` already keeps an answer from landing on a screen the user navigated away from: it
/// lands on its own key and is simply not shown. What the key cannot decide is whether an answer is
/// still the *current* answer for its own key, and there are two ways it is not.
///
/// The first is supersession. Two requests for one key can be outstanding at once — a forced
/// reload, a retry, a continuation issued before a reload — and nothing says the first to be sent
/// is the first to come back. Now that the service answers catalog reads on a worker pool, it
/// frequently is not.
///
/// The second is identity, and it is the one users see. Home is requested anonymously at launch,
/// because the active account is not known yet; the accounts snapshot arrives a moment later and
/// Home is requested again, signed in. Whichever answer lands last wins, and the anonymous one
/// often does — so a signed-in user is shown the guest feed, and no amount of reloading fixes it,
/// because the reload is not what is wrong.
///
/// A ticket answers both. Every request takes one; an answer is applied only if its ticket is
/// still the one being waited for. Changing account invalidates every ticket outstanding, because
/// every answer in flight was asked for as somebody else.
struct CatalogRequestLedger {
    private var nextTicket: UInt64 = 0
    private var outstanding: [CatalogKey: UInt64] = [:]

    /// Registers a request for `key`, superseding any earlier one for the same key, and returns
    /// the ticket its answer must present.
    mutating func issue(for key: CatalogKey) -> UInt64 {
        nextTicket += 1
        outstanding[key] = nextTicket
        return nextTicket
    }

    /// Whether the answer holding `ticket` is still the one `key` is waiting for.
    func accepts(_ ticket: UInt64, for key: CatalogKey) -> Bool {
        outstanding[key] == ticket
    }

    /// Marks the request holding `ticket` as settled. A ticket that has already been superseded
    /// is left alone, so a late answer cannot retire the request that replaced it.
    mutating func retire(_ ticket: UInt64, for key: CatalogKey) {
        guard outstanding[key] == ticket else { return }
        outstanding.removeValue(forKey: key)
    }

    /// Every answer in flight was asked for under an identity that no longer applies.
    ///
    /// Returns the keys that were waiting, because dropping an answer silently would leave those
    /// screens on "Loading…" forever — the request they are waiting for will never be applied and
    /// nothing else would know to ask again.
    mutating func invalidateAll() -> [CatalogKey] {
        let waiting = Array(outstanding.keys)
        outstanding.removeAll()
        return waiting
    }

    var isEmpty: Bool { outstanding.isEmpty }
}
