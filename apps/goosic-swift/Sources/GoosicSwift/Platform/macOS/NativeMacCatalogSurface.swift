#if os(macOS) && !GOOSIC_PORTABLE
import AppKit
import AppKitBackend
import SwiftCrossUI
import SwiftUI

/// Hosts the catalog in native SwiftUI on macOS.
///
/// SwiftCrossUI 0.9 eagerly computes every `ForEach` child, including children outside a
/// `ScrollView`'s viewport. A YouTube Music page can contain hundreds of cards, which blocks
/// AppKit's main run loop and produces the beach ball. Native SwiftUI's lazy stacks retain only
/// the visible neighborhood, which is the same rendering strategy used by Kaset.
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

struct NativeMacCatalogPage: SwiftUI.View {
    let key: CatalogKey
    let title: String
    let subtitle: String
    let state: CatalogLoadState
    let model: GoosicAppModel

    var body: some SwiftUI.View {
        SwiftUI.ScrollView {
            SwiftUI.LazyVStack(alignment: .leading, spacing: 30) {
                SwiftUI.VStack(alignment: .leading, spacing: 4) {
                    SwiftUI.Text(title)
                        .font(.largeTitle)
                    SwiftUI.Text(subtitle)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }

                content
            }
            .padding(24)
            .padding(.bottom, 96)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    @SwiftUI.ViewBuilder
    private var content: some SwiftUI.View {
        if state.isLoading {
            SwiftUI.ProgressView("Loading \(title.lowercased())…")
                .controlSize(.small)
                .padding(.vertical, 18)
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
        } else if let page = state.page {
            if page.isEmpty {
                SwiftUI.ContentUnavailableView(
                    "Nothing to show",
                    systemImage: "music.note",
                    description: SwiftUI.Text("\(title) came back empty.")
                )
            } else {
                if !page.tracks.isEmpty {
                    NativeMacTrackList(tracks: page.tracks, model: model)
                }
                SwiftUI.ForEach(page.shelves) { shelf in
                    NativeMacShelf(shelf: shelf, model: model)
                }
                if let cursor = page.nextCursor {
                    SwiftUI.HStack {
                        SwiftUI.Spacer()
                        SwiftUI.ProgressView().controlSize(.small)
                        SwiftUI.Text("Loading more…")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        SwiftUI.Spacer()
                    }
                    .padding(.vertical, 20)
                    .id(cursor)
                    .onAppear { model.loadMore(key) }
                }
                if page.truncated {
                    SwiftUI.Text("This page was long, so only the first part is shown.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        } else {
            SwiftUI.VStack(alignment: .leading, spacing: 8) {
                SwiftUI.Text("Not loaded yet.").foregroundStyle(.secondary)
                SwiftUI.Button("Load \(title.lowercased())") { model.retry(key) }
            }
        }
    }
}

struct NativeMacLibraryPage: SwiftUI.View {
    let model: GoosicAppModel

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
                .padding(.horizontal, 24)
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
    let shelf: GoosicShelf
    let model: GoosicAppModel

    var body: some SwiftUI.View {
        SwiftUI.VStack(alignment: .leading, spacing: 10) {
            SwiftUI.Text(shelf.title)
                .font(.title2.weight(.semibold))

            if let tracks = shelf.trackList {
                NativeMacTrackList(tracks: tracks, model: model)
            } else {
                SwiftUI.ScrollView(.horizontal, showsIndicators: false) {
                    SwiftUI.LazyHStack(alignment: .top, spacing: 16) {
                        SwiftUI.ForEach(shelf.cards) { card in
                            NativeMacCatalogCard(card: card, model: model)
                        }
                    }
                    .padding(.vertical, 2)
                }
            }
        }
    }
}

private struct NativeMacTrackList: SwiftUI.View {
    let tracks: [GoosicTrack]
    let model: GoosicAppModel

    var body: some SwiftUI.View {
        SwiftUI.LazyVStack(spacing: 0) {
            SwiftUI.ForEach(tracks) { track in
                SwiftUI.Button {
                    model.play(track, in: tracks)
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
                SwiftUI.Divider()
            }
        }
    }
}

private struct NativeMacCatalogCard: SwiftUI.View {
    let card: GoosicCard
    let model: GoosicAppModel

    var body: some SwiftUI.View {
        SwiftUI.Button(action: activate) {
            SwiftUI.VStack(alignment: .leading, spacing: 6) {
                NativeMacArtwork(url: card.thumbnail, width: 180, height: 180)
                SwiftUI.Text(card.title)
                    .font(.subheadline)
                    .lineLimit(1)
                SwiftUI.Text(card.subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            .frame(width: 180, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(card.action == nil)
    }

    private func activate() {
        switch card.action {
        case .show(let entity): model.show(entity)
        case .play(let track): model.play(track)
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
#endif
