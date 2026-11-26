import Foundation
import GoosicShellSupportFFI

struct AccountLoginSummary: Codable, Equatable {
    var displayName: String
    var email: String?
    var channel: String?
    var avatarUrl: String?
}

struct AccountLoginResult: Equatable {
    let accountId: UUID
    let profileId: UUID
    let summary: AccountLoginSummary
}

enum LoginCompletionDecision: Equatable {
    case wait
    case accept(AccountLoginSummary)

    static func from(_ summary: AccountLoginSummary?) -> LoginCompletionDecision {
        guard let summary else { return .wait }
        // A display name is easy to obtain from a signed-out/default shell. Require a second,
        // account-specific field before accepting the page-side marker.
        let hasEmailOrChannel = [summary.email, summary.channel]
            .compactMap { $0?.trimmingCharacters(in: .whitespacesAndNewlines) }
            .contains { !$0.isEmpty }
        let hasValidAvatar = summary.avatarUrl
            .flatMap { URL(string: $0.trimmingCharacters(in: .whitespacesAndNewlines)) }
            .map { $0.scheme == "https" && $0.host != nil && $0.user == nil && $0.password == nil } ?? false
        let hasSecondaryIdentity = hasEmailOrChannel || hasValidAvatar
        guard hasSecondaryIdentity else { return .wait }
        return .accept(summary)
    }
}

enum AccountSnapshotSelection {
    static func activeAccount(in snapshot: GoosicAccountsSnapshot) -> GoosicAccountSummary? {
        guard let id = snapshot.activeAccountId else { return nil }
        return snapshot.accounts.first { $0.id == id }
    }

    static func accepts(epoch: UInt64, currentEpoch: UInt64, initial: Bool = false) -> Bool {
        initial || epoch >= currentEpoch
    }
}

enum AccountProfileValidation {
    static func isValidStableUUID(_ value: String) -> Bool {
        guard let uuid = UUID(uuidString: value) else { return false }
        return uuid.uuidString.lowercased() == value.lowercased()
    }

    static func areDistinct(_ accountId: String, _ profileId: String) -> Bool {
        isValidStableUUID(accountId) && isValidStableUUID(profileId) && accountId.lowercased() != profileId.lowercased()
    }
}

enum AccountTransitionGate {
    static func canStart(owner: GoosicOwner, advertisement: Bool, transition: PlaybackTransition) -> Bool {
        !advertisement && transition == .idle
    }
}

enum AccountOperationGate {
    static func canInteract(isInProgress: Bool) -> Bool { !isInProgress }
}

enum AccountLoginPollingDecision: Equatable {
    case wait
    case accept
    case cancel

    static func decide(token: UInt64, activeToken: UInt64, now: Date, deadline: Date,
                       exactOrigin: Bool, summary: AccountLoginSummary?) -> Self {
        guard token == activeToken, now < deadline else { return .cancel }
        guard exactOrigin else { return .wait }
        return LoginCompletionDecision.from(summary) == .wait ? .wait : .accept
    }
}

enum AccountTransitionCommand {
    static func activationCommand(for target: String?) -> String {
        target == nil ? "account.change" : "accounts.activate"
    }
}

enum AccountStagingLifecycle {
    static func canCommit(upsertSucceeded: Bool, activationSucceeded: Bool, rebindSucceeded: Bool) -> Bool {
        upsertSucceeded && activationSucceeded && rebindSucceeded
    }

    static func shouldDiscard(upsertSucceeded: Bool, activationSucceeded: Bool, rebindSucceeded: Bool) -> Bool {
        !canCommit(upsertSucceeded: upsertSucceeded, activationSucceeded: activationSucceeded, rebindSucceeded: rebindSucceeded)
    }
}

/// Pure validation shared by the AppKit login host and focused tests. It accepts only the small,
/// metadata-only object returned by the page; no cookies, headers, or URL query values cross this
/// boundary.
enum AccountLoginValidation {
    /// How long a staged sign-in may sit on one page before it is abandoned. Each navigation
    /// restarts it; a second-factor prompt can easily take a minute of the user's attention.
    static let completionTimeout: TimeInterval = goosic_login_completion_timeout_seconds()

    /// Whether the login surface may navigate its main frame to `url`.
    ///
    /// A window that follows an arbitrary redirect is a window collecting a password for
    /// somebody else, so the rule stays narrow, and it is `goosic-shell-support`'s `login` module
    /// rather than a copy here: a rule about where credentials may be typed must not have two
    /// versions that can drift.
    static func isAllowedLoginURL(_ url: URL?) -> Bool {
        guard let url else { return false }
        return goosic_login_is_allowed_url(url.absoluteString)
    }

    /// Whether `host` is one of Google's own sign-in services on one of its country domains,
    /// such as `accounts.google.com.ni`. The shape is checked rather than a list of spellings;
    /// `is_google_sign_in_host` in Rust explains what that concedes.
    static func isGoogleSignInHost(_ host: String) -> Bool {
        goosic_login_is_google_sign_in_host(host)
    }

    /// Runs in the login page as the body of an async function (`callAsyncJavaScript`), so it
    /// can open the account menu and wait for it. It returns `''` until the page itself says it
    /// is signed in and an account-specific field is visible. Every host runs the same script,
    /// so what counts as a completed sign-in cannot depend on which engine is running.
    static let completionScript = sharedText(goosic_login_completion_script())

    static func isExactCompletionOrigin(_ url: URL?) -> Bool {
        guard let url else { return false }
        return goosic_login_is_completion_origin(url.absoluteString)
    }

    /// The page's report, bounded and cleaned, or `nil` when it is too large or unreadable.
    static func sanitizeMetadata(_ data: Data) -> AccountLoginSummary? {
        guard let text = String(data: data, encoding: .utf8) else { return nil }
        return SharedRules.call("loginSummary", ["metadata": text])
    }

    static func makeResult(accountId: UUID, profileId: UUID, metadata: Data, pageURL: URL?) -> AccountLoginResult? {
        guard let text = String(data: metadata, encoding: .utf8) else { return nil }
        let answer = sharedText(goosic_login_make_result(
            accountId.uuidString, profileId.uuidString, text, pageURL?.absoluteString ?? ""
        ))
        guard !answer.isEmpty,
              let summary = try? JSONDecoder().decode(AccountLoginSummary.self, from: Data(answer.utf8))
        else { return nil }
        return AccountLoginResult(accountId: accountId, profileId: profileId, summary: summary)
    }
}
