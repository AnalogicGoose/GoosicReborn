#if os(macOS) && !GOOSIC_PORTABLE
import AppKit
#if !GOOSIC_PREVIEW_NO_WEBKIT
import AppKitBackend
#endif
import SwiftCrossUI
import SwiftUI

/// Hosts the catalog in native SwiftUI on macOS.
///
/// SwiftCrossUI 0.9 eagerly computes every `ForEach` child, including children outside a
/// `ScrollView`'s viewport. A YouTube Music page can contain hundreds of cards, which blocks
/// AppKit's main run loop and produces the beach ball. Native SwiftUI's lazy stacks retain only
/// the visible neighborhood, which is the same rendering strategy used by Kaset.
#if !GOOSIC_PREVIEW_NO_WEBKIT
struct NativeMacCatalogRouteSurfaceBackend: GoosicAppKitRepresentable {
    let route: GoosicRoute
    let title: String
    let subtitle: String
    let state: CatalogLoadState
    let model: GoosicAppModel

    func makeNSView(context: Context) -> NSHostingView<SwiftUI.AnyView> {
        NSHostingView(rootView: rootView)
    }

    func updateNSView(_ nsView: NSHostingView<SwiftUI.AnyView>, context: Context) {
        nsView.rootView = rootView
    }

    private var rootView: SwiftUI.AnyView {
        SwiftUI.AnyView(
            NativeMacCatalogPage(
                key: .route(route),
                title: title,
                subtitle: subtitle,
                state: state,
                model: model
            )
        )
    }
}

/// SwiftCrossUI wrapper around the AppKit representable. Keeping this separate mirrors the
/// backend boundary used by the existing material and playback surfaces.
struct NativeMacCatalogRouteSurface: SwiftCrossUI.View {
    let route: GoosicRoute
    let title: String
    let subtitle: String
    let state: CatalogLoadState
    let model: GoosicAppModel

    var body: some SwiftCrossUI.View {
        NativeMacCatalogRouteSurfaceBackend(
            route: route,
            title: title,
            subtitle: subtitle,
            state: state,
            model: model
        )
    }
}
#endif

struct NativeMacCatalogPage: SwiftUI.View {
    @SwiftUI.Environment(\.nativeMacLeadingInset) private var leadingInset
    let key: CatalogKey
    let title: String
    let subtitle: String
    let state: CatalogLoadState
    let model: GoosicAppModel

    var body: some SwiftUI.View {
        SwiftUI.ScrollView {
            SwiftUI.LazyVStack(alignment: .leading, spacing: 28) {
                if showsHeading {
                    SwiftUI.VStack(alignment: .leading, spacing: 4) {
                        SwiftUI.Text(heading).font(.largeTitle)
                        SwiftUI.Text(subtitle).font(.subheadline).foregroundStyle(.secondary)
                    }
                    .padding(.leading, leadingInset + 24)
                    .padding(.trailing, 24)
                }

                content
            }
            .padding(.vertical, 24)
            .padding(.bottom, 112)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var showsHeading: Bool {
        if case .route = key { return false }
        return true
    }

    private var heading: String {
        guard let page = state.page, !page.title.isEmpty else { return title }
        return page.title
    }

    @SwiftUI.ViewBuilder
    private var content: some SwiftUI.View {
        if state.isLoading {
            NativeMacCatalogSkeleton(rows: key.expectsTrackList ? .tracks : .shelves)
                .padding(.leading, leadingInset + 24)
                .padding(.trailing, 24)
        } else if let failure = state.failure {
            let text = catalogFailureText(
                code: failure.code,
                message: failure.message,
                subject: title.lowercased()
            )
            SwiftUI.VStack(alignment: .leading, spacing: 8) {
                SwiftUI.Text(text.title).font(.headline)
                SwiftUI.Text(text.detail).foregroundStyle(.secondary)
                SwiftUI.Button("Try again") { model.retry(key) }
            }
            .padding(.leading, leadingInset + 24)
            .padding(.trailing, 24)
        } else if let page = state.page {
            if page.isEmpty {
                SwiftUI.ContentUnavailableView(
                    "Nothing to show",
                    systemImage: "music.note",
                    description: SwiftUI.Text("\(title) came back empty.")
                )
                .padding(.leading, leadingInset)
            } else {
                if !page.tracks.isEmpty {
                    NativeMacTrackList(tracks: page.tracks, model: model)
                        .padding(.leading, leadingInset + 24)
                        .padding(.trailing, 24)
                }
                SwiftUI.ForEach(page.shelves) { shelf in
                    NativeMacShelf(shelf: shelf, model: model, presentation: .preferred(for: key, shelf: shelf))
                }
                if let cursor = page.nextCursor {
                    SwiftUI.HStack {
                        SwiftUI.Spacer()
                        switch model.continuationState(for: key) {
                        case .idle:
                            SwiftUI.EmptyView()
                        case .loading:
                            SwiftUI.ProgressView().controlSize(.small)
                        case .failed:
                            // An automatic continuation may fail for a transient network reason.
                            // Keep a compact retry affordance without turning the end of the page
                            // into a status message or restoring an explicit "Load more" button.
                            SwiftUI.Button(action: { model.retryContinuation(key) }) {
                                SwiftUI.Image(systemName: "arrow.clockwise")
                            }
                            .buttonStyle(.plain)
                            .help("Retry loading more")
                            .accessibilityLabel("Retry loading more")
                        }
                        SwiftUI.Spacer()
                    }
                    .frame(height: 24)
                    .id(cursor)
                    .onAppear {
                        if model.continuationState(for: key) == .idle {
                            model.loadMore(key)
                        }
                    }
                }
                if model.staleRefreshFailed.contains(key) {
                    // The page is still worth showing; it just should not imply it is current.
                    SwiftUI.Label(
                        "Showing saved content — couldn't refresh.",
                        systemImage: "wifi.exclamationmark"
                    )
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(.leading, leadingInset + 24)
                }
                if page.truncated {
                    SwiftUI.Text("This page was long, so only the first part is shown.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .padding(.leading, leadingInset + 24)
                }
            }
        } else {
            SwiftUI.VStack(alignment: .leading, spacing: 8) {
                SwiftUI.Text("Not loaded yet.").foregroundStyle(.secondary)
                SwiftUI.Button("Load \(title.lowercased())") { model.retry(key) }
            }
            .padding(.leading, leadingInset + 24)
        }
    }
}

struct NativeMacLibraryPage: SwiftUI.View {
    @SwiftUI.Environment(\.nativeMacLeadingInset) private var leadingInset
    @ObservedObject var store: NativeMacModelStore

    private var model: GoosicAppModel { store.model }

    private var section: PersonalLibrarySection {
        PersonalLibrarySection(rawValue: model.libraryTab) ?? .playlists
    }

    var body: some SwiftUI.View {
        if model.activeAccount == nil {
            NativeMacEmptyPage(
                title: "Library",
                icon: "music.note.list",
                message: "Sign in from Settings to load your playlists, liked songs, albums, and artists."
            )
            .padding(.leading, leadingInset)
        } else {
            SwiftUI.VStack(spacing: 0) {
                SwiftUI.Picker(
                    "Library section",
                    selection: SwiftUI.Binding(
                        get: { section },
                        set: { model.selectLibrarySection($0) }
                    )
                ) {
                    SwiftUI.ForEach(PersonalLibrarySection.allCases) { item in
                        SwiftUI.Text(item.rawValue).tag(item)
                    }
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .padding(.leading, leadingInset + 24)
                .padding(.trailing, 24)
                .padding(.top, 18)

                NativeMacCatalogPage(
                    key: section.key,
                    title: section.rawValue,
                    subtitle: "Your saved YouTube Music collection",
                    state: model.state(for: section.key),
                    model: model
                )
            }
        }
    }
}

private struct NativeMacShelf: SwiftUI.View {
    @SwiftUI.Environment(\.accessibilityReduceMotion) private var reduceMotion
    @SwiftUI.Environment(\.nativeMacLeadingInset) private var leadingInset
    @State private var leadingCard: String?
    let shelf: GoosicShelf
    let model: GoosicAppModel
    let presentation: ShelfPresentation

    private var currentIndex: Int { shelf.cards.firstIndex { $0.id == leadingCard } ?? 0 }
    /// Discovery cards start a station. Only track lists provide an explicit ordered context.
    private var playbackContext: [GoosicTrack] {
        []
    }

    var body: some SwiftUI.View {
        SwiftUI.VStack(alignment: .leading, spacing: 10) {
            SwiftUI.HStack {
                SwiftUI.Text(shelf.title).font(.title3.weight(.semibold))
                SwiftUI.Spacer()
                if presentation == .cards && shelf.cards.count > 1 {
                    shelfArrow("Previous items", icon: "chevron.left", offset: -3)
                        .disabled(currentIndex == 0)
                    shelfArrow("More items", icon: "chevron.right", offset: 3)
                        .disabled(currentIndex >= shelf.cards.count - 1)
                }
            }
            .padding(.leading, leadingInset + 24)
            .padding(.trailing, 24)
            if case .rows(let tracks) = presentation {
                NativeMacTrackList(tracks: tracks, model: model)
                    .padding(.leading, leadingInset + 24)
                    .padding(.trailing, 24)
            } else {
                SwiftUI.ScrollView(.horizontal, showsIndicators: false) {
                    SwiftUI.LazyHStack(alignment: .top, spacing: 16) {
                        SwiftUI.ForEach(shelf.cards) { card in
                            NativeMacCatalogCard(
                                card: card, model: model, playbackContext: playbackContext
                            )
                        }
                    }
                    .scrollTargetLayout()
                    .padding(.vertical, 2)
                }
                .safeAreaPadding(.leading, leadingInset + 24)
                .safeAreaPadding(.trailing, 24)
                .scrollPosition(id: $leadingCard, anchor: .leading)
            }
        }
    }

    private func shelfArrow(_ title: String, icon: String, offset: Int) -> some SwiftUI.View {
        SwiftUI.Button {
            let index = min(max(currentIndex + offset, 0), shelf.cards.count - 1)
            SwiftUI.withAnimation(reduceMotion ? nil : .easeInOut(duration: 0.25)) {
                leadingCard = shelf.cards[index].id
            }
        } label: {
            SwiftUI.Image(systemName: icon).frame(width: 22, height: 22)
        }
        .modifier(NativeMacGlassButtons())
        .buttonBorderShape(.circle)
        .accessibilityLabel("\(title) in \(shelf.title)")
        .help(title)
    }
}

private struct NativeMacTrackList: SwiftUI.View {
    let tracks: [GoosicTrack]
    let model: GoosicAppModel

    var body: some SwiftUI.View {
        SwiftUI.LazyVStack(spacing: 0) {
            SwiftUI.ForEach(tracks) { track in
                SwiftUI.Button {
                    model.launch(.ordered(track, tracks))
                } label: {
                    SwiftUI.HStack(spacing: 10) {
                        NativeMacArtwork(url: track.thumbnail, width: 38, height: 38)
                        SwiftUI.VStack(alignment: .leading, spacing: 2) {
                            SwiftUI.Text(track.title)
                                .lineLimit(1)
                            SwiftUI.Text(track.secondaryText)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                        SwiftUI.Spacer()
                        SwiftUI.Text(track.duration)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        SwiftUI.Image(systemName: "play.fill")
                            .imageScale(.small)
                    }
                    .contentShape(Rectangle())
                    .padding(.vertical, 6)
                }
                .buttonStyle(.plain)
                .contextMenu { NativeMacTrackMenuItems(track: track, model: model, context: tracks) }
                SwiftUI.Divider()
            }
        }
    }
}

private struct NativeMacCatalogCard: SwiftUI.View {
    @SwiftUI.Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var hovering = false
    let card: GoosicCard
    let model: GoosicAppModel
    let playbackContext: [GoosicTrack]

    var body: some SwiftUI.View {
        SwiftUI.Button(action: activate) {
            SwiftUI.VStack(alignment: .leading, spacing: 6) {
                NativeMacArtwork(url: card.thumbnail, width: 158, height: 158)
                    .overlay(alignment: .bottomTrailing) {
                        if hovering && card.action != nil {
                            SwiftUI.Image(systemName: actionIcon)
                                .font(.title3.weight(.semibold))
                                .frame(width: 36, height: 36)
                                .background(.regularMaterial, in: Circle())
                                .padding(8)
                                .transition(.opacity)
                                .accessibilityHidden(true)
                        }
                    }
                SwiftUI.Text(card.title)
                    .font(.subheadline)
                    .lineLimit(1)
                SwiftUI.Text(card.subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            .frame(width: 158, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(card.action == nil)
        .contextMenu {
            switch card.action {
            case .play(let track):
                NativeMacTrackMenuItems(track: track, model: model, context: playbackContext)
            case .show(let entity):
                SwiftUI.Button("Open", systemImage: "arrow.right.circle") { model.show(entity) }
                SwiftUI.Divider()
                SwiftUI.Button("Copy link", systemImage: "link") { NativeMacLinks.copy(NativeMacLinks.url(for: entity)) }
            case .none:
                SwiftUI.EmptyView()
            }
        }
        .onHover { hovering = $0 }
        .animation(reduceMotion ? nil : .easeOut(duration: 0.18), value: hovering)
        .help(card.title)
        .accessibilityElement(children: .combine)
    }

    private var actionIcon: String {
        if case .play = card.action { return "play.fill" }
        return "chevron.right"
    }

    private func activate() {
        switch card.action {
        case .show(let entity): model.show(entity)
        case .play(let track): model.launch(.resolve(track: track, explicitTracks: playbackContext))
        case .none: break
        }
    }
}

private struct NativeMacArtwork: SwiftUI.View {
    let url: String?
    let width: CGFloat
    let height: CGFloat

    var body: some SwiftUI.View {
        SwiftUI.AsyncImage(url: validatedURL) { phase in
            switch phase {
            case .success(let image):
                image.resizable().scaledToFill()
            default:
                SwiftUI.ZStack {
                    SwiftUI.Color.accentColor.opacity(0.14)
                    SwiftUI.Image(systemName: "music.note")
                        .foregroundStyle(.secondary)
                }
            }
        }
        .frame(width: width, height: height)
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    private var validatedURL: URL? {
        guard let url, let value = URL(string: url), ArtworkCache.isAllowed(value) else { return nil }
        return value
    }
}

/// Menu entries for a track, shared by catalog cards, track rows, the queue, and the player's
/// More button, so a song offers the same actions wherever it appears. Only actions the model
/// can carry out today are listed: queue insertion and account-bound actions (like, playlists)
/// are Core work and are not offered until Core has them.
struct NativeMacTrackMenuItems: SwiftUI.View {
    let track: GoosicTrack
    let model: GoosicAppModel
    var context: [GoosicTrack] = []
    var includePlay = true

    var body: some SwiftUI.View {
        if includePlay {
            SwiftUI.Button("Play", systemImage: "play.fill") {
                model.launch(.resolve(track: track, explicitTracks: context))
            }
        }
        SwiftUI.Button("Start radio", systemImage: "dot.radiowaves.left.and.right") { model.startRadio(from: track) }
        if track.artistID != nil || track.albumID != nil {
            SwiftUI.Divider()
        }
        if let id = track.artistID {
            SwiftUI.Button("Go to artist", systemImage: "music.mic") { model.show(.artist(id)) }
        }
        if let id = track.albumID {
            SwiftUI.Button("Go to album", systemImage: "square.stack") { model.show(.album(id)) }
        }
        if model.activeAccount != nil {
            SwiftUI.Divider()
            NativeMacAddToPlaylistMenu(track: track, model: model)
        }
        SwiftUI.Divider()
        SwiftUI.Button("Copy link", systemImage: "link") { NativeMacLinks.copy(NativeMacLinks.url(for: track)) }
    }
}

/// The destinations a track can be added to.
///
/// The list is read when the submenu opens rather than kept for the session: the destinations are
/// the entire point of the menu, and a playlist made in another client — or here, a moment ago —
/// missing from it is indistinguishable from that playlist not existing.
private struct NativeMacAddToPlaylistMenu: SwiftUI.View {
    let track: GoosicTrack
    let model: GoosicAppModel

    var body: some SwiftUI.View {
        SwiftUI.Menu("Add to playlist") {
            SwiftUI.Button("New playlist…", systemImage: "plus") {
                model.beginNewPlaylist(for: track)
            }
            switch model.userPlaylistsState {
            case .loading:
                SwiftUI.Text("Loading your playlists…")
            case .failed(let message):
                SwiftUI.Text(message)
                SwiftUI.Button("Try again") { model.loadUserPlaylists(force: true) }
            case .idle:
                if !model.userPlaylists.isEmpty { SwiftUI.Divider() }
                SwiftUI.ForEach(model.userPlaylists) { playlist in
                    SwiftUI.Button(playlist.title) {
                        model.addTrackToPlaylist(track, playlist: playlist)
                    }
                }
            }
        }
        .onAppear { model.loadUserPlaylists() }
    }
}

/// Public YouTube Music links for the clipboard. These carry an identifier and nothing else:
/// no account, no cookie, no media URL.
enum NativeMacLinks {
    static func url(for track: GoosicTrack) -> URL {
        var components = URLComponents(string: "https://music.youtube.com/watch")!
        components.queryItems = [URLQueryItem(name: "v", value: track.videoID)]
        return components.url!
    }

    static func url(for entity: GoosicEntityReference) -> URL {
        switch entity {
        case .album(let id), .artist(let id):
            var components = URLComponents(string: "https://music.youtube.com/browse/")!
            components.path = "/browse/" + id
            return components.url!
        case .playlist(let id):
            var components = URLComponents(string: "https://music.youtube.com/playlist")!
            components.queryItems = [URLQueryItem(name: "list", value: id)]
            return components.url!
        }
    }

    static func copy(_ url: URL) {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(url.absoluteString, forType: .string)
    }
}
#endif


/// The shape of a page, drawn before its content arrives.
///
/// A spinner on a blank page says only that something is happening. It gives the eye nothing to
/// settle on, and when the content lands it lands all at once, in a layout that shares nothing
/// with what was there a moment ago — so every load reads as a jump. Blocks in the positions the
/// real rows and cards will occupy answer the question the spinner does not: what is coming, and
/// roughly how much of it.
///
/// Deliberately still. A shimmer here would animate several dozen layers behind whatever the user
/// is actually listening to, and `accessibilityReduceMotion` would have to turn it off again for
/// the people most likely to be bothered by it — so the placeholder simply holds its place.
struct NativeMacCatalogSkeleton: SwiftUI.View {
    enum Shape {
        /// A playlist or album: one column of rows.
        case tracks
        /// Home, Explore, a library section: titled shelves of artwork.
        case shelves
    }

    let rows: Shape

    var body: some SwiftUI.View {
        SwiftUI.VStack(alignment: .leading, spacing: rows == .tracks ? 10 : 26) {
            switch rows {
            case .tracks:
                SwiftUI.ForEach(0..<8, id: \.self) { _ in trackRow }
            case .shelves:
                SwiftUI.ForEach(0..<2, id: \.self) { _ in shelf }
            }
        }
        .padding(.top, 6)
        // One label for the whole placeholder: a screen reader announcing forty blocks
        // individually is worse than the spinner this replaces.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Loading")
    }

    private var trackRow: some SwiftUI.View {
        SwiftUI.HStack(spacing: 10) {
            block(width: 38, height: 38, radius: 6)
            SwiftUI.VStack(alignment: .leading, spacing: 6) {
                block(width: 220, height: 11, radius: 3)
                block(width: 140, height: 9, radius: 3)
            }
            SwiftUI.Spacer()
        }
    }

    private var shelf: some SwiftUI.View {
        SwiftUI.VStack(alignment: .leading, spacing: 12) {
            block(width: 170, height: 15, radius: 4)
            SwiftUI.HStack(alignment: .top, spacing: 16) {
                SwiftUI.ForEach(0..<6, id: \.self) { _ in
                    SwiftUI.VStack(alignment: .leading, spacing: 8) {
                        block(width: 150, height: 150, radius: 10)
                        block(width: 120, height: 11, radius: 3)
                        block(width: 80, height: 9, radius: 3)
                    }
                }
            }
            // The real shelf scrolls sideways off the page; clipping keeps the placeholder from
            // widening the window it is standing in for.
            .frame(maxWidth: .infinity, alignment: .leading)
            .clipped()
        }
    }

    private func block(width: CGFloat, height: CGFloat, radius: CGFloat) -> some SwiftUI.View {
        SwiftUI.RoundedRectangle(cornerRadius: radius, style: .continuous)
            .fill(.quaternary)
            .frame(width: width, height: height)
    }
}
