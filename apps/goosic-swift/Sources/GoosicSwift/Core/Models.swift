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

/// What happens when the queue reaches its end.
enum RepeatMode: String, CaseIterable, Equatable {
    case off
    case all
    case one

    /// The next mode in the cycle a single button steps through.
    var next: RepeatMode {
        switch self {
        case .off: return .all
        case .all: return .one
        case .one: return .off
        }
    }

    var label: String {
        switch self {
        case .off: return "Repeat off"
        case .all: return "Repeat all"
        case .one: return "Repeat one"
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

/// A radio station is an ordered upstream queue, not a sequence of unrelated searches. Its
/// continuation remains tied to the song that started the station, so recommendations stay in
/// the listener's chosen context as the queue grows.
private struct GoosicRadioStation: Hashable {
    let seedVideoID: String
    var continuation: String?
    let accountID: String?
}

@MainActor
final class GoosicAppModel: SwiftCrossUI.ObservableObject {
    /// Test-only fixture state is selected at launch and never travels over the service wire.
    let usesDebugSidebarFixture: Bool
    @SwiftCrossUI.Published var route: GoosicRoute = .home
    /// Whether the user has chosen a screen since launch. Restoring the last route is startup
    /// state, and startup stops being true the moment somebody navigates.
    private var hasNavigatedSinceLaunch = false
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
    @SwiftCrossUI.Published private(set) var shuffle = false
    @SwiftCrossUI.Published private(set) var repeatMode: RepeatMode = .off
    @SwiftCrossUI.Published private(set) var theme: GoosicTheme = .system
    /// Draw the playing track's artwork, blurred, behind the content. Only macOS can draw it;
    /// elsewhere the choice is stored and has no effect.
    @SwiftCrossUI.Published private(set) var artworkBackground = true
    @SwiftCrossUI.Published var lyricsVisible = false
    /// The immersive full-screen player is showing. It displays lyrics, so it asks for them the
    /// way the lyrics panel does.
    @SwiftCrossUI.Published private(set) var fullPlayerOpen = false
    @SwiftCrossUI.Published private(set) var lyrics: GoosicLyrics?
    @SwiftCrossUI.Published private(set) var lyricsStatus = "Nothing playing."
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
    /// Which catalog answers may still be applied. See `CatalogRequestLedger`: a key says which
    /// screen an answer belongs to, and this says whether it is still that screen's answer.
    private var catalogRequests = CatalogRequestLedger()
    @SwiftCrossUI.Published private(set) var continuations: [CatalogKey: CatalogContinuationState] = [:]

    func continuationState(for key: CatalogKey) -> CatalogContinuationState {
        continuations[key] ?? .idle
    }

    /// When each cached page was last confirmed fresh. See `CatalogFreshness`.
    private var pageCachedAt: [CatalogKey: Date] = [:]

    /// Which reader produced each page, so its continuation goes back to the same one. See
    /// `CatalogPageSource`.
    private var pageSources: [CatalogKey: CatalogPageSource] = [:]

    /// The playlists this account owns, for the "Add to playlist" destinations. Loaded on
    /// demand, because most sessions never open the menu.
    @SwiftCrossUI.Published private(set) var userPlaylists: [PersonalPlaylistSummary] = []
    @SwiftCrossUI.Published private(set) var userPlaylistsState: CatalogContinuationState = .idle
    /// A change the account has been asked to make and has not yet confirmed. The menu closes
    /// immediately, so this is what lets a failure be reported after the fact.
    @SwiftCrossUI.Published private(set) var libraryOperationInProgress = false

    /// Pages being shown from cache whose refresh did not succeed. The page is still worth
    /// showing; the screen just should not imply it is current.
    @SwiftCrossUI.Published private(set) var staleRefreshFailed: Set<CatalogKey> = []

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
    /// A volume pushed into the renderer that it has not echoed back yet. While this is set, the
    /// volume the renderer reports describes the past. See `VolumeSync`.
    private var requestedVolume: Double?
    private var requestedMuted: Bool?
    /// Coalesces preference writes: a volume drag would otherwise queue a file write and a
    /// service round trip per step, on a transport that is strictly serial.
    private var pendingPreferenceSave: GoosicPreferencesPatch?
    private var preferenceSaveToken: UInt64 = 0
    let officialPlaybackHost: OfficialPlaybackHost
    let localPlaybackHost: LocalPlaybackHost
    let personalCatalogHost: PersonalCatalogHost
    private var systemMediaControls: SystemMediaControls?
    private var accountLoginHost: AccountLoginHost?
    private var accountTransitionToken: UInt64 = 0
    private let artwork = ArtworkCache()
    /// A radio request is outstanding. Guards against a burst of `ended` events each starting
    /// their own continuation.
    private var radioExtensionInFlight = false
    /// The station that owns autoplay recommendations. Clearing this is a deliberate new queue
    /// context; preserving it lets `catalog.radio` page the same station instead of drifting.
    private var radioStation: GoosicRadioStation?
    /// Bumped whenever a new listening context supersedes an in-flight radio request. A late
    /// response must never append its recommendations to the user's replacement queue.
    private var radioRequestRevision: UInt64 = 0
    /// The track the loaded lyrics belong to, so a stale answer cannot land on a new song.
    private var lyricsVideoID: String?
    private var lyricsRequestInFlight = false
    /// Playback bridges report twice a second. Remembering the last presentation state lets us
    /// avoid publishing an identical status/control tree on every poll.
    private var lastOfficialEventState: String?
    private var lastOfficialEventWasAdvertisement: Bool?
    private var lastLocalEventState: String?
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

    init(debugSidebarFixture: Bool = false) {
        usesDebugSidebarFixture = debugSidebarFixture
        officialPlaybackHost = OfficialPlaybackHost()
        localPlaybackHost = LocalPlaybackHost()
        personalCatalogHost = PersonalCatalogHost()
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
        if debugSidebarFixture { installDebugSidebarFixture() }
    }

    /// A dense, local-only sidebar makes visual and accessibility regressions reproducible
    /// without a YouTube request, account login, or Rust child process.
    private func installDebugSidebarFixture() {
        let accountID = "00000000-0000-0000-0000-000000000001"
        accounts = [GoosicAccountSummary(
            id: accountID,
            webkitProfileId: "00000000-0000-0000-0000-000000000002",
            displayName: "UI Test Account",
            email: "ui-test@example.invalid"
        )]
        activeAccountId = accountID
        serviceConnected = true
        status = "UI test fixture"
        userPlaylists = (1...40).map { index in
            PersonalPlaylistSummary(
                id: "ui-test-playlist-\(index)",
                title: String(format: "Fixture playlist %02d", index),
                subtitle: "UI test data",
                thumbnail: nil
            )
        }
        userPlaylistsState = .idle
    }

    // MARK: - Navigation

    func navigate(to route: GoosicRoute) {
        // A second click on the selected tab used to republish the whole model, rebuild the
        // catalog tree, write preferences, and refresh Downloads. On a dense page that was
        // enough to trigger macOS's beach ball even though nothing had changed.
        guard self.route != route || detail != nil else { return }
        hasNavigatedSinceLaunch = true
        self.route = route
        detail = nil
        loadRoute(route)
        if route == .downloads { loadDownloads() }
        savePreferences(GoosicPreferencesPatch(lastRoute: route.rawValue))
    }

    func show(_ entity: GoosicEntityReference) {
        hasNavigatedSinceLaunch = true
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
        if activeAccountId != snapshot.activeAccountId {
            radioRequestRevision &+= 1
            radioExtensionInFlight = false
            radioStation = nil
        }
        accounts = snapshot.accounts
        activeAccountId = snapshot.activeAccountId
        accountSnapshotEpoch = snapshot.epoch
        let personalProfile = activeAccount.flatMap { UUID(uuidString: $0.webkitProfileId) }
        personalCatalogHost.bind(profileIdentifier: personalProfile)
        pages = pages.filter { key, _ in
            if case .library = key { return false }
            return true
        }
        // Every catalog answer still in flight was asked for as whoever was signed in a moment
        // ago. Home is the one that bites: it is requested anonymously at launch because the
        // active account is not known yet, and the signed-in request below races the guest answer
        // that is already on its way. Without this the guest feed frequently wins, and reloading
        // does not help because the reload is not what was wrong.
        pageCachedAt.removeAll()
        pageSources.removeAll()
        staleRefreshFailed.removeAll()
        for waiting in catalogRequests.invalidateAll() {
            // A dropped answer leaves its screen on "Loading…" with nothing coming, so anything
            // that was waiting goes back to idle and is asked for again when it is next shown.
            continuations.removeValue(forKey: waiting)
            if case .loading = state(for: waiting) { pages[waiting] = .idle }
        }
        if activeAccount != nil {
            switch route {
            case .library:
                loadLibrary(section: PersonalLibrarySection(rawValue: libraryTab) ?? .playlists)
            case .home:
                // Home may already hold the guest feed from before the account was known; the
                // signed-in feed replaces it rather than continuing it.
                loadPersonalHome(force: true)
            default:
                break
            }
        } else {
            // Signing out is the same problem in reverse: what is on screen belongs to an account
            // that is no longer active, so the visible route is asked for again anonymously.
            loadRoute(route, force: true)
        }
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
        radioRequestRevision &+= 1
        radioExtensionInFlight = false
        radioStation = nil
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

    // MARK: - Lyrics

    /// The lyric line to highlight at the current position, or `nil` when nothing should be.
    var activeLyricIndex: Int? {
        guard let lyrics else { return nil }
        return Self.activeLyricIndex(
            lines: lyrics.lines,
            synced: lyrics.synced,
            positionMs: Int64(displayedPosition * 1_000)
        )
    }

    /// Which line is current at `positionMs`.
    ///
    /// The shell owns this rather than Rust: it is the side that knows the playback position
    /// moment to moment, and one implementation cannot drift from another.
    ///
    /// Unsynced lyrics never highlight, because guessing a position would be worse than showing
    /// the words plainly, and nothing is highlighted before the first line begins.
    static func activeLyricIndex(
        lines: [GoosicLyricsLine],
        synced: Bool,
        positionMs: Int64
    ) -> Int? {
        guard synced, let first = lines.first, first.atMs <= positionMs else { return nil }
        var index = 0
        for (offset, line) in lines.enumerated() where line.atMs <= positionMs {
            index = offset
        }
        return index
    }

    func toggleLyrics() {
        lyricsVisible.toggle()
        if lyricsVisible {
            queueVisible = false
            loadLyricsIfNeeded()
        }
    }

    /// Opens or closes the full-screen player. Only presentation changes; playback is untouched.
    func setFullPlayerOpen(_ open: Bool) {
        guard open != fullPlayerOpen else { return }
        fullPlayerOpen = open
        if open { loadLyricsIfNeeded() }
    }

    /// Fetches lyrics for the current track, unless they are already loaded for it.
    func loadLyricsIfNeeded() {
        guard lyricsVisible || fullPlayerOpen else { return }
        guard let track = currentTrack else {
            lyrics = nil
            lyricsVideoID = nil
            lyricsStatus = "Nothing playing."
            return
        }
        guard lyricsVideoID != track.videoID, !lyricsRequestInFlight else { return }
        lyricsRequestInFlight = true
        lyricsVideoID = track.videoID
        lyrics = nil
        lyricsStatus = "Looking up lyrics for \(track.title)…"
        let requestedFor = track.videoID
        send(
            command: "lyrics.get",
            payload: GoosicRequestPayload(lyrics: GoosicLyricsQuery(
                title: track.title,
                artist: track.artist,
                album: track.album,
                durationSeconds: Self.durationSeconds(track.duration)
            ))
        ) { [weak self] response in
            guard let self else { return }
            self.lyricsRequestInFlight = false
            // The track may have changed while this was in flight.
            guard self.currentTrack?.videoID == requestedFor else { return }
            guard let document = response.payload?.lyrics, !document.lines.isEmpty else {
                self.lyricsStatus = "No lyrics were found for this track."
                return
            }
            self.lyrics = document
            self.lyricsStatus = document.synced
                ? "Synced lyrics from \(document.source)."
                : "Lyrics from \(document.source); this version is not synced."
        } failure: { [weak self] error in
            guard let self else { return }
            self.lyricsRequestInFlight = false
            guard self.currentTrack?.videoID == requestedFor else { return }
            self.lyrics = nil
            let described = Self.describe(error)
            self.lyricsStatus = described.code == "lyricsNotFound"
                ? "No lyrics were found for this track."
                : "Could not load lyrics: \(described.message)"
        }
    }

    /// Parses a display duration such as `3:42` or `1:02:03` into whole seconds.
    ///
    /// Returns `nil` for anything else, so a malformed value narrows the lyrics lookup with a
    /// wrong length rather than being guessed at.
    static func durationSeconds(_ text: String) -> UInt32? {
        let parts = text.split(separator: ":")
        guard (2...3).contains(parts.count) else { return nil }
        var total: UInt32 = 0
        for part in parts {
            guard let value = UInt32(part.trimmingCharacters(in: .whitespaces)) else { return nil }
            let (scaled, scaleOverflow) = total.multipliedReportingOverflow(by: 60)
            guard !scaleOverflow else { return nil }
            let (sum, sumOverflow) = scaled.addingReportingOverflow(value)
            guard !sumOverflow else { return nil }
            total = sum
        }
        return total
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
            // `self.route` deliberately, not the restored value: if the user navigated while this
            // was in flight, the screen to load is the one they are looking at.
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
        shuffle = settings.shuffle
        repeatMode = RepeatMode(rawValue: settings.repeatMode) ?? .off
        setTheme(GoosicTheme.named(settings.theme), persist: false)
        artworkBackground = settings.artworkBackground ?? true
        queueVisible = settings.queueVisible
        legacyImported = settings.importedFromLegacy
        legacyImportAvailable = settings.legacyAvailable
        // Preferences are read at launch and the answer can take seconds. Somebody who has
        // already picked a screen in that time has said where they want to be more recently than
        // the stored preference did, and yanking them back to last session's route is the kind of
        // bug that reads as the app fighting the user.
        if restoringRoute, !hasNavigatedSinceLaunch,
           let restored = GoosicRoute(rawValue: settings.lastRoute) {
            route = restored
        }
    }

    func setAutoplay(_ enabled: Bool) {
        guard allowPlaybackInteraction() else { return }
        autoplay = enabled
        savePreferences(GoosicPreferencesPatch(autoplay: enabled))
    }

    /// Chooses an appearance, and stores it unless it came from storage in the first place.
    ///
    /// Publishing `theme` is what applies it: the shell hands it to SwiftCrossUI as a preferred
    /// colour scheme, which is the only thing the backend honours.
    func setTheme(_ theme: GoosicTheme, persist: Bool = true) {
        self.theme = theme
        if persist {
            savePreferences(GoosicPreferencesPatch(theme: theme.rawValue))
        }
    }

    /// Decoration, not playback, so unlike the controls around it this is never gated on a
    /// playback transition.
    func setArtworkBackground(_ enabled: Bool) {
        guard enabled != artworkBackground else { return }
        artworkBackground = enabled
        savePreferences(GoosicPreferencesPatch(artworkBackground: enabled))
    }

    func toggleShuffle() {
        guard allowPlaybackInteraction() else { return }
        shuffle.toggle()
        savePreferences(GoosicPreferencesPatch(shuffle: shuffle))
    }

    func cycleRepeatMode() {
        guard allowPlaybackInteraction() else { return }
        repeatMode = repeatMode.next
        savePreferences(GoosicPreferencesPatch(repeatMode: repeatMode.rawValue))
    }

    /// The queue position to play after `index`, honouring shuffle and repeat.
    ///
    /// Returns `nil` when the queue is finished, which is what lets radio take over.
    /// `wrapping` is false at the natural end of a track and true for a deliberate Next, so
    /// pressing Next at the end of a list moves rather than stopping.
    func indexAfter(_ index: Int, wrapping: Bool) -> Int? {
        let count = queue.tracks.count
        guard count > 0 else { return nil }
        if repeatMode == .one { return index }
        if shuffle {
            guard count > 1 else { return (repeatMode == .all || wrapping) ? index : nil }
            // Any position but the current one, so shuffle never repeats a track back to back.
            var candidate = index
            while candidate == index { candidate = Int.random(in: 0..<count) }
            return candidate
        }
        let next = index + 1
        if next < count { return next }
        return (repeatMode == .all || wrapping) ? 0 : nil
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

    /// Coalesces independent preference changes without dropping a field from an earlier update.
    ///
    /// This stays visible to the test target because rapid UI changes are otherwise easy to
    /// regress: a volume drag followed by Shuffle or Repeat must persist both values.
    static func merge(
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
        merged.shuffle = update.shuffle ?? merged.shuffle
        merged.repeatMode = update.repeatMode ?? merged.repeatMode
        merged.artworkBackground = update.artworkBackground ?? merged.artworkBackground
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
        if route == .library {
            loadLibrary(section: PersonalLibrarySection(rawValue: libraryTab) ?? .playlists, force: force)
            return
        }
        if route == .home, activeAccount != nil {
            loadPersonalHome(force: force)
            return
        }
        guard let catalogRoute = route.catalogRoute else { return }
        loadCatalog(
            key: .route(route),
            command: "catalog.browse",
            payload: GoosicRequestPayload(query: route.title, catalogId: catalogRoute),
            force: force
        )
    }

    func loadEntity(_ entity: GoosicEntityReference, force: Bool = false) {
        // A signed-in user's own playlists and albums are invisible to the anonymous client, and
        // that is not an error it can report — upstream answers a private browse with an empty
        // page, so tapping your own playlist opened something that looked like it had no tracks.
        // When there is an account, its own view of an entity is the only correct one.
        if activeAccount != nil, let personal = Self.personalEntityBrowse(for: entity) {
            loadPersonalEntity(entity, browse: personal, force: force)
            return
        }
        loadCatalogForEntity(entity, force: force)
    }

    /// How an entity is browsed inside the account's profile, or `nil` for kinds the personal
    /// reader has nothing better to say about than the anonymous one.
    private static func personalEntityBrowse(
        for entity: GoosicEntityReference
    ) -> (browseID: String, title: String, shape: CatalogPageShape)? {
        switch entity {
        case .playlist(let id):
            return (PersonalBrowseID.playlist(id), "Playlist", .tracks)
        case .album(let id):
            return (id, "Album", .tracks)
        // An artist page is public, and the anonymous reader already renders it with the shelf
        // structure the screen expects. Routing it through the account would trade a better
        // answer for a slower one.
        case .artist:
            return nil
        }
    }

    private func loadPersonalEntity(
        _ entity: GoosicEntityReference,
        browse: (browseID: String, title: String, shape: CatalogPageShape),
        force: Bool
    ) {
        let key = CatalogKey.entity(entity)
        guard let start = beginLoad(key, force: force) else { return }
        if !start.revalidating { pages[key] = .loading }
        let ticket = catalogRequests.issue(for: key)
        personalCatalogHost.loadBrowse(
            browseID: browse.browseID,
            title: browse.title,
            shape: browse.shape
        ) { [weak self] result in
            guard let self, self.catalogRequests.accepts(ticket, for: key) else { return }
            // A failure is not the only way the account can decline to answer, and it is not the
            // common one. Upstream replies to a browse it will not serve with a perfectly valid,
            // entirely empty page — so "shows nothing" is what a refusal looks like from here,
            // and treating only the error case as a fallback leaves the user staring at "came
            // back empty" on an album that the anonymous route could have loaded.
            let unusable: Bool
            switch result {
            case .failure: unusable = true
            case .success(let page): unusable = (page.tracks?.isEmpty ?? true) && (page.shelves?.isEmpty ?? true)
            }
            if unusable {
                self.catalogRequests.retire(ticket, for: key)
                self.loadCatalogForEntity(entity, force: true)
                return
            }
            self.pageSources[key] = .personal(
                browseID: browse.browseID, title: browse.title, shape: browse.shape
            )
            self.applyPersonalPage(result, key: key, ticket: ticket, revalidating: start.revalidating)
        }
    }

    /// The anonymous route for an entity, used directly when signed out and as the fallback when
    /// the account cannot answer.
    private func loadCatalogForEntity(_ entity: GoosicEntityReference, force: Bool) {
        let command: String
        let id: String
        switch entity {
        case .album(let value): (command, id) = ("catalog.album", value)
        case .artist(let value): (command, id) = ("catalog.artist", value)
        case .playlist(let value): (command, id) = ("catalog.playlist", value)
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
        load(key, force: true)
    }

    /// Asks for whatever `key` names. Every screen's loader has a different name and a different
    /// argument, and this is the one place that knows which is which.
    private func load(_ key: CatalogKey, force: Bool) {
        switch key {
        case .route(let route):
            loadRoute(route, force: force)
        case .search:
            search(force: force)
        case .album(let id):
            loadEntity(.album(id), force: force)
        case .artist(let id):
            loadEntity(.artist(id), force: force)
        case .playlist(let id):
            loadEntity(.playlist(id), force: force)
        case .library(let raw):
            loadLibrary(section: PersonalLibrarySection(rawValue: raw) ?? .playlists, force: force)
        }
    }

    func selectLibrarySection(_ section: PersonalLibrarySection) {
        libraryTab = section.rawValue
        loadLibrary(section: section)
    }

    func loadLibrary(section: PersonalLibrarySection, force: Bool = false) {
        let key = section.key
        guard activeAccount != nil else {
            pages[key] = .idle
            return
        }
        guard let start = beginLoad(key, force: force) else { return }
        if !start.revalidating { pages[key] = .loading }
        let ticket = catalogRequests.issue(for: key)
        pageSources[key] = .personal(
            browseID: section.browseID, title: section.rawValue, shape: .auto
        )
        personalCatalogHost.load(section: section) { [weak self] result in
            guard let self, self.catalogRequests.accepts(ticket, for: key) else { return }
            self.applyPersonalPage(result, key: key, ticket: ticket, revalidating: start.revalidating)
        }
    }

    func loadMore(_ key: CatalogKey) {
        guard case .loaded(let existing) = state(for: key),
              let cursor = existing.nextCursor,
              !cursor.isEmpty,
              continuationState(for: key) == .idle,
              (client != nil || isPersonalPage(key)) else { return }
        continuations[key] = .loading
        // A continuation shares the page's ticket space: a reload issued while one is in flight
        // replaces the page it was going to be appended to, so the append must not happen.
        let ticket = catalogRequests.issue(for: key)
        // A cursor is only meaningful to the reader that issued it, so it goes back to whichever
        // one produced the page rather than to whichever one the key looks like it belongs to.
        if case .personal(let browseID, let title, let shape) = pageSources[key] {
            personalCatalogHost.loadBrowse(
                browseID: browseID,
                title: title,
                continuation: cursor,
                shape: shape
            ) { [weak self] result in
                guard let self else { return }
                self.finishPersonalContinuation(
                    result, key: key, cursor: cursor, ticket: ticket, subject: title.lowercased()
                )
            }
            return
        }
        send(
            command: "catalog.continue",
            payload: GoosicRequestPayload(continuation: cursor)
        ) { [weak self] response in
            guard let self, self.catalogRequests.accepts(ticket, for: key) else { return }
            guard case .loaded(let current) = self.state(for: key),
                  current.nextCursor == cursor,
                  let wire = response.payload?.catalog else {
                self.catalogRequests.retire(ticket, for: key)
                self.continuations[key] = .idle
                return
            }
            Task { [weak self] in
                let page = await Self.appendPage(current, wire)
                guard let self, self.catalogRequests.accepts(ticket, for: key) else { return }
                self.catalogRequests.retire(ticket, for: key)
                self.continuations[key] = .idle
                self.finishLoad(key, page: page)
            }
        } failure: { [weak self] error in
            guard let self, self.catalogRequests.accepts(ticket, for: key) else { return }
            self.catalogRequests.retire(ticket, for: key)
            // Failure is recorded on the row itself rather than only in the status line, because
            // the row is what decides whether to ask again.
            self.continuations[key] = .failed(Self.describe(error).message)
        }
    }

    // MARK: - Library mutations

    /// State for the "New playlist" prompt. It lives on the model rather than in the menu that
    /// opens it because a context menu is gone by the time its action runs, and an alert owned by
    /// a view that no longer exists never appears.
    @SwiftCrossUI.Published var isNamingNewPlaylist = false
    @SwiftCrossUI.Published var newPlaylistName = ""
    private var newPlaylistTrack: GoosicTrack?

    func beginNewPlaylist(for track: GoosicTrack?) {
        newPlaylistTrack = track
        newPlaylistName = ""
        isNamingNewPlaylist = true
    }

    func confirmNewPlaylist() {
        isNamingNewPlaylist = false
        let track = newPlaylistTrack
        newPlaylistTrack = nil
        createPlaylist(named: newPlaylistName, with: track)
    }

    func cancelNewPlaylist() {
        isNamingNewPlaylist = false
        newPlaylistTrack = nil
    }

    /// The rename prompt, owned here for the same reason the create prompt is: the menu that
    /// opens it is gone by the time its action runs.
    @SwiftCrossUI.Published var isRenamingPlaylist = false
    @SwiftCrossUI.Published var renamedPlaylistName = ""
    private var renamingPlaylist: PersonalPlaylistSummary?

    func beginRenaming(_ playlist: PersonalPlaylistSummary) {
        renamingPlaylist = playlist
        renamedPlaylistName = playlist.title
        isRenamingPlaylist = true
    }

    func confirmRename() {
        isRenamingPlaylist = false
        guard let playlist = renamingPlaylist else { return }
        renamingPlaylist = nil
        renamePlaylist(playlist, to: renamedPlaylistName)
    }

    func cancelRename() {
        isRenamingPlaylist = false
        renamingPlaylist = nil
    }

    /// Deletion is confirmed rather than done, because upstream has no undo.
    @SwiftCrossUI.Published var playlistPendingDeletion: PersonalPlaylistSummary?

    func confirmDeletion() {
        guard let playlist = playlistPendingDeletion else { return }
        playlistPendingDeletion = nil
        deletePlaylist(playlist)
    }

    /// Reads the playlists a track could be added to.
    ///
    /// Refreshed rather than cached for the life of the session: the destinations are the point
    /// of the menu, and a playlist created in another client — or by this one a moment ago —
    /// missing from the list looks exactly like the playlist not existing.
    func loadUserPlaylists(force: Bool = false) {
        guard activeAccount != nil else {
            userPlaylists = []
            userPlaylistsState = .idle
            return
        }
        if userPlaylistsState == .loading { return }
        if !force, !userPlaylists.isEmpty { return }
        userPlaylistsState = .loading
        personalCatalogHost.mutate(.listUserPlaylists) { [weak self] result in
            guard let self else { return }
            switch result {
            case .success(let answer):
                self.userPlaylists = answer.playlists
                self.userPlaylistsState = .idle
            case .failure(let error):
                self.userPlaylistsState = .failed(error.localizedDescription)
            }
        }
    }

    func addTrackToPlaylist(_ track: GoosicTrack, playlist: PersonalPlaylistSummary) {
        apply(
            .addToPlaylist(playlistID: playlist.id, videoID: track.id),
            describing: "Added \(track.title) to \(playlist.title)",
            failing: "Could not add \(track.title) to \(playlist.title)"
        )
    }

    /// Creates a playlist and puts `track` in it, which is one upstream call rather than a
    /// create followed by an add — so there is no state where the playlist exists and the track
    /// the user was adding is not in it.
    func createPlaylist(named title: String, with track: GoosicTrack?) {
        let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            status = "A playlist needs a name."
            return
        }
        apply(
            .createPlaylist(
                title: trimmed, description: nil, privacy: .private,
                videoIDs: track.map { [$0.id] } ?? []
            ),
            describing: track == nil ? "Created \(trimmed)" : "Created \(trimmed)",
            failing: "Could not create \(trimmed)"
        ) { [weak self] _ in
            // The new playlist has to appear in the destinations, and the library index upstream
            // lags a create — so ask again rather than inserting a guess that a later refresh
            // would silently contradict.
            self?.loadUserPlaylists(force: true)
            self?.invalidatePersonalLibrary()
        }
    }

    /// Whether the open playlist is one this account owns and may therefore edit.
    ///
    /// Answered from the account's own list of playlists rather than guessed from the id, because
    /// a `PL…` id says nothing about who owns it: following someone else's playlist puts it in
    /// this library while leaving every edit refused. Offering Rename on it would produce a
    /// failure only after the user had typed a new name.
    func ownedPlaylist(for entity: GoosicEntityReference) -> PersonalPlaylistSummary? {
        guard case .playlist(let id) = entity, activeAccount != nil else { return nil }
        let bare = id.hasPrefix("VL") ? String(id.dropFirst(2)) : id
        return userPlaylists.first { $0.id == bare }
    }

    func renamePlaylist(_ playlist: PersonalPlaylistSummary, to title: String) {
        let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            status = "A playlist needs a name."
            return
        }
        apply(
            .renamePlaylist(playlistID: playlist.id, title: trimmed),
            describing: "Renamed to \(trimmed)",
            failing: "Could not rename \(playlist.title)"
        ) { [weak self] _ in self?.loadUserPlaylists(force: true) }
    }

    func setPlaylistDescription(_ playlist: PersonalPlaylistSummary, to description: String) {
        apply(
            .setPlaylistDescription(playlistID: playlist.id, description: description),
            describing: description.isEmpty ? "Cleared the description" : "Updated the description",
            failing: "Could not update \(playlist.title)"
        )
    }

    func setPlaylistPrivacy(_ playlist: PersonalPlaylistSummary, to privacy: PlaylistPrivacy) {
        apply(
            .setPlaylistPrivacy(playlistID: playlist.id, privacy: privacy),
            describing: "\(playlist.title) is now \(privacy.label.lowercased())",
            failing: "Could not change who can see \(playlist.title)"
        )
    }

    /// There is no undo upstream, so the caller confirms before reaching here.
    func deletePlaylist(_ playlist: PersonalPlaylistSummary) {
        apply(
            .deletePlaylist(playlistID: playlist.id),
            describing: "Deleted \(playlist.title)",
            failing: "Could not delete \(playlist.title)"
        ) { [weak self] _ in
            guard let self else { return }
            // The page the user is on no longer exists. Staying would leave them looking at a
            // playlist that has been deleted, refreshing into an error.
            if case .playlist = self.detail { self.closeDetail() }
            self.loadUserPlaylists(force: true)
        }
    }

    /// Runs a mutation and says what happened.
    ///
    /// Nothing here is applied optimistically. Upstream answers HTTP 200 for edits it refuses —
    /// not the owner, stale cookies — so "the request was sent" is not evidence the change
    /// happened, and a screen updated on that basis would show a library the account does not
    /// have. The reader checks the envelope status and fails loudly; this waits for that. What is
    /// rolled back, therefore, is the cache: anything the account's own view would now answer
    /// differently is dropped rather than left to look current.
    private func apply(
        _ mutation: PersonalMutation,
        describing success: String,
        failing failure: String,
        then finish: ((PersonalMutationResult) -> Void)? = nil
    ) {
        guard activeAccount != nil else {
            status = "Sign in to change your library."
            return
        }
        libraryOperationInProgress = true
        personalCatalogHost.mutate(mutation) { [weak self] result in
            guard let self else { return }
            self.libraryOperationInProgress = false
            switch result {
            case .success(let answer):
                self.status = success
                if mutation.changesLibrary { self.invalidatePersonalLibrary() }
                finish?(answer)
            case .failure(let error):
                self.status = "\(failure): \(error.localizedDescription)"
            }
        }
    }

    /// Drops what is cached about the account's own content after a change to it.
    ///
    /// Only the personally-read pages: the anonymous ones say nothing about this library and
    /// re-fetching them would spend requests to learn nothing. Cached pages are marked stale
    /// rather than cleared, so a screen keeps what it is showing and refreshes behind the user
    /// instead of blanking under them.
    private func invalidatePersonalLibrary() {
        for (key, source) in pageSources where source != .service {
            pageCachedAt.removeValue(forKey: key)
        }
        // The page in front of the user, which on a detail page is the entity rather than the
        // route behind it — adding a track to the playlist being viewed has to refresh that
        // playlist. Not forced: the timestamp above is already gone, so this revalidates behind
        // the user rather than blanking the page they are reading.
        let visible = currentPageKey
        if case .personal = pageSources[visible] { load(visible, force: false) }
    }

    /// The page the user is looking at, which is the one worth refreshing first.
    private var currentPageKey: CatalogKey {
        if let detail { return .entity(detail) }
        if route == .library {
            return (PersonalLibrarySection(rawValue: libraryTab) ?? .playlists).key
        }
        return .route(route)
    }

    /// Clears a refusal so `loadMore` will try again. Only a person calls this: leaving the
    /// failure in place is what stops the row from retrying on its own every time it scrolls back
    /// into view.
    func retryContinuation(_ key: CatalogKey) {
        guard case .failed = continuationState(for: key) else { return }
        continuations[key] = .idle
        loadMore(key)
    }

    /// A page the account's own reader produced can be continued without the service, which is
    /// why this asks about the page rather than about the key.
    private func isPersonalPage(_ key: CatalogKey) -> Bool {
        if case .personal = pageSources[key] { return true }
        return false
    }

    private func finishPersonalContinuation(
        _ result: Result<GoosicCatalogPage, Error>,
        key: CatalogKey,
        cursor: String,
        ticket: UInt64,
        subject: String
    ) {
        guard catalogRequests.accepts(ticket, for: key) else { return }
        switch result {
        case .success(let wire):
            guard case .loaded(let current) = state(for: key), current.nextCursor == cursor else {
                catalogRequests.retire(ticket, for: key)
                continuations[key] = .idle
                return
            }
            Task { [weak self] in
                let page = await Self.appendPage(current, wire)
                guard let self, self.catalogRequests.accepts(ticket, for: key) else { return }
                self.catalogRequests.retire(ticket, for: key)
                self.continuations[key] = .idle
                self.finishLoad(key, page: page)
            }
        case .failure(let error):
            catalogRequests.retire(ticket, for: key)
            continuations[key] = .failed("Could not load more \(subject) content: \(error.localizedDescription)")
        }
    }

    private func loadPersonalHome(force: Bool) {
        let key = CatalogKey.route(.home)
        guard let start = beginLoad(key, force: force) else { return }
        if !start.revalidating { pages[key] = .loading }
        let ticket = catalogRequests.issue(for: key)
        pageSources[key] = .personal(browseID: "FEmusic_home", title: "Home", shape: .shelves)
        personalCatalogHost.loadBrowse(
            browseID: "FEmusic_home", title: "Home", shape: .shelves
        ) { [weak self] result in
            guard let self, self.catalogRequests.accepts(ticket, for: key) else { return }
            self.applyPersonalPage(result, key: key, ticket: ticket, revalidating: start.revalidating)
        }
    }

    /// Whether a usable page is already on screen for the duration of a request.
    private struct LoadStart {
        let revalidating: Bool
    }

    /// Whether to request `key`, and whether the screen should wait for the answer.
    ///
    /// Returns `nil` when the cache is current and nothing needs to happen.
    private func beginLoad(_ key: CatalogKey, force: Bool) -> LoadStart? {
        if force { return LoadStart(revalidating: false) }
        switch state(for: key) {
        case .loading:
            return nil
        case .idle, .failed:
            return LoadStart(revalidating: false)
        case .loaded:
            switch CatalogFreshness.verdict(for: key, cachedAt: pageCachedAt[key]) {
            case .serve: return nil
            // Deliberately not `.loading`: the cached page stays on screen and is replaced only
            // if an answer arrives. Putting a spinner over content that is already good enough to
            // show is what makes returning to a screen feel slower than opening a new one.
            case .serveAndRevalidate, .load: return LoadStart(revalidating: true)
            }
        }
    }

    private func finishLoad(_ key: CatalogKey, page: CatalogPageView) {
        pageCachedAt[key] = Date()
        staleRefreshFailed.remove(key)
        pages[key] = .loaded(page)
    }

    private func failLoad(_ key: CatalogKey, revalidating: Bool, code: String, message: String) {
        if revalidating, case .loaded = state(for: key) {
            // There is still a page worth looking at. Replacing it with an error would throw away
            // content the user can use because the *refresh* failed, which is a worse answer than
            // saying the content might be behind.
            staleRefreshFailed.insert(key)
            status = message
            return
        }
        pages[key] = .failed(code: code, message: message)
    }

    private func applyPersonalPage(
        _ result: Result<GoosicCatalogPage, Error>,
        key: CatalogKey,
        ticket: UInt64,
        revalidating: Bool
    ) {
        switch result {
        case .success(let wire):
            Task { [weak self] in
                let page = await Self.buildPage(wire)
                // Re-checked after the hop: conversion runs off the main actor, and an account
                // can change or a reload can be issued while a page is still being built.
                guard let self, self.catalogRequests.accepts(ticket, for: key) else { return }
                self.catalogRequests.retire(ticket, for: key)
                self.finishLoad(key, page: page)
            }
        case .failure(let error):
            catalogRequests.retire(ticket, for: key)
            failLoad(
                key, revalidating: revalidating,
                code: "personalCatalog", message: error.localizedDescription
            )
        }
    }

    private func loadCatalog(
        key: CatalogKey,
        command: String,
        payload: GoosicRequestPayload,
        force: Bool
    ) {
        guard let start = beginLoad(key, force: force) else { return }
        guard client != nil else {
            failLoad(
                key, revalidating: start.revalidating, code: "offline",
                message: "Connect to the Rust service to load the catalog."
            )
            return
        }
        if !start.revalidating { pages[key] = .loading }
        pageSources[key] = .service
        let ticket = catalogRequests.issue(for: key)
        send(command: command, payload: payload) { [weak self] response in
            guard let self, self.catalogRequests.accepts(ticket, for: key) else { return }
            guard let wirePage = response.payload?.catalog else {
                self.catalogRequests.retire(ticket, for: key)
                self.failLoad(
                    key, revalidating: start.revalidating, code: "invalidResponse",
                    message: "The service answered without a catalog page."
                )
                return
            }
            Task { [weak self] in
                let page = await Self.buildPage(wirePage)
                // Checked again after the hop: conversion is off the main actor, so an account
                // switch or a reload can happen while a page is still being built.
                guard let self, self.catalogRequests.accepts(ticket, for: key) else { return }
                self.catalogRequests.retire(ticket, for: key)
                self.finishLoad(key, page: page)
            }
        } failure: { [weak self] error in
            guard let self, self.catalogRequests.accepts(ticket, for: key) else { return }
            self.catalogRequests.retire(ticket, for: key)
            let described = Self.describe(error)
            self.failLoad(
                key, revalidating: start.revalidating,
                code: described.code, message: described.message
            )
        }
    }

    /// Builds a page off the main actor.
    ///
    /// A Home or Library response can carry hundreds of rows, and converting, deduplicating, and
    /// constructing them where the UI runs made the app beach-ball at exactly the moment the
    /// loading label disappeared — the frame the user is most likely to be looking at. Every path
    /// that turns a wire page into a rendered one goes through here, because the expensive part is
    /// the conversion and it does not become cheap for being reached from the personal host.
    private static func buildPage(_ wire: GoosicCatalogPage) async -> CatalogPageView {
        await Task.detached(priority: .userInitiated) { CatalogPageView(wire: wire) }.value
    }

    /// Appending is the more expensive half: it deduplicates shelf identifiers across the pages
    /// already shown, so it grows with everything scrolled past rather than with the new page.
    private static func appendPage(
        _ current: CatalogPageView,
        _ wire: GoosicCatalogPage
    ) async -> CatalogPageView {
        await Task.detached(priority: .userInitiated) {
            current.appending(CatalogPageView(wire: wire))
        }.value
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
        requestedVolume = clamped
        isMuted = false
        volumeAppliedForLoad = true
        if playbackState.owner == .localDownloadedFile {
            localPlaybackHost.setVolume(clamped)
        } else {
            officialPlaybackHost.setVolume(clamped)
            officialPlaybackHost.setMuted(false)
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
            // A deliberately chosen list is a fresh listening context, so its later autoplay
            // must not inherit recommendations from the previous station.
            radioExtensionInFlight = false
            radioStation = nil
            radioRequestRevision &+= 1
        } else {
            select(track)
        }

        if playbackState.owner == .officialWebView {
            currentTrack = track
            beginTrack()
            officialPlaybackHost.load(videoID: track.videoID, generation: playbackState.generation, volume: volume, muted: isMuted)
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
            self.officialPlaybackHost.load(videoID: track.videoID, generation: self.playbackState.generation, volume: self.volume, muted: self.isMuted)
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
        // A deliberate Next wraps even with repeat off; only the end of a track stops.
        guard let index = indexAfter(queue.currentIndex, wrapping: true) else { return }
        play(queue.tracks[index])
    }

    func toggleQueue() {
        guard allowPlaybackInteraction() else { return }
        queueVisible.toggle()
        if queueVisible { lyricsVisible = false }
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
            officialPlaybackHost.load(videoID: videoID, generation: playbackState.generation, volume: volume, muted: isMuted)
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
            self.officialPlaybackHost.load(videoID: videoID, generation: self.playbackState.generation, volume: self.volume, muted: self.isMuted)
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
        // A new track invalidates the loaded lyrics; the next render asks for the right ones.
        lyrics = nil
        lyricsVideoID = nil
        isPaused = true
        currentTime = 0
        duration = 0
        pendingSeek = nil
        endedVideoID = nil
        isAdvertisement = false
        lastOfficialEventState = nil
        lastOfficialEventWasAdvertisement = nil
        lastLocalEventState = nil
        hasConfirmedPlaybackSample = false
        volumeAppliedForLoad = false
        requestedVolume = nil
        requestedMuted = nil
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
            applyPlaybackStateIfMeaningfullyChanged(state)
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
        var presentationChanged = false
        if !hasConfirmedPlaybackSample {
            hasConfirmedPlaybackSample = true
            presentationChanged = true
        }
        loadLyricsIfNeeded()
        let nextPaused = event.state != "playing"
        if isPaused != nextPaused {
            isPaused = nextPaused
            presentationChanged = true
        }
        if isAdvertisement != event.isAdvertisement {
            // YouTube Music can replace its media element when a pre-roll ends. That new content
            // element reports the page default before it has received the user's preference;
            // treating that report as a deliberate slider change is how a saved volume slowly
            // drifted back to 100% after advertisements.
            if VolumeSync.shouldReapplyPreference(
                wasAdvertisement: isAdvertisement,
                isAdvertisement: event.isAdvertisement
            ) {
                volumeAppliedForLoad = false
                requestedVolume = nil
                requestedMuted = nil
            }
            isAdvertisement = event.isAdvertisement
            presentationChanged = true
        }
        // A one-second visual cadence is smooth enough for a music progress bar and halves the
        // number of whole-shell invalidations caused by WebKit's 500 ms observer.
        if abs(currentTime - event.currentTime) >= 0.9 || event.state == "ended" {
            currentTime = event.currentTime
            presentationChanged = true
        }
        if abs(duration - event.duration) >= 0.1 {
            duration = event.duration
            presentationChanged = true
        }
        if !event.isAdvertisement {
            if volumeAppliedForLoad {
                switch VolumeSync.reconcileOfficialRenderer(
                    reported: event.volume, preferred: volume, requested: requestedVolume
                ) {
                case .waitingForEcho:
                    break
                case .settled:
                    requestedVolume = nil
                case .reapply(let preferred):
                    // The official player is mounted off-screen, so this cannot be a direct user
                    // action. A changed value means its media element was replaced or reset;
                    // keep the native preference authoritative and send it to the new element.
                    requestedVolume = preferred
                    officialPlaybackHost.setVolume(preferred)
                }
                if let requestedMuted {
                    if requestedMuted == event.isMuted { self.requestedMuted = nil }
                } else if isMuted != event.isMuted {
                    requestedMuted = isMuted
                    officialPlaybackHost.setMuted(isMuted)
                }
            } else if abs(event.volume - volume) > 0.01 || event.isMuted != isMuted {
                // A fresh content page starts at its own volume. Push the stored preference once,
                // then follow what the player reports. Advertisements never enter this path.
                volumeAppliedForLoad = true
                requestedVolume = volume
                requestedMuted = isMuted
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
        if lastOfficialEventState != event.state || lastOfficialEventWasAdvertisement != event.isAdvertisement {
            let nextStatus: String
            if event.isAdvertisement {
                nextStatus = "Official host confirmed advertisement playback (informational marker; ads are not bypassed)."
            } else if event.state == "playing" {
                nextStatus = "Official host confirmed media playback for \(event.videoID)."
            } else if event.state == "ended" {
                nextStatus = "Official host reported the video ended."
            } else {
                nextStatus = "Official host reported \(event.state) for \(event.videoID)."
            }
            if status != nextStatus {
                status = nextStatus
                presentationChanged = true
            }
            lastOfficialEventState = event.state
            lastOfficialEventWasAdvertisement = event.isAdvertisement
        }
        if presentationChanged { updateSystemMediaControls() }
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
        var presentationChanged = false
        if !hasConfirmedPlaybackSample {
            hasConfirmedPlaybackSample = true
            presentationChanged = true
        }
        let nextPaused = event.state != "playing"
        if isPaused != nextPaused { isPaused = nextPaused; presentationChanged = true }
        if isAdvertisement { isAdvertisement = false; presentationChanged = true }
        if abs(currentTime - event.currentTime) >= 0.9 || event.state == "ended" {
            currentTime = event.currentTime
            presentationChanged = true
        }
        if abs(duration - event.duration) >= 0.1 { duration = event.duration; presentationChanged = true }
        if isMuted != event.isMuted { isMuted = event.isMuted; presentationChanged = true }
        if !event.isMuted, case .adopt(let reported) = VolumeSync.reconcile(
            reported: event.volume, current: volume, requested: requestedVolume
        ) {
            volume = reported
            presentationChanged = true
            savePreferences(GoosicPreferencesPatch(volume: reported))
        }
        if lastLocalEventState != event.state {
            let nextStatus = event.state == "ended"
                ? "Local playback ended."
                : "Local playback confirmed \(event.state) for \(event.videoID)."
            if status != nextStatus { status = nextStatus; presentationChanged = true }
            lastLocalEventState = event.state
        }
        if presentationChanged { updateSystemMediaControls() }
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
        guard let nextIndex = indexAfter(queue.currentIndex, wrapping: false) else {
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
        guard !radioExtensionInFlight else {
            status = "Queue finished."
            return
        }
        // A station with no next cursor is genuinely exhausted. Re-seeding from the last
        // recommendation is what made long queues wander into unrelated music.
        if let station = radioStation, station.continuation == nil {
            status = "Queue finished."
            return
        }
        radioExtensionInFlight = true
        let station = radioStation ?? GoosicRadioStation(seedVideoID: seed.videoID, continuation: nil, accountID: activeAccountId)
        let revision = radioRequestRevision
        status = station.continuation == nil
            ? "Queue finished. Starting radio from \(seed.title)…"
            : "Queue finished. Loading more from this radio…"
        requestRadio(seedVideoID: station.seedVideoID, continuation: station.continuation, accountID: station.accountID) { [weak self] page in
            guard let self else { return }
            guard revision == self.radioRequestRevision else { return }
            self.radioExtensionInFlight = false
            guard let page else {
                self.status = "Queue finished. Radio had nothing to continue with."
                return
            }
            let tracks = Self.freshRadioTracks(page.playableTracks, excluding: self.queue.tracks)
            self.radioStation = GoosicRadioStation(
                seedVideoID: station.seedVideoID,
                continuation: page.nextCursor == station.continuation ? nil : page.nextCursor,
                accountID: station.accountID
            )
            guard let first = tracks.first else {
                self.status = "Queue finished. Radio had nothing new to continue with."
                return
            }
            self.queue.tracks.append(contentsOf: tracks)
            self.play(first)
        }
    }

    /// Starts the seed song, then appends its personalized recommendations.
    func startRadio(from track: GoosicTrack) {
        launch(.station(track))
    }

    func launch(_ intent: PlaybackLaunchIntent) {
        switch intent {
        case .ordered(let track, let tracks):
            play(track, in: tracks)
        case .station(let track):
            launchStation(track)
        }
    }

    private func launchStation(_ track: GoosicTrack) {
        guard allowPlaybackInteraction(), playbackTransition == .idle,
              !isAdvertisement, playbackState.owner != .localDownloadedFile, client != nil else { return }
        // Start the selected song immediately, while recommendations load independently.
        play(track, in: [track])
        radioExtensionInFlight = true
        let revision = radioRequestRevision
        let accountID = activeAccountId
        requestRadio(seedVideoID: track.videoID, continuation: nil, accountID: accountID) { [weak self] page in
            guard let self else { return }
            guard revision == self.radioRequestRevision, accountID == self.activeAccountId else { return }
            self.radioExtensionInFlight = false
            guard let page else {
                self.status = "Radio had nothing to play after \(track.title)."
                return
            }
            let tracks = Self.freshRadioTracks(page.playableTracks, excluding: [track])
            self.radioStation = GoosicRadioStation(
                seedVideoID: track.videoID, continuation: page.nextCursor, accountID: accountID
            )
            self.queue.tracks.append(contentsOf: tracks)
        }
    }

    /// Keeps only recommendations that do not already occur in this queue, while also removing
    /// repeats inside one upstream page. Explicit user queues may contain duplicates; radio
    /// recommendations may not, because a duplicate reads as a broken "up next" sequence.
    static func freshRadioTracks(_ candidates: [GoosicTrack], excluding queue: [GoosicTrack]) -> [GoosicTrack] {
        var seen = Set(queue.map(\.videoID))
        return candidates.filter { seen.insert($0.videoID).inserted }
    }

    private func requestRadio(
        seedVideoID: String,
        continuation: String?,
        accountID: String?,
        completion: @escaping (CatalogPageView?) -> Void
    ) {
        let identity = RadioRequestIdentity(revision: radioRequestRevision, accountID: accountID)
        if let accountID {
            guard accountID == activeAccountId else { completion(nil); return }
            personalCatalogHost.loadRadio(seedVideoID: seedVideoID, continuation: continuation) { [weak self] result in
                guard let self, identity.accepts(revision: self.radioRequestRevision, accountID: self.activeAccountId) else { return }
                switch result {
                case .success(let page): completion(CatalogPageView(wire: page))
                case .failure: completion(nil)
                }
            }
            return
        }
        send(
            command: "catalog.radio",
            payload: GoosicRequestPayload(catalogId: seedVideoID, continuation: continuation)
        ) { [weak self] response in
            guard let self, identity.accepts(revision: self.radioRequestRevision, accountID: self.activeAccountId) else { return }
            completion(response.payload?.catalog.map(CatalogPageView.init(wire:)))
        } failure: { [weak self] error in
            guard let self, identity.accepts(revision: self.radioRequestRevision, accountID: self.activeAccountId) else { return }
            completion(nil)
        }
    }

    private func applyState(_ response: GoosicResponse) {
        // A sample acknowledgement can arrive after account work has detached its renderer.
        // Do not let that old lease-bound response resurrect playback UI/state.
        guard !accountOperationInProgress else { return }
        if let state = response.payload?.state {
            applyPlaybackStateIfMeaningfullyChanged(state)
        }
    }

    /// Rust advances `sampleSequence` for every 500 ms player observation. That sequence is a
    /// protocol replay guard, not presentation state, and publishing it rebuilt every catalog
    /// view twice per second. Only ownership changes belong in the observable UI model.
    private func applyPlaybackStateIfMeaningfullyChanged(_ state: GoosicPlaybackState) {
        let previous = playbackState
        guard state.owner != previous.owner
            || state.generation != previous.generation
            || state.accountId != previous.accountId else {
            return
        }
        playbackState = state
        hasConfirmedPlaybackSample = false
        updateSystemMediaControls()
    }
}
