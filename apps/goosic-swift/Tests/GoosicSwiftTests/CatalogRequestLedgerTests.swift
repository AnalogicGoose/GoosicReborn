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
}
