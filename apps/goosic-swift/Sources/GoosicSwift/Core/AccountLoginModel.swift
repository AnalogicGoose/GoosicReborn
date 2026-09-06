import Foundation

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
    static let maxMetadataBytes = 16 * 1024
    static let maxDisplayNameBytes = 128
    static let maxEmailBytes = 320
    static let maxChannelBytes = 128
    static let maxAvatarURLBytes = 2_048

    static func isExactCompletionOrigin(_ url: URL?) -> Bool {
        guard let url else { return false }
        return url.scheme == "https" && url.host == "music.youtube.com" &&
            (url.port == nil || url.port == 443) && url.user == nil && url.password == nil
    }

    static func sanitizeMetadata(_ data: Data) -> AccountLoginSummary? {
        guard data.count <= maxMetadataBytes,
              let raw = try? JSONDecoder().decode(AccountLoginSummary.self, from: data) else { return nil }
        let display = clean(raw.displayName, maxBytes: maxDisplayNameBytes) ?? "YouTube Music account"
        let email = clean(raw.email, maxBytes: maxEmailBytes)
        let channel = clean(raw.channel, maxBytes: maxChannelBytes)
        let avatar = cleanAvatar(raw.avatarUrl)
        return AccountLoginSummary(displayName: display, email: email, channel: channel, avatarUrl: avatar)
    }

    static func makeResult(accountId: UUID, profileId: UUID, metadata: Data, pageURL: URL?) -> AccountLoginResult? {
        guard accountId != profileId, isExactCompletionOrigin(pageURL),
              let summary = sanitizeMetadata(metadata) else { return nil }
        guard case .accept = LoginCompletionDecision.from(summary) else { return nil }
        return AccountLoginResult(accountId: accountId, profileId: profileId, summary: summary)
    }

    private static func clean(_ value: String?, maxBytes: Int) -> String? {
        guard let value else { return nil }
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, trimmed.utf8.count <= maxBytes,
              !trimmed.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }) else { return nil }
        return trimmed
    }

    private static func cleanAvatar(_ value: String?) -> String? {
        guard let value = clean(value, maxBytes: maxAvatarURLBytes),
              let url = URL(string: value), url.scheme == "https", url.user == nil,
              url.password == nil, url.host != nil else { return nil }
        return url.absoluteString
    }
}
