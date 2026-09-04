import Foundation
import SwiftCrossUI

enum GoosicRoute: String, CaseIterable, Hashable {
    case home
    case explore
    case search
    case library
    case charts
    case moodsAndGenres
    case newReleases
    case downloads
    case settings

    var title: String {
        switch self {
        case .home: return "Home"
        case .explore: return "Explore"
        case .search: return "Search"
        case .library: return "Library"
        case .charts: return "Charts"
        case .moodsAndGenres: return "Moods & genres"
        case .newReleases: return "New releases"
        case .downloads: return "Downloads"
        case .settings: return "Settings"
        }
    }

    var symbol: String {
        switch self {
        case .home: return "⌂"
        case .explore: return "✦"
        case .search: return "⌕"
        case .library: return "▣"
        case .charts: return "▥"
        case .moodsAndGenres: return "♫"
        case .newReleases: return "✚"
        case .downloads: return "⇩"
        case .settings: return "⚙"
        }
    }

    /// The `catalog.browse` route name, for routes backed by a live catalog surface.
    ///
    /// Search has its own command; Library, Downloads, and Settings are local surfaces that no
    /// guest catalog read can answer.
    var catalogRoute: String? {
        switch self {
        case .home: return "home"
        case .explore: return "explore"
        case .charts: return "charts"
        case .moodsAndGenres: return "moodsAndGenres"
        case .newReleases: return "newReleases"
        case .search, .library, .downloads, .settings: return nil
        }
    }
}

enum GoosicEntityReference: Hashable {
    case album(String)
    case artist(String)
    case playlist(String)

    var kindLabel: String {
        switch self {
        case .album: return "Album"
        case .artist: return "Artist"
        case .playlist: return "Playlist"
        }
    }
}

enum PlaybackTransition: String, Equatable {
    case idle
    case claiming
    case preparingLocal
    case releasing
}

/// A playable catalog row. Every track carries a real official-player video id; rows that are
/// not directly playable are cards, never tracks.
struct GoosicTrack: Identifiable, Hashable {
    let id: String
    let title: String
    /// The upstream descriptor, e.g. `Song • Artist • Album • 3:42`.
    let subtitle: String
    let artist: String
    let artistID: String?
    let album: String
    let albumID: String?
    let duration: String
    let videoID: String
    let explicit: Bool
    /// Optional upstream thumbnail URL used by macOS Now Playing artwork.
    let thumbnail: String?

    init(
        id: String,
        title: String,
        subtitle: String,
        artist: String,
        artistID: String?,
        album: String,
        albumID: String?,
        duration: String,
        videoID: String,
        explicit: Bool,
        thumbnail: String? = nil
    ) {
        self.id = id
        self.title = title
        self.subtitle = subtitle
        self.artist = artist
        self.artistID = artistID
        self.album = album
        self.albumID = albumID
        self.duration = duration
        self.videoID = videoID
        self.explicit = explicit
        self.thumbnail = thumbnail
    }

    /// One line under the title.
    ///
    /// Prefers the artist and album the parser resolved. Otherwise it falls back to the upstream
    /// descriptor with the parts the row already shows elsewhere removed, so a row does not read
    /// "Song • 5:21" next to its own "5:21" column.
    var secondaryText: String {
        let known = [artist, album].filter { !$0.isEmpty }
        if !known.isEmpty {
            return known.joined(separator: " · ")
        }
        let redundant: Set<String> = ["Song", "Video", duration]
        let remaining = subtitle
            .split(separator: "•")
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty && !redundant.contains($0) }
        return remaining.joined(separator: " · ")
    }
}

enum GoosicCardAction: Hashable {
    case show(GoosicEntityReference)
    case play(GoosicTrack)
}

struct GoosicCard: Identifiable, Hashable {
    let id: String
    let title: String
    let subtitle: String
    let action: GoosicCardAction?
    /// Upstream artwork URL. Fetched and cached by the shell, never rendered from the network
    /// directly, because `Image` loads its source during layout.
    let thumbnail: String?

    init(id: String, title: String, subtitle: String, action: GoosicCardAction?, thumbnail: String? = nil) {
        self.id = id
        self.title = title
        self.subtitle = subtitle
        self.action = action
        self.thumbnail = thumbnail
    }
}

struct GoosicShelf: Identifiable, Hashable {
    let id: String
    let title: String
    let cards: [GoosicCard]
}

struct GoosicQueue: Hashable {
    var tracks: [GoosicTrack]
    var currentIndex: Int

    var current: GoosicTrack? {
        guard tracks.indices.contains(currentIndex) else { return nil }
        return tracks[currentIndex]
    }
}

@MainActor
final class GoosicAppModel: SwiftCrossUI.ObservableObject {
    @SwiftCrossUI.Published var route: GoosicRoute = .home
    @SwiftCrossUI.Published var detail: GoosicEntityReference?
    @SwiftCrossUI.Published var query = ""
    @SwiftCrossUI.Published var submittedQuery = ""
    @SwiftCrossUI.Published var searchFilter: CatalogSearchFilter = .all
    @SwiftCrossUI.Published var libraryTab = "Playlists"
    @SwiftCrossUI.Published var status = "Service not connected"
    @SwiftCrossUI.Published var serviceConnected = false
    @SwiftCrossUI.Published var playbackState = GoosicPlaybackState(accountId: nil, owner: .none, generation: 0, sampleSequence: 0)
    @SwiftCrossUI.Published var queue = GoosicQueue(tracks: [], currentIndex: 0)
    @SwiftCrossUI.Published var currentTrack: GoosicTrack?
    @SwiftCrossUI.Published var isPaused = true
    @SwiftCrossUI.Published var queueVisible = false
    /// Confirmed by the player, never assumed from a request.
    @SwiftCrossUI.Published private(set) var currentTime: Double = 0
    @SwiftCrossUI.Published private(set) var duration: Double = 0
    @SwiftCrossUI.Published private(set) var volume: Double = 1
    @SwiftCrossUI.Published private(set) var isMuted = false
    @SwiftCrossUI.Published private(set) var isAdvertisement = false
    /// System media controls stay empty until the active renderer confirms one valid sample.
    @SwiftCrossUI.Published private(set) var hasConfirmedPlaybackSample = false
    @SwiftCrossUI.Published private(set) var autoplay = true
    @SwiftCrossUI.Published private(set) var legacyImportAvailable = false
    @SwiftCrossUI.Published private(set) var legacyImported = false
    @SwiftCrossUI.Published private(set) var playbackTransition: PlaybackTransition = .idle
    @SwiftCrossUI.Published private(set) var playbackTransitionToken: UInt64 = 0
    /// Serializes account/profile operations with every playback selection and control action.
    /// This is set before renderer detachment begins, so no user action can claim a new lease
    /// while a login or account promotion is between quiesce and Rust confirmation.
    @SwiftCrossUI.Published private(set) var accountOperationInProgress = false
    @SwiftCrossUI.Published var playbackLabVideoID = ""
    @SwiftCrossUI.Published private(set) var hostStatus = "No official video loaded."
    @SwiftCrossUI.Published private(set) var hostDiagnostics = "No page loaded."
    @SwiftCrossUI.Published private(set) var downloadedTracks: [GoosicDownloadedTrack] = []
    @SwiftCrossUI.Published private(set) var accounts: [GoosicAccountSummary] = []
    @SwiftCrossUI.Published private(set) var activeAccountId: String?
    @SwiftCrossUI.Published private(set) var accountSnapshotEpoch: UInt64 = 0
    @SwiftCrossUI.Published private(set) var downloadsLoading = false
    /// Every catalog page this session has requested, keyed so a late response cannot land on
    /// the wrong screen.
    @SwiftCrossUI.Published private(set) var pages: [CatalogKey: CatalogLoadState] = [:]

    private var client: GoosicServiceClient?
    /// A seek the user asked for but the player has not confirmed yet. Without this the slider
    /// snaps back to the live position between a drag and the next bridge event.
    private var pendingSeek: (position: Double, requestedAt: Date)?
    /// The video whose end has already advanced the queue, so the player's repeated `ended`
    /// polls cannot skip several tracks at once.
    private var endedVideoID: String?
    /// The preferred volume has not been pushed to this load's player yet. The page reports its
    /// own volume, so the preference is applied once per load rather than fought over.
    private var volumeAppliedForLoad = false
    /// Coalesces preference writes: a volume drag would otherwise queue a file write and a
    /// service round trip per step, on a transport that is strictly serial.
    private var pendingPreferenceSave: GoosicPreferencesPatch?
    private var preferenceSaveToken: UInt64 = 0
    let officialPlaybackHost: OfficialPlaybackHost
    let localPlaybackHost: LocalPlaybackHost
    private var systemMediaControls: SystemMediaControls?
    private var accountLoginHost: AccountLoginHost?
    private var accountTransitionToken: UInt64 = 0
    private let artwork = ArtworkCache()
    /// A radio request is outstanding. Guards against a burst of `ended` events each starting
    /// their own continuation.
    private var radioExtensionInFlight = false
    /// The track the current radio was seeded from, so a radio that itself runs out does not
    /// immediately reseed from the same song and loop.
    private var radioSeedVideoID: String?
    /// Bumped when artwork arrives. Views read it so a late thumbnail re-renders its card.
    @SwiftCrossUI.Published private(set) var artworkVersion: UInt64 = 0

    var activeAccount: GoosicAccountSummary? {
        guard let activeAccountId else { return nil }
        return accounts.first { $0.id == activeAccountId }
    }

    var activeAccountLabel: String {
        activeAccount?.displayName ?? "Guest profile"
    }

    /// The local file for a piece of artwork, or `nil` while it is still being fetched.
    ///
    /// Safe to call from a view body: it never blocks and never touches the network inline.
    func artworkFile(for remote: String?) -> URL? {
        artwork.localFile(for: remote)
    }

    init() {
        officialPlaybackHost = OfficialPlaybackHost()
        localPlaybackHost = LocalPlaybackHost()
        artwork.onArtworkLoaded = { [weak self] in
            self?.artworkVersion &+= 1
        }
        officialPlaybackHost.onEvent = { [weak self] event in
            self?.receive(event)
        }
        officialPlaybackHost.onDiagnostics = { [weak self] report in
            self?.hostDiagnostics = report
        }
        officialPlaybackHost.onPageAdvanced = { [weak self] finishedVideoID in
            self?.officialPlayerMovedOn(from: finishedVideoID)
        }
        officialPlaybackHost.onStatus = { [weak self] message in
            self?.hostStatus = message
            if self?.playbackState.owner == .officialWebView {
                self?.status = message
            }
        }
        localPlaybackHost.onEvent = { [weak self] event in
            self?.receive(event)
        }
        localPlaybackHost.onStatus = { [weak self] message in
            self?.hostStatus = message
            if self?.playbackState.owner == .localDownloadedFile {
                self?.status = message
            }
        }
        systemMediaControls = SystemMediaControls(model: self)
        updateSystemMediaControls()
    }

    // MARK: - Navigation

    func navigate(to route: GoosicRoute) {
        self.route = route
        detail = nil
        loadRoute(route)
        if route == .downloads { loadDownloads() }
        savePreferences(GoosicPreferencesPatch(lastRoute: route.rawValue))
    }

    func show(_ entity: GoosicEntityReference) {
        detail = entity
        loadEntity(entity)
    }

    func closeDetail() {
        detail = nil
    }

    func connect() {
        guard client == nil else { return }
        do {
            client = try GoosicServiceClient()
            serviceConnected = false
            status = "Connecting to Rust service…"
            send(command: "hello") { [weak self] response in
                guard let self else { return }
                self.apply(response)
                self.serviceConnected = true
                self.status = response.payload?.message ?? "Rust service connected."
                self.loadPreferences()
                self.loadAccounts()
            }
        } catch {
            status = error.localizedDescription
        }
    }

    // MARK: - Accounts

    func loadAccounts() {
        guard client != nil else { return }
        send(command: "accounts.get") { [weak self] response in
            guard let self, let snapshot = response.payload?.accounts else { return }
            self.applyAccounts(snapshot, initial: self.accountSnapshotEpoch == 0)
        } failure: { [weak self] error in
            self?.status = "Could not read account profiles: \(Self.describe(error).message)"
        }
    }

    /// Applies a Rust snapshot only if it is not older than the one already rendered. This keeps
    /// a late accounts.get response from resurrecting an account after a switch.
    private func applyAccounts(_ snapshot: GoosicAccountsSnapshot, initial: Bool = false) {
        guard AccountSnapshotSelection.accepts(epoch: snapshot.epoch, currentEpoch: accountSnapshotEpoch, initial: initial) else { return }
        accounts = snapshot.accounts
        activeAccountId = snapshot.activeAccountId
        accountSnapshotEpoch = snapshot.epoch
        guard initial else { return }
        // A persisted active account is startup state, not a user transition. Bind its profile
        // directly after accounts.get and do not manufacture a playback lease transition.
        if let profile = activeAccount?.webkitProfileId,
           let uuid = UUID(uuidString: profile),
           playbackState.owner == .none {
            officialPlaybackHost.bind(profile: OfficialPlaybackProfile(identifier: uuid))
        }
    }

    func signIn() {
        beginAccountLogin()
    }

    func beginAccountLogin() {
        guard canChangeAccount else { return }
        guard client != nil else {
            status = "Connect to the Rust service before signing in."
            return
        }
        guard beginAccountOperation() else { return }
        let priorProfile = activeAccount.flatMap { UUID(uuidString: $0.webkitProfileId) } ?? OfficialPlaybackProfile.guest.identifier
        prepareForAccountTransition(success: { [weak self] in
            guard let self else { return }
            let host = AccountLoginHost()
            self.accountLoginHost = host
            host.onCompleted = { [weak self] result, host in
                guard let self else { return }
                self.upsertAndActivate(result, host: host, priorProfile: priorProfile)
            }
            host.onCancelled = { [weak self] in
                self?.accountLoginHost = nil
                self?.officialPlaybackHost.bind(profile: OfficialPlaybackProfile(identifier: priorProfile))
                self?.finishAccountOperation()
            }
            host.start()
            self.status = "Sign in in the secure account window."
        }, failure: { [weak self] message in
            guard let self else { return }
            self.officialPlaybackHost.bind(profile: OfficialPlaybackProfile(identifier: priorProfile))
            self.finishAccountOperation()
            self.status = message
        })
    }

    func switchAccount(to id: String) {
        guard accounts.contains(where: { $0.id == id }) else { return }
        guard id != activeAccountId else { return }
        guard canChangeAccount else { return }
        guard beginAccountOperation() else { return }
        performAccountTransition(command: "accounts.activate", payload: GoosicRequestPayload(generation: playbackState.generation, accountId: id), target: id)
    }

    func signOut() {
        guard activeAccountId != nil else { return }
        guard canChangeAccount else { return }
        guard beginAccountOperation() else { return }
        performAccountTransition(command: AccountTransitionCommand.activationCommand(for: nil), payload: GoosicRequestPayload(generation: playbackState.generation, accountId: nil), target: nil)
    }

    func removeAccount(_ id: String) {
        guard accounts.contains(where: { $0.id == id }) else { return }
        guard canChangeAccount else { return }
        let target = id == activeAccountId ? nil : activeAccountId
        guard beginAccountOperation() else { return }
        performAccountTransition(command: "accounts.remove", payload: GoosicRequestPayload(generation: playbackState.generation, accountId: id), target: target, removeId: id)
    }

    private var canChangeAccount: Bool {
        guard AccountOperationGate.canInteract(isInProgress: accountOperationInProgress) else {
            status = "An account operation is already in progress."
            return false
        }
        guard AccountTransitionGate.canStart(owner: playbackState.owner, advertisement: isAdvertisement, transition: playbackTransition) else {
            if isAdvertisement {
                status = "Account changes are unavailable during an advertisement."
            } else {
            status = "Account changes are unavailable while playback is transitioning."
            }
            return false
        }
        return true
    }

    @discardableResult
    private func beginAccountOperation() -> Bool {
        guard AccountOperationGate.canInteract(isInProgress: accountOperationInProgress) else {
            status = "An account operation is already in progress."
            return false
        }
        accountOperationInProgress = true
        return true
    }

    private func finishAccountOperation() {
        accountOperationInProgress = false
    }

    private func upsertAndActivate(_ result: AccountLoginResult, host: AccountLoginHost, priorProfile: UUID) {
        guard client != nil else {
            host.discardStaging()
            officialPlaybackHost.bind(profile: OfficialPlaybackProfile(identifier: priorProfile))
            accountLoginHost = nil
            finishAccountOperation()
            status = "Could not save account profile: Rust service is unavailable."
            return
        }
        let upsert = GoosicAccountUpsert(
            id: result.accountId.uuidString.lowercased(),
            webkitProfileId: result.profileId.uuidString.lowercased(),
            displayName: result.summary.displayName,
            email: result.summary.email,
            channel: result.summary.channel,
            avatarUrl: result.summary.avatarUrl
        )
        status = "Saving account profile…"
        send(command: "accounts.upsert", payload: GoosicRequestPayload(account: upsert)) { [weak self] response in
            guard let self else { return }
            let snapshot = response.payload?.accounts
            let id = snapshot?.accounts.first(where: { $0.id == upsert.id })?.id ?? upsert.id!
            self.activateStagedAccount(id: id, host: host, priorProfile: priorProfile, snapshot: snapshot)
        } failure: { [weak self] error in
            guard let self else { return }
            host.discardStaging()
            self.officialPlaybackHost.bind(profile: OfficialPlaybackProfile(identifier: priorProfile))
            self.accountLoginHost = nil
            self.finishAccountOperation()
            self.status = "Could not save account profile: \(Self.describe(error).message)"
        }
    }

    private func activateStagedAccount(id: String, host: AccountLoginHost, priorProfile: UUID, snapshot: GoosicAccountsSnapshot?) {
        prepareForAccountTransition(success: { [weak self] in
            guard let self else { return }
            let token = self.beginAccountTransition()
            var payload = GoosicRequestPayload(accountId: id)
            payload.generation = self.playbackState.generation
            self.send(command: "accounts.activate", payload: payload) { [weak self] response in
                guard let self, self.isCurrentAccountTransition(token) else { return }
                guard response.ok else {
                    self.officialPlaybackHost.bind(profile: OfficialPlaybackProfile(identifier: priorProfile))
                    self.rollbackStagedAccount(id: id, host: host, priorProfile: priorProfile, token: token,
                                               message: "Account activation was rejected by Rust.")
                    return
                }
                self.apply(response)
                if let accounts = response.payload?.accounts ?? snapshot { self.applyAccounts(accounts) }
                let profile = self.accounts.first(where: { $0.id == id }).flatMap { UUID(uuidString: $0.webkitProfileId) }
                guard let profile else {
                    self.rollbackStagedAccount(id: id, host: host, priorProfile: priorProfile, token: token, message: "Rust activated an account without a valid profile.")
                    return
                }
                guard AccountStagingLifecycle.canCommit(upsertSucceeded: true, activationSucceeded: true, rebindSucceeded: true) else {
                    self.rollbackStagedAccount(id: id, host: host, priorProfile: priorProfile, token: token, message: "Could not commit the staged account profile.")
                    return
                }
                self.officialPlaybackHost.bind(profile: OfficialPlaybackProfile(identifier: profile))
                host.commitPromotion()
                self.accountLoginHost = nil
                self.clearAccountScopedUI()
                self.finishAccountTransition(token)
                self.finishAccountOperation()
                self.status = "Signed in to the new account."
            } failure: { [weak self] error in
                guard let self, self.isCurrentAccountTransition(token) else { return }
                self.officialPlaybackHost.bind(profile: OfficialPlaybackProfile(identifier: priorProfile))
                self.rollbackStagedAccount(id: id, host: host, priorProfile: priorProfile, token: token, message: "Account activation failed: \(Self.describe(error).message)")
            }
        }, failure: { [weak self] message in
            guard let self else { return }
            // Upsert has already persisted metadata, so a release/quiesce failure must follow
            // the same deterministic rollback path as activation failure.
            self.rollbackStagedAccount(id: id, host: host, priorProfile: priorProfile,
                                       token: self.accountTransitionToken,
                                       message: message)
        })
    }

    private func rollbackStagedAccount(id: String, host: AccountLoginHost, priorProfile: UUID, token: UInt64, message: String) {
        host.discardStaging()
        officialPlaybackHost.bind(profile: OfficialPlaybackProfile(identifier: priorProfile))
        var payload = GoosicRequestPayload(accountId: id)
        payload.generation = playbackState.generation
        send(command: "accounts.remove", payload: payload) { [weak self] response in
            guard let self, self.isCurrentAccountTransition(token) else { return }
            self.apply(response)
            if let accounts = response.payload?.accounts { self.applyAccounts(accounts) }
            self.accountLoginHost = nil
            self.finishAccountTransition(token)
            self.finishAccountOperation()
            self.status = message
        } failure: { [weak self] _ in
            guard let self, self.isCurrentAccountTransition(token) else { return }
            self.accountLoginHost = nil
            self.finishAccountTransition(token)
            self.finishAccountOperation()
            self.status = "\(message) Metadata rollback also failed; refresh accounts before retrying."
        }
    }

    private func performAccountTransition(
        command: String,
        payload: GoosicRequestPayload,
        target: String?,
        removeId: String? = nil,
        snapshot: GoosicAccountsSnapshot? = nil
    ) {
        let priorProfile = activeAccount.flatMap { UUID(uuidString: $0.webkitProfileId) } ?? OfficialPlaybackProfile.guest.identifier
        prepareForAccountTransition(success: { [weak self] in
            guard let self else { return }
            let token = self.beginAccountTransition()
            self.status = "Changing account…"
            var transitionPayload = payload
            // Releasing a lease may advance Rust's generation. Account transitions must carry
            // the generation returned by that release, never the pre-quiesce value.
            transitionPayload.generation = self.playbackState.generation
            self.send(command: command, payload: transitionPayload) { [weak self] response in
                guard let self, self.isCurrentAccountTransition(token) else { return }
                guard response.ok else {
                    self.officialPlaybackHost.bind(profile: OfficialPlaybackProfile(identifier: priorProfile))
                    self.finishAccountTransition(token)
                    self.finishAccountOperation()
                    self.status = "Account change was rejected by Rust."
                    return
                }
                self.apply(response)
                if let snapshot = response.payload?.accounts ?? snapshot {
                    self.applyAccounts(snapshot)
                } else {
                    self.loadAccounts()
                }
                // Rebind only after Rust confirms the durable transition. A failed transition
                // leaves the previous profile and its UI intact.
                let profile = target.flatMap { id in self.accounts.first(where: { $0.id == id })?.webkitProfileId }
                    .flatMap(UUID.init(uuidString:))
                    ?? OfficialPlaybackProfile.guest.identifier
                self.officialPlaybackHost.bind(profile: OfficialPlaybackProfile(identifier: profile))
                self.clearAccountScopedUI()
                self.finishAccountTransition(token)
                self.finishAccountOperation()
                self.status = target == nil ? "Signed out." : "Active account changed."
                _ = removeId
            } failure: { [weak self] error in
                guard let self, self.isCurrentAccountTransition(token) else { return }
                self.officialPlaybackHost.bind(profile: OfficialPlaybackProfile(identifier: priorProfile))
                self.finishAccountTransition(token)
                self.finishAccountOperation()
                self.status = "Account change failed: \(Self.describe(error).message)"
            }
        }, failure: { [weak self] message in
            guard let self else { return }
            self.officialPlaybackHost.bind(profile: OfficialPlaybackProfile(identifier: priorProfile))
            self.finishAccountOperation()
            self.status = message
        })
    }

    private func beginAccountTransition() -> UInt64 {
        accountTransitionToken &+= 1
        playbackTransition = .releasing
        return accountTransitionToken
    }

    private func isCurrentAccountTransition(_ token: UInt64) -> Bool { token == accountTransitionToken }

    private func finishAccountTransition(_ token: UInt64) {
        guard token == accountTransitionToken else { return }
        playbackTransition = .idle
    }

    private func clearAccountScopedUI() {
        queue = GoosicQueue(tracks: [], currentIndex: 0)
        currentTrack = nil
        detail = nil
        submittedQuery = ""
        pages.removeAll()
        beginTrack()
        hasConfirmedPlaybackSample = false
        updateSystemMediaControls()
    }

    private func prepareForAccountTransition(
        success: @escaping @MainActor () -> Void,
        failure: @escaping @MainActor (String) -> Void
    ) {
        let priorProfile = activeAccount.flatMap { UUID(uuidString: $0.webkitProfileId) } ?? OfficialPlaybackProfile.guest.identifier
        guard playbackState.owner != .none else {
            // Keep the login-only web view from coexisting with a mounted playback renderer,
            // even when Rust currently reports no lease.
            officialPlaybackHost.detach(completion: success)
            return
        }
        let token = beginPlaybackTransition(.releasing)
        hasConfirmedPlaybackSample = false
        updateSystemMediaControls()
        let release: @MainActor () -> Void = { [weak self] in
            guard let self, self.isCurrentPlaybackTransition(token, kind: .releasing) else { return }
            let owner = self.playbackState.owner
            if owner == .localDownloadedFile { self.localPlaybackHost.stop() }
            else { self.officialPlaybackHost.invalidateExpectations() }
            self.send(command: "playback.release", payload: GoosicRequestPayload(owner: owner, generation: self.playbackState.generation)) { [weak self] response in
                guard let self, self.isCurrentPlaybackTransition(token, kind: .releasing) else { return }
                self.apply(response)
                self.officialPlaybackHost.detach { [weak self] in
                    guard let self, self.isCurrentPlaybackTransition(token, kind: .releasing) else { return }
                    self.finishPlaybackTransition(token)
                    success()
                }
            } failure: { [weak self] error in
                guard let self else { return }
                self.officialPlaybackHost.bind(profile: OfficialPlaybackProfile(identifier: priorProfile))
                self.finishPlaybackTransition(token)
                failure("Could not release playback for account change: \(Self.describe(error).message)")
            }
        }
        if playbackState.owner == .officialWebView { officialPlaybackHost.quiesce { release() } }
        else { release() }
    }

    // MARK: - Preferences

    /// Reads stored preferences, applies them, and only then loads the first screen — so the
    /// app opens where it was left rather than snapping there a moment later.
    private func loadPreferences() {
        send(command: "settings.get") { [weak self] response in
            guard let self else { return }
            if let settings = response.payload?.settings {
                self.apply(settings, restoringRoute: true)
            }
            self.loadRoute(self.route)
            if self.route == .downloads { self.loadDownloads() }
        } failure: { [weak self] _ in
            guard let self else { return }
            self.loadRoute(self.route)
            if self.route == .downloads { self.loadDownloads() }
        }
    }

    private func apply(_ settings: GoosicSettings, restoringRoute: Bool) {
        volume = min(max(settings.volume, 0), 1)
        isMuted = settings.muted
        autoplay = settings.autoplay
        queueVisible = settings.queueVisible
        legacyImported = settings.importedFromLegacy
        legacyImportAvailable = settings.legacyAvailable
        if restoringRoute, let restored = GoosicRoute(rawValue: settings.lastRoute) {
            route = restored
        }
    }

    func setAutoplay(_ enabled: Bool) {
        guard allowPlaybackInteraction() else { return }
        autoplay = enabled
        savePreferences(GoosicPreferencesPatch(autoplay: enabled))
    }

    /// Imports preferences from a previous Goosic install.
    ///
    /// The legacy store is only read, never changed, and credentials are never carried over.
    func importLegacyPreferences() {
        guard legacyImportAvailable else {
            status = "No previous Goosic preferences were found on this machine."
            return
        }
        status = "Importing preferences from the previous Goosic…"
        send(command: "settings.importLegacy") { [weak self] response in
            guard let self else { return }
            if let settings = response.payload?.settings {
                self.apply(settings, restoringRoute: false)
            }
            self.status = response.payload?.message ?? "Imported preferences from the previous Goosic."
        } failure: { [weak self] error in
            guard let self else { return }
            self.status = "Could not import previous preferences: \(Self.describe(error).message)"
        }
    }

    /// Queues a preference change, coalescing rapid ones such as a volume drag.
    private func savePreferences(_ patch: GoosicPreferencesPatch) {
        pendingPreferenceSave = Self.merge(pendingPreferenceSave, patch)
        preferenceSaveToken &+= 1
        let token = preferenceSaveToken
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) { [weak self] in
            guard let self, self.preferenceSaveToken == token else { return }
            self.flushPreferences()
        }
    }

    private func flushPreferences() {
        guard let patch = pendingPreferenceSave else { return }
        pendingPreferenceSave = nil
        send(command: "settings.set", payload: GoosicRequestPayload(preferences: patch)) { [weak self] response in
            guard let self, let settings = response.payload?.settings else { return }
            self.legacyImported = settings.importedFromLegacy
            self.legacyImportAvailable = settings.legacyAvailable
        }
    }

    private static func merge(
        _ existing: GoosicPreferencesPatch?,
        _ update: GoosicPreferencesPatch
    ) -> GoosicPreferencesPatch {
        guard var merged = existing else { return update }
        merged.theme = update.theme ?? merged.theme
        merged.volume = update.volume ?? merged.volume
        merged.muted = update.muted ?? merged.muted
        merged.autoplay = update.autoplay ?? merged.autoplay
        merged.lastRoute = update.lastRoute ?? merged.lastRoute
        merged.queueVisible = update.queueVisible ?? merged.queueVisible
        return merged
    }

    // MARK: - Downloads

    func loadDownloads() {
        guard client != nil else {
            status = "Connect to the Rust service to read downloaded files."
            return
        }
        downloadsLoading = true
        send(command: "downloads.list") { [weak self] response in
            guard let self else { return }
            self.downloadsLoading = false
            self.downloadedTracks = response.payload?.downloads ?? []
        } failure: { [weak self] error in
            guard let self else { return }
            self.downloadsLoading = false
            self.status = "Could not read downloaded files: \(Self.describe(error).message)"
        }
    }

    /// Imports only finalized files already present in the previous Goosic media directory.
    /// Rust leaves those files in place and does not invoke a downloader.
    func importLegacyDownloads() {
        guard client != nil else {
            status = "Connect to the Rust service before importing downloaded files."
            return
        }
        downloadsLoading = true
        status = "Reading finalized files from the previous Goosic…"
        send(command: "downloads.importLegacy") { [weak self] response in
            guard let self else { return }
            self.downloadsLoading = false
            self.downloadedTracks = response.payload?.downloads ?? self.downloadedTracks
            self.status = response.payload?.message ?? "Imported downloaded files from the previous Goosic."
        } failure: { [weak self] error in
            guard let self else { return }
            self.downloadsLoading = false
            self.status = "Could not import downloaded files: \(Self.describe(error).message)"
        }
    }

    // MARK: - Catalog

    func state(for key: CatalogKey) -> CatalogLoadState {
        pages[key] ?? .idle
    }

    var currentSearchKey: CatalogKey {
        .search(query: submittedQuery, filter: searchFilter.protocolName)
    }

    func loadRoute(_ route: GoosicRoute, force: Bool = false) {
        guard let catalogRoute = route.catalogRoute else { return }
        loadCatalog(
            key: .route(route),
            command: "catalog.browse",
            payload: GoosicRequestPayload(query: route.title, catalogId: catalogRoute),
            force: force
        )
    }

    func loadEntity(_ entity: GoosicEntityReference, force: Bool = false) {
        let command: String
        let id: String
        switch entity {
        case .album(let value):
            command = "catalog.album"
            id = value
        case .artist(let value):
            command = "catalog.artist"
            id = value
        case .playlist(let value):
            command = "catalog.playlist"
            id = value
        }
        loadCatalog(
            key: .entity(entity),
            command: command,
            payload: GoosicRequestPayload(catalogId: id),
            force: force
        )
    }

    func search(force: Bool = false) {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            submittedQuery = ""
            status = "Enter a title, artist, or album to search."
            return
        }
        submittedQuery = trimmed
        loadCatalog(
            key: .search(query: trimmed, filter: searchFilter.protocolName),
            command: "catalog.search",
            payload: GoosicRequestPayload(query: trimmed, filter: searchFilter.protocolName),
            force: force
        )
    }

    func selectSearchFilter(_ filter: CatalogSearchFilter) {
        guard searchFilter != filter else { return }
        searchFilter = filter
        // Each filter is a separate upstream query, so switching tabs is a new request rather
        // than a client-side narrowing of the previous one.
        if !submittedQuery.isEmpty {
            loadCatalog(
                key: .search(query: submittedQuery, filter: filter.protocolName),
                command: "catalog.search",
                payload: GoosicRequestPayload(query: submittedQuery, filter: filter.protocolName),
                force: false
            )
        }
    }

    func retry(_ key: CatalogKey) {
        switch key {
        case .route(let route):
            loadRoute(route, force: true)
        case .search:
            search(force: true)
        case .album(let id):
            loadEntity(.album(id), force: true)
        case .artist(let id):
            loadEntity(.artist(id), force: true)
        case .playlist(let id):
            loadEntity(.playlist(id), force: true)
        }
    }

    private func loadCatalog(
        key: CatalogKey,
        command: String,
        payload: GoosicRequestPayload,
        force: Bool
    ) {
        if !force {
            switch state(for: key) {
            case .loading, .loaded:
                return
            case .idle, .failed:
                break
            }
        }
        guard client != nil else {
            pages[key] = .failed(
                code: "offline",
                message: "Connect to the Rust service to load the catalog."
            )
            return
        }
        pages[key] = .loading
        send(command: command, payload: payload) { [weak self] response in
            guard let self else { return }
            guard let page = response.payload?.catalog else {
                self.pages[key] = .failed(
                    code: "invalidResponse",
                    message: "The service answered without a catalog page."
                )
                return
            }
            self.pages[key] = .loaded(CatalogPageView(wire: page))
        } failure: { [weak self] error in
            guard let self else { return }
            let described = Self.describe(error)
            self.pages[key] = .failed(code: described.code, message: described.message)
        }
    }

    /// Splits a remote protocol rejection from a transport failure so the UI can say which.
    private static func describe(_ error: Error) -> (code: String, message: String) {
        if let clientError = error as? ServiceClientError, case .remote(let remote) = clientError {
            return (remote.code, remote.message)
        }
        return ("transport", error.localizedDescription)
    }

    /// What the now-playing bar shows under the title, including when there is nothing to say.
    var nowPlayingSubtitle: String {
        guard let track = currentTrack else { return "Choose a track to begin" }
        let text = track.secondaryText
        return text.isEmpty ? track.duration : text
    }

    /// A value-type view of model state consumed by the system media adapter and its tests.
    var mediaSnapshot: SystemMediaPlaybackSnapshot {
        SystemMediaPlaybackSnapshot(
            track: currentTrack,
            currentTime: currentTime,
            duration: duration,
            isPaused: isPaused,
            owner: playbackState.owner,
            isAdvertisement: isAdvertisement,
            hasQueue: !queue.tracks.isEmpty,
            transition: playbackTransition,
            volume: volume,
            isMuted: isMuted,
            isReady: hasConfirmedPlaybackSample
        )
    }

    private func updateSystemMediaControls() {
        systemMediaControls?.update(snapshot: mediaSnapshot)
    }

    /// Where the scrubber should sit: the pending seek while it is still settling, otherwise
    /// the position the player last confirmed.
    var displayedPosition: Double {
        if let pending = pendingSeek, Date().timeIntervalSince(pending.requestedAt) < Self.seekSettleWindow {
            return pending.position
        }
        return currentTime
    }

    /// The scrubber's upper bound. Zero-length media would make an empty range, so it is only
    /// ever seekable once the player has reported a real duration.
    var isSeekable: Bool {
        duration > 0 && playbackState.owner != .none && !isAdvertisement
    }

    var elapsedText: String { Self.timeText(displayedPosition) }
    var durationText: String { duration > 0 ? Self.timeText(duration) : "--:--" }

    private static let seekSettleWindow: TimeInterval = 1.0

    static func timeText(_ seconds: Double) -> String {
        guard seconds.isFinite, seconds >= 0 else { return "0:00" }
        let total = Int(seconds.rounded(.down))
        let hours = total / 3_600
        let minutes = (total % 3_600) / 60
        let secs = total % 60
        if hours > 0 {
            return String(format: "%d:%02d:%02d", hours, minutes, secs)
        }
        return String(format: "%d:%02d", minutes, secs)
    }

    // MARK: - Playback

    @discardableResult
    private func allowPlaybackInteraction() -> Bool {
        guard AccountOperationGate.canInteract(isInProgress: accountOperationInProgress) else {
            status = "Playback is temporarily unavailable while the account changes."
            return false
        }
        return true
    }

    func seek(to position: Double) {
        guard allowPlaybackInteraction() else { return }
        guard isSeekable else {
            if isAdvertisement {
                status = "Seeking is unavailable during advertisements."
            }
            return
        }
        let clamped = min(max(position, 0), duration)
        pendingSeek = (clamped, Date())
        if playbackState.owner == .localDownloadedFile {
            localPlaybackHost.seek(to: clamped)
        } else {
            officialPlaybackHost.seek(to: clamped)
        }
    }

    func setVolume(_ newVolume: Double) {
        guard allowPlaybackInteraction() else { return }
        guard !isAdvertisement else {
            status = "Volume is unchanged during advertisements."
            return
        }
        let clamped = min(max(newVolume, 0), 1)
        volume = clamped
        isMuted = false
        volumeAppliedForLoad = true
        if playbackState.owner == .localDownloadedFile {
            localPlaybackHost.setVolume(clamped)
        } else {
            officialPlaybackHost.setVolume(clamped)
        }
        savePreferences(GoosicPreferencesPatch(volume: clamped, muted: false))
    }

    func toggleMuted() {
        guard allowPlaybackInteraction() else { return }
        guard !isAdvertisement else {
            status = "Mute is unavailable during advertisements."
            return
        }
        isMuted.toggle()
        volumeAppliedForLoad = true
        if playbackState.owner == .localDownloadedFile {
            localPlaybackHost.setMuted(isMuted)
        } else {
            officialPlaybackHost.setMuted(isMuted)
        }
        savePreferences(GoosicPreferencesPatch(muted: isMuted))
    }

    func play(_ track: GoosicTrack, in tracks: [GoosicTrack] = []) {
        guard allowPlaybackInteraction() else { return }
        guard playbackTransition == .idle else {
            status = "Playback command pending; wait for Rust to finish before choosing another action."
            return
        }
        if playbackState.owner == .localDownloadedFile {
            status = "This catalog track is online. Release local playback before using officialWebView."
            return
        }
        guard !isAdvertisement else {
            status = "Track changes are unavailable while the official player is showing an advertisement."
            return
        }
        guard playbackState.owner != .localDownloadedFile else {
            status = "Playback conflict: a local downloaded file owns playback. Release it before using officialWebView."
            return
        }
        guard client != nil else {
            status = "Connect to the Rust service before playing."
            return
        }

        if !tracks.isEmpty {
            queue = GoosicQueue(tracks: tracks, currentIndex: tracks.firstIndex(of: track) ?? 0)
            // A deliberately chosen list is a fresh listening context, so the radio seed guard
            // is cleared and this queue may start its own radio when it runs out.
            radioSeedVideoID = nil
        } else {
            select(track)
        }

        if playbackState.owner == .officialWebView {
            currentTrack = track
            beginTrack()
            officialPlaybackHost.load(videoID: track.videoID, generation: playbackState.generation)
            return
        }

        let operationToken = beginPlaybackTransition(.claiming)
        status = "Requesting officialWebView playback claim…"
        send(command: "playback.claim", payload: GoosicRequestPayload(owner: .officialWebView, generation: playbackState.generation)) { [weak self] response in
            guard let self else { return }
            guard self.isCurrentPlaybackTransition(operationToken, kind: .claiming) else { return }
            self.apply(response)
            guard self.playbackState.owner == .officialWebView else {
                self.finishPlaybackTransition(operationToken)
                self.status = "Rust did not grant the officialWebView playback claim."
                return
            }
            self.currentTrack = track
            self.beginTrack()
            self.finishPlaybackTransition(operationToken)
            self.officialPlaybackHost.load(videoID: track.videoID, generation: self.playbackState.generation)
        } failure: { [weak self] _ in
            guard let self else { return }
            self.finishPlaybackTransition(operationToken)
        }
    }

    /// Plays an imported local file after Rust grants the localDownloadedFile lease and returns a
    /// decoded cache path. A catalog/official renderer is always quiesced before that claim.
    func playDownloaded(_ track: GoosicDownloadedTrack) {
        guard allowPlaybackInteraction() else { return }
        guard track.available else {
            status = "This downloaded file is missing from disk. Refresh Downloads to recheck it."
            return
        }
        guard !isAdvertisement else {
            status = "Downloaded-file playback is unavailable while the official player is showing an advertisement."
            return
        }
        guard playbackTransition == .idle else {
            status = "Playback command pending; wait for Rust to finish before choosing another action."
            return
        }
        guard client != nil else {
            status = "Connect to the Rust service before playing a downloaded file."
            return
        }
        if playbackState.owner == .localDownloadedFile {
            // A local track replacement is also a renderer switch: stop the old file before
            // asking Rust for the next decoded path, even though the owner stays local.
            localPlaybackHost.stopForReplacement()
            currentTrack = nil
            beginTrack()
            prepareLocalTrack(track, token: beginPlaybackTransition(.preparingLocal))
            return
        }
        if playbackState.owner == .officialWebView {
            hasConfirmedPlaybackSample = false
            updateSystemMediaControls()
            let token = beginPlaybackTransition(.releasing)
            status = "Quiescing official playback before switching to the local file…"
            officialPlaybackHost.quiesce { [weak self] in
                guard let self, self.isCurrentPlaybackTransition(token, kind: .releasing) else { return }
                self.officialPlaybackHost.invalidateExpectations()
                self.send(command: "playback.release", payload: GoosicRequestPayload(
                    owner: .officialWebView,
                    generation: self.playbackState.generation
                )) { [weak self] response in
                    guard let self, self.isCurrentPlaybackTransition(token, kind: .releasing) else { return }
                    self.apply(response)
                    self.currentTrack = nil
                    self.beginTrack()
                    self.finishPlaybackTransition(token)
                    self.claimLocalTrack(track)
                } failure: { [weak self] _ in
                    self?.finishPlaybackTransition(token)
                }
            }
            return
        }
        claimLocalTrack(track)
    }

    private func claimLocalTrack(_ track: GoosicDownloadedTrack) {
        let token = beginPlaybackTransition(.claiming)
        status = "Requesting localDownloadedFile playback claim from Rust…"
        send(command: "playback.claim", payload: GoosicRequestPayload(
            owner: .localDownloadedFile,
            generation: playbackState.generation
        )) { [weak self] response in
            guard let self, self.isCurrentPlaybackTransition(token, kind: .claiming) else { return }
            self.apply(response)
            guard self.playbackState.owner == .localDownloadedFile else {
                self.finishPlaybackTransition(token)
                self.status = "Rust did not grant the localDownloadedFile playback claim."
                return
            }
            self.finishPlaybackTransition(token)
            self.prepareLocalTrack(track, token: self.beginPlaybackTransition(.preparingLocal))
        } failure: { [weak self] _ in
            self?.finishPlaybackTransition(token)
        }
    }

    private func prepareLocalTrack(_ track: GoosicDownloadedTrack, token: UInt64) {
        guard playbackState.owner == .localDownloadedFile else {
            finishPlaybackTransition(token)
            status = "Local playback lease was lost before preparation."
            return
        }
        status = "Preparing decoded local audio for \(track.title)…"
        send(command: "downloads.prepare", payload: GoosicRequestPayload(
            owner: .localDownloadedFile,
            generation: playbackState.generation,
            catalogId: track.videoId
        )) { [weak self] response in
            guard let self, self.isCurrentPlaybackTransition(token, kind: .preparingLocal) else { return }
            guard let path = response.payload?.localFile, !path.isEmpty else {
                self.failLocalPlayback(token, message: "Rust did not return a decoded cache path.")
                return
            }
            do {
                try self.localPlaybackHost.prepare(localFile: path, videoID: track.videoId, generation: self.playbackState.generation)
            } catch {
                self.failLocalPlayback(token, message: "AVFoundation could not open the decoded file: \(error.localizedDescription)")
                return
            }
            self.queue = GoosicQueue(tracks: [], currentIndex: 0)
            self.currentTrack = GoosicTrack(
                id: track.videoId,
                title: track.title,
                subtitle: track.subtitle,
                artist: track.artist,
                artistID: nil,
                album: "",
                albumID: nil,
                duration: "",
                videoID: track.videoId,
                explicit: false
            )
            self.beginTrack()
            guard self.localPlaybackHost.play() else {
                self.failLocalPlayback(token, message: "AVFoundation did not confirm local playback.")
                return
            }
            self.finishPlaybackTransition(token)
            self.status = "Local playback confirmed for \(track.title)."
        } failure: { [weak self] error in
            guard let self else { return }
            self.failLocalPlayback(token, message: "Could not prepare downloaded audio: \(Self.describe(error).message)")
        }
    }

    private func failLocalPlayback(_ token: UInt64, message: String) {
        guard isCurrentPlaybackTransition(token, kind: .preparingLocal) else { return }
        localPlaybackHost.stop()
        currentTrack = nil
        beginTrack()
        send(command: "playback.release", payload: GoosicRequestPayload(
            owner: .localDownloadedFile,
            generation: playbackState.generation
        )) { [weak self] response in
            guard let self, self.isCurrentPlaybackTransition(token, kind: .preparingLocal) else { return }
            self.apply(response)
            self.finishPlaybackTransition(token)
            self.status = message
        } failure: { [weak self] _ in
            self?.finishPlaybackTransition(token)
            self?.status = message
        }
    }

    func togglePause() {
        guard allowPlaybackInteraction() else { return }
        guard playbackTransition == .idle else {
            status = "Playback command pending; pause is temporarily unavailable."
            return
        }
        if officialPlaybackHost.loadedVideoID != nil {
            if isPaused {
                playOfficialVideo()
            } else {
                pauseOfficialVideo()
            }
            return
        }
        if playbackState.owner == .localDownloadedFile, localPlaybackHost.isLoaded {
            if isPaused {
                guard localPlaybackHost.play() else { return }
                status = "Local play requested; waiting for the next confirmed sample."
            } else {
                localPlaybackHost.pause()
            }
            return
        }
        guard let track = queue.current ?? queue.tracks.first else {
            status = "Choose a track to begin."
            return
        }
        play(track)
    }

    func previous() {
        guard allowPlaybackInteraction() else { return }
        guard playbackTransition == .idle else {
            status = "Playback command pending; previous is temporarily unavailable."
            return
        }
        guard !queue.tracks.isEmpty else { return }
        guard !isAdvertisement else {
            status = "Track changes are unavailable while the official player is showing an advertisement."
            return
        }
        let index = queue.currentIndex > 0 ? queue.currentIndex - 1 : queue.tracks.count - 1
        play(queue.tracks[index])
    }

    func next() {
        guard allowPlaybackInteraction() else { return }
        guard playbackTransition == .idle else {
            status = "Playback command pending; next is temporarily unavailable."
            return
        }
        guard !queue.tracks.isEmpty else { return }
        guard !isAdvertisement else {
            status = "Track changes are unavailable while the official player is showing an advertisement."
            return
        }
        let index = (queue.currentIndex + 1) % queue.tracks.count
        play(queue.tracks[index])
    }

    func toggleQueue() {
        guard allowPlaybackInteraction() else { return }
        queueVisible.toggle()
        savePreferences(GoosicPreferencesPatch(queueVisible: queueVisible))
    }

    func releasePlayback() {
        guard allowPlaybackInteraction() else { return }
        guard playbackTransition == .idle else {
            status = "Playback command pending; release is temporarily unavailable."
            return
        }
        guard playbackState.owner != .none else {
            status = "Rust playback is already released."
            return
        }
        // Do not advertise stale media while the renderer is being quiesced and the lease
        // response is still in flight.
        hasConfirmedPlaybackSample = false
        updateSystemMediaControls()
        let operationToken = beginPlaybackTransition(.releasing)
        status = "Releasing playback…"
        if playbackState.owner == .localDownloadedFile {
            // Stop synchronously first; no timer callback may race the lease release.
            localPlaybackHost.stop()
            send(command: "playback.release", payload: GoosicRequestPayload(
                owner: .localDownloadedFile,
                generation: playbackState.generation
            )) { [weak self] response in
                guard let self, self.isCurrentPlaybackTransition(operationToken, kind: .releasing) else { return }
                self.apply(response)
                self.currentTrack = nil
                self.beginTrack()
                self.finishPlaybackTransition(operationToken)
                self.status = "Playback released by Rust authority."
            } failure: { [weak self] _ in
                self?.finishPlaybackTransition(operationToken)
            }
            return
        }
        officialPlaybackHost.quiesce { [weak self] in
            guard let self else { return }
            guard self.isCurrentPlaybackTransition(operationToken, kind: .releasing) else { return }
            self.officialPlaybackHost.invalidateExpectations()
            self.send(command: "playback.release", payload: GoosicRequestPayload(owner: self.playbackState.owner, generation: self.playbackState.generation)) { [weak self] response in
                guard let self else { return }
                guard self.isCurrentPlaybackTransition(operationToken, kind: .releasing) else { return }
                self.apply(response)
                self.currentTrack = nil
                self.beginTrack()
                self.hostStatus = "No official video loaded."
                self.finishPlaybackTransition(operationToken)
                self.status = "Playback released by Rust authority."
            } failure: { [weak self] _ in
                guard let self else { return }
                self.finishPlaybackTransition(operationToken)
            }
        }
    }

    func loadOfficialVideo() {
        guard allowPlaybackInteraction() else { return }
        guard playbackTransition == .idle else {
            status = "Playback command pending; wait for Rust to finish before loading another video."
            return
        }
        guard !isAdvertisement else {
            status = "Track changes are unavailable while the official player is showing an advertisement."
            return
        }
        let videoID = playbackLabVideoID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !videoID.isEmpty else {
            status = "Enter a real YouTube Music video ID before loading the official player."
            return
        }
        guard playbackState.owner != .localDownloadedFile else {
            status = "Playback conflict: a local downloaded file owns playback. Release it first."
            return
        }
        if playbackState.owner == .officialWebView {
            currentTrack = nil
            beginTrack()
            officialPlaybackHost.load(videoID: videoID, generation: playbackState.generation)
            return
        }
        guard client != nil else {
            status = "Connect to the Rust service before loading the official player."
            return
        }
        let operationToken = beginPlaybackTransition(.claiming)
        status = "Requesting officialWebView playback claim for Playback Lab…"
        send(command: "playback.claim", payload: GoosicRequestPayload(owner: .officialWebView, generation: playbackState.generation)) { [weak self] response in
            guard let self else { return }
            guard self.isCurrentPlaybackTransition(operationToken, kind: .claiming) else { return }
            self.apply(response)
            guard self.playbackState.owner == .officialWebView else {
                self.finishPlaybackTransition(operationToken)
                self.status = "Rust did not grant the officialWebView playback claim."
                return
            }
            self.officialPlaybackHost.load(videoID: videoID, generation: self.playbackState.generation)
            self.finishPlaybackTransition(operationToken)
        } failure: { [weak self] _ in
            guard let self else { return }
            self.finishPlaybackTransition(operationToken)
        }
    }

    func playOfficialVideo() {
        guard allowPlaybackInteraction() else { return }
        guard playbackState.owner == .officialWebView else {
            status = "Claim officialWebView through Rust before controlling the official player."
            return
        }
        guard officialPlaybackHost.loadedVideoID != nil else {
            status = "Choose a track, or load a video ID in Settings, before pressing play."
            return
        }
        officialPlaybackHost.play()
        status = "Play requested; waiting for a validated official-player event."
    }

    func pauseOfficialVideo() {
        guard allowPlaybackInteraction() else { return }
        guard playbackState.owner == .officialWebView else {
            status = "Claim officialWebView through Rust before controlling the official player."
            return
        }
        guard officialPlaybackHost.loadedVideoID != nil else {
            status = "Choose a track, or load a video ID in Settings, before pressing pause."
            return
        }
        officialPlaybackHost.pause()
        status = "Pause requested; waiting for a validated official-player event."
    }

    func stopOfficialVideo() {
        guard allowPlaybackInteraction() else { return }
        guard playbackState.owner == .officialWebView else {
            status = "Claim officialWebView through Rust before stopping it."
            return
        }
        officialPlaybackHost.quiesce { [weak self] in
            guard let self else { return }
            self.isPaused = true
            self.status = "Official media quiesced. Rust still owns the playback lease until Release."
        }
    }

    // MARK: - Transport

    private func send(command: String, payload: GoosicRequestPayload = .init(), completion: ((GoosicResponse) -> Void)? = nil, failure: ((Error) -> Void)? = nil) {
        guard let client else {
            status = "Connect to the Rust service to send \(command)."
            failure?(ServiceClientError.unavailable("goosic-service is not connected."))
            return
        }
        client.send(command: command, payload: payload) { [weak self] result in
            guard let self else { return }
            switch result {
            case .success(let response):
                if let completion {
                    completion(response)
                } else {
                    self.apply(response)
                }
            case .failure(let error):
                if let clientError = error as? ServiceClientError, case .remote = clientError {
                    // The service answered and rejected the request. The transport is healthy,
                    // so a rejected search must not drop the connection.
                    failure?(error)
                } else {
                    self.client = nil
                    self.status = error.localizedDescription
                    self.serviceConnected = false
                    failure?(error)
                }
            }
        }
    }

    private func beginPlaybackTransition(_ transition: PlaybackTransition) -> UInt64 {
        playbackTransitionToken &+= 1
        playbackTransition = transition
        updateSystemMediaControls()
        return playbackTransitionToken
    }

    private func isCurrentPlaybackTransition(_ token: UInt64, kind: PlaybackTransition) -> Bool {
        playbackTransitionToken == token && playbackTransition == kind
    }

    private func finishPlaybackTransition(_ token: UInt64) {
        guard playbackTransitionToken == token else { return }
        playbackTransition = .idle
        updateSystemMediaControls()
    }

    /// Clears everything the previous track confirmed, so no stale position or end marker is
    /// carried into the next one.
    private func beginTrack() {
        isPaused = true
        currentTime = 0
        duration = 0
        pendingSeek = nil
        endedVideoID = nil
        isAdvertisement = false
        hasConfirmedPlaybackSample = false
        volumeAppliedForLoad = false
        updateSystemMediaControls()
    }

    private func select(_ track: GoosicTrack) {
        if let index = queue.tracks.firstIndex(of: track) {
            queue.currentIndex = index
        } else {
            queue.tracks.insert(track, at: 0)
            queue.currentIndex = 0
        }
    }

    private func apply(_ response: GoosicResponse) {
        if let state = response.payload?.state {
            let previous = playbackState
            playbackState = state
            if state.owner != previous.owner
                || state.generation != previous.generation
                || state.accountId != previous.accountId
                || state.owner == .none {
                hasConfirmedPlaybackSample = false
            }
            updateSystemMediaControls()
        }
        if response.requestId.hasPrefix("swift-") && response.payload?.message != nil {
            status = response.payload?.message ?? status
        }
    }

    private func receive(_ event: OfficialPlaybackEvent) {
        guard !accountOperationInProgress else { return }
        guard playbackState.owner == .officialWebView,
              event.generation == playbackState.generation,
              event.videoID == officialPlaybackHost.loadedVideoID else {
            return
        }
        guard event.currentTime.isFinite, event.currentTime >= 0,
              event.duration.isFinite, event.duration >= 0 else {
            return
        }
        hasConfirmedPlaybackSample = true
        isPaused = event.state != "playing"
        isAdvertisement = event.isAdvertisement
        currentTime = event.currentTime
        duration = event.duration
        if !event.isAdvertisement {
            if volumeAppliedForLoad {
                volume = event.volume
                isMuted = event.isMuted
            } else if abs(event.volume - volume) > 0.01 || event.isMuted != isMuted {
                // A fresh content page starts at its own volume. Push the stored preference once,
                // then follow what the player reports. Advertisements never enter this path.
                volumeAppliedForLoad = true
                officialPlaybackHost.setVolume(volume)
                officialPlaybackHost.setMuted(isMuted)
            } else {
                volumeAppliedForLoad = true
            }
        }
        if let pending = pendingSeek,
           abs(event.currentTime - pending.position) < 1.5
            || Date().timeIntervalSince(pending.requestedAt) >= Self.seekSettleWindow {
            pendingSeek = nil
        }
        if event.state == "ended", !event.isAdvertisement, endedVideoID != event.videoID {
            endedVideoID = event.videoID
            advanceAfterEnd()
        }
        if event.isAdvertisement {
            status = "Official host confirmed advertisement playback (informational marker; ads are not bypassed)."
        } else if event.state == "playing" {
            status = "Official host confirmed media playback for \(event.videoID)."
        } else if event.state == "ended" {
            status = "Official host reported the video ended."
        } else {
            status = "Official host reported \(event.state) for \(event.videoID)."
        }
        updateSystemMediaControls()
        send(
            command: "playback.sample",
            payload: GoosicRequestPayload(
                owner: .officialWebView,
                generation: event.generation,
                sequence: event.sequence,
                marker: event.isAdvertisement ? "advertisement" : "audio"
            )
        ) { [weak self] response in
            // Rust remains authoritative for accepted sequence state, but a sample acknowledgement
            // must not overwrite the human-facing host status on every observer tick.
            self?.applyState(response)
        }
    }

    private func receive(_ event: LocalPlaybackEvent) {
        guard !accountOperationInProgress else { return }
        guard playbackState.owner == .localDownloadedFile,
              event.generation == playbackState.generation,
              event.videoID == localPlaybackHost.loadedVideoID else {
            return
        }
        guard event.currentTime.isFinite, event.currentTime >= 0,
              event.duration.isFinite, event.duration >= 0 else {
            return
        }
        hasConfirmedPlaybackSample = true
        isPaused = event.state != "playing"
        isAdvertisement = false
        currentTime = event.currentTime
        duration = event.duration
        isMuted = event.isMuted
        if !event.isMuted { volume = event.volume }
        status = event.state == "ended"
            ? "Local playback ended."
            : "Local playback confirmed \(event.state) for \(event.videoID)."
        updateSystemMediaControls()
        send(
            command: "playback.sample",
            payload: GoosicRequestPayload(
                owner: .localDownloadedFile,
                generation: event.generation,
                sequence: event.sequence,
                marker: "audio"
            )
        ) { [weak self] response in
            self?.applyState(response)
        }
        if event.state == "ended", endedVideoID != event.videoID {
            endedVideoID = event.videoID
            advanceAfterEnd()
        }
    }

    /// The official app followed its own queue. Treat the requested track as finished and let
    /// Goosic's queue decide, so the app never plays something the user did not choose.
    private func officialPlayerMovedOn(from finishedVideoID: String) {
        guard !accountOperationInProgress else { return }
        guard playbackState.owner == .officialWebView else { return }
        guard endedVideoID != finishedVideoID else { return }
        endedVideoID = finishedVideoID
        isPaused = true
        hasConfirmedPlaybackSample = false
        updateSystemMediaControls()
        advanceAfterEnd()
    }

    /// Moves to the next queued track when one finishes.
    ///
    /// Unlike `next()` this does not wrap: reaching the end of the queue stops, so a
    /// single-track queue cannot loop forever on its own `ended` event.
    private func advanceAfterEnd() {
        guard autoplay else {
            status = "Track finished. Autoplay is off."
            return
        }
        let nextIndex = queue.currentIndex + 1
        guard queue.tracks.indices.contains(nextIndex) else {
            extendWithRadio()
            return
        }
        play(queue.tracks[nextIndex])
    }

    /// Continues past the end of the queue with the radio that follows the last track.
    ///
    /// This is what the previous Goosic called "auto radio", and the imported preference maps
    /// onto `autoplay`, so a user who had it off does not suddenly get endless playback.
    private func extendWithRadio() {
        guard let seed = queue.current ?? currentTrack else {
            status = "Queue finished."
            return
        }
        // Radio is only meaningful for the official player: a local file has no upstream queue,
        // and asking for one would claim the wrong owner.
        guard playbackState.owner == .officialWebView else {
            status = "Queue finished."
            return
        }
        guard !radioExtensionInFlight, radioSeedVideoID != seed.videoID else {
            status = "Queue finished."
            return
        }
        radioExtensionInFlight = true
        radioSeedVideoID = seed.videoID
        status = "Queue finished. Starting radio from \(seed.title)…"
        requestRadio(seed: seed) { [weak self] tracks in
            guard let self else { return }
            self.radioExtensionInFlight = false
            guard let first = tracks.first else {
                self.status = "Queue finished. Radio had nothing to continue with."
                return
            }
            self.queue = GoosicQueue(tracks: [seed] + tracks, currentIndex: 0)
            self.play(first)
        }
    }

    /// Replaces the queue with the radio that follows `track` and starts it.
    func startRadio(from track: GoosicTrack) {
        guard allowPlaybackInteraction() else { return }
        guard !radioExtensionInFlight else { return }
        radioExtensionInFlight = true
        radioSeedVideoID = track.videoID
        status = "Starting radio from \(track.title)…"
        requestRadio(seed: track) { [weak self] tracks in
            guard let self else { return }
            self.radioExtensionInFlight = false
            guard let first = tracks.first else {
                self.status = "Radio had nothing to play after \(track.title)."
                return
            }
            self.queue = GoosicQueue(tracks: [track] + tracks, currentIndex: 0)
            self.play(first)
        }
    }

    private func requestRadio(seed: GoosicTrack, completion: @escaping ([GoosicTrack]) -> Void) {
        send(
            command: "catalog.radio",
            payload: GoosicRequestPayload(catalogId: seed.videoID)
        ) { [weak self] response in
            guard self != nil else { return }
            let page = response.payload?.catalog.map(CatalogPageView.init(wire:))
            completion(page?.tracks ?? [])
        } failure: { [weak self] error in
            guard let self else { return }
            self.radioExtensionInFlight = false
            self.status = "Could not start radio: \(Self.describe(error).message)"
        }
    }

    private func applyState(_ response: GoosicResponse) {
        // A sample acknowledgement can arrive after account work has detached its renderer.
        // Do not let that old lease-bound response resurrect playback UI/state.
        guard !accountOperationInProgress else { return }
        if let state = response.payload?.state {
            let previous = playbackState
            playbackState = state
            if state.owner != previous.owner
                || state.generation != previous.generation
                || state.accountId != previous.accountId
                || state.owner == .none {
                hasConfirmedPlaybackSample = false
            }
            updateSystemMediaControls()
        }
    }
}
