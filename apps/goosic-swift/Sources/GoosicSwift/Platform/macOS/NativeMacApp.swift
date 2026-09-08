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
        }
        .modifier(NativeMacTransparentToolbar())
        .background(NativeMacWindowChrome())
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.22), value: model.queueVisible)
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.22), value: model.lyricsVisible)
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
                .padding(.leading, leadingInset)
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

private struct NativeMacSidebar: SwiftUI.View {
    static let width: CGFloat = 210

    @ObservedObject var store: NativeMacModelStore

    private var model: GoosicAppModel { store.model }

    var body: some SwiftUI.View {
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

            SwiftUI.Section("Collection") {
                row(.library, icon: "music.note.list")
                row(.downloads, icon: "arrow.down.circle")
            }

            SwiftUI.Section("Goosic") {
                row(.settings, icon: "gearshape")
            }
        }
        .listStyle(.sidebar)
        .environment(\.defaultMinListRowHeight, 30)
        .scrollContentBackground(.hidden)
        .safeAreaInset(edge: .bottom, spacing: 0) { footer }
        .frame(width: Self.width)
        .frame(maxHeight: .infinity)
        .background { NativeMacSidebarSurface().ignoresSafeArea() }
    }

    /// Rows are plain buttons on a native list, so they take the system row metrics while the
    /// selection stays the neutral pill Music uses rather than the focused accent highlight.
    private func row(_ route: GoosicRoute, icon: String) -> some SwiftUI.View {
        let selected = model.detail == nil && model.route == route
        return SwiftUI.Button { model.navigate(to: route) } label: {
            SwiftUI.Label {
                SwiftUI.Text(route.title).fontWeight(selected ? .semibold : .regular)
            } icon: {
                SwiftUI.Image(systemName: icon).foregroundStyle(SwiftUI.Color.blue)
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

    private var footer: some SwiftUI.View {
        SwiftUI.Button { model.navigate(to: .settings) } label: {
            SwiftUI.HStack(spacing: 10) {
                SwiftUI.Text(String(model.activeAccountLabel.prefix(1)).uppercased())
                    .font(.headline)
                    .frame(width: 34, height: 34)
                    .background(SwiftUI.Color.goosicPink, in: Circle())
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

    private var model: GoosicAppModel { store.model }
    private var busy: Bool { model.accountOperationInProgress || model.playbackTransition != .idle }
    private var canControl: Bool { model.currentTrack != nil && model.serviceConnected && !busy }
    private var canAdjustVolume: Bool { !busy && !model.isAdvertisement }
    private var showsTimes: Bool { progressHovered || isScrubbing }

    var body: some SwiftUI.View {
        SwiftUI.HStack(spacing: 14) {
            transport
            nowPlaying
                .frame(maxWidth: .infinity)
            trailingControls
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 6)
        .frame(minWidth: 640, maxWidth: 960)
        .modifier(NativeMacPlayerGlass())
        .shadow(color: .black.opacity(0.18), radius: 16, y: 8)
        .animation(reduceMotion ? nil : .spring(duration: 0.28, bounce: 0.12), value: volumeExpanded)
        .onChange(of: model.currentTrack?.id) { _, _ in isScrubbing = false }
        .onChange(of: model.isSeekable) { _, seekable in if !seekable { isScrubbing = false } }
    }

    private var transport: some SwiftUI.View {
        SwiftUI.HStack(spacing: 2) {
            glyph("Shuffle", "shuffle", size: 13, active: model.shuffle, subtle: true, action: model.toggleShuffle)
            glyph("Previous track", "backward.fill", size: 17, action: model.previous)
                .disabled(!canControl || model.isAdvertisement)
            SwiftUI.Button(action: model.togglePause) {
                SwiftUI.ZStack {
                    if busy {
                        SwiftUI.ProgressView().controlSize(.small)
                    } else {
                        SwiftUI.Image(systemName: model.isPaused ? "play.fill" : "pause.fill")
                            .font(.system(size: 22, weight: .semibold))
                    }
                }
                .frame(width: 40, height: 40)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(model.isPaused ? "Play" : "Pause")
            .help(busy ? "Preparing playback" : model.isPaused ? "Play" : "Pause")
            .disabled(!canControl)
            glyph("Next track", "forward.fill", size: 17, action: model.next)
                .disabled(!canControl || model.isAdvertisement)
            glyph(model.repeatMode.label, model.repeatMode == .one ? "repeat.1" : "repeat", size: 13,
                  active: model.repeatMode != .off, subtle: true, action: model.cycleRepeatMode)
        }
    }

    private var nowPlaying: some SwiftUI.View {
        SwiftUI.VStack(spacing: 4) {
            SwiftUI.HStack(spacing: 10) {
                NativeMacPanelArtwork(url: model.currentTrack?.thumbnail, size: 36)
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
                SwiftUI.Text(model.durationText).transition(.opacity)
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
                glyph("Lyrics", "quote.bubble", size: 14, active: model.lyricsVisible, action: model.toggleLyrics)
                    .transition(.move(edge: .trailing).combined(with: .opacity))
                glyph("Queue", "list.bullet", size: 14, active: model.queueVisible, action: model.toggleQueue)
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
                      model.isMuted ? "speaker.slash.fill" : "speaker.wave.2.fill", size: 14, action: model.toggleMuted)
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
            SwiftUI.Image(systemName: "ellipsis")
                .font(.system(size: 14, weight: .medium))
                .foregroundStyle(SwiftUI.Color.primary)
                .frame(width: 28, height: 32)
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
            SwiftUI.Image(systemName: name)
                .font(.system(size: size, weight: .medium))
                .foregroundStyle(active ? SwiftUI.Color.goosicPink : subtle ? SwiftUI.Color.secondary : SwiftUI.Color.primary)
                .frame(width: 28, height: 32)
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
            }
            .padding(.horizontal, 24)
            .padding(.leading, leadingInset)
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
        nsView.setAccessibilityHidden(true)
    }
}

private extension SwiftUI.Color {
    static let goosicPink = SwiftUI.Color(red: 1.0, green: 0.02, blue: 0.32)
}
#endif
