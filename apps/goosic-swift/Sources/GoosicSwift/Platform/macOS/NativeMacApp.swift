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
        NativeMacApplicationIcon.install()
    }

    var body: some SwiftUI.Scene {
        SwiftUI.WindowGroup("Goosic") {
            NativeMacRootView(store: store)
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

/// Installs the supplied Goosic artwork for both `swift run` and packaged macOS launches.
/// SwiftPM executables do not have an Xcode asset catalog, so the icon must be loaded from the
/// target resource bundle explicitly. The complete appearance set stays bundled for future
/// packaging; the default artwork is the stable macOS Dock/application icon for now.
@MainActor
private enum NativeMacApplicationIcon {
    static func install() {
        guard let url = Bundle.module.url(
            forResource: "Icon-iOS-Default-1024@1x", withExtension: "png", subdirectory: "AppIcons"
        ), let image = NSImage(contentsOf: url) else {
            return
        }

        // The supplied iOS artwork is edge-to-edge, while macOS Dock icons reserve a little
        // transparent breathing room around their visible mark. Add that padding at runtime so
        // Goosic occupies the same visual size as neighboring Dock icons without altering the
        // source PNGs or their appearance in other platforms.
        let inset = image.size.width * 0.10
        let padded = NSImage(size: image.size)
        padded.lockFocus()
        image.draw(
            in: NSRect(
                x: inset,
                y: inset,
                width: image.size.width - (inset * 2),
                height: image.size.height - (inset * 2)
            ),
            from: NSRect(origin: .zero, size: image.size),
            operation: .sourceOver,
            fraction: 1
        )
        padded.unlockFocus()
        NSApplication.shared.applicationIconImage = padded
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

/// Lays the window out the way Music does on macOS 26: the detail column is drawn full-width
/// and the sidebar floats over its leading edge as one continuous glass surface. Page layout
/// starts beside the sidebar through `nativeMacLeadingInset`; shelf rows are the exception and
/// scroll underneath it, which is what gives the glass something to refract.
///
/// `NavigationSplitView` cannot produce this: it lays the detail column beside the sidebar, so
/// nothing ever passes under the glass and the two columns read as separate panels.
private struct NativeMacRootView: SwiftUI.View {
    @ObservedObject var store: NativeMacModelStore
    @SwiftUI.Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var sidebarVisible = true

    private var model: GoosicAppModel { store.model }
    private var leadingInset: CGFloat { sidebarVisible ? NativeMacSidebar.width : 0 }

    var body: some SwiftUI.View {
        ZStack(alignment: .topLeading) {
            // Bottommost, so the detail column, the glass sidebar, and the player bar all sit on it.
            if model.artworkBackground {
                NativeMacArtworkBackdrop(file: model.artworkFile(for: model.currentTrack?.thumbnail))
                    .transition(.opacity)
            }

            ZStack(alignment: .bottom) {
                detailContent
                    .frame(maxWidth: .infinity, maxHeight: .infinity)

                NativeMacPlayerBar(store: store)
                    .padding(.leading, leadingInset)
                    .padding(.horizontal, 20)
                    .padding(.bottom, 12)

                NativeMacOfficialPlaybackSurface(model: model)
                    .frame(width: 640, height: 360)
                    .opacity(0.001)
                    .offset(x: -2_000, y: -2_000)
                    .allowsHitTesting(false)
                    .accessibilityHidden(true)

                if model.queueVisible {
                    NativeMacQueuePanel(store: store)
                        .transition(.move(edge: .trailing).combined(with: .opacity))
                        .padding(.trailing, 20)
                        .padding(.bottom, 112)
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)
                } else if model.lyricsVisible {
                    NativeMacLyricsPanel(store: store)
                        .transition(.move(edge: .trailing).combined(with: .opacity))
                        .padding(.trailing, 20)
                        .padding(.bottom, 112)
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)
                }
            }
            .safeAreaInset(edge: .top, spacing: 0) {
                if !model.serviceConnected || (model.currentTrack != nil && !model.hasConfirmedPlaybackSample) {
                    SwiftUI.HStack(spacing: 8) {
                        SwiftUI.Image(systemName: model.serviceConnected ? "info.circle" : "wifi.slash")
                        SwiftUI.Text(model.serviceConnected ? model.hostStatus : model.status)
                            .lineLimit(2).font(.caption)
                        SwiftUI.Spacer()
                        if !model.serviceConnected { SwiftUI.Button("Reconnect", action: model.connect) }
                    }
                    .padding(10)
                    .padding(.leading, leadingInset)
                    .background(.thinMaterial)
                }
            }
            .environment(\.nativeMacLeadingInset, leadingInset)

            if sidebarVisible {
                NativeMacSidebar(store: store)
                    .transition(.move(edge: .leading))
            }

            // Above everything, but the layers beneath stay mounted: the official playback
            // surface in particular must outlive opening and closing this.
            if model.fullPlayerOpen && model.currentTrack != nil {
                NativeMacFullPlayer(store: store)
                    .transition(.opacity.combined(with: .scale(scale: 0.98)))
                    .zIndex(10)
            }
        }
        // Owned here rather than by the menu that opens it: a context menu is gone by the time
        // its action runs, and an alert presented from a view that no longer exists never appears.
        .alert("New playlist", isPresented: SwiftUI.Binding(
            get: { model.isNamingNewPlaylist },
            set: { if !$0 { model.cancelNewPlaylist() } }
        )) {
            SwiftUI.TextField("Name", text: SwiftUI.Binding(
                get: { model.newPlaylistName },
                set: { model.newPlaylistName = $0 }
            ))
            SwiftUI.Button("Cancel", role: .cancel) { model.cancelNewPlaylist() }
            SwiftUI.Button("Create") { model.confirmNewPlaylist() }
        } message: {
            SwiftUI.Text("The playlist is private until you change it.")
        }
        .alert("Rename playlist", isPresented: SwiftUI.Binding(
            get: { model.isRenamingPlaylist },
            set: { if !$0 { model.cancelRename() } }
        )) {
            SwiftUI.TextField("Name", text: SwiftUI.Binding(
                get: { model.renamedPlaylistName },
                set: { model.renamedPlaylistName = $0 }
            ))
            SwiftUI.Button("Cancel", role: .cancel) { model.cancelRename() }
            SwiftUI.Button("Rename") { model.confirmRename() }
        }
        // Confirmed rather than undoable: YouTube Music offers no way back from this.
        .alert(
            "Delete \(model.playlistPendingDeletion?.title ?? "playlist")?",
            isPresented: SwiftUI.Binding(
                get: { model.playlistPendingDeletion != nil },
                set: { if !$0 { model.playlistPendingDeletion = nil } }
            )
        ) {
            SwiftUI.Button("Cancel", role: .cancel) { model.playlistPendingDeletion = nil }
            SwiftUI.Button("Delete", role: .destructive) { model.confirmDeletion() }
        } message: {
            SwiftUI.Text("This cannot be undone.")
        }
        .toolbar {
            ToolbarItem(placement: .navigation) {
                SwiftUI.Button {
                    SwiftUI.withAnimation(reduceMotion ? nil : .easeInOut(duration: 0.22)) {
                        sidebarVisible.toggle()
                    }
                } label: {
                    SwiftUI.Image(systemName: "sidebar.leading")
                }
                .help(sidebarVisible ? "Hide sidebar" : "Show sidebar")
                .keyboardShortcut("s", modifiers: [.command, .control])
            }
            // The full-screen player's title-bar controls. They are toolbar items rather than
            // part of that player's view because only a toolbar item keeps its drag in the title
            // bar; a slider drawn there by the content moves the window instead.
            ToolbarItemGroup(placement: .primaryAction) {
                if model.fullPlayerOpen && model.currentTrack != nil {
                    NativeMacFullPlayerVolume(store: store)
                    SwiftUI.Button {
                        model.setFullPlayerOpen(false)
                    } label: {
                        SwiftUI.Image(systemName: "arrow.down.right.and.arrow.up.left")
                    }
                    .keyboardShortcut(.cancelAction)
                    .help("Exit full-screen player")
                    .accessibilityLabel("Exit full-screen player")
                }
            }
        }
        .modifier(NativeMacTransparentToolbar())
        .background(NativeMacWindowChrome())
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.22), value: model.queueVisible)
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.22), value: model.lyricsVisible)
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.4), value: model.artworkBackground)
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.35), value: model.fullPlayerOpen)
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
                NativeMacLibraryPage(store: store)
            case .downloads:
                NativeMacDownloadsView(store: store)
                    .padding(.leading, leadingInset)
            case .settings:
                NativeMacSettingsView(store: store)
                    .padding(.leading, leadingInset)
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

/// The toolbar draws no background of its own, so the catalog scrolls under it and the sidebar
/// column reads as one continuous surface from the traffic lights down. A visible toolbar bar
/// is what split the sidebar into a titlebar band and a list band.
private struct NativeMacTransparentToolbar: SwiftUI.ViewModifier {
    @SwiftUI.ViewBuilder
    func body(content: Content) -> some SwiftUI.View {
        if #available(macOS 15.0, *) {
            content.toolbarBackgroundVisibility(.hidden, for: .automatic)
                .toolbar(removing: .title)
        } else {
            content
        }
    }
}

/// Makes the hosting window edge-to-edge: content extends under the titlebar, so the system
/// sidebar and toolbar glass have something to refract instead of sitting on window chrome.
/// The playing track's artwork, blurred and tinted with the window colour, filling the window
/// under the transparent title bar. Crossfades when the track changes.
private struct NativeMacArtworkBackdrop: SwiftUI.View {
    let file: URL?
    @SwiftUI.Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some SwiftUI.View {
        let image = file.flatMap { ArtworkBackdropRenderer.backdrop(for: $0) }
        SwiftUI.Color.clear
            .overlay {
                if let file, let image {
                    SwiftUI.Image(decorative: image, scale: 1)
                        .resizable()
                        .scaledToFill()
                        .id(file)
                        .transition(.opacity)
                }
            }
            .overlay {
                if image != nil {
                    SwiftUI.Color(nsColor: .windowBackgroundColor)
                        .opacity(ArtworkBackdropRenderer.tintOpacity)
                }
            }
            .clipped()
            .ignoresSafeArea()
            .allowsHitTesting(false)
            .accessibilityHidden(true)
            .animation(reduceMotion ? nil : .easeInOut(duration: 0.6), value: file)
    }
}

private struct NativeMacWindowChrome: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async { configure(view.window) }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        configure(nsView.window)
    }

    private func configure(_ window: NSWindow?) {
        guard let window else { return }
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.styleMask.insert(.fullSizeContentView)
    }
}

/// Width of the floating sidebar, so page layout begins beside it while the detail column is
/// drawn full-width underneath. Zero when the sidebar is hidden.
private struct NativeMacLeadingInsetKey: SwiftUI.EnvironmentKey {
    static let defaultValue: CGFloat = 0
}

extension SwiftUI.EnvironmentValues {
    var nativeMacLeadingInset: CGFloat {
        get { self[NativeMacLeadingInsetKey.self] }
        set { self[NativeMacLeadingInsetKey.self] = newValue }
    }
}

/// One glass sheet behind the whole sidebar, titlebar included. The list and the account
/// footer sit on it without materials of their own, so the column has no bands.
private struct NativeMacSidebarSurface: SwiftUI.View {
    var body: some SwiftUI.View {
        if #available(macOS 26.0, *) {
            SwiftUI.Rectangle().fill(.clear)
                .glassEffect(.regular, in: .rect(cornerRadius: 0))
        } else {
            SwiftUI.Rectangle().fill(.ultraThinMaterial)
        }
    }
}

/// SF Symbols have different intrinsic drawing bounds: a gear or theatre masks fills more of a
/// point-sized box than a magnifying glass. Giving `Image` a frame only aligned those unequal
/// drawings; it did not make them the same visual size. This resizes the artwork first, then
/// gives it a consistent hit and alignment canvas.
private struct NativeMacFixedSymbol: SwiftUI.View {
    let name: String
    let glyphSize: CGFloat
    let width: CGFloat
    let height: CGFloat

    var body: some SwiftUI.View {
        SwiftUI.Image(systemName: name)
            .resizable()
            .scaledToFit()
            .frame(width: glyphSize, height: glyphSize)
            .frame(width: width, height: height)
    }
}

private struct NativeMacSidebar: SwiftUI.View {
    static let width: CGFloat = 210

    @ObservedObject var store: NativeMacModelStore
    @SwiftUI.Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    private var model: GoosicAppModel { store.model }

    var body: some SwiftUI.View {
        SwiftUI.VStack(spacing: 0) {
            sidebarList
                .mask(sidebarScrollMask)
            accountButton
        }
        .frame(width: Self.width)
        .frame(maxHeight: .infinity)
        .background { NativeMacSidebarSurface().ignoresSafeArea() }
        .onAppear { model.loadUserPlaylists() }
        // The sidebar is created before the asynchronous account snapshot returns. Re-read when
        // the account appears (or changes) so an initially empty sidebar is not mistaken for an
        // account with no playlists.
        .onChange(of: model.activeAccount?.id) { _, _ in
            model.loadUserPlaylists(force: true)
        }
    }

    private var sidebarList: some SwiftUI.View {
        SwiftUI.List {
            SwiftUI.Section {
                row(.search, icon: "magnifyingglass")
                row(.home, icon: "house.fill")
            }
            SwiftUI.Section("Discover") {
                row(.explore, icon: "globe")
                row(.charts, icon: "chart.xyaxis.line")
                row(.moodsAndGenres, icon: "theatermasks")
                row(.newReleases, icon: "sparkles")
            }
            SwiftUI.Section("Library") {
                libraryRow(.songs, title: "Favourite Songs", icon: "star.square.on.square")
                libraryRow(.artists, title: "Artists", icon: "music.mic")
                libraryRow(.albums, title: "Albums", icon: "square.stack")
                row(.downloads, icon: "arrow.down.circle")
            }
            SwiftUI.Section("Playlists") {
                libraryRow(.playlists, title: "All Playlists", icon: "square.grid.2x2")
                SwiftUI.ForEach(model.userPlaylists) { playlist in playlistRow(playlist) }
            }
            SwiftUI.Section("Goosic") { row(.settings, icon: "gearshape") }
        }
        .listStyle(.sidebar)
        .environment(\.defaultMinListRowHeight, 30)
        .scrollContentBackground(.hidden)
    }

    /// Fade the scrolling content, not the sidebar surface. The one continuous sidebar material
    /// therefore shows through at both edges instead of producing a footer-colored rectangle.
    @SwiftUI.ViewBuilder
    private var sidebarScrollMask: some SwiftUI.View {
        if reduceTransparency {
            SwiftUI.Rectangle().fill(SwiftUI.Color.black)
        } else {
            SwiftUI.VStack(spacing: 0) {
                SwiftUI.LinearGradient(
                    colors: [SwiftUI.Color.clear, SwiftUI.Color.black],
                    startPoint: .top,
                    endPoint: .bottom
                )
                    .frame(height: 62)
                SwiftUI.Rectangle().fill(SwiftUI.Color.black)
                SwiftUI.LinearGradient(
                    colors: [SwiftUI.Color.black, SwiftUI.Color.clear],
                    startPoint: .top,
                    endPoint: .bottom
                )
                    .frame(height: 58)
            }
        }
    }

    /// Rows are plain buttons on a native list, so they take the system row metrics while the
    /// selection stays the neutral pill Music uses rather than the focused accent highlight.
    private func row(_ route: GoosicRoute, icon: String) -> some SwiftUI.View {
        let selected = model.detail == nil && model.route == route
        return SwiftUI.Button { model.navigate(to: route) } label: {
            SwiftUI.Label {
                SwiftUI.Text(route.title).fontWeight(selected ? .semibold : .regular)
            } icon: {
                NativeMacFixedSymbol(name: icon, glyphSize: 16, width: 22, height: 22)
                    .foregroundStyle(SwiftUI.Color.blue)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .listRowBackground(
            RoundedRectangle(cornerRadius: 6)
                .fill(selected ? SwiftUI.Color.primary.opacity(0.09) : SwiftUI.Color.clear)
                .padding(.horizontal, 10)
        )
    }

    private func libraryRow(
        _ section: PersonalLibrarySection, title: String, icon: String
    ) -> some SwiftUI.View {
        let selected = model.detail == nil
            && model.route == .library
            && model.libraryTab == section.rawValue
        return SwiftUI.Button {
            model.navigate(to: .library)
            model.selectLibrarySection(section)
        } label: {
            SwiftUI.Label {
                SwiftUI.Text(title).fontWeight(selected ? .semibold : .regular)
            } icon: {
                NativeMacFixedSymbol(name: icon, glyphSize: 16, width: 22, height: 22)
                    .foregroundStyle(SwiftUI.Color.blue)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .listRowBackground(
            RoundedRectangle(cornerRadius: 6)
                .fill(selected ? SwiftUI.Color.primary.opacity(0.09) : SwiftUI.Color.clear)
                .padding(.horizontal, 10)
        )
    }

    private func playlistRow(_ playlist: PersonalPlaylistSummary) -> some SwiftUI.View {
        let selected = selectedPlaylistID == playlist.id
        return SwiftUI.Button {
            model.show(.playlist(playlist.id))
        } label: {
            SwiftUI.HStack(spacing: 6) {
                NativeMacSidebarArtwork(url: playlist.thumbnail)
                SwiftUI.Text(playlist.title)
                    .fontWeight(selected ? .semibold : .regular)
                    .lineLimit(1)
                SwiftUI.Spacer(minLength: 0)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(playlist.title)
        .listRowBackground(
            RoundedRectangle(cornerRadius: 6)
                .fill(selected ? SwiftUI.Color.primary.opacity(0.09) : SwiftUI.Color.clear)
                .padding(.horizontal, 10)
        )
    }

    private var selectedPlaylistID: String? {
        guard case .playlist(let id) = model.detail else { return nil }
        return id.hasPrefix("VL") ? String(id.dropFirst(2)) : id
    }

    private var accountButton: some SwiftUI.View {
        SwiftUI.Button { model.navigate(to: .settings) } label: {
            SwiftUI.HStack(spacing: 10) {
                NativeMacAccountAvatar(
                    url: model.activeAccount?.avatarUrl,
                    fallback: String(model.activeAccountLabel.prefix(1)).uppercased()
                )
                SwiftUI.VStack(alignment: .leading, spacing: 1) {
                    SwiftUI.Text(model.activeAccountLabel).font(.subheadline.weight(.semibold)).lineLimit(1)
                    SwiftUI.Text(model.serviceConnected ? "Connected" : "Offline")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                SwiftUI.Spacer()
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.horizontal, 8)
        .padding(.bottom, 8)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Account: \(model.activeAccountLabel)")
    }
}

private struct NativeMacAccountAvatar: SwiftUI.View {
    let url: String?
    let fallback: String

    var body: some SwiftUI.View {
        SwiftUI.AsyncImage(url: url.flatMap(URL.init(string:))) { phase in
            if case .success(let image) = phase {
                image.resizable().scaledToFill()
            } else {
                SwiftUI.Text(fallback)
                    .font(.headline)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(SwiftUI.Color.goosicPink)
            }
        }
        .frame(width: 34, height: 34)
        .clipShape(Circle())
        .overlay(Circle().stroke(.white.opacity(0.18), lineWidth: 1))
    }
}

/// A playlist list is personal data, so its artwork comes from the authenticated library rather
/// than from a hard-coded selection. The same URL allow-list used by catalog artwork prevents a
/// malformed personal response from turning the sidebar into an arbitrary remote-image loader.
private struct NativeMacSidebarArtwork: SwiftUI.View {
    let url: String?

    var body: some SwiftUI.View {
        SwiftUI.AsyncImage(url: validatedURL) { phase in
            if case .success(let image) = phase {
                image.resizable().scaledToFill()
            } else {
                SwiftUI.Color.goosicPink.opacity(0.14)
                    .overlay(
                        NativeMacFixedSymbol(name: "music.note", glyphSize: 11, width: 18, height: 18)
                            .foregroundStyle(SwiftUI.Color.goosicPink)
                    )
            }
        }
        .frame(width: 18, height: 18)
        .clipShape(RoundedRectangle(cornerRadius: 4))
    }

    private var validatedURL: URL? {
        guard let url, let value = URL(string: url), ArtworkCache.isAllowed(value) else { return nil }
        return value
    }
}

/// The compact now-playing capsule, laid out like Music's: transport, the current song over a
/// hairline progress bar, then More, Lyrics, Queue, and a volume slider that unfolds out of the
/// speaker button in place of the two panel toggles. Every control is a plain glyph on the
/// glass; the capsule itself is the only chrome.
private struct NativeMacPlayerBar: SwiftUI.View {
    @ObservedObject var store: NativeMacModelStore
    @SwiftUI.Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var scrubPosition: Double = 0
    @State private var isScrubbing = false
    @State private var statusVisible = false
    @State private var volumeExpanded = false
    @State private var progressHovered = false
    @State private var playerHovered = false

    private var model: GoosicAppModel { store.model }
    private var busy: Bool { model.accountOperationInProgress || model.playbackTransition != .idle }
    private var canControl: Bool { model.currentTrack != nil && model.serviceConnected && !busy }
    private var canAdjustVolume: Bool { !busy && !model.isAdvertisement }
    /// Music's player grows into a transport surface once there is something to control. The
    /// idle state remains a compact capsule, while a playing (or hovered) bar makes room for a
    /// readable scrubber and elapsed/remaining time.
    private var playerExpanded: Bool {
        model.currentTrack != nil || playerHovered || isScrubbing || volumeExpanded
    }
    private var showsTimes: Bool { playerExpanded || progressHovered || isScrubbing }

    var body: some SwiftUI.View {
        SwiftUI.HStack(spacing: 12) {
            transport
            nowPlaying
                .frame(maxWidth: .infinity)
            trailingControls
        }
        .padding(.horizontal, 14)
        .padding(.vertical, playerExpanded ? 9 : 6)
        .frame(minWidth: playerExpanded ? 720 : 640, maxWidth: 960)
        .modifier(NativeMacPlayerGlass())
        .shadow(color: .black.opacity(0.18), radius: 16, y: 8)
        .onHover { playerHovered = $0 }
        .animation(reduceMotion ? nil : .spring(duration: 0.28, bounce: 0.12), value: playerExpanded)
        .onChange(of: model.currentTrack?.id) { _, _ in isScrubbing = false }
        .onChange(of: model.isSeekable) { _, seekable in if !seekable { isScrubbing = false } }
    }

    private var transport: some SwiftUI.View {
        SwiftUI.HStack(spacing: 2) {
            glyph("Shuffle", "shuffle", size: 15, active: model.shuffle, subtle: true, action: model.toggleShuffle)
            glyph("Previous track", "backward.fill", size: 17, action: model.previous)
                .disabled(!canControl || model.isAdvertisement)
            SwiftUI.Button(action: model.togglePause) {
                SwiftUI.ZStack {
                    if busy {
                        SwiftUI.ProgressView().controlSize(.small)
                    } else {
                        NativeMacFixedSymbol(
                            name: model.isPaused ? "play.fill" : "pause.fill",
                            glyphSize: 19,
                            width: 34,
                            height: 30
                        )
                    }
                }
                .frame(width: 34, height: 30)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(model.isPaused ? "Play" : "Pause")
            .help(busy ? "Preparing playback" : model.isPaused ? "Play" : "Pause")
            .disabled(!canControl)
            glyph("Next track", "forward.fill", size: 17, action: model.next)
                .disabled(!canControl || model.isAdvertisement)
            glyph(model.repeatMode.label, model.repeatMode == .one ? "repeat.1" : "repeat", size: 15,
                  active: model.repeatMode != .off, subtle: true, action: model.cycleRepeatMode)
        }
    }

    private var nowPlaying: some SwiftUI.View {
        SwiftUI.VStack(spacing: 4) {
            SwiftUI.HStack(spacing: 10) {
                NativeMacExpandableArtwork(
                    url: model.currentTrack?.thumbnail,
                    size: playerExpanded ? 42 : 36,
                    enabled: model.currentTrack != nil
                ) { model.setFullPlayerOpen(true) }
                SwiftUI.VStack(alignment: .leading, spacing: 2) {
                    SwiftUI.Text(model.currentTrack?.title ?? "Nothing playing")
                        .font(.subheadline.weight(.semibold)).lineLimit(1)
                    SwiftUI.Text(busy ? "Preparing playback…" : model.nowPlayingSubtitle)
                        .font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            progress
        }
    }

    /// A hairline that thickens on hover and reveals the times at either end. Dragging scrubs;
    /// the seek is sent once on release so the transport is not flooded mid-drag.
    private var progress: some SwiftUI.View {
        let total = max(model.duration, 1)
        let position = isScrubbing ? scrubPosition : min(model.displayedPosition, total)
        return SwiftUI.HStack(spacing: 6) {
            if showsTimes {
                SwiftUI.Text(GoosicAppModel.timeText(position)).transition(.opacity)
            }
            SwiftUI.GeometryReader { proxy in
                SwiftUI.ZStack(alignment: .leading) {
                    Capsule().fill(.primary.opacity(0.15))
                    Capsule().fill(.primary.opacity(0.75))
                        .frame(width: max(proxy.size.width * position / total, 0))
                }
                .frame(height: showsTimes ? 4 : 2)
                .frame(maxHeight: .infinity)
                .contentShape(Rectangle())
                .gesture(
                    DragGesture(minimumDistance: 0)
                        .onChanged { value in
                            guard model.isSeekable, !busy else { return }
                            isScrubbing = true
                            scrubPosition = min(max(value.location.x / proxy.size.width, 0), 1) * total
                        }
                        .onEnded { _ in
                            guard isScrubbing else { return }
                            model.seek(to: scrubPosition)
                            isScrubbing = false
                        }
                )
            }
            .frame(height: 12)
            if showsTimes {
                SwiftUI.Text("−" + GoosicAppModel.timeText(max(total - position, 0)))
                    .transition(.opacity)
            }
        }
        .font(.caption2.monospacedDigit())
        .foregroundStyle(.secondary)
        .opacity(model.isSeekable ? 1 : 0.45)
        .onHover { progressHovered = $0 }
        .animation(reduceMotion ? nil : .easeOut(duration: 0.15), value: showsTimes)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Playback position")
        .accessibilityValue("\(model.elapsedText) of \(model.durationText)")
    }

    private var trailingControls: some SwiftUI.View {
        SwiftUI.HStack(spacing: 2) {
            moreMenu
            if !volumeExpanded {
                glyph("Lyrics", "quote.bubble", size: 15, active: model.lyricsVisible, action: model.toggleLyrics)
                    .transition(.move(edge: .trailing).combined(with: .opacity))
                glyph("Queue", "list.bullet", size: 15, active: model.queueVisible, action: model.toggleQueue)
                    .transition(.move(edge: .trailing).combined(with: .opacity))
            }
            SwiftUI.HStack(spacing: 6) {
                if volumeExpanded {
                    SwiftUI.Slider(value: Binding(get: { model.isMuted ? 0 : model.volume }, set: model.setVolume), in: 0...1)
                        .controlSize(.small)
                        .frame(width: 84)
                        .accessibilityLabel("Volume")
                        .disabled(!canAdjustVolume)
                        .transition(.scale(scale: 0.4, anchor: .trailing).combined(with: .opacity))
                }
                glyph(model.isMuted ? "Unmute" : "Mute",
                      model.isMuted ? "speaker.slash.fill" : "speaker.wave.2.fill", size: 15, action: model.toggleMuted)
                    .disabled(!canAdjustVolume)
            }
            .onHover { volumeExpanded = $0 }
        }
    }

    private var moreMenu: some SwiftUI.View {
        SwiftUI.Menu {
            if let track = model.currentTrack {
                NativeMacTrackMenuItems(track: track, model: model, includePlay: false)
                SwiftUI.Divider()
            }
            SwiftUI.Button("Playback status…") { statusVisible = true }
        } label: {
                NativeMacFixedSymbol(name: "ellipsis", glyphSize: 15, width: 30, height: 30)
                    .foregroundStyle(SwiftUI.Color.primary)
                .contentShape(Rectangle())
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
        .help("More")
        .popover(isPresented: $statusVisible) {
            SwiftUI.VStack(alignment: .leading, spacing: 8) {
                SwiftUI.Text("Playback status").font(.headline)
                SwiftUI.Text(model.status)
                if model.isAdvertisement { SwiftUI.Text("Advertisement · Seeking and track changes are unavailable.") }
            }
            .font(.callout).padding(18).frame(width: 300)
        }
    }

    private func glyph(_ title: String, _ name: String, size: CGFloat, active: Bool = false,
                       subtle: Bool = false, action: @escaping () -> Void) -> some SwiftUI.View {
        SwiftUI.Button(action: action) {
            NativeMacFixedSymbol(name: name, glyphSize: size, width: 30, height: 30)
                .foregroundStyle(active ? SwiftUI.Color.goosicPink : subtle ? SwiftUI.Color.secondary : SwiftUI.Color.primary)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(title)
        .help(title)
    }
}

/// Glass chrome for a single control. Apply it to the button itself, never to a container: a
/// button style propagates through the environment, so one placed on the split view turns every
/// list row and catalog card into a filled control too.
///
/// The tint is cleared because the app tint is `goosicPink`, and a tinted glass button is drawn
/// as a solid fill of that colour rather than as glass. Pink stays an accent applied to icons and
/// selection, so call sites do not need to reset the tint themselves.
struct NativeMacGlassButtons: SwiftUI.ViewModifier {
    @SwiftUI.ViewBuilder
    func body(content: Content) -> some SwiftUI.View {
        if #available(macOS 26.0, *) {
            content.buttonStyle(.glass).tint(Optional<SwiftUI.Color>.none)
        } else {
            content.buttonStyle(.bordered).tint(Optional<SwiftUI.Color>.none)
        }
    }
}

private struct NativeMacSearchGlass: SwiftUI.ViewModifier {
    @SwiftUI.ViewBuilder
    func body(content: Content) -> some SwiftUI.View {
        if #available(macOS 26.0, *) {
            content.glassEffect(.regular, in: .rect(cornerRadius: 12))
        } else {
            content.background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
        }
    }
}

private struct NativeMacPlayerGlass: SwiftUI.ViewModifier {
    @SwiftUI.ViewBuilder
    func body(content: Content) -> some SwiftUI.View {
        if #available(macOS 26.0, *) {
            content.glassEffect(.regular, in: .rect(cornerRadius: 28))
        } else {
            content
                .background(.ultraThinMaterial, in: Capsule())
                .overlay(Capsule().stroke(.white.opacity(0.12), lineWidth: 1))
        }
    }
}

private struct NativeMacSearchView: SwiftUI.View {
    @ObservedObject var store: NativeMacModelStore
    @SwiftUI.Environment(\.nativeMacLeadingInset) private var leadingInset

    private var model: GoosicAppModel { store.model }

    var body: some SwiftUI.View {
        SwiftUI.VStack(spacing: 0) {
            SwiftUI.HStack {
                SwiftUI.TextField(
                    "Search music",
                    text: Binding(get: { model.query }, set: { model.query = $0 })
                )
                .textFieldStyle(.plain)
                .padding(10)
                .modifier(NativeMacSearchGlass())
                .onSubmit { model.search() }
                SwiftUI.Button("Search") { model.search() }
            }
            .padding(24)
            .padding(.leading, leadingInset)

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
    @SwiftUI.Environment(\.nativeMacLeadingInset) private var leadingInset

    private var model: GoosicAppModel { store.model }

    var body: some SwiftUI.View {
        SwiftUI.VStack(spacing: 0) {
            SwiftUI.HStack {
                SwiftUI.Button("Back", systemImage: "chevron.left", action: model.closeDetail)
                SwiftUI.Spacer()
                // Only for a playlist this account owns. Following someone else's playlist puts
                // it in this library while leaving every edit refused, so offering these on one
                // would produce a failure after the user had already typed a new name.
                if let playlist = model.ownedPlaylist(for: entity) {
                    SwiftUI.Menu {
                        SwiftUI.Button("Rename…", systemImage: "pencil") {
                            model.beginRenaming(playlist)
                        }
                        SwiftUI.Menu("Who can see this") {
                            SwiftUI.ForEach(PlaylistPrivacy.allCases) { privacy in
                                SwiftUI.Button(privacy.label) {
                                    model.setPlaylistPrivacy(playlist, to: privacy)
                                }
                            }
                        }
                        SwiftUI.Divider()
                        SwiftUI.Button("Delete…", systemImage: "trash", role: .destructive) {
                            model.playlistPendingDeletion = playlist
                        }
                    } label: {
                        SwiftUI.Label("Manage", systemImage: "ellipsis.circle")
                    }
                    .disabled(model.libraryOperationInProgress)
                }
            }
            .padding(.horizontal, 24)
            .padding(.leading, leadingInset)
            .padding(.top, 12)
            .onAppear { model.loadUserPlaylists() }
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
                .modifier(NativeMacGlassButtons())
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
                            .contextMenu { NativeMacTrackMenuItems(track: track, model: model, context: model.queue.tracks) }
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
                .modifier(NativeMacGlassButtons())
            }

            SwiftUI.ScrollView {
                if let lyrics = model.lyrics {
                    let active = model.activeLyricIndex
                    SwiftUI.LazyVStack(alignment: .leading, spacing: 10) {
                        SwiftUI.ForEach(Array(lyrics.lines.enumerated()), id: \.element.id) { index, line in
                            SwiftUI.Text(line.text.isEmpty ? "♪" : line.text)
                                .font(index == active ? .headline : .body)
                                .foregroundStyle(index == active ? SwiftUI.Color.primary : .secondary)
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
    var size: CGFloat = 42

    var body: some SwiftUI.View {
        SwiftUI.AsyncImage(url: url.flatMap(URL.init(string:))) { phase in
            if case .success(let image) = phase {
                image.resizable().scaledToFill()
            } else {
                SwiftUI.Color.primary.opacity(0.08)
                    .overlay(SwiftUI.Image(systemName: "music.note"))
            }
        }
        .frame(width: size, height: size)
        .clipShape(RoundedRectangle(cornerRadius: size / 6))
    }
}

/// The player bar's cover, which opens the full-screen player. On hover the art dims and lifts
/// slightly and an expand badge appears over it, as in Music, so it reads as a control.
private struct NativeMacExpandableArtwork: SwiftUI.View {
    let url: String?
    let size: CGFloat
    let enabled: Bool
    let action: () -> Void
    @SwiftUI.Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var hovered = false

    private var highlighted: Bool { hovered && enabled }

    var body: some SwiftUI.View {
        SwiftUI.Button(action: action) {
            NativeMacPanelArtwork(url: url, size: size)
                .overlay {
                    if highlighted {
                        SwiftUI.ZStack {
                            RoundedRectangle(cornerRadius: size / 6)
                                .fill(SwiftUI.Color.black.opacity(0.38))
                            SwiftUI.Image(systemName: "arrow.up.left.and.arrow.down.right")
                                .font(.system(size: size * 0.28, weight: .bold))
                                .foregroundStyle(.white)
                                .frame(width: size * 0.56, height: size * 0.56)
                                .overlay(
                                    RoundedRectangle(cornerRadius: size * 0.14)
                                        .stroke(.white.opacity(0.9), lineWidth: 1.5)
                                )
                        }
                        .transition(.opacity.combined(with: .scale(scale: 0.8)))
                    }
                }
                .scaleEffect(highlighted ? 1.06 : 1)
                .shadow(color: .black.opacity(highlighted ? 0.28 : 0), radius: 6, y: 3)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .onHover { hovered = $0 }
        .animation(reduceMotion ? nil : .spring(duration: 0.24, bounce: 0.3), value: highlighted)
        .help("Open full-screen player")
        .accessibilityLabel("Open full-screen player")
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
    private var accountChangesDisabled: Bool {
        !model.serviceConnected
            || model.accountOperationInProgress
            || model.playbackTransition != .idle
            || model.isAdvertisement
    }

    var body: some SwiftUI.View {
        SwiftUI.ScrollView {
            SwiftUI.VStack(alignment: .leading, spacing: 22) {
                SwiftUI.VStack(alignment: .leading, spacing: 4) {
                    SwiftUI.Text("Settings")
                        .font(.largeTitle.bold())
                    SwiftUI.Text("Personalize playback and manage your Goosic account.")
                        .foregroundStyle(.secondary)
                }

                settingsCard("Connection", systemImage: "bolt.horizontal.circle.fill") {
                    settingsRow(
                        title: "Goosic service",
                        detail: model.serviceConnected ? "Connected and ready" : "The playback service is offline",
                        systemImage: model.serviceConnected ? "checkmark.circle.fill" : "exclamationmark.triangle.fill",
                        tint: model.serviceConnected ? .green : .orange
                    ) {
                        SwiftUI.Button(model.serviceConnected ? "Connected" : "Reconnect", action: model.connect)
                            .disabled(model.serviceConnected)
                            .modifier(NativeMacSettingsButtonStyle(prominent: !model.serviceConnected))
                    }
                }

                settingsCard("Playback", systemImage: "play.circle.fill") {
                    settingToggle(
                        "Autoplay",
                        detail: "Keep the music going with related recommendations.",
                        systemImage: "infinity",
                        isOn: SwiftUI.Binding(get: { model.autoplay }, set: model.setAutoplay)
                    )
                    SwiftUI.Divider().opacity(0.45)
                    settingToggle(
                        "Shuffle",
                        detail: "Play the current queue in a randomized order.",
                        systemImage: "shuffle",
                        isOn: SwiftUI.Binding(
                            get: { model.shuffle },
                            set: { enabled in if enabled != model.shuffle { model.toggleShuffle() } }
                        )
                    )
                    SwiftUI.Divider().opacity(0.45)
                    settingToggle(
                        "Album Art Background",
                        detail: "Blur the playing track's artwork behind the window.",
                        systemImage: "photo.fill",
                        isOn: SwiftUI.Binding(
                            get: { model.artworkBackground },
                            set: model.setArtworkBackground
                        )
                    )
                }

                settingsCard("Accounts", systemImage: "person.crop.circle.fill") {
                    SwiftUI.HStack(spacing: 14) {
                        NativeMacAccountAvatar(
                            url: model.activeAccount?.avatarUrl,
                            fallback: String(model.activeAccountLabel.prefix(1)).uppercased()
                        )
                        .scaleEffect(1.2)
                        .frame(width: 42, height: 42)

                        SwiftUI.VStack(alignment: .leading, spacing: 2) {
                            SwiftUI.Text(model.activeAccountLabel).font(.headline)
                            SwiftUI.Text(model.activeAccount?.email ?? model.activeAccount?.channel ?? "Not signed in")
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                        SwiftUI.Spacer()
                        SwiftUI.Button("Add Account", systemImage: "person.badge.plus", action: model.signIn)
                            .disabled(accountChangesDisabled)
                            .modifier(NativeMacSettingsButtonStyle(prominent: model.accounts.isEmpty))
                    }

                    if !model.accounts.isEmpty {
                        SwiftUI.Divider().opacity(0.45)
                        SwiftUI.VStack(spacing: 0) {
                            SwiftUI.ForEach(Array(model.accounts.enumerated()), id: \.element.id) { index, account in
                                accountRow(account)
                                if index < model.accounts.count - 1 {
                                    SwiftUI.Divider().padding(.leading, 46).opacity(0.45)
                                }
                            }
                        }
                    }

                    SwiftUI.Text("Sign-in data remains inside each account's isolated WebKit profile and never enters the playback service.")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .frame(maxWidth: 720, alignment: .leading)
            .padding(.horizontal, 32)
            .padding(.top, 28)
            .padding(.bottom, 130)
            .frame(maxWidth: .infinity)
        }
        .background(SwiftUI.Color(nsColor: .windowBackgroundColor).opacity(0.35))
        .onAppear { model.loadAccounts() }
    }

    private func settingsCard<Content: SwiftUI.View>(
        _ title: String,
        systemImage: String,
        @SwiftUI.ViewBuilder content: () -> Content
    ) -> some SwiftUI.View {
        SwiftUI.VStack(alignment: .leading, spacing: 16) {
            SwiftUI.Label(title, systemImage: systemImage)
                .font(.headline)
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(SwiftUI.Color.goosicPink)
            content()
        }
        .padding(18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .modifier(NativeMacSettingsCardStyle())
    }

    private func settingToggle(
        _ title: String,
        detail: String,
        systemImage: String,
        isOn: SwiftUI.Binding<Bool>
    ) -> some SwiftUI.View {
        SwiftUI.HStack(spacing: 13) {
            settingsIcon(systemImage, tint: .goosicPink)
            SwiftUI.VStack(alignment: .leading, spacing: 2) {
                SwiftUI.Text(title).font(.body.weight(.medium))
                SwiftUI.Text(detail).font(.caption).foregroundStyle(.secondary)
            }
            SwiftUI.Spacer()
            SwiftUI.Toggle(title, isOn: isOn)
                .labelsHidden()
                .toggleStyle(.switch)
                .tint(.goosicPink)
                .controlSize(.large)
        }
        .padding(.vertical, 2)
    }

    private func settingsRow<Trailing: SwiftUI.View>(
        title: String,
        detail: String,
        systemImage: String,
        tint: SwiftUI.Color,
        @SwiftUI.ViewBuilder trailing: () -> Trailing
    ) -> some SwiftUI.View {
        SwiftUI.HStack(spacing: 13) {
            settingsIcon(systemImage, tint: tint)
            SwiftUI.VStack(alignment: .leading, spacing: 2) {
                SwiftUI.Text(title).font(.body.weight(.medium))
                SwiftUI.Text(detail).font(.caption).foregroundStyle(.secondary)
            }
            SwiftUI.Spacer()
            trailing()
        }
    }

    private func settingsIcon(_ systemImage: String, tint: SwiftUI.Color) -> some SwiftUI.View {
        SwiftUI.Image(systemName: systemImage)
            .symbolRenderingMode(.hierarchical)
            .font(.system(size: 16, weight: .semibold))
            .foregroundStyle(tint)
            .frame(width: 32, height: 32)
            .background(tint.opacity(0.12), in: RoundedRectangle(cornerRadius: 9, style: .continuous))
    }

    private func accountRow(_ account: GoosicAccountSummary) -> some SwiftUI.View {
        let active = account.id == model.activeAccountId
        return SwiftUI.HStack(spacing: 12) {
            NativeMacAccountAvatar(
                url: account.avatarUrl,
                fallback: String(account.displayName.prefix(1)).uppercased()
            )
            SwiftUI.VStack(alignment: .leading, spacing: 1) {
                SwiftUI.HStack(spacing: 6) {
                    SwiftUI.Text(account.displayName).fontWeight(.medium)
                    if active {
                        SwiftUI.Text("ACTIVE")
                            .font(.caption2.bold())
                            .foregroundStyle(.green)
                    }
                }
                SwiftUI.Text(account.email ?? account.channel ?? "YouTube Music account")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            SwiftUI.Spacer()
            if !active {
                SwiftUI.Button("Use") { model.switchAccount(to: account.id) }
                    .disabled(accountChangesDisabled)
                    .modifier(NativeMacSettingsButtonStyle(prominent: false))
            }
            SwiftUI.Menu {
                SwiftUI.Button(active ? "Sign Out" : "Remove Account", systemImage: "person.crop.circle.badge.minus", role: .destructive) {
                    if active { model.signOut() } else { model.removeAccount(account.id) }
                }
                .disabled(accountChangesDisabled)
            } label: {
                SwiftUI.Image(systemName: "ellipsis")
                    .frame(width: 26, height: 26)
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            .help("Account options")
        }
        .padding(.vertical, 6)
    }
}

private struct NativeMacSettingsCardStyle: SwiftUI.ViewModifier {
    @SwiftUI.ViewBuilder
    func body(content: Content) -> some SwiftUI.View {
        if #available(macOS 26.0, *) {
            content.glassEffect(.regular, in: .rect(cornerRadius: 20))
        } else {
            content
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: 20, style: .continuous)
                        .stroke(.primary.opacity(0.08), lineWidth: 1)
                )
        }
    }
}

private struct NativeMacSettingsButtonStyle: SwiftUI.ViewModifier {
    let prominent: Bool

    @SwiftUI.ViewBuilder
    func body(content: Content) -> some SwiftUI.View {
        if #available(macOS 26.0, *) {
            if prominent {
                content.buttonStyle(.glassProminent).tint(.goosicPink)
            } else {
                content.buttonStyle(.glass).tint(Optional<SwiftUI.Color>.none)
            }
        } else {
            if prominent {
                content.buttonStyle(.borderedProminent).tint(.goosicPink)
            } else {
                content.buttonStyle(.bordered).tint(Optional<SwiftUI.Color>.none)
            }
        }
    }
}

struct NativeMacEmptyPage: SwiftUI.View {
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
        nsView.setAccessibilityHidden(true)
    }
}

private extension SwiftUI.Color {
    static let goosicPink = SwiftUI.Color(red: 1.0, green: 0.02, blue: 0.32)
}

#if DEBUG
/// Canvas host for the complete native shell. It intentionally renders the model without
/// calling `connect()`, so previews never start the Rust child or touch account WebKit state.
@MainActor
private struct NativeMacAppPreview: SwiftUI.View {
    @StateObject private var store = NativeMacModelStore()
    let colorScheme: SwiftUI.ColorScheme?
    let height: CGFloat

    var body: some SwiftUI.View {
        NativeMacRootView(store: store)
            .tint(.goosicPink)
            .preferredColorScheme(colorScheme)
            .frame(width: 1_280, height: height)
    }
}

#Preview("Full App — System") {
    NativeMacAppPreview(colorScheme: nil, height: 800)
}

#Preview("Full App — Light") {
    NativeMacAppPreview(colorScheme: .light, height: 800)
}

#Preview("Full App — Dark") {
    NativeMacAppPreview(colorScheme: .dark, height: 800)
}

#Preview("Full App — Compact Height") {
    NativeMacAppPreview(colorScheme: nil, height: 680)
}

#Preview("Settings — macOS 26") {
    NativeMacSettingsView(store: NativeMacModelStore())
        .tint(.goosicPink)
        .frame(width: 900, height: 760)
}
#endif
#endif
