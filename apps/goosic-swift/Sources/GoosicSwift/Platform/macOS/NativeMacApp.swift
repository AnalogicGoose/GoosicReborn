#if os(macOS) && !GOOSIC_PORTABLE
import AppKit
import Combine
import SwiftCrossUI
import SwiftUI

@main
struct GoosicMacApp: SwiftUI.App {
    @StateObject private var store = NativeMacModelStore()

    init() {
        // `swift run` is not wrapped in an .app bundle, so explicitly opt into a foreground
        // activation policy during development. Packaged builds already receive this behavior.
        NSApplication.shared.setActivationPolicy(.regular)
    }

    var body: some SwiftUI.Scene {
        SwiftUI.WindowGroup("Goosic") {
            NativeMacRootView(store: store)
                .preferredColorScheme(.dark)
                .tint(.goosicPink)
                .frame(minWidth: 1_020, minHeight: 680)
                .onAppear {
                    NSApplication.shared.activate(ignoringOtherApps: true)
                    store.model.connect()
                }
        }
        .defaultSize(width: 1_280, height: 800)
    }
}

/// Bridges the existing shared application model into SwiftUI while the Rust/service contracts
/// remain unchanged. macOS can therefore move to a fully native renderer without forking the
/// playback and catalog behavior used by future WinUI and GTK shells.
@MainActor
final class NativeMacModelStore: Combine.ObservableObject {
    let model = GoosicAppModel()
    private var observation: SwiftCrossUI.Cancellable?

    init() {
        observation = model.didChange.observe { [weak self] in
            DispatchQueue.main.async {
                self?.objectWillChange.send()
            }
        }
    }
}

private struct NativeMacRootView: SwiftUI.View {
    @ObservedObject var store: NativeMacModelStore
    @State private var columnVisibility: NavigationSplitViewVisibility = .all

    private var model: GoosicAppModel { store.model }

    var body: some SwiftUI.View {
        NavigationSplitView(columnVisibility: $columnVisibility) {
            NativeMacSidebar(store: store)
                .navigationSplitViewColumnWidth(min: 220, ideal: 250, max: 290)
        } detail: {
            ZStack(alignment: .bottom) {
                detailContent
                    .frame(maxWidth: .infinity, maxHeight: .infinity)

                NativeMacPlayerBar(store: store)
                    .padding(.horizontal, 20)
                    .padding(.bottom, 12)

                NativeMacOfficialPlaybackSurface(model: model)
                    .frame(width: 640, height: 360)
                    .opacity(0.001)
                    .offset(x: -2_000, y: -2_000)
                    .allowsHitTesting(false)

                if model.queueVisible {
                    NativeMacQueuePanel(store: store)
                        .padding(.trailing, 20)
                        .padding(.bottom, 88)
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)
                } else if model.lyricsVisible {
                    NativeMacLyricsPanel(store: store)
                        .padding(.trailing, 20)
                        .padding(.bottom, 88)
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)
                }
            }
            .toolbar {
                ToolbarItem(placement: .principal) {
                    SwiftUI.Text(model.detail == nil ? model.route.title : model.detail?.kindLabel ?? "Goosic")
                        .font(.headline)
                }
            }
        }
        .navigationSplitViewStyle(.balanced)
        .modifier(NativeMacTransparentToolbar())
        .background(SwiftUI.Color.black)
    }

    @SwiftUI.ViewBuilder
    private var detailContent: some SwiftUI.View {
        if let entity = model.detail {
            NativeMacEntityView(entity: entity, store: store)
        } else {
            switch model.route {
            case .home, .explore, .charts, .moodsAndGenres, .newReleases:
                NativeMacCatalogPage(
                    key: .route(model.route),
                    title: model.route.title,
                    subtitle: subtitle(for: model.route),
                    state: model.state(for: .route(model.route)),
                    model: model
                )
            case .search:
                NativeMacSearchView(store: store)
            case .library:
                NativeMacEmptyPage(
                    title: "Library",
                    icon: "music.note.list",
                    message: model.activeAccount == nil
                        ? "Sign in to load your saved music."
                        : "Your personal library connection is the next data step."
                )
            case .downloads:
                NativeMacDownloadsView(store: store)
            case .settings:
                NativeMacSettingsView(store: store)
            }
        }
    }

    private func subtitle(for route: GoosicRoute) -> String {
        switch route {
        case .home: "Made for listening right now"
        case .explore: "New releases, charts, moods, and genres"
        case .charts: "What people are playing now"
        case .moodsAndGenres: "Find a feeling, then a playlist"
        case .newReleases: "Albums and singles out now"
        default: "YouTube Music"
        }
    }
}

private struct NativeMacTransparentToolbar: SwiftUI.ViewModifier {
    @SwiftUI.ViewBuilder
    func body(content: Content) -> some SwiftUI.View {
        if #available(macOS 15.0, *) {
            content.toolbarBackgroundVisibility(.hidden, for: .automatic)
        } else {
            content
        }
    }
}

private struct NativeMacSidebar: SwiftUI.View {
    @ObservedObject var store: NativeMacModelStore

    private var model: GoosicAppModel { store.model }

    var body: some SwiftUI.View {
        SwiftUI.List {
            SwiftUI.Section {
                    sidebarButton(.search, icon: "magnifyingglass")
                    sidebarButton(.home, icon: "house.fill")
            }

            SwiftUI.Section("Discover") {
                sidebarButton(.explore, icon: "globe")
                sidebarButton(.charts, icon: "chart.xyaxis.line")
                sidebarButton(.moodsAndGenres, icon: "theatermasks")
                sidebarButton(.newReleases, icon: "sparkles")
            }

            SwiftUI.Section("Collection") {
                sidebarButton(.library, icon: "music.note.list")
                sidebarButton(.downloads, icon: "arrow.down.circle")
            }

            SwiftUI.Section("Goosic") {
                sidebarButton(.settings, icon: "gearshape")
            }
        }
        .listStyle(.sidebar)
        .scrollContentBackground(.hidden)
        .modifier(NativeMacSidebarGlass())
        .safeAreaInset(edge: .bottom, spacing: 0) {
            SwiftUI.VStack(spacing: 10) {
                SwiftUI.HStack {
                    SwiftUI.Image(systemName: "music.note")
                    SwiftUI.Text("Music").fontWeight(.semibold)
                    SwiftUI.Spacer()
                }
                .font(.caption)
                .padding(.horizontal, 14)
                .padding(.vertical, 8)
                .background(SwiftUI.Color.goosicPink, in: Capsule())

                SwiftUI.HStack(spacing: 10) {
                    SwiftUI.Text(String(model.activeAccountLabel.prefix(1)).uppercased())
                        .font(.headline)
                        .frame(width: 38, height: 38)
                        .background(SwiftUI.Color.goosicPink, in: Circle())
                    SwiftUI.VStack(alignment: .leading, spacing: 1) {
                        SwiftUI.Text(model.activeAccountLabel).lineLimit(1)
                        SwiftUI.Text(model.serviceConnected ? "Connected" : "Offline")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                    SwiftUI.Spacer()
                }
            }
            .padding(12)
            .modifier(NativeMacSidebarFooterBackground())
        }
    }

    private func sidebarButton(_ route: GoosicRoute, icon: String) -> some SwiftUI.View {
        let selected = model.route == route && model.detail == nil
        return SwiftUI.Button {
            model.navigate(to: route)
        } label: {
            SwiftUI.HStack(spacing: 10) {
                SwiftUI.Image(systemName: icon).frame(width: 18)
                SwiftUI.Text(route.title).fontWeight(selected ? .semibold : .regular)
                SwiftUI.Spacer()
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .contentShape(Rectangle())
            .background(selected ? SwiftUI.Color.goosicPink : .clear, in: RoundedRectangle(cornerRadius: 8))
        }
        .buttonStyle(.plain)
        .listRowInsets(EdgeInsets(top: 2, leading: 8, bottom: 2, trailing: 8))
        .listRowBackground(SwiftUI.Color.clear)
    }
}

private struct NativeMacSidebarGlass: SwiftUI.ViewModifier {
    @SwiftUI.ViewBuilder
    func body(content: Content) -> some SwiftUI.View {
        if #available(macOS 26.0, *) {
            content.glassEffect(.regular, in: Rectangle())
        } else {
            content.background(.ultraThinMaterial)
        }
    }
}

private struct NativeMacSidebarFooterBackground: SwiftUI.ViewModifier {
    @SwiftUI.ViewBuilder
    func body(content: Content) -> some SwiftUI.View {
        if #available(macOS 26.0, *) {
            content.background(SwiftUI.Color.clear)
        } else {
            content.background(.ultraThinMaterial)
        }
    }
}

private struct NativeMacPlayerBar: SwiftUI.View {
    @ObservedObject var store: NativeMacModelStore

    private var model: GoosicAppModel { store.model }

    var body: some SwiftUI.View {
        SwiftUI.HStack(spacing: 14) {
            SwiftUI.HStack(spacing: 8) {
                SwiftUI.Button(action: model.previous) { SwiftUI.Image(systemName: "backward.fill") }
                SwiftUI.Button(action: model.togglePause) {
                    SwiftUI.Image(systemName: model.isPaused ? "play.fill" : "pause.fill")
                        .font(.title3)
                }
                SwiftUI.Button(action: model.next) { SwiftUI.Image(systemName: "forward.fill") }
            }
            .disabled(model.currentTrack == nil)

            SwiftUI.Divider().frame(height: 28)

            SwiftUI.VStack(alignment: .leading, spacing: 2) {
                SwiftUI.Text(model.currentTrack?.title ?? "Nothing playing")
                    .font(.subheadline.weight(.semibold)).lineLimit(1)
                SwiftUI.Text(model.nowPlayingSubtitle)
                    .font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            .frame(minWidth: 180, alignment: .leading)

            SwiftUI.Spacer()

            SwiftUI.Button(action: model.toggleLyrics) { SwiftUI.Image(systemName: "quote.bubble") }
            SwiftUI.Button(action: model.toggleQueue) { SwiftUI.Image(systemName: "list.bullet") }
            SwiftUI.Button(action: model.toggleMuted) {
                SwiftUI.Image(systemName: model.isMuted ? "speaker.slash.fill" : "speaker.wave.2.fill")
            }
            SwiftUI.Slider(
                value: Binding(get: { model.isMuted ? 0 : model.volume }, set: model.setVolume),
                in: 0...1
            )
            .frame(width: 105)
        }
        .buttonStyle(.plain)
        .padding(.horizontal, 20)
        .frame(height: 64)
        .modifier(NativeMacPlayerGlass())
        .shadow(color: .black.opacity(0.35), radius: 22, y: 10)
    }
}

private struct NativeMacPlayerGlass: SwiftUI.ViewModifier {
    @SwiftUI.ViewBuilder
    func body(content: Content) -> some SwiftUI.View {
        if #available(macOS 26.0, *) {
            content.glassEffect(.regular.interactive(), in: .capsule)
        } else {
            content
                .background(.ultraThinMaterial, in: Capsule())
                .overlay(Capsule().stroke(.white.opacity(0.12), lineWidth: 1))
        }
    }
}

private struct NativeMacSearchView: SwiftUI.View {
    @ObservedObject var store: NativeMacModelStore

    private var model: GoosicAppModel { store.model }

    var body: some SwiftUI.View {
        SwiftUI.VStack(spacing: 0) {
            SwiftUI.HStack {
                SwiftUI.TextField(
                    "Search music",
                    text: Binding(get: { model.query }, set: { model.query = $0 })
                )
                .textFieldStyle(.roundedBorder)
                .onSubmit { model.search() }
                SwiftUI.Button("Search") { model.search() }
            }
            .padding(24)

            if model.submittedQuery.isEmpty {
                NativeMacEmptyPage(title: "Search", icon: "magnifyingglass", message: "Find songs, albums, artists, and playlists.")
            } else {
                NativeMacCatalogPage(
                    key: model.currentSearchKey,
                    title: "Search",
                    subtitle: "Results for “\(model.submittedQuery)”",
                    state: model.state(for: model.currentSearchKey),
                    model: model
                )
            }
        }
    }
}

private struct NativeMacEntityView: SwiftUI.View {
    let entity: GoosicEntityReference
    @ObservedObject var store: NativeMacModelStore

    private var model: GoosicAppModel { store.model }

    var body: some SwiftUI.View {
        SwiftUI.VStack(spacing: 0) {
            SwiftUI.HStack {
                SwiftUI.Button("Back", systemImage: "chevron.left", action: model.closeDetail)
                SwiftUI.Spacer()
            }
            .padding(.horizontal, 24)
            .padding(.top, 12)
            NativeMacCatalogPage(
                key: .entity(entity),
                title: entity.kindLabel,
                subtitle: "YouTube Music",
                state: model.state(for: .entity(entity)),
                model: model
            )
        }
    }
}

private struct NativeMacQueuePanel: SwiftUI.View {
    @ObservedObject var store: NativeMacModelStore

    private var model: GoosicAppModel { store.model }

    var body: some SwiftUI.View {
        SwiftUI.VStack(alignment: .leading, spacing: 12) {
            SwiftUI.HStack {
                SwiftUI.Text("Queue").font(.title2.bold())
                SwiftUI.Spacer()
                SwiftUI.Button(action: model.toggleQueue) {
                    SwiftUI.Image(systemName: "xmark")
                }
                .buttonStyle(.plain)
            }

            if model.queue.tracks.isEmpty {
                SwiftUI.ContentUnavailableView(
                    "Queue is empty",
                    systemImage: "list.bullet",
                    description: SwiftUI.Text("Choose a song or video to start a queue.")
                )
            } else {
                SwiftUI.ScrollView {
                    SwiftUI.LazyVStack(spacing: 4) {
                        SwiftUI.ForEach(model.queue.tracks) { track in
                            SwiftUI.Button {
                                model.play(track, in: model.queue.tracks)
                            } label: {
                                SwiftUI.HStack(spacing: 10) {
                                    NativeMacPanelArtwork(url: track.thumbnail)
                                    SwiftUI.VStack(alignment: .leading, spacing: 2) {
                                        SwiftUI.Text(track.title).lineLimit(1)
                                        SwiftUI.Text(track.artist)
                                            .font(.caption)
                                            .foregroundStyle(.secondary)
                                            .lineLimit(1)
                                    }
                                    SwiftUI.Spacer()
                                    if track.id == model.currentTrack?.id {
                                        SwiftUI.Image(systemName: "speaker.wave.2.fill")
                                            .foregroundStyle(SwiftUI.Color.goosicPink)
                                    }
                                }
                                .padding(6)
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                        }
                    }
                }
            }
        }
        .padding(18)
        .frame(width: 370, height: 420, alignment: .top)
        .modifier(NativeMacPanelGlass())
    }
}

private struct NativeMacLyricsPanel: SwiftUI.View {
    @ObservedObject var store: NativeMacModelStore

    private var model: GoosicAppModel { store.model }

    var body: some SwiftUI.View {
        SwiftUI.VStack(alignment: .leading, spacing: 12) {
            SwiftUI.HStack {
                SwiftUI.VStack(alignment: .leading, spacing: 2) {
                    SwiftUI.Text("Lyrics").font(.title2.bold())
                    SwiftUI.Text(model.lyricsStatus)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                SwiftUI.Spacer()
                SwiftUI.Button(action: model.toggleLyrics) {
                    SwiftUI.Image(systemName: "xmark")
                }
                .buttonStyle(.plain)
            }

            SwiftUI.ScrollView {
                if let lyrics = model.lyrics {
                    let active = model.activeLyricIndex
                    SwiftUI.LazyVStack(alignment: .leading, spacing: 10) {
                        SwiftUI.ForEach(Array(lyrics.lines.enumerated()), id: \.element.id) { index, line in
                            SwiftUI.Text(line.text.isEmpty ? "♪" : line.text)
                                .font(index == active ? .headline : .body)
                                .foregroundStyle(index == active ? SwiftUI.Color.white : .secondary)
                        }
                    }
                } else {
                    SwiftUI.ContentUnavailableView(
                        "No lyrics yet",
                        systemImage: "quote.bubble",
                        description: SwiftUI.Text(model.lyricsStatus)
                    )
                }
            }
        }
        .padding(18)
        .frame(width: 370, height: 420, alignment: .top)
        .modifier(NativeMacPanelGlass())
    }
}

private struct NativeMacPanelArtwork: SwiftUI.View {
    let url: String?

    var body: some SwiftUI.View {
        SwiftUI.AsyncImage(url: url.flatMap(URL.init(string:))) { phase in
            if case .success(let image) = phase {
                image.resizable().scaledToFill()
            } else {
                SwiftUI.Color.white.opacity(0.08)
                    .overlay(SwiftUI.Image(systemName: "music.note"))
            }
        }
        .frame(width: 42, height: 42)
        .clipShape(RoundedRectangle(cornerRadius: 7))
    }
}

private struct NativeMacPanelGlass: SwiftUI.ViewModifier {
    @SwiftUI.ViewBuilder
    func body(content: Content) -> some SwiftUI.View {
        if #available(macOS 26.0, *) {
            content.glassEffect(.regular.interactive(), in: .rect(cornerRadius: 22))
        } else {
            content.background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 22))
        }
    }
}

private struct NativeMacDownloadsView: SwiftUI.View {
    @ObservedObject var store: NativeMacModelStore

    private var model: GoosicAppModel { store.model }

    var body: some SwiftUI.View {
        SwiftUI.ScrollView {
            SwiftUI.LazyVStack(alignment: .leading, spacing: 12) {
                SwiftUI.Text("Downloads").font(.largeTitle.bold())
                SwiftUI.HStack {
                    SwiftUI.Button("Refresh", action: model.loadDownloads)
                    SwiftUI.Button("Import previous Goosic files", action: model.importLegacyDownloads)
                }
                SwiftUI.ForEach(model.downloadedTracks) { track in
                    SwiftUI.HStack {
                        SwiftUI.Image(systemName: track.available ? "music.note" : "exclamationmark.triangle")
                        SwiftUI.VStack(alignment: .leading) {
                            SwiftUI.Text(track.title)
                            SwiftUI.Text(track.subtitle).font(.caption).foregroundStyle(.secondary)
                        }
                        SwiftUI.Spacer()
                        SwiftUI.Button("Play") { model.playDownloaded(track) }
                            .disabled(!track.available)
                    }
                    SwiftUI.Divider()
                }
            }
            .padding(24)
        }
    }
}

private struct NativeMacSettingsView: SwiftUI.View {
    @ObservedObject var store: NativeMacModelStore

    private var model: GoosicAppModel { store.model }

    var body: some SwiftUI.View {
        SwiftUI.Form {
            SwiftUI.Section("Connection") {
                SwiftUI.LabeledContent("Rust service", value: model.serviceConnected ? "Connected" : "Offline")
                SwiftUI.Button("Connect", action: model.connect).disabled(model.serviceConnected)
            }
            SwiftUI.Section("Playback") {
                SwiftUI.Toggle("Autoplay", isOn: Binding(get: { model.autoplay }, set: model.setAutoplay))
                SwiftUI.Toggle("Shuffle", isOn: Binding(get: { model.shuffle }, set: { _ in model.toggleShuffle() }))
            }
            SwiftUI.Section("Account") {
                SwiftUI.LabeledContent("Profile", value: model.activeAccountLabel)
                SwiftUI.Button(model.activeAccount == nil ? "Add account" : "Manage accounts", action: model.signIn)
            }
        }
        .formStyle(.grouped)
        .padding(20)
    }
}

private struct NativeMacEmptyPage: SwiftUI.View {
    let title: String
    let icon: String
    let message: String

    var body: some SwiftUI.View {
        SwiftUI.ContentUnavailableView(title, systemImage: icon, description: SwiftUI.Text(message))
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct NativeMacOfficialPlaybackSurface: SwiftUI.NSViewRepresentable {
    let model: GoosicAppModel

    func makeNSView(context: Context) -> OfficialPlaybackContainer {
        model.officialPlaybackHost.makeContainer()
    }

    func updateNSView(_ nsView: OfficialPlaybackContainer, context: Context) {
        nsView.wantsLayer = true
        nsView.layer?.opacity = 0.01
    }
}

private extension SwiftUI.Color {
    static let goosicPink = SwiftUI.Color(red: 1.0, green: 0.02, blue: 0.32)
}
#endif
