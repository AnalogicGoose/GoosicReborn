import XCTest

@testable import GoosicSwift

final class CatalogRequestLedgerTests: XCTestCase {
    func testAnAnswerToTheOnlyOutstandingRequestIsApplied() {
        var ledger = CatalogRequestLedger()
        let ticket = ledger.issue(for: .route(.home))
        XCTAssertTrue(ledger.accepts(ticket, for: .route(.home)))
    }

    /// Two requests for one key can be outstanding at once, and the service answers catalog reads
    /// concurrently, so the first sent is not necessarily the first back.
    func testASupersededRequestIsRefusedNoMatterWhichAnswerArrivesFirst() {
        var ledger = CatalogRequestLedger()
        let first = ledger.issue(for: .route(.home))
        let second = ledger.issue(for: .route(.home))
        XCTAssertFalse(ledger.accepts(first, for: .route(.home)), "the reload replaced this request")
        XCTAssertTrue(ledger.accepts(second, for: .route(.home)))
    }

    /// The failure users actually saw: Home is asked for anonymously at launch, the account
    /// arrives, Home is asked for again signed in, and the guest answer lands last.
    func testTheGuestAnswerCannotOverwriteTheSignedInOneItRaced() {
        var ledger = CatalogRequestLedger()
        let guest = ledger.issue(for: .route(.home))
        // The accounts snapshot lands: everything asked for as nobody is now stale.
        XCTAssertEqual(ledger.invalidateAll(), [.route(.home)])
        let signedIn = ledger.issue(for: .route(.home))

        XCTAssertFalse(ledger.accepts(guest, for: .route(.home)))
        XCTAssertTrue(ledger.accepts(signedIn, for: .route(.home)))
    }

    /// Screens waiting on an answer that will now be dropped have to be named, because otherwise
    /// they sit on "Loading…" forever waiting for something nothing will deliver.
    func testInvalidationReportsEveryScreenThatWasWaiting() {
        var ledger = CatalogRequestLedger()
        _ = ledger.issue(for: .route(.home))
        _ = ledger.issue(for: .album("a"))
        _ = ledger.issue(for: .search(query: "q", filter: "songs"))

        XCTAssertEqual(
            Set(ledger.invalidateAll()),
            [.route(.home), .album("a"), .search(query: "q", filter: "songs")]
        )
        XCTAssertTrue(ledger.isEmpty)
    }

    func testKeysDoNotInvalidateEachOther() {
        var ledger = CatalogRequestLedger()
        let home = ledger.issue(for: .route(.home))
        _ = ledger.issue(for: .album("a"))
        XCTAssertTrue(ledger.accepts(home, for: .route(.home)), "another screen's request is not this one's")
    }

    func testASettledRequestIsNoLongerOutstanding() {
        var ledger = CatalogRequestLedger()
        let ticket = ledger.issue(for: .route(.home))
        ledger.retire(ticket, for: .route(.home))
        XCTAssertFalse(ledger.accepts(ticket, for: .route(.home)))
        XCTAssertTrue(ledger.isEmpty)
    }

    /// A late answer arriving after its request was replaced must not clear the replacement, or
    /// the newer answer would be refused when it arrives and the screen would never settle.
    func testALateAnswerCannotRetireTheRequestThatReplacedIt() {
        var ledger = CatalogRequestLedger()
        let first = ledger.issue(for: .route(.home))
        let second = ledger.issue(for: .route(.home))
        ledger.retire(first, for: .route(.home))
        XCTAssertTrue(ledger.accepts(second, for: .route(.home)))
    }

    /// Continuation pages are converted off the main actor before they can be appended. The
    /// ticket has to remain current across that hop; retiring it at the callback boundary makes
    /// the append reject itself and leaves the visible footer loading forever.
    func testAContinuationTicketRemainsAcceptableUntilItsAppendIsCommitted() {
        var ledger = CatalogRequestLedger()
        let ticket = ledger.issue(for: .route(.home))

        XCTAssertTrue(ledger.accepts(ticket, for: .route(.home)))
        XCTAssertTrue(ledger.accepts(ticket, for: .route(.home)), "the converted page may still append")

        ledger.retire(ticket, for: .route(.home))
        XCTAssertFalse(ledger.accepts(ticket, for: .route(.home)))
    }
}

/// A continuation that failed used to be indistinguishable from one that had not started, so the
/// row kept promising it was loading and asked again every time it scrolled back into view.
final class CatalogContinuationStateTests: XCTestCase {
    func testTheLabelSaysWhatActuallyHappened() {
        XCTAssertEqual(CatalogContinuationLabel.text(for: .idle), "Load more")
        XCTAssertEqual(CatalogContinuationLabel.text(for: .loading), "Loading more…")
        XCTAssertEqual(CatalogContinuationLabel.text(for: .failed("upstream said no")), "Try again")
    }

    /// The states must not compare equal across kinds, because "refused" being mistaken for
    /// "ready to ask" is exactly the retry loop.
    func testAFailureIsNotIdle() {
        XCTAssertNotEqual(CatalogContinuationState.failed("no"), .idle)
        XCTAssertNotEqual(CatalogContinuationState.failed("no"), .loading)
        XCTAssertEqual(CatalogContinuationState.failed("no"), .failed("no"))
    }
}

/// A page used to be cached for the life of the process, so Home showed whatever it showed at
/// launch until the app was restarted. Expiring it outright is the opposite mistake: it puts a
/// spinner over content that was already good enough to show.
final class CatalogFreshnessTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_000_000)

    func testNothingCachedMeansTheScreenWaits() {
        XCTAssertEqual(CatalogFreshness.verdict(cachedAt: nil, now: now, lifetime: 60), .load)
    }

    func testARecentPageIsServedWithoutARequest() {
        let cached = now.addingTimeInterval(-30)
        XCTAssertEqual(CatalogFreshness.verdict(cachedAt: cached, now: now, lifetime: 60), .serve)
    }

    func testAnExpiredPageIsShownAndRefreshedBehindIt() {
        let cached = now.addingTimeInterval(-61)
        XCTAssertEqual(
            CatalogFreshness.verdict(cachedAt: cached, now: now, lifetime: 60),
            .serveAndRevalidate
        )
    }

    /// The boundary belongs to the request: at exactly the lifetime the page has run out.
    func testTheLifetimeBoundaryRefreshes() {
        let cached = now.addingTimeInterval(-60)
        XCTAssertEqual(
            CatalogFreshness.verdict(cachedAt: cached, now: now, lifetime: 60),
            .serveAndRevalidate
        )
    }

    /// A timestamp in the future means the clock moved, not that the page is unusually fresh —
    /// otherwise one clock correction pins a page as current until the app restarts.
    func testAFutureTimestampRefreshesRatherThanTrustingItself() {
        let cached = now.addingTimeInterval(3_600)
        XCTAssertEqual(
            CatalogFreshness.verdict(cachedAt: cached, now: now, lifetime: 60),
            .serveAndRevalidate
        )
    }

    /// Lifetimes are chosen by how fast the thing behind the page moves. The ordering is the
    /// claim worth protecting: a library the user edits must not be staler than an album.
    func testLifetimesFollowHowFastEachKindActuallyChanges() {
        let library = CatalogFreshness.lifetime(for: .library("Playlists"))
        let home = CatalogFreshness.lifetime(for: .route(.home))
        let album = CatalogFreshness.lifetime(for: .album("a"))
        XCTAssertLessThan(library, home)
        XCTAssertLessThan(home, album)
        XCTAssertGreaterThan(library, 0)
    }
}
