import Foundation

/// How long a cached catalog page may be shown before it is refreshed behind the user's back.
///
/// A page was previously cached for the life of the process: navigating back to Home showed
/// whatever it showed at launch, for hours, and the only way to see anything newer was the retry
/// button on a screen that did not look broken. Expiring the cache instead is the opposite
/// mistake — it puts a spinner in front of content that was already good enough to show, and makes
/// going back somewhere feel slower than going somewhere new.
///
/// So a stale page is served *and* refreshed: the screen paints from cache immediately and quietly
/// replaces itself if a newer answer differs. The only question this type answers is which of the
/// three situations a key is in.
enum CatalogFreshness {
    enum Verdict: Equatable {
        /// Nothing usable is cached; the screen has to wait for an answer.
        case load
        /// Cached and recent enough to stand on its own. No request.
        case serve
        /// Cached, shown at once, and refreshed behind it.
        case serveAndRevalidate
    }

    /// How long each kind of page stays believable, chosen by how fast the thing behind it moves.
    ///
    /// An album's track list is effectively fixed once published, so re-reading it on every visit
    /// spends a request to learn nothing. A personal library changes because the user changed it —
    /// often in another client, and often seconds ago — so it gets the shortest life of the lot.
    /// Home and the editorial routes are refreshed upstream on the order of hours, not seconds.
    static func lifetime(for key: CatalogKey) -> TimeInterval {
        switch key {
        case .library: return 60
        case .route(.home): return 5 * 60
        case .route: return 15 * 60
        case .search: return 10 * 60
        case .album, .playlist: return 60 * 60
        case .artist: return 30 * 60
        }
    }

    static func verdict(cachedAt: Date?, now: Date = Date(), lifetime: TimeInterval) -> Verdict {
        guard let cachedAt else { return .load }
        // A timestamp in the future means the clock moved, not that the page is unusually fresh.
        // Refreshing is the safe reading, and it still shows the cached page while it happens.
        guard cachedAt <= now else { return .serveAndRevalidate }
        return now.timeIntervalSince(cachedAt) < lifetime ? .serve : .serveAndRevalidate
    }

    static func verdict(for key: CatalogKey, cachedAt: Date?, now: Date = Date()) -> Verdict {
        verdict(cachedAt: cachedAt, now: now, lifetime: lifetime(for: key))
    }
}
