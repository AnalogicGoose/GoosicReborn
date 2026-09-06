import SwiftCrossUI

struct RouteScreen: View {
    let route: GoosicRoute
    let model: GoosicAppModel

    var body: some View {
        switch route {
        case .home:
            CatalogRouteScreen(
                route: .home,
                title: "Home",
                subtitle: "Live from YouTube Music, browsed as a guest",
                model: model
            )
        case .explore:
            CatalogRouteScreen(
                route: .explore,
                title: "Explore",
                subtitle: "New releases, charts, moods, and genres",
                model: model
            )
        case .charts:
            CatalogRouteScreen(
                route: .charts,
                title: "Charts",
                subtitle: "What is being played right now",
                model: model
            )
        case .moodsAndGenres:
            CatalogRouteScreen(
                route: .moodsAndGenres,
                title: "Moods & genres",
                subtitle: "Find a feeling, then a playlist",
                model: model
            )
        case .newReleases:
            CatalogRouteScreen(
                route: .newReleases,
                title: "New releases",
                subtitle: "Albums and singles out now",
                model: model
            )
        case .search:
            SearchScreen(model: model)
        case .library:
            LibraryScreen(model: model)
        case .downloads:
            DownloadsScreen(model: model)
        case .settings:
            SettingsScreen(model: model)
        }
    }
}

struct ScreenHeader: View {
    let title: String
    let subtitle: String

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.largeTitle)
            Text(subtitle)
                .font(.subheadline)
                .foregroundColor(.gray)
        }
        .padding(.bottom, 12)
    }
}

/// Renders one catalog page's load state: its shelves and tracks, or why they are missing.
struct CatalogPageBody: View {
    let key: CatalogKey
    let state: CatalogLoadState
    let subject: String
    let model: GoosicAppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            if state.isLoading {
                Text("Loading \(subject)…")
                    .font(.subheadline)
                    .foregroundColor(.gray)
                    .padding(.vertical, 18)
            } else if let failure = state.failure {
                let text = catalogFailureText(code: failure.code, message: failure.message, subject: subject)
                EmptyState(title: text.title, message: text.detail)
                Button("Try again") { model.retry(key) }
                    .font(.caption)
            } else if let page = state.page {
                if page.isEmpty {
                    EmptyState(title: "Nothing to show", message: "\(subject) came back empty.")
                } else {
                    if !page.tracks.isEmpty {
                        ForEach(page.tracks) { track in
                            TrackRow(track: track, context: page.tracks, model: model)
                        }
                    }
                    ForEach(page.shelves) { shelf in
                        ShelfView(shelf: shelf, model: model)
                    }
                    if page.truncated {
                        Text("This page was long, so only the first part is shown.")
                            .font(.caption2)
                            .foregroundColor(.gray)
                            .padding(.top, 4)
                    }
                }
            } else {
                Text("Not loaded yet.")
                    .font(.subheadline)
                    .foregroundColor(.gray)
                Button("Load \(subject)") { model.retry(key) }
                    .font(.caption)
            }
        }
    }
}

struct CatalogRouteScreen: View {
    let route: GoosicRoute
    let title: String
    let subtitle: String
    let model: GoosicAppModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                ScreenHeader(title: title, subtitle: subtitle)
                GuestCatalogNotice()
                CatalogPageBody(
                    key: .route(route),
                    state: model.state(for: .route(route)),
                    subject: title.lowercased(),
                    model: model
                )
            }
            .padding(24)
        }
    }
}

struct SearchScreen: View {
    let model: GoosicAppModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                ScreenHeader(title: "Search", subtitle: "Search YouTube Music by title, artist, or album")
                HStack(spacing: 8) {
                    TextField("Search music", text: Binding(get: { model.query }, set: { model.query = $0 }))
                        .frame(minWidth: 280)
                    Button("Search") { model.search() }
                }
                HStack(spacing: 7) {
                    ForEach(CatalogSearchFilter.allCases, id: \.self) { filter in
                        Button(filter.rawValue) { model.selectSearchFilter(filter) }
                            .font(.caption)
                            // Without a floor the row squeezes the labels into ellipses.
                            .frame(minWidth: 68)
                            .background(model.searchFilter == filter ? Color.blue.opacity(0.18) : Color.clear)
                    }
                }
                if model.submittedQuery.isEmpty {
                    EmptyState(
                        title: "Start a search",
                        message: "Every result is a real YouTube Music entry, and songs play through the official player."
                    )
                } else {
                    CatalogPageBody(
                        key: model.currentSearchKey,
                        state: model.state(for: model.currentSearchKey),
                        subject: "“\(model.submittedQuery)”",
                        model: model
                    )
                }
            }
            .padding(24)
        }
    }
}

struct LibraryScreen: View {
    let model: GoosicAppModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                ScreenHeader(title: "Library", subtitle: "Your saved collection will live here")
                EmptyState(
                    title: model.activeAccount == nil ? "Not connected to an account" : "Personal library is next",
                    message: model.activeAccount == nil
                        ? "The catalog is browsed as a guest, so there is no personal library to read. Sign in from Settings to create an isolated account profile."
                        : "This account is signed in, but personal library reads are intentionally left as the next authenticated-data step."
                )
                Text("What works today: Home, Explore, Charts, Moods & genres, New releases, and Search all read the live catalog, and songs play through the official player.")
                    .font(.caption)
                    .foregroundColor(.gray)
            }
            .padding(24)
        }
    }
}

struct DownloadsScreen: View {
    let model: GoosicAppModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                ScreenHeader(title: "Downloads", subtitle: "Local files already present on this Mac")
                HStack(spacing: 8) {
                    Button("Refresh") { model.loadDownloads() }
                    Button("Import previous Goosic files") { model.importLegacyDownloads() }
                        .disabled(model.downloadsLoading)
                }
                Text("This screen is read-only: Goosic only imports finalized WebM/Opus files already on disk. It never starts a downloader or reads account cookies.")
                    .font(.caption)
                    .foregroundColor(.gray)
                if model.downloadsLoading {
                    Text("Reading the local downloads index…")
                        .font(.subheadline)
                } else if model.downloadedTracks.isEmpty {
                    EmptyState(
                        title: "No imported files",
                        message: "Import a previous Goosic installation if its finalized offline-media files are still on this Mac."
                    )
                } else {
                    ForEach(model.downloadedTracks) { track in
                        DownloadedTrackRow(track: track, model: model)
                    }
                }
            }
            .padding(24)
        }
    }
}

struct DownloadedTrackRow: View {
    let track: GoosicDownloadedTrack
    let model: GoosicAppModel

    var body: some View {
        HStack(spacing: 10) {
            Text(track.available ? "♪" : "!")
                .frame(width: 22)
            VStack(alignment: .leading, spacing: 2) {
                Text(track.title)
                Text(track.subtitle)
                    .font(.caption)
                    .foregroundColor(.gray)
                Text(track.available ? "\(track.bytes) bytes · \(track.videoId)" : "File is missing")
                    .font(.caption2)
                    .foregroundColor(.gray)
            }
            Spacer()
            Button("Play downloaded") { model.playDownloaded(track) }
                .font(.caption)
                .disabled(!track.available || model.accountOperationInProgress || model.playbackTransition != .idle || model.isAdvertisement)
        }
        .padding(.vertical, 6)
    }
}

struct SettingsScreen: View {
    let model: GoosicAppModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                ScreenHeader(title: "Settings", subtitle: "Connection and migration status")
                Text("Service")
                    .font(.headline)
                Text(model.serviceConnected ? "Connected to goosic-service over async NDJSON." : "Not connected. Use “Connect to Rust service” in the sidebar.")
                Text("Catalog")
                    .font(.headline)
                    .padding(.top, 8)
                Text("Catalog reads go through Rust to YouTube Music as an anonymous guest. No cookies, account headers, or credentials are sent, and no catalog data reaches the protocol beyond titles and identifiers.")
                    .font(.caption)
                    .foregroundColor(.gray)
                AccountSettingsSection(model: model)
                Text("Preferences")
                    .font(.headline)
                    .padding(.top, 8)
                Text("Volume, mute, autoplay, the queue panel, and the last screen are stored by Rust and restored on launch.")
                    .font(.caption)
                    .foregroundColor(.gray)
                if model.legacyImported {
                    Text("Preferences were imported from a previous Goosic install. The old data was read, never changed.")
                        .font(.caption)
                        .foregroundColor(.gray)
                } else if model.legacyImportAvailable {
                    Text("A previous Goosic install's preferences were found on this machine. Importing reads them without changing them, and never carries over saved credentials.")
                        .font(.caption)
                        .foregroundColor(.gray)
                    Button("Import previous Goosic preferences") { model.importLegacyPreferences() }
                        .font(.caption)
                } else {
                    Text("No previous Goosic install was found to import from.")
                        .font(.caption)
                        .foregroundColor(.gray)
                }
                AppearanceSection(model: model)
                Text("Playback")
                    .font(.headline)
                    .padding(.top, 8)
                Text("Rust owns playback state and leases. Every song plays in the one official WebKit host; advertisements are observed as informational markers and are never bypassed.")
                Text("Account: \(model.playbackState.accountId ?? "none")")
                    .font(.caption)
                    .foregroundColor(.gray)
                PlaybackLabSection(model: model)
            }
            .padding(24)
        }
    }
}

/// The official-host test controls, extracted so the settings screen stays inside the view
/// builder's child limit.
struct PlaybackLabSection: View {
    let model: GoosicAppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Playback Lab")
                .font(.headline)
                .padding(.top, 8)
            Text("Load a specific 11-character YouTube Music video ID directly, for testing the host without going through the catalog.")
                .font(.caption)
                .foregroundColor(.gray)
            HStack(spacing: 8) {
                TextField("YouTube Music video ID", text: Binding(get: { model.playbackLabVideoID }, set: { model.playbackLabVideoID = $0 }))
                    .frame(minWidth: 260)
                Button("Load official video") { model.loadOfficialVideo() }
                    .disabled(model.accountOperationInProgress || model.playbackTransition != .idle)
            }
            HStack(spacing: 8) {
                // Without a floor these squeeze their own labels into ellipses.
                Button("Play") { model.playOfficialVideo() }
                    .frame(minWidth: 72)
                    .disabled(model.accountOperationInProgress)
                Button("Pause") { model.pauseOfficialVideo() }
                    .frame(minWidth: 72)
                    .disabled(model.accountOperationInProgress)
                Button("Stop") { model.stopOfficialVideo() }
                    .frame(minWidth: 72)
                    .disabled(model.accountOperationInProgress)
                Button("Release") { model.releasePlayback() }
                    .frame(minWidth: 72)
                    .disabled(model.accountOperationInProgress || model.playbackTransition != .idle)
            }
            Text(model.hostStatus)
                .font(.caption)
                .foregroundColor(.gray)
            Text("Page probe")
                .font(.headline)
                .padding(.top, 8)
            Text(model.hostDiagnostics)
                .font(.caption2)
                .foregroundColor(.gray)
            Text("The status above reports confirmed host events only; a load or play request is not treated as audible playback until WebKit sends a validated event.")
                .font(.caption2)
                .foregroundColor(.gray)
        }
        .padding(.top, 8)
    }
}

struct AccountSettingsSection: View {
    let model: GoosicAppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Accounts")
                .font(.headline)
            Text("Each account has an isolated WebKit profile. Goosic stores only metadata; authentication remains in that profile and never crosses into Rust.")
                .font(.caption)
                .foregroundColor(.gray)
            Button("Add account") { model.signIn() }
                .disabled(!model.serviceConnected || model.accountOperationInProgress || model.playbackTransition != .idle || model.isAdvertisement)
            if model.accounts.isEmpty {
                Text("No signed-in accounts. Use Add account to open the secure Google sign-in window.")
                    .font(.caption)
                    .foregroundColor(.gray)
            } else {
                ForEach(model.accounts) { account in
                    HStack(spacing: 8) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(account.displayName)
                            Text(account.email ?? account.channel ?? "YouTube Music account")
                                .font(.caption)
                                .foregroundColor(.gray)
                        }
                        Spacer()
                        if account.id == model.activeAccountId {
                            Text("Active")
                                .font(.caption)
                        } else {
                            Button("Switch") { model.switchAccount(to: account.id) }
                                .font(.caption)
                                .disabled(model.accountOperationInProgress || model.playbackTransition != .idle || model.isAdvertisement)
                        }
                        Button(account.id == model.activeAccountId ? "Sign out" : "Remove") {
                            if account.id == model.activeAccountId { model.signOut() }
                            else { model.removeAccount(account.id) }
                        }
                        .font(.caption)
                        .disabled(model.accountOperationInProgress || model.playbackTransition != .idle || model.isAdvertisement)
                    }
                    .padding(.vertical, 4)
                }
            }
        }
        .padding(.top, 8)
    }
}

/// Appearance choice. Its own view because a view builder takes only so many direct children,
/// and the settings screen was already at that limit.
struct AppearanceSection: View {
    let model: GoosicAppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Appearance")
                .font(.headline)
            HStack(spacing: 8) {
                ForEach(GoosicTheme.allCases, id: \.self) { option in
                    Button(option.label) { model.setTheme(option) }
                        .font(.caption)
                        .frame(minWidth: 68)
                        .background(model.theme == option ? Color.blue.opacity(0.18) : Color.clear)
                }
            }
            Text("System follows whatever macOS is set to; the app does not reimplement light and dark.")
                .font(.caption2)
                .foregroundColor(.gray)
        }
        .padding(.top, 8)
    }
}

struct EmptyState: View {
    let title: String
    let message: String

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(title)
                .font(.headline)
            Text(message)
                .font(.subheadline)
                .foregroundColor(.gray)
        }
        .padding(.vertical, 22)
    }
}

struct EntityDetailScreen: View {
    let entity: GoosicEntityReference
    let model: GoosicAppModel

    private var key: CatalogKey { .entity(entity) }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                Button("‹ Back") { model.closeDetail() }
                    .font(.caption)
                let state = model.state(for: key)
                if let page = state.page {
                    ScreenHeader(
                        title: page.title,
                        subtitle: detailSubtitle(kindLabel: entity.kindLabel, pageSubtitle: page.subtitle)
                    )
                    if !page.tracks.isEmpty {
                        Button("Play all") {
                            if let first = page.tracks.first {
                                model.play(first, in: page.tracks)
                            }
                        }
                        .font(.caption)
                    }
                } else {
                    ScreenHeader(title: entity.kindLabel, subtitle: "Loading from YouTube Music")
                }
                CatalogPageBody(
                    key: key,
                    state: state,
                    subject: "this \(entity.kindLabel.lowercased())",
                    model: model
                )
            }
            .padding(24)
        }
    }
}
