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
    @SwiftCrossUI.Published var autoplay = true
    @SwiftCrossUI.Published private(set) var playbackTransition: PlaybackTransition = .idle
    @SwiftCrossUI.Published private(set) var playbackTransitionToken: UInt64 = 0
    @SwiftCrossUI.Published var playbackLabVideoID = ""
    @SwiftCrossUI.Published private(set) var hostStatus = "No official video loaded."
    @SwiftCrossUI.Published private(set) var hostDiagnostics = "No page loaded."
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
    let officialPlaybackHost: OfficialPlaybackHost

    init() {
        officialPlaybackHost = OfficialPlaybackHost()
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
    }

    // MARK: - Navigation

    func navigate(to route: GoosicRoute) {
        self.route = route
        detail = nil
        loadRoute(route)
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
                self.loadRoute(self.route)
            }
        } catch {
            status = error.localizedDescription
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
    var isSeekable: Bool { duration > 0 && playbackState.owner == .officialWebView }

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

    func seek(to position: Double) {
        guard isSeekable else { return }
        let clamped = min(max(position, 0), duration)
        pendingSeek = (clamped, Date())
        officialPlaybackHost.seek(to: clamped)
    }

    func setVolume(_ newVolume: Double) {
        let clamped = min(max(newVolume, 0), 1)
        volume = clamped
        isMuted = false
        officialPlaybackHost.setVolume(clamped)
    }

    func toggleMuted() {
        isMuted.toggle()
        officialPlaybackHost.setMuted(isMuted)
    }

    func play(_ track: GoosicTrack, in tracks: [GoosicTrack] = []) {
        guard playbackTransition == .idle else {
            status = "Playback command pending; wait for Rust to finish before choosing another action."
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

    func togglePause() {
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
        guard let track = queue.current ?? queue.tracks.first else {
            status = "Choose a track to begin."
            return
        }
        play(track)
    }

    func previous() {
        guard playbackTransition == .idle else {
            status = "Playback command pending; previous is temporarily unavailable."
            return
        }
        guard !queue.tracks.isEmpty else { return }
        let index = queue.currentIndex > 0 ? queue.currentIndex - 1 : queue.tracks.count - 1
        play(queue.tracks[index])
    }

    func next() {
        guard playbackTransition == .idle else {
            status = "Playback command pending; next is temporarily unavailable."
            return
        }
        guard !queue.tracks.isEmpty else { return }
        let index = (queue.currentIndex + 1) % queue.tracks.count
        play(queue.tracks[index])
    }

    func toggleQueue() {
        queueVisible.toggle()
    }

    func releasePlayback() {
        guard playbackTransition == .idle else {
            status = "Playback command pending; release is temporarily unavailable."
            return
        }
        guard playbackState.owner != .none else {
            status = "Rust playback is already released."
            return
        }
        let operationToken = beginPlaybackTransition(.releasing)
        status = "Releasing playback…"
        officialPlaybackHost.quiesce { [weak self] in
            guard let self else { return }
            guard self.isCurrentPlaybackTransition(operationToken, kind: .releasing) else { return }
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
        guard playbackTransition == .idle else {
            status = "Playback command pending; wait for Rust to finish before loading another video."
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
        return playbackTransitionToken
    }

    private func isCurrentPlaybackTransition(_ token: UInt64, kind: PlaybackTransition) -> Bool {
        playbackTransitionToken == token && playbackTransition == kind
    }

    private func finishPlaybackTransition(_ token: UInt64) {
        guard playbackTransitionToken == token else { return }
        playbackTransition = .idle
    }

    /// Clears everything the previous track confirmed, so no stale position or end marker is
    /// carried into the next one.
    private func beginTrack() {
        isPaused = true
        currentTime = 0
        duration = 0
        pendingSeek = nil
        endedVideoID = nil
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
            playbackState = state
        }
        if response.requestId.hasPrefix("swift-") && response.payload?.message != nil {
            status = response.payload?.message ?? status
        }
    }

    private func receive(_ event: OfficialPlaybackEvent) {
        guard playbackState.owner == .officialWebView,
              event.generation == playbackState.generation,
              event.videoID == officialPlaybackHost.loadedVideoID else {
            return
        }
        isPaused = event.state != "playing"
        currentTime = event.currentTime
        duration = event.duration
        volume = event.volume
        isMuted = event.isMuted
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

    /// The official app followed its own queue. Treat the requested track as finished and let
    /// Goosic's queue decide, so the app never plays something the user did not choose.
    private func officialPlayerMovedOn(from finishedVideoID: String) {
        guard playbackState.owner == .officialWebView else { return }
        guard endedVideoID != finishedVideoID else { return }
        endedVideoID = finishedVideoID
        isPaused = true
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
            status = "Queue finished."
            return
        }
        play(queue.tracks[nextIndex])
    }

    private func applyState(_ response: GoosicResponse) {
        if let state = response.payload?.state {
            playbackState = state
        }
    }
}
