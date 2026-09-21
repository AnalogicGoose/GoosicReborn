import XCTest

@testable import GoosicSwift

final class AccountProfileWireTests: XCTestCase {
    func testProtocolAndAccountSnapshotDecode() throws {
        let wire = """
        {"protocolVersion":"0.3.0","requestId":"swift-1","ok":true,"payload":{"accounts":{"accounts":[{"id":"11111111-1111-1111-1111-111111111111","webkitProfileId":"22222222-2222-2222-2222-222222222222","displayName":"Ada","email":"ada@example.test"}],"activeAccountId":"11111111-1111-1111-1111-111111111111","epoch":4}}}
        """
        let response = try JSONDecoder().decode(GoosicResponse.self, from: Data(wire.utf8))
        XCTAssertEqual(response.protocolVersion, "0.3.0")
        XCTAssertEqual(response.payload?.accounts?.epoch, 4)
        XCTAssertEqual(AccountSnapshotSelection.activeAccount(in: response.payload!.accounts!)?.displayName, "Ada")
    }

    func testAccountUpsertUsesRustFieldNames() throws {
        let request = GoosicRequest(
            requestId: "swift-2",
            command: "accounts.upsert",
            payload: GoosicRequestPayload(account: GoosicAccountUpsert(
                webkitProfileId: "22222222-2222-2222-2222-222222222222",
                displayName: "Ada",
                avatarUrl: "https://example.test/avatar.jpg"
            ))
        )
        let object = try JSONSerialization.jsonObject(with: JSONEncoder().encode(request)) as! [String: Any]
        let payload = object["payload"] as! [String: Any]
        let account = payload["account"] as! [String: Any]
        XCTAssertEqual(account["webkitProfileId"] as? String, "22222222-2222-2222-2222-222222222222")
        XCTAssertEqual(account["avatarUrl"] as? String, "https://example.test/avatar.jpg")
    }

    func testStaleAccountEpochIsIgnoredButInitialPersistedStateIsAccepted() {
        XCTAssertFalse(AccountSnapshotSelection.accepts(epoch: 2, currentEpoch: 3))
        XCTAssertTrue(AccountSnapshotSelection.accepts(epoch: 0, currentEpoch: 3, initial: true))
    }

    func testGuestTransitionUsesNilCapableCompatibilityCommand() throws {
        let request = GoosicRequest(
            requestId: "swift-guest",
            command: AccountTransitionCommand.activationCommand(for: nil),
            payload: GoosicRequestPayload(generation: 9, accountId: nil)
        )
        let object = try JSONSerialization.jsonObject(with: JSONEncoder().encode(request)) as! [String: Any]
        XCTAssertEqual(object["command"] as? String, "account.change")
        XCTAssertEqual((object["payload"] as! [String: Any])["generation"] as? NSNumber, 9)
    }
}

final class AccountProfileValidationTests: XCTestCase {
    func testProfileUUIDsMustBeCanonicalAndDistinct() {
        XCTAssertTrue(AccountProfileValidation.areDistinct(
            "11111111-1111-1111-1111-111111111111",
            "22222222-2222-2222-2222-222222222222"
        ))
        XCTAssertFalse(AccountProfileValidation.areDistinct("not-a-uuid", "22222222-2222-2222-2222-222222222222"))
        XCTAssertFalse(AccountProfileValidation.areDistinct(
            "11111111-1111-1111-1111-111111111111",
            "11111111-1111-1111-1111-111111111111"
        ))
    }

    func testLoginSummaryIsBoundedAndSanitized() throws {
        let raw = try JSONEncoder().encode(AccountLoginSummary(
            displayName: "  Ada  ",
            email: "ada@example.test",
            channel: nil,
            avatarUrl: "https://example.test/avatar.jpg"
        ))
        let summary = try XCTUnwrap(AccountLoginValidation.sanitizeMetadata(raw))
        XCTAssertEqual(summary.displayName, "Ada")
        XCTAssertEqual(summary.avatarUrl, "https://example.test/avatar.jpg")
        XCTAssertNil(AccountLoginValidation.sanitizeMetadata(Data(repeating: 0x20, count: 16 * 1024 + 1)))
    }

    func testCompletionRequiresExactHTTPSOriginAndAccountMarkerIsPageSide() {
        XCTAssertTrue(AccountLoginValidation.isExactCompletionOrigin(URL(string: "https://music.youtube.com")))
        XCTAssertFalse(AccountLoginValidation.isExactCompletionOrigin(URL(string: "https://www.youtube.com")))
        XCTAssertFalse(AccountLoginValidation.isExactCompletionOrigin(URL(string: "http://music.youtube.com")))
        XCTAssertEqual(LoginCompletionDecision.from(AccountLoginSummary(displayName: "YouTube Music account", email: nil, channel: nil, avatarUrl: nil)), .wait)
        XCTAssertEqual(LoginCompletionDecision.from(AccountLoginSummary(displayName: "Ada", email: nil, channel: nil, avatarUrl: nil)), .wait)
        XCTAssertEqual(LoginCompletionDecision.from(AccountLoginSummary(displayName: "Ada", email: nil, channel: nil, avatarUrl: "http://evil.example/avatar")), .wait)
        XCTAssertNotEqual(LoginCompletionDecision.from(AccountLoginSummary(displayName: "Ada", email: "ada@example.test", channel: nil, avatarUrl: nil)), .wait)
    }

    func testPollingRejectsStaleTokensAndExpiredDeadlines() {
        let now = Date(timeIntervalSince1970: 100)
        let summary = AccountLoginSummary(displayName: "Ada", email: "ada@example.test", channel: nil, avatarUrl: nil)
        XCTAssertEqual(AccountLoginPollingDecision.decide(token: 1, activeToken: 2, now: now,
                                                          deadline: now.addingTimeInterval(30), exactOrigin: true,
                                                          summary: summary), .cancel)
        XCTAssertEqual(AccountLoginPollingDecision.decide(token: 2, activeToken: 2, now: now.addingTimeInterval(31),
                                                          deadline: now.addingTimeInterval(30), exactOrigin: true,
                                                          summary: summary), .cancel)
        XCTAssertEqual(AccountLoginPollingDecision.decide(token: 2, activeToken: 2, now: now,
                                                          deadline: now.addingTimeInterval(30), exactOrigin: false,
                                                          summary: summary), .wait)
    }

    /// No longer behind `#if os(macOS)`: the rule moved into the shared half, so the platform
    /// that reaches these hosts second is covered by the same test rather than by a copy of it.
    func testLoginNavigationAllowsOnlyMainFrameHTTPSAllowlist() {
        XCTAssertTrue(AccountLoginValidation.isAllowedLoginURL(URL(string: "https://accounts.google.com/signin")))
        XCTAssertTrue(AccountLoginValidation.isAllowedLoginURL(URL(string: "https://music.youtube.com/")))
        // Each of these was refused during a real sign-in, which is why they are covered.
        XCTAssertTrue(AccountLoginValidation.isAllowedLoginURL(URL(string: "https://accounts.youtube.com/accounts/SetSID")))
        XCTAssertTrue(AccountLoginValidation.isAllowedLoginURL(URL(string: "https://gds.google.com/web/consent")))
        XCTAssertFalse(AccountLoginValidation.isAllowedLoginURL(URL(string: "https://evil.example/")))
        XCTAssertFalse(AccountLoginValidation.isAllowedLoginURL(URL(string: "http://accounts.google.com/")))
        XCTAssertFalse(AccountLoginValidation.isAllowedLoginURL(URL(string: "about:blank")))
        XCTAssertFalse(AccountLoginValidation.isAllowedLoginURL(URL(string: "https://user:pass@accounts.google.com/")))
        XCTAssertFalse(AccountLoginValidation.isAllowedLoginURL(nil))
    }

    /// Google localises sign-in onto country domains, so the rule checks the shape of a host
    /// rather than its exact spelling. These are the shapes it must keep refusing.
    func testGoogleCountryDomainsAreAcceptedWithoutOpeningTheDoor() {
        for allowed in [
            "https://accounts.google.com.ni/signin",     // the one that stalled a real sign-in
            "https://accounts.google.es/",
            "https://accounts.google.co.uk/",
            "https://consent.google.com.mx/",
            "https://myaccount.google.de/",
        ] {
            XCTAssertTrue(
                AccountLoginValidation.isAllowedLoginURL(URL(string: allowed)), allowed
            )
        }
        for refused in [
            "https://accounts.google.com.evil.example/",  // too many labels after google
            "https://accounts.google.evil.com/",          // a label too long to be a suffix
            "https://evil.google.com/",                   // not a service this knows
            "https://accounts.notgoogle.com/",            // second label is not google
            "https://accounts.google.com.ni.evil.example/",
            "https://google.com/",                        // no service label at all
            "https://accounts.google/",                   // no suffix at all
        ] {
            XCTAssertFalse(
                AccountLoginValidation.isAllowedLoginURL(URL(string: refused)), refused
            )
        }
    }
}

final class AccountTransitionGateTests: XCTestCase {
    func testTransitionsAreBlockedDuringAdsOrAnotherTransition() {
        XCTAssertTrue(AccountTransitionGate.canStart(owner: .none, advertisement: false, transition: .idle))
        XCTAssertFalse(AccountTransitionGate.canStart(owner: .officialWebView, advertisement: true, transition: .idle))
        XCTAssertFalse(AccountTransitionGate.canStart(owner: .none, advertisement: false, transition: .releasing))
        XCTAssertEqual(AccountTransitionCommand.activationCommand(for: nil), "account.change")
        XCTAssertEqual(AccountTransitionCommand.activationCommand(for: "account"), "accounts.activate")
        XCTAssertTrue(AccountStagingLifecycle.canCommit(upsertSucceeded: true, activationSucceeded: true, rebindSucceeded: true))
        XCTAssertTrue(AccountStagingLifecycle.shouldDiscard(upsertSucceeded: true, activationSucceeded: false, rebindSucceeded: true))
        XCTAssertTrue(AccountOperationGate.canInteract(isInProgress: false))
        XCTAssertFalse(AccountOperationGate.canInteract(isInProgress: true))
    }

    func testReleaseFailureAfterUpsertRoutesToRollback() {
        // The staged transaction has durable metadata after upsert, but no activation/rebind;
        // production therefore takes the discard/remove rollback path.
        XCTAssertTrue(AccountStagingLifecycle.shouldDiscard(upsertSucceeded: true, activationSucceeded: false, rebindSucceeded: false))
        XCTAssertFalse(AccountStagingLifecycle.canCommit(upsertSucceeded: true, activationSucceeded: false, rebindSucceeded: false))
    }

    #if os(macOS)
    func testGuestAndAccountProfilesHaveIndependentIdentities() {
        let account = OfficialPlaybackProfile(identifier: UUID(uuidString: "33333333-3333-3333-3333-333333333333")!)
        XCTAssertNotEqual(OfficialPlaybackProfile.guest, account)
        XCTAssertEqual(account, OfficialPlaybackProfile(identifier: account.identifier))
    }
    #endif
}
