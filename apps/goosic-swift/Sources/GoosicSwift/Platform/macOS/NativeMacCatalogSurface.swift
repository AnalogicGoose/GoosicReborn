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
                    SwiftUI.HStack(alignment: .firstTextBaseline, spacing: 16) {
                        SwiftUI.VStack(alignment: .leading, spacing: 4) {
                            SwiftUI.Text(heading).font(.system(size: 30, weight: .bold))
                            // A page's own name says enough; Music adds no tagline under it.
                            if case .route = key {} else {
                                SwiftUI.Text(subtitle).font(.subheadline).foregroundStyle(.secondary)
                            }
                        }
                        SwiftUI.Spacer()
                        if case .artist(let channelID) = key, model.activeAccount != nil {
                            subscribeButton(channelID: channelID)
                        }
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
        // The page fades under the title bar, so a scrolled title never runs into the window's
        // buttons.
        .mask(
            SwiftUI.VStack(spacing: 0) {
                SwiftUI.LinearGradient(colors: [.clear, .black], startPoint: .top, endPoint: .bottom)
                    .frame(height: 52)
                SwiftUI.Rectangle().fill(SwiftUI.Color.black)
            }
            .ignoresSafeArea()
        )
    }

    private var entity: GoosicEntityReference? {
        switch key {
        case .album(let id): return .album(id)
        case .playlist(let id): return .playlist(id)
        case .artist(let id): return .artist(id)
        case .category(let id): return .category(id)
        default: return nil
        }
    }

    private var showsHeading: Bool {
        switch key {
        // Music opens every page with its name in a large bold title.
        case .route:
            return true
        case .album, .playlist:
            return false
        case .search, .artist, .category, .library:
            return true
        }
    }

    private var collectionKind: String? {
        switch key {
        case .album: return "Album"
        case .playlist(let id): return GoosicEntityReference.playlist(id).isLikedMusic ? "Liked Music" : "Playlist"
        default: return nil
        }
    }

    /// Sorting and finding are offered on lists of songs someone put together.
    private var canSortAndFind: Bool {
        if case .playlist = key { return true }
        return false
    }

    private var heading: String {
        guard let page = state.page, !page.title.isEmpty else { return title }
        return page.title
    }

    /// The rows on screen, in the order on screen. Play and a clicked row queue exactly these.
    private func shownTracks(_ page: CatalogPageView) -> [GoosicTrack] {
        model.trackSortOrder.sorted(model.visible(page.tracks)).filter { TrackSortOrder.matches($0, model.trackFindText) }
    }

    private func source(_ page: CatalogPageView) -> QueueSource {
        let name = page.title.isEmpty ? title : page.title
        if case .search = key { return QueueSource(title: "Search", key: key) }
        return QueueSource(title: name, key: key)
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
                if model.debugMode {
                    SwiftUI.Text(text.detail).foregroundStyle(.secondary)
                    SwiftUI.Text(failure.code).font(.caption.monospaced()).foregroundStyle(.tertiary)
                }
                SwiftUI.Button("Try again") { model.retry(key) }
            }
            .frame(maxWidth: .infinity, minHeight: 360, alignment: .center)
            .multilineTextAlignment(.center)
            .padding(.leading, leadingInset)
            .padding(.trailing, 24)
        } else if let page = state.page {
            if page.isEmpty {
                SwiftUI.ContentUnavailableView(
                    "Nothing to show",
                    systemImage: "music.note",
                    description: SwiftUI.Text("\(title) came back empty.")
                )
                .frame(maxWidth: .infinity, minHeight: 360, alignment: .center)
                .multilineTextAlignment(.center)
                .padding(.leading, leadingInset)
                .padding(.trailing, 24)
            } else {
                if model.staleRefreshFailed.contains(key) {
                    SwiftUI.Label(
                        "Showing saved content — couldn't refresh.",
                        systemImage: "wifi.exclamationmark"
                    )
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .padding(.leading, leadingInset + 24)
                    .padding(.trailing, 24)
                }
                loaded(page)
            }
        } else {
            SwiftUI.VStack(alignment: .leading, spacing: 8) {
                SwiftUI.Text("Not loaded yet.").foregroundStyle(.secondary)
                SwiftUI.Button("Load \(title.lowercased())") { model.retry(key) }
            }
            .frame(maxWidth: .infinity, minHeight: 360, alignment: .center)
            .multilineTextAlignment(.center)
            .padding(.leading, leadingInset)
            .padding(.trailing, 24)
        }
    }

    @SwiftUI.ViewBuilder
    private func loaded(_ page: CatalogPageView) -> some SwiftUI.View {
        let tracks = shownTracks(page)
        if let collectionKind, !page.tracks.isEmpty {
            NativeMacCollectionHeader(
                page: page,
                kind: collectionKind,
                key: key,
                entity: entity,
                shownTracks: tracks,
                source: source(page),
                model: model
            )
            .padding(.leading, leadingInset + 32)
            .padding(.trailing, 48)
            .padding(.bottom, 8)
        }
        if !page.tracks.isEmpty {
            NativeMacTrackList(
                tracks: tracks,
                model: model,
                source: source(page),
                container: entity,
                // Album pages leave the album column off, since every row shares one.
                showsAlbum: { if case .playlist = key { return true } else { return false } }(),
                // Moving is addressed by position, so it waits while a sort or find reorders
                // or hides the list's own rows.
                canReorder: model.canReorder(entity) && model.trackSortOrder == .custom && model.trackFindText.isEmpty,
                numbered: { if case .album = key { return true } else { return false } }()
            )
            .padding(.leading, leadingInset + 24)
            .padding(.trailing, 24)
            if tracks.isEmpty && !model.trackFindText.isEmpty {
                SwiftUI.Text("No songs match “\(model.trackFindText)”.")
                    .foregroundStyle(.secondary)
                    .padding(.leading, leadingInset + 24)
            }
            if let allTracksID = page.allTracksID {
                SwiftUI.Button("Show all") { model.show(.playlist(allTracksID)) }
                    .buttonStyle(.bordered)
                    .buttonBorderShape(.capsule)
                    .padding(.leading, leadingInset + 24)
            }
        }
        if case .library = key {
            // A library page is one grid that scrolls down, however many parts it arrives in.
            NativeMacCardGrid(
                cards: page.shelves.flatMap(\.cards), model: model, inLibrary: true
            )
            .padding(.leading, leadingInset + 24)
            .padding(.trailing, 24)
        } else {
            SwiftUI.ForEach(page.shelves) { shelf in
                NativeMacShelf(
                    shelf: shelf, model: model,
                    presentation: .preferred(for: key, shelf: shelf),
                    source: QueueSource(title: shelf.title, key: key)
                )
            }
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
        // How Goosic works inside is a debugging detail; the count above already says a list
        // is partial.
        if page.truncated && model.debugMode {
            SwiftUI.Text("This page was long, so only the first part is shown.")
                .font(.caption)
                .foregroundStyle(.secondary)
                .padding(.leading, leadingInset + 24)
        }
    }

    private func subscribeButton(channelID: String) -> some SwiftUI.View {
        let subscribed = model.isSubscribed(to: channelID)
        return SwiftUI.Button {
            model.setSubscribed(!subscribed, to: channelID, name: heading)
        } label: {
            SwiftUI.Label(subscribed ? "Subscribed" : "Subscribe",
                          systemImage: subscribed ? "checkmark" : "plus")
        }
        .modifier(NativeMacGlassButtons())
        .disabled(model.libraryOperationInProgress)
    }
}

/// The top of an album, a playlist, or Liked Music, laid out as Music lays it out: large art,
/// a large bold title, the artist or owner in the accent colour, a quiet line of facts, then
/// Shuffle, Play, and Add. It uses only what is true: the page's own cover (the first song's art
/// only when there is none), the upstream title and subtitle, and a count that says when the
/// list is not finished.
private struct NativeMacCollectionHeader: SwiftUI.View {
    let page: CatalogPageView
    let kind: String
    let key: CatalogKey
    let entity: GoosicEntityReference?
    let shownTracks: [GoosicTrack]
    let source: QueueSource
    let model: GoosicAppModel

    private var cover: String? {
        page.thumbnail ?? page.tracks.first?.thumbnail ?? page.shelves.first?.cards.first?.thumbnail
    }

    /// While the list this page queued is playing, its Play button pauses and resumes it
    /// instead of starting it again.
    private var playingHere: Bool { model.isPlayingFrom(key) }

    private var lines: CollectionHeaderLines {
        CollectionHeaderLines(subtitle: page.subtitle, kind: kind)
    }

    var body: some SwiftUI.View {
        SwiftUI.HStack(alignment: .bottom, spacing: 34) {
            NativeMacArtwork(url: cover, width: 270, height: 270, cornerRadius: 10)
                .shadow(color: .black.opacity(0.22), radius: 18, y: 10)

            SwiftUI.VStack(alignment: .leading, spacing: 6) {
                SwiftUI.Text(page.title.isEmpty ? kind : page.title)
                    .font(.system(size: 32, weight: .bold))
                    .lineLimit(3)
                    .fixedSize(horizontal: false, vertical: true)
                if let byline = lines.byline {
                    byline_(byline)
                }
                SwiftUI.Text(metaLine)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)

                SwiftUI.HStack(spacing: 12) {
                    circleButton("Shuffle", "shuffle", active: model.shuffle) {
                        if !model.shuffle { model.toggleShuffle() }
                        if let pick = shownTracks.randomElement() {
                            model.launch(.ordered(pick, shownTracks), source: source)
                        }
                    }
                    .disabled(shownTracks.isEmpty)

                    SwiftUI.Button {
                        if playingHere {
                            model.togglePause()
                        } else if let first = shownTracks.first {
                            model.launch(.ordered(first, shownTracks), source: source)
                        }
                    } label: {
                        SwiftUI.Label(
                            playingHere && !model.isPaused ? "Pause" : "Play",
                            systemImage: playingHere && !model.isPaused ? "pause.fill" : "play.fill"
                        )
                        .font(.headline)
                        .foregroundStyle(SwiftUI.Color.goosicPink)
                        .frame(width: 150, height: 34)
                        .contentShape(Capsule())
                    }
                    .buttonStyle(.plain)
                    .modifier(NativeMacCapsuleSurface())
                    .disabled(shownTracks.isEmpty && !playingHere)

                    if let entity, case .playlist(let id) = entity, model.activeAccount != nil,
                       !entity.isLikedMusic,
                       (model.ownedPlaylist(for: entity) == nil || model.isSaved(playlist: id)) {
                        let saved = model.isSaved(playlist: id)
                        SwiftUI.Button {
                            model.setSaved(!saved, playlist: id, title: page.title)
                        } label: {
                            SwiftUI.Label(saved ? "Saved" : "Save",
                                          systemImage: saved ? "checkmark" : "plus")
                        }
                        .modifier(NativeMacGlassButtons())
                        .foregroundStyle(SwiftUI.Color.goosicPink)
                        .help(saved ? "Remove from Library" : "Save to Library")
                        .accessibilityLabel(saved ? "Saved. Remove from Library" : "Save to Library")
                        .disabled(model.libraryOperationInProgress)
                    }
                }
                .padding(.top, 18)
            }
            .frame(maxWidth: 760, alignment: .leading)

            SwiftUI.Spacer(minLength: 0)
        }
        .padding(.top, 8)
    }

    private func byline_(_ text: String) -> some SwiftUI.View {
        SwiftUI.Group {
            if let artistID = page.tracks.first(where: { $0.artist == text })?.artistID {
                SwiftUI.Button(text) { model.show(.artist(artistID)) }
                    .buttonStyle(.plain)
            } else {
                SwiftUI.Text(text)
            }
        }
        .font(.system(size: 24, weight: .medium))
        .foregroundStyle(SwiftUI.Color.goosicPink)
        .lineLimit(1)
    }

    /// "Album · 2023 · 12 songs, 41 minutes" — the facts after the byline, and a count that says
    /// "100+" while more rows are still to come.
    private var metaLine: String {
        var parts = lines.facts
        parts.append(PlayerText.songCount(page.tracks.count, hasMore: page.hasMore)
            + (page.hasMore ? "" : totalLength.map { ", \($0)" } ?? ""))
        return parts.joined(separator: " · ")
    }

    private var totalLength: String? {
        let seconds = page.tracks.compactMap { GoosicAppModel.durationSeconds($0.duration) }.reduce(0, +)
        guard seconds > 0 else { return nil }
        let minutes = Int(seconds) / 60
        return minutes >= 60 ? "\(minutes / 60) hr \(minutes % 60) min" : "\(minutes) min"
    }

    private func circleButton(_ title: String, _ symbol: String, active: Bool = false,
                              action: @escaping () -> Void) -> some SwiftUI.View {
        SwiftUI.Button(action: action) {
            SwiftUI.Image(systemName: symbol)
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(active ? SwiftUI.Color.goosicPink : SwiftUI.Color.primary)
                .frame(width: 34, height: 34)
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .modifier(NativeMacCircleSurface())
        .help(title)
        .accessibilityLabel(title)
    }
}

/// Music's quiet filled capsule behind Play.
private struct NativeMacCapsuleSurface: SwiftUI.ViewModifier {
    func body(content: Content) -> some SwiftUI.View {
        if #available(macOS 26.0, *) {
            content.glassEffect(.regular.interactive(), in: .capsule)
        } else {
            content.background(.quaternary, in: Capsule())
        }
    }
}

private struct NativeMacCircleSurface: SwiftUI.ViewModifier {
    func body(content: Content) -> some SwiftUI.View {
        if #available(macOS 26.0, *) {
            content.glassEffect(.regular.interactive(), in: .circle)
        } else {
            content.background(.quaternary, in: Circle())
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
            // The section picker is a window toolbar item (see `NativeMacRootView`): macOS 26
            // draws the Liquid Glass capsule only for controls in the toolbar.
            SwiftUI.VStack(spacing: 0) {
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
    @SwiftUI.Environment(\.goosicReduceMotion) private var reduceMotion
    @SwiftUI.Environment(\.nativeMacLeadingInset) private var leadingInset
    @SwiftUI.State private var leadingCard: String?
    let shelf: GoosicShelf
    let model: GoosicAppModel
    let presentation: ShelfPresentation
    let source: QueueSource

    /// Explicit songs are left out of every shelf when the listener asked for that.
    private var cards: [GoosicCard] {
        guard model.hideExplicit else { return shelf.cards }
        return shelf.cards.filter { card in
            if case .play(let track) = card.action { return !track.explicit }
            return true
        }
    }

    private var currentIndex: Int { cards.firstIndex { $0.id == leadingCard } ?? 0 }
    /// Discovery cards start a station. Only track lists provide an explicit ordered context.
    private var playbackContext: [GoosicTrack] {
        []
    }

    var body: some SwiftUI.View {
        SwiftUI.VStack(alignment: .leading, spacing: 10) {
            SwiftUI.HStack {
                SwiftUI.Text(shelf.title).font(.title3.weight(.semibold))
                SwiftUI.Spacer()
                if presentation == .cards && cards.count > 1 {
                    shelfArrow("Previous items", icon: "chevron.left", offset: -3)
                        .disabled(currentIndex == 0)
                    shelfArrow("More items", icon: "chevron.right", offset: 3)
                        .disabled(currentIndex >= cards.count - 1)
                }
            }
            .padding(.leading, leadingInset + 24)
            .padding(.trailing, 24)
            switch presentation {
            case .rows(let tracks):
                NativeMacTrackList(tracks: model.visible(tracks), model: model, source: source)
                    .padding(.leading, leadingInset + 24)
                    .padding(.trailing, 24)
            case .columns(let tracks):
                NativeMacTrackColumns(tracks: model.visible(tracks), model: model, source: source)
            case .tiles:
                NativeMacCategoryGrid(cards: cards, model: model)
                    .padding(.leading, leadingInset + 24)
                    .padding(.trailing, 24)
            case .grid:
                NativeMacCardGrid(cards: cards, model: model, inLibrary: false)
                    .padding(.leading, leadingInset + 24)
                    .padding(.trailing, 24)
            case .cards:
                SwiftUI.ScrollView(.horizontal, showsIndicators: false) {
                    SwiftUI.LazyHStack(alignment: .top, spacing: 16) {
                        SwiftUI.ForEach(cards) { card in
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
            let index = min(max(currentIndex + offset, 0), cards.count - 1)
            SwiftUI.withAnimation(reduceMotion ? nil : .easeInOut(duration: 0.25)) {
                leadingCard = cards[index].id
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

/// Quick picks and the like: song rows in columns of four that scroll sideways.
private struct NativeMacTrackColumns: SwiftUI.View {
    @SwiftUI.Environment(\.nativeMacLeadingInset) private var leadingInset
    let tracks: [GoosicTrack]
    let model: GoosicAppModel
    let source: QueueSource

    var body: some SwiftUI.View {
        SwiftUI.ScrollView(.horizontal, showsIndicators: false) {
            SwiftUI.LazyHGrid(
                rows: Array(repeating: SwiftUI.GridItem(.fixed(56), spacing: 6), count: min(4, max(tracks.count, 1))),
                spacing: 18
            ) {
                SwiftUI.ForEach(Array(tracks.enumerated()), id: \.offset) { _, track in
                    cell(track)
                }
            }
            .padding(.vertical, 2)
        }
        .safeAreaPadding(.leading, leadingInset + 24)
        .safeAreaPadding(.trailing, 24)
    }

    private func cell(_ track: GoosicTrack) -> some SwiftUI.View {
        let playing = model.currentTrack?.videoID == track.videoID
        return SwiftUI.Button {
            model.launch(.ordered(track, tracks), source: source)
        } label: {
            SwiftUI.HStack(spacing: 10) {
                NativeMacArtwork(url: track.thumbnail, width: 48, height: 48)
                SwiftUI.VStack(alignment: .leading, spacing: 2) {
                    SwiftUI.Text(track.title)
                        .lineLimit(1)
                        .foregroundStyle(playing ? SwiftUI.Color.goosicPink : SwiftUI.Color.primary)
                    SwiftUI.Text(track.secondaryText)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                SwiftUI.Spacer(minLength: 0)
            }
            .frame(width: 300, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .contextMenu { NativeMacTrackMenuItems(track: track, model: model, context: tracks, source: source) }
        .help(track.title)
    }
}

/// Cards laid out as one grid that scrolls down, as Apple Music draws a library.
private struct NativeMacCardGrid: SwiftUI.View {
    let cards: [GoosicCard]
    let model: GoosicAppModel
    let inLibrary: Bool

    var body: some SwiftUI.View {
        SwiftUI.LazyVGrid(
            columns: [SwiftUI.GridItem(.adaptive(minimum: 158, maximum: 190), spacing: 18, alignment: .top)],
            alignment: .leading,
            spacing: 22
        ) {
            SwiftUI.ForEach(cards) { card in
                NativeMacCatalogCard(card: card, model: model, playbackContext: [], inLibrary: inLibrary)
            }
        }
    }
}

/// Moods & genres: a wrapping grid of tiles, each with the colour YouTube Music gives it as a
/// stripe. A tile opens its category page.
private struct NativeMacCategoryGrid: SwiftUI.View {
    let cards: [GoosicCard]
    let model: GoosicAppModel

    var body: some SwiftUI.View {
        SwiftUI.LazyVGrid(
            columns: [SwiftUI.GridItem(.adaptive(minimum: 170, maximum: 260), spacing: 12)],
            alignment: .leading,
            spacing: 12
        ) {
            SwiftUI.ForEach(cards) { card in
                SwiftUI.Button {
                    if case .show(let entity) = card.action { model.show(entity) }
                } label: {
                    SwiftUI.HStack(spacing: 0) {
                        SwiftUI.Rectangle()
                            .fill(stripe(card.color))
                            .frame(width: 6)
                        SwiftUI.Text(card.title)
                            .font(.subheadline.weight(.semibold))
                            .lineLimit(2)
                            .padding(.horizontal, 12)
                        SwiftUI.Spacer(minLength: 0)
                    }
                    .frame(height: 48)
                    .background(.quaternary.opacity(0.6))
                    .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(card.action == nil)
                .help(card.title)
            }
        }
    }

    private func stripe(_ hex: String?) -> SwiftUI.Color {
        guard let rgb = categoryColor(hex) else { return SwiftUI.Color.goosicPink }
        return SwiftUI.Color(red: rgb.red, green: rgb.green, blue: rgb.blue)
    }
}

/// A list of songs, drawn as Music draws one: album tracks by number, other lists by artwork;
/// a click selects and a double-click plays; hovering shows a play button in place of the number
/// or over the art, and every row has its own More button. A liked song shows its heart in the
/// leading gutter, where Music shows its star.
private struct NativeMacTrackList: SwiftUI.View {
    let tracks: [GoosicTrack]
    let model: GoosicAppModel
    var source: QueueSource? = nil
    /// The playlist the rows belong to, when they can be taken out of it.
    var container: GoosicEntityReference? = nil
    /// Playlist rows name their album in a column of their own, as Spotify's and Music's do.
    var showsAlbum = false
    /// Rows may be moved: an owned playlist, shown in its own order.
    var canReorder = false
    /// Album tracks are numbered rather than shown with the same cover on every row.
    var numbered = false

    @SwiftUI.State private var hovered: Int?
    @SwiftUI.State private var selected: Int?

    var body: some SwiftUI.View {
        SwiftUI.LazyVStack(spacing: 0) {
            SwiftUI.ForEach(Array(tracks.enumerated()), id: \.offset) { index, track in
                row(track, index: index)
                if index < tracks.count - 1 {
                    SwiftUI.Divider().padding(.leading, numbered ? 72 : 90).opacity(0.6)
                }
            }
        }
    }

    private func play(_ track: GoosicTrack) {
        model.launch(.ordered(track, tracks), source: source)
    }

    private func row(_ track: GoosicTrack, index: Int) -> some SwiftUI.View {
        let playing = model.currentTrack?.videoID == track.videoID
        let isHovered = hovered == index
        let liked = model.rating(of: track) == .liked
        return SwiftUI.HStack(spacing: 12) {
            // The gutter: a liked song's heart, or on hover the way to like it.
            SwiftUI.Button { model.toggleLike(track) } label: {
                SwiftUI.Image(systemName: liked ? "heart.fill" : "heart")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(liked ? SwiftUI.Color.goosicPink : SwiftUI.Color.secondary)
                    .frame(width: 18, height: 18)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .opacity(liked || (isHovered && model.activeAccount != nil) ? 1 : 0)
            .disabled(model.activeAccount == nil)
            .help(liked ? "Remove from Liked Music" : "Like")

            leading(track, index: index, playing: playing, hovered: isHovered)

            SwiftUI.VStack(alignment: .leading, spacing: 2) {
                SwiftUI.HStack(spacing: 5) {
                    SwiftUI.Text(track.title)
                        .lineLimit(1)
                        .foregroundStyle(playing ? SwiftUI.Color.goosicPink : SwiftUI.Color.primary)
                    if track.explicit {
                        SwiftUI.Image(systemName: "e.square.fill")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                            .accessibilityLabel("Explicit")
                    }
                }
                SwiftUI.Text(showsAlbum || numbered ? (track.artist.isEmpty ? track.secondaryText : track.artist) : track.secondaryText)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            if showsAlbum {
                SwiftUI.Text(track.album)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }

            SwiftUI.Text(track.duration)
                .font(.callout.monospacedDigit())
                .foregroundStyle(.secondary)
                .frame(minWidth: 44, alignment: .trailing)

            SwiftUI.Menu {
                NativeMacTrackMenuItems(
                    track: track, model: model, context: tracks, source: source, container: container
                )
                reorderItems(track)
            } label: {
                SwiftUI.Image(systemName: "ellipsis")
                    .font(.system(size: 13, weight: .bold))
                    .foregroundStyle(isHovered || selected == index ? SwiftUI.Color.goosicPink : SwiftUI.Color.secondary)
                    .frame(width: 26, height: 26)
                    .contentShape(Rectangle())
            }
            .menuStyle(.button)
            .buttonStyle(.plain)
            .menuIndicator(.hidden)
            .fixedSize()
            .help("More")
        }
        .padding(.horizontal, 8)
        .padding(.vertical, numbered ? 10 : 6)
        .background(
            SwiftUI.RoundedRectangle(cornerRadius: 8, style: .continuous)
                .fill(SwiftUI.Color.primary.opacity(selected == index ? 0.1 : (isHovered ? 0.05 : 0)))
        )
        .contentShape(Rectangle())
        .onHover { inside in
            if inside { hovered = index } else if hovered == index { hovered = nil }
        }
        .onTapGesture(count: 2) { play(track) }
        .onTapGesture { selected = index }
        .contextMenu {
            NativeMacTrackMenuItems(
                track: track, model: model, context: tracks, source: source, container: container
            )
            reorderItems(track)
        }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isButton)
        .accessibilityAction { play(track) }
    }

    /// The number or the art, which becomes a play button under the pointer and a speaker for
    /// the song playing now.
    @SwiftUI.ViewBuilder
    private func leading(_ track: GoosicTrack, index: Int, playing: Bool, hovered: Bool) -> some SwiftUI.View {
        let button = SwiftUI.Button { play(track) } label: {
            SwiftUI.Image(systemName: "play.fill")
                .font(.system(size: 13))
                .foregroundStyle(numbered ? SwiftUI.Color.goosicPink : SwiftUI.Color.white)
                .frame(width: 30, height: 30)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help("Play")
        if numbered {
            SwiftUI.ZStack {
                if hovered && !playing {
                    button
                } else if playing {
                    SwiftUI.Image(systemName: model.isPaused ? "speaker.fill" : "speaker.wave.2.fill")
                        .font(.caption)
                        .foregroundStyle(SwiftUI.Color.goosicPink)
                } else {
                    SwiftUI.Text("\(index + 1)")
                        .font(.callout.monospacedDigit())
                        .foregroundStyle(.secondary)
                }
            }
            .frame(width: 28)
        } else {
            NativeMacArtwork(url: track.thumbnail, width: 40, height: 40, cornerRadius: 5)
                .overlay {
                    if hovered || playing {
                        SwiftUI.RoundedRectangle(cornerRadius: 5).fill(.black.opacity(0.35))
                        if hovered && !playing {
                            button
                        } else {
                            SwiftUI.Image(systemName: model.isPaused ? "speaker.fill" : "speaker.wave.2.fill")
                                .font(.caption)
                                .foregroundStyle(.white)
                        }
                    }
                }
        }
    }

    private func reorderItems(_ track: GoosicTrack) -> some SwiftUI.View {
        reorderButtons(track).labelStyle(.titleAndIcon)
    }

    @SwiftUI.ViewBuilder
    private func reorderButtons(_ track: GoosicTrack) -> some SwiftUI.View {
        if canReorder, let container {
            SwiftUI.Divider()
            SwiftUI.Button("Move Up", systemImage: "arrow.up") { model.move(track, in: container, up: true) }
                .disabled(track.id == tracks.first?.id)
            SwiftUI.Button("Move Down", systemImage: "arrow.down") { model.move(track, in: container, up: false) }
                .disabled(track.id == tracks.last?.id)
        }
    }
}

private struct NativeMacCatalogCard: SwiftUI.View {
    @SwiftUI.Environment(\.goosicReduceMotion) private var reduceMotion
    @SwiftUI.State private var hovering = false
    let card: GoosicCard
    let model: GoosicAppModel
    let playbackContext: [GoosicTrack]
    var inLibrary = false

    /// Music videos and episodes are drawn wide, in their own shape.
    private var isWide: Bool {
        if case .play(let track) = card.action { return track.isVideo }
        return false
    }

    private var artWidth: CGFloat { isWide ? 280 : 158 }

    var body: some SwiftUI.View {
        SwiftUI.Button(action: activate) {
            SwiftUI.VStack(alignment: card.isArtist ? .center : .leading, spacing: 6) {
                NativeMacArtwork(url: card.thumbnail, width: artWidth, height: 158, round: card.isArtist)
                    .overlay(alignment: .bottomTrailing) {
                        if hovering && card.action != nil && !card.isArtist {
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
            .frame(width: artWidth, alignment: card.isArtist ? .center : .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(card.action == nil)
        .contextMenu {
            switch card.action {
            case .play(let track):
                NativeMacTrackMenuItems(track: track, model: model, context: playbackContext)
            case .show(let entity):
                SwiftUI.Group {
                    SwiftUI.Button("Open", systemImage: "arrow.right.circle") { model.show(entity) }
                    entityActions(entity)
                    if let url = NativeMacLinks.url(for: entity) {
                        SwiftUI.Divider()
                        SwiftUI.Button("Copy Link", systemImage: "link") { NativeMacLinks.copy(url) }
                    }
                }
                .labelStyle(.titleAndIcon)
            case .none:
                SwiftUI.EmptyView()
            }
        }
        .onHover { hovering = $0 }
        .animation(reduceMotion ? nil : .easeOut(duration: 0.18), value: hovering)
        .help(card.title)
        .accessibilityElement(children: .combine)
    }

    @SwiftUI.ViewBuilder
    private func entityActions(_ entity: GoosicEntityReference) -> some SwiftUI.View {
        if model.activeAccount != nil {
            switch entity {
            case .playlist(let id) where !entity.isLikedMusic &&
                (model.ownedPlaylist(for: entity) == nil || model.isSaved(playlist: id)):
                let saved = inLibrary || model.isSaved(playlist: id)
                SwiftUI.Button(saved ? "Remove from Library" : "Save to Library",
                               systemImage: saved ? "minus.circle" : "plus.circle") {
                    model.setSaved(!saved, playlist: id, title: card.title)
                }
            case .artist(let id):
                let subscribed = model.isSubscribed(to: id)
                SwiftUI.Button(subscribed ? "Unsubscribe" : "Subscribe",
                               systemImage: subscribed ? "person.badge.minus" : "person.badge.plus") {
                    model.setSubscribed(!subscribed, to: id, name: card.title)
                }
            default:
                SwiftUI.EmptyView()
            }
        }
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
    /// Artists are round, in cards and in rows.
    var round = false
    var cornerRadius: CGFloat = 8

    var body: some SwiftUI.View {
        SwiftUI.AsyncImage(url: validatedURL) { phase in
            switch phase {
            case .success(let image):
                image.resizable().scaledToFill()
            default:
                SwiftUI.ZStack {
                    SwiftUI.Color.goosicPink.opacity(0.14)
                    SwiftUI.Image(systemName: round ? "music.mic" : "music.note")
                        .foregroundStyle(.secondary)
                }
            }
        }
        .frame(width: width, height: height)
        .clipShape(SwiftUI.RoundedRectangle(cornerRadius: round ? min(width, height) / 2 : cornerRadius, style: .continuous))
    }

    private var validatedURL: URL? {
        guard let url, let value = URL(string: url), ArtworkCache.isAllowed(value) else { return nil }
        return value
    }
}

/// Menu entries for a track, shared by catalog cards, track rows, the queue, and the player's
/// More button, so a song offers the same actions wherever it appears.
struct NativeMacTrackMenuItems: SwiftUI.View {
    let track: GoosicTrack
    let model: GoosicAppModel
    var context: [GoosicTrack] = []
    var includePlay = true
    var source: QueueSource? = nil
    /// The playlist the row is shown in, when it can be taken out of it.
    var container: GoosicEntityReference? = nil
    /// The row's position in the queue, when the menu belongs to a queue row.
    var queueIndex: Int? = nil

    // Menus are built from their own content, so the icon style is set here rather than
    // inherited from the window; without it macOS drops every menu icon.
    var body: some SwiftUI.View {
        SwiftUI.Group { items }.labelStyle(.titleAndIcon)
    }

    @SwiftUI.ViewBuilder
    private var items: some SwiftUI.View {
        if includePlay {
            SwiftUI.Button("Play", systemImage: "play.fill") {
                model.launch(.resolve(track: track, explicitTracks: context), source: source)
            }
        }
        if queueIndex == nil {
            SwiftUI.Button("Play Next", systemImage: "text.line.first.and.arrowtriangle.forward") {
                model.playNext(track)
            }
            SwiftUI.Button("Add to Queue", systemImage: "text.append") { model.addToQueue(track) }
        }
        SwiftUI.Button("Start Radio", systemImage: "dot.radiowaves.left.and.right") { model.startRadio(from: track) }
        if let queueIndex, queueIndex != model.queue.currentIndex {
            SwiftUI.Button("Remove from Queue", systemImage: "minus.circle") {
                model.removeFromQueue(at: queueIndex)
            }
        }
        if model.activeAccount != nil {
            SwiftUI.Divider()
            let rating = model.rating(of: track)
            SwiftUI.Button(rating == .liked ? "Remove from Liked Music" : "Like",
                           systemImage: rating == .liked ? "heart.slash" : "heart") {
                model.toggleLike(track)
            }
            SwiftUI.Button(rating == .disliked ? "Remove Dislike" : "Dislike",
                           systemImage: "hand.thumbsdown") {
                model.toggleDislike(track)
            }
        }
        if track.artistID != nil || track.albumID != nil {
            SwiftUI.Divider()
        }
        if let id = track.artistID {
            SwiftUI.Button("Go to Artist", systemImage: "music.mic") { model.show(.artist(id)) }
        }
        if let id = track.albumID {
            SwiftUI.Button("Go to Album", systemImage: "square.stack") { model.show(.album(id)) }
        }
        if model.activeAccount != nil {
            SwiftUI.Divider()
            NativeMacAddToPlaylistMenu(track: track, model: model)
            if let container, model.canRemove(from: container) {
                SwiftUI.Button(
                    container.isLikedMusic ? "Remove from Liked Music" : "Remove from Playlist",
                    systemImage: "trash"
                ) {
                    model.remove(track, from: container)
                }
            }
        }
        SwiftUI.Divider()
        SwiftUI.Button("Copy Link", systemImage: "link") { NativeMacLinks.copy(NativeMacLinks.url(for: track)) }
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

    // Menus are built from their own content, so the icon style is set here rather than
    // inherited from the window; without it macOS drops every menu icon.
    var body: some SwiftUI.View {
        SwiftUI.Group { items }.labelStyle(.titleAndIcon)
    }

    @SwiftUI.ViewBuilder
    private var items: some SwiftUI.View {
        SwiftUI.Menu("Add to Playlist", systemImage: "text.badge.plus") {
            SwiftUI.Button("New Playlist…", systemImage: "plus") {
                model.beginNewPlaylist(for: track)
            }
            switch model.userPlaylistsState {
            case .loading:
                SwiftUI.Text("Loading your playlists…")
            case .failed(let message):
                SwiftUI.Text(message)
                SwiftUI.Button("Try Again", systemImage: "arrow.clockwise") { model.loadUserPlaylists(force: true) }
            case .idle:
                if !model.userPlaylists.isEmpty { SwiftUI.Divider() }
                SwiftUI.ForEach(model.userPlaylists) { playlist in
                    SwiftUI.Button(playlist.title, systemImage: "music.note.list") {
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

    /// `nil` for a category, whose id is Goosic's own packing and names no public page.
    static func url(for entity: GoosicEntityReference) -> URL? {
        switch entity {
        case .album(let id), .artist(let id):
            var components = URLComponents(string: "https://music.youtube.com/browse/")!
            components.path = "/browse/" + id
            return components.url
        case .playlist(let id):
            var components = URLComponents(string: "https://music.youtube.com/playlist")!
            components.queryItems = [URLQueryItem(name: "list", value: id)]
            return components.url
        case .category:
            return nil
        }
    }


    static func copy(_ url: URL) {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(url.absoluteString, forType: .string)
    }
}


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
#endif
