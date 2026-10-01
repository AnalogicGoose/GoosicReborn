#if os(macOS)
import AppKit
import Combine
import SwiftUI

#if !GOOSIC_UI_TEST_HOST
@main
struct GoosicMacApp: SwiftUI.App {
    @SwiftUI.StateObject private var store = NativeMacModelStore()

    init() {
        // A distributed app launches the private service beside its own executable. Keep an
        // explicit override for tests and development builds that have no bundled service.
        if ProcessInfo.processInfo.environment["GOOSIC_SERVICE_PATH"] == nil,
           let executable = Bundle.main.executableURL {
            let service = executable.deletingLastPathComponent().appendingPathComponent("goosic-service")
            if FileManager.default.isExecutableFile(atPath: service.path) {
                setenv("GOOSIC_SERVICE_PATH", service.path, 1)
            }
        }
        // `swift run` is not wrapped in an .app bundle, so explicitly opt into a foreground
        // activation policy during development. Packaged builds already receive this behavior.
        NSApplication.shared.setActivationPolicy(.regular)
        NativeMacApplicationIcon.install()
        NativeMacUpdater.shared.start()
    }

    var body: some SwiftUI.Scene {
        SwiftUI.WindowGroup("Goosic", id: "main") {
            NativeMacRootView(store: store)
                .tint(.goosicPink)
                .frame(minWidth: 920, minHeight: 680)
                .onAppear {
                    NSApplication.shared.activate(ignoringOtherApps: true)
                    if !store.model.usesDebugSidebarFixture { store.model.connect() }
                }
        }
        .defaultSize(width: 1_280, height: 800)
        .commands { NativeMacCommands(store: store) }

        // The full player in a small always-on-top window. It is the same layout, not a second
        // one to keep in step; playback stays in the main window, where the player surface is.
        SwiftUI.Window("Mini Player", id: NativeMacMiniPlayer.windowID) {
            NativeMacMiniPlayer(store: store)
                .tint(.goosicPink)
        }
        .windowResizability(.contentMinSize)
        .defaultSize(width: 320, height: 470)
    }
}
#endif

/// The menu bar's commands. Settings answers ⌘, as every Mac app's does, and Like takes
/// ⌥⇧B, Spotify's shortcut, which the Windows shell uses too.
private struct NativeMacCommands: SwiftUI.Commands {
    @ObservedObject var store: NativeMacModelStore
    @SwiftUI.Environment(\.openWindow) private var openWindow

    private var model: GoosicAppModel { store.model }

    var body: some SwiftUI.Commands {
        SwiftUI.CommandGroup(after: .appInfo) {
            SwiftUI.Button("Check for Updates…") { NativeMacUpdater.shared.check() }
                .disabled(!NativeMacUpdater.shared.isAvailable)
        }
        SwiftUI.CommandGroup(replacing: .appSettings) {
            SwiftUI.Button("Settings…") { model.navigate(to: .settings) }
                .keyboardShortcut(",", modifiers: .command)
        }
        SwiftUI.CommandGroup(after: .textEditing) {
            SwiftUI.Button("Search") {
                model.navigate(to: .search)
                store.searchFocusRequest += 1
            }
                .keyboardShortcut("f", modifiers: .command)
        }
        SwiftUI.CommandMenu("Controls") {
            SwiftUI.Button(model.isPaused ? "Play" : "Pause", action: model.togglePause)
                .disabled(model.currentTrack == nil)
            SwiftUI.Button("Next", action: model.next)
                .keyboardShortcut(.rightArrow, modifiers: .command)
                .disabled(model.queue.tracks.isEmpty || model.isAdvertisement)
            SwiftUI.Button("Previous", action: model.previous)
                .keyboardShortcut(.leftArrow, modifiers: .command)
                .disabled(model.queue.tracks.isEmpty || model.isAdvertisement)
            SwiftUI.Divider()
            SwiftUI.Button(model.isCurrentTrackLiked ? "Remove from Liked Music" : "Like",
                           action: model.toggleLikeCurrentTrack)
                .keyboardShortcut("b", modifiers: [.option, .shift])
                .disabled(!model.canRateCurrentTrack)
            SwiftUI.Button(model.shuffle ? "Shuffle Off" : "Shuffle On", action: model.toggleShuffle)
            SwiftUI.Button(model.repeatMode.label, action: model.cycleRepeatMode)
            SwiftUI.Divider()
            SwiftUI.Button("Lyrics", action: model.toggleLyrics)
                .keyboardShortcut("l", modifiers: [.command, .option])
            SwiftUI.Button("Up Next", action: model.toggleQueue)
                .keyboardShortcut("u", modifiers: [.command, .option])
            SwiftUI.Divider()
            NativeMacSleepTimerMenu(model: model)
            SwiftUI.Button("Mini Player") { openWindow(id: NativeMacMiniPlayer.windowID) }
                .keyboardShortcut("m", modifiers: [.command, .option])
            SwiftUI.Button("Full-Screen Player") { model.setFullPlayerOpen(true) }
                .keyboardShortcut("f", modifiers: [.command, .control, .shift])
                .disabled(model.currentTrack == nil)
        }
    }
}

/// The sleep timer: 15, 30, 45 or 60 minutes, or the end of the song. It pauses rather than
/// quitting, so the queue is where it was.
struct NativeMacSleepTimerMenu: SwiftUI.View {
    let model: GoosicAppModel

    // Menus are built from their own content, so the icon style is set here rather than
    // inherited from the window; without it macOS drops every menu icon.
    var body: some SwiftUI.View {
        SwiftUI.Group { items }.labelStyle(.titleAndIcon)
    }

    @SwiftUI.ViewBuilder
    private var items: some SwiftUI.View {
        SwiftUI.Menu(model.sleepTimerLabel, systemImage: "moon.zzz") {
            SwiftUI.ForEach(SleepTimerChoice.menu, id: \.self) { choice in
                SwiftUI.Button(choice.label, systemImage: choice == .endOfSong ? "music.note" : "timer") {
                    model.setSleepTimer(choice)
                }
            }
            if model.sleepTimerActive {
                SwiftUI.Divider()
                SwiftUI.Button("Turn Off", systemImage: "xmark.circle") { model.setSleepTimer(nil) }
            }
        }
    }
}

/// The mini player's window: the full player's layout, compact, floating above other windows.
struct NativeMacMiniPlayer: SwiftUI.View {
    static let windowID = "mini-player"
    @ObservedObject var store: NativeMacModelStore
    @SwiftUI.Environment(\.accessibilityReduceMotion) private var systemReduceMotion

    var body: some SwiftUI.View {
        SwiftUI.Group {
            if store.model.currentTrack != nil {
                NativeMacFullPlayer(store: store, compact: true)
            } else {
                SwiftUI.ContentUnavailableView(
                    "Nothing playing", systemImage: "music.note",
                    description: SwiftUI.Text("Choose a song in Goosic to see it here.")
                )
            }
        }
        .frame(minWidth: 260, minHeight: 380)
        .background(NativeMacFloatingWindow())
        .preferredColorScheme(store.model.theme.colorScheme)
        .environment(\.goosicArtworkModel, store.model)
        .environment(\.goosicReduceMotion, systemReduceMotion || store.model.reduceMotion)
    }
}

/// Keeps the hosting window above others and joins it to every Space, as a mini player should.
private struct NativeMacFloatingWindow: NSViewRepresentable {
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
        // Only what differs is written. This runs on every SwiftUI update, and setting a
        // window property to the value it already has still asks AppKit for another layout
        // pass, which fed a layout loop that ended in the "Update Constraints in Window" crash.
        if window.level != .floating { window.level = .floating }
        if !window.collectionBehavior.contains(.canJoinAllSpaces) { window.collectionBehavior.insert(.canJoinAllSpaces) }
        if !window.titlebarAppearsTransparent { window.titlebarAppearsTransparent = true }
        if window.titleVisibility != .hidden { window.titleVisibility = .hidden }
        if !window.styleMask.contains(.fullSizeContentView) { window.styleMask.insert(.fullSizeContentView) }
    }
}

#if !GOOSIC_UI_TEST_HOST
/// Installs the supplied Goosic artwork for both `swift run` and packaged macOS launches.
/// SwiftPM executables do not have an Xcode asset catalog, so the icon must be loaded from the
/// target resource bundle explicitly. The complete appearance set stays bundled for future
/// packaging; the default artwork is the stable macOS Dock/application icon for now.
@MainActor
private enum NativeMacApplicationIcon {
    static func install() {
        guard let url = GoosicResources.bundle.url(
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
#endif

/// Bridges the existing shared application model into SwiftUI while the Rust/service contracts
/// remain unchanged. macOS can therefore move to a fully native renderer without forking the
/// playback and catalog behavior used by future WinUI and GTK shells.
@MainActor
final class NativeMacModelStore: Combine.ObservableObject {
    let model: GoosicAppModel
    @Combine.Published var searchFocusRequest = 0
    private var observation: AnyCancellable?
    /// Tells Discord what is playing while the listener has that turned on.
    private let discord = DiscordPresenceBridge()

    init() {
        model = GoosicAppModel(
            debugSidebarFixture: ["sidebar", "quality"].contains(ProcessInfo.processInfo.environment["GOOSIC_UI_FIXTURE"] ?? "")
        )
        observation = model.objectWillChange.sink { [weak self] _ in
            self?.objectWillChange.send()
        }
    }
}

/// Public only for the dedicated Xcode UI-test host. It renders the production root with a
/// launch-selected local fixture, rather than duplicating the sidebar in a test-only screen.
public struct GoosicMacUITestHost: SwiftUI.View {
    @SwiftUI.StateObject private var store = NativeMacModelStore()

    public init() {}

    public var body: some SwiftUI.View {
        NativeMacRootView(store: store)
            .tint(.goosicPink)
            .frame(minWidth: 920, minHeight: 680)
            .onAppear {
                NSApplication.shared.activate(ignoringOtherApps: true)
                if !store.model.usesDebugSidebarFixture { store.model.connect() }
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
    @SwiftUI.Environment(\.accessibilityReduceMotion) private var systemReduceMotion
    @SwiftUI.State private var findVisible = false
    @SwiftUI.State private var playerHeight: CGFloat = 76
    @SwiftUI.Environment(\.openWindow) private var openWindow

    private var model: GoosicAppModel { store.model }
    /// The system's setting or the in-app one, whichever asks for less motion.
    private var reduceMotion: Bool { systemReduceMotion || model.reduceMotion }
    /// Pages start at the detail column's own edge now that the sidebar is a real column.
    private let leadingInset: CGFloat = 0
    private var nowPlayingPanelVisible: Bool { model.queueVisible || model.lyricsVisible }

    var body: some SwiftUI.View {
        ZStack {
            // The native split view, as Music uses: a real sidebar column, which macOS 26 draws as
            // floating Liquid Glass, and a detail column that owns its own toolbar. Back sits at
            // the detail's leading edge and Share and More at its trailing edge only because the
            // detail column is a real column; the hand-built overlay this replaces had a single
            // window toolbar, so every item crowded in beside the traffic lights.
            // The hidden YouTube Music player. It sits behind the split view at a fixed size, so
            // resizing a column never resizes this web view: its frame changes asked AppKit for
            // another constraints pass on every column layout.
            NativeMacOfficialPlaybackSurface(model: model)
                .frame(width: 640, height: 360)
                .opacity(0.001)
                .allowsHitTesting(false)
                .accessibilityHidden(true)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)

            // The sidebar steps aside under the full player, so that player's title-bar capsule
            // sits beside the traffic lights as Music's does rather than after the sidebar.
            SwiftUI.NavigationSplitView(columnVisibility: SwiftUI.Binding(
                get: { model.fullPlayerOpen && model.currentTrack != nil ? .detailOnly : .all },
                set: { _ in }
            )) {
                NativeMacSidebar(store: store)
                    .navigationSplitViewColumnWidth(min: 180, ideal: 212, max: 300)
                    // The split view adds a sidebar button of its own; Music has none, and with
                    // nowhere to sit it was folded into a stray » overflow button.
                    .toolbar(removing: .sidebarToggle)
            } detail: {
                detailColumn
                    .frame(minWidth: 0, maxWidth: .infinity)
                    // No width of its own: the content column takes whatever the sidebar and the
                    // inspector leave. Given an ideal width, the split view defended it by taking
                    // the space from the sidebar when the inspector opened, crushing the sidebar
                    // to a third of its width. The window's minimum (1,100) leaves the content at
                    // least 520 beside a 220 sidebar and a 320 inspector.
            }

            // Above everything, but the layers beneath stay mounted: the official playback
            // surface in particular must outlive opening and closing this.
            if model.fullPlayerOpen && model.currentTrack != nil {
                NativeMacFullPlayer(store: store)
                    // Scaling a view that ignores the safe area briefly exposes the content
                    // underneath around the title bar. Music keeps the surface full-size and
                    // cross-fades it instead, so the transition cannot look clipped.
                    .transition(.opacity)
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
        .alert("Edit description", isPresented: SwiftUI.Binding(
            get: { model.isEditingDescription },
            set: { if !$0 { model.cancelDescription() } }
        )) {
            SwiftUI.TextField("Description", text: SwiftUI.Binding(
                get: { model.editedDescription },
                set: { model.editedDescription = $0 }
            ))
            SwiftUI.Button("Cancel", role: .cancel) { model.cancelDescription() }
            SwiftUI.Button("Save") { model.confirmDescription() }
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
        .modifier(NativeMacTransparentToolbar())
        // Every menu shows its icons, as Music's do; macOS menus drop them unless asked.
        .labelStyle(.titleAndIcon)
        .background(NativeMacWindowChrome())
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.4), value: model.artworkBackground)
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.35), value: model.fullPlayerOpen)
        .animation(reduceMotion ? nil : .spring(duration: 0.3), value: model.notice)
        .environment(\.goosicArtworkModel, model)
        .environment(\.goosicReduceMotion, reduceMotion)
        .preferredColorScheme(model.theme.colorScheme)
    }

    /// The detail column: the page, the player bar floating over it, and the now-playing rail.
    /// The detail column: the page with the player bar floating over it, and Lyrics / Up Next
    /// beside it when open.
    ///
    /// The panel is a plain fixed-width column inside the detail rather than SwiftUI's
    /// `.inspector`. The inspector is a split view of its own: nested inside the detail it looped
    /// AppKit's constraint passes until the app crashed, and attached to the navigation split view
    /// it squeezed the sidebar to about 140 points, open or closed. A column drawn here has
    /// neither problem, and the page and player bar still make room for it.
    private var detailColumn: some SwiftUI.View {
        SwiftUI.HStack(spacing: 0) {
            pageColumn
            if nowPlayingPanelVisible {
                SwiftUI.Divider()
                NativeMacNowPlayingInspector(store: store)
                    .frame(width: 320)
                    .frame(maxHeight: .infinity, alignment: .top)
                    .background(.regularMaterial)
            }
        }
        // Deliberately not animated. Opening the panel changes the page's width, and animating
        // that re-lays out every row, grid, scroll view and the blurred backdrop on every frame;
        // lazy stacks and scroll views do not interpolate, so the whole window shook and tore
        // while it ran. Music switches the column in one step too.
        .transaction(value: nowPlayingPanelVisible) { $0.animation = nil }
            // Each item exists only while it has something to show. An empty group still
            // reserves a slot, and the toolbar folds unused slots into a stray » overflow button.
            .toolbar {
                if model.fullPlayerOpen && model.currentTrack != nil {
                    if #available(macOS 26.0, *) {
                        ToolbarItem(placement: .navigation) { fullPlayerWindowControls }
                            .sharedBackgroundVisibility(.hidden)
                    } else {
                        ToolbarItem(placement: .navigation) { fullPlayerWindowControls }
                    }
                }
                if model.detail != nil && !model.fullPlayerOpen {
                    ToolbarItem(placement: .navigation) {
                        SwiftUI.Button(action: model.closeDetail) {
                            SwiftUI.Image(systemName: "chevron.left")
                        }
                        .help("Back")
                        .accessibilityLabel("Back")
                        .keyboardShortcut("[", modifiers: .command)
                    }
                }
                if model.route == .library, model.detail == nil, model.activeAccount != nil,
                   !model.fullPlayerOpen {
                    // The library's section picker, centred in the title bar as Music does.
                    ToolbarItem(placement: .principal) {
                        SwiftUI.Picker(
                            "Library section",
                            selection: SwiftUI.Binding(
                                get: { PersonalLibrarySection(rawValue: model.libraryTab) ?? .playlists },
                                set: { model.selectLibrarySection($0) }
                            )
                        ) {
                            SwiftUI.ForEach(PersonalLibrarySection.pickerSections) { item in
                                SwiftUI.Text(item.rawValue).tag(item)
                            }
                        }
                        .pickerStyle(.segmented)
                        .labelsHidden()
                    }
                }
                // In the split view's detail column `.automatic` is the column's leading edge; a
                // flexible spacer is what carries the items after it to the trailing edge.
                if model.detail != nil || (model.fullPlayerOpen && model.currentTrack != nil) {
                    NativeMacTrailingSpacer()
                }
                if let entity = model.detail, !model.fullPlayerOpen {
                    // Find, Share and More at the trailing edge, as in Music.
                    ToolbarItemGroup(placement: .automatic) {
                        if case .playlist = entity {
                            if findVisible || !model.trackFindText.isEmpty {
                                SwiftUI.TextField("Find in Playlist", text: SwiftUI.Binding(
                                    get: { model.trackFindText },
                                    set: { model.trackFindText = $0 }
                                ))
                                .textFieldStyle(.roundedBorder)
                                .frame(width: 180)
                            }
                            SwiftUI.Button {
                                if findVisible { model.trackFindText = "" }
                                findVisible.toggle()
                            } label: {
                                SwiftUI.Image(systemName: findVisible ? "xmark" : "magnifyingglass")
                            }
                            .help(findVisible ? "Close Find" : "Find in Playlist")
                            .keyboardShortcut("f", modifiers: [.command, .shift])
                        }
                        if let url = NativeMacLinks.url(for: entity) {
                            SwiftUI.ShareLink(item: url) {
                                SwiftUI.Image(systemName: "square.and.arrow.up")
                            }
                            .help("Share")
                        }
                        NativeMacPageMenu(entity: entity, model: model)
                    }
                }
                if model.fullPlayerOpen && model.currentTrack != nil {
                    // Only a toolbar item keeps its drag in the title bar; a slider drawn there by
                    // the content would move the window instead.
                    // Volume at the trailing edge, as in Music.
                    if #available(macOS 26.0, *) {
                        ToolbarItem(placement: .automatic) { fullPlayerVolume }
                            .sharedBackgroundVisibility(.hidden)
                    } else {
                        ToolbarItem(placement: .automatic) { fullPlayerVolume }
                    }
                }
            }
    }

    private var fullPlayerWindowControls: some SwiftUI.View {
        SwiftUI.HStack(spacing: 2) {
            SwiftUI.Button { model.setFullPlayerOpen(false) } label: {
                NativeMacFixedSymbol(name: "xmark", glyphSize: 15, width: 34, height: 34)
            }
            .keyboardShortcut(.cancelAction)
            .help("Close")
            .accessibilityLabel("Close full-screen player")
            SwiftUI.Button {
                openWindow(id: NativeMacMiniPlayer.windowID)
                model.setFullPlayerOpen(false)
            } label: {
                NativeMacFixedSymbol(name: "pip.enter", glyphSize: 17, width: 34, height: 34)
            }
            .help("Mini Player")
        }
        .buttonStyle(NativeMacPlayerButtonStyle())
        .padding(4)
        .modifier(NativeMacGlassCapsule())
        .environment(\.colorScheme, .dark)
    }

    private var fullPlayerVolume: some SwiftUI.View {
        NativeMacFullPlayerVolume(store: store)
            .modifier(NativeMacGlassCapsule())
            .environment(\.colorScheme, .dark)
    }

    /// The page, the artwork backdrop under it, and the player bar and notices floating over it.
    private var pageColumn: some SwiftUI.View {
        ZStack {
            // Bottommost, and extended under the glass sidebar, so the sidebar has the playing
            // artwork to refract as Music's does.
            if model.artworkBackground {
                NativeMacArtworkBackdrop(file: model.artworkFile(for: model.currentTrack?.thumbnail))
                    .modifier(NativeMacBackgroundExtension())
                    .transition(.opacity)
            }
        ZStack(alignment: .bottom) {
            detailContent
                .frame(maxWidth: .infinity, maxHeight: .infinity)

            NativeMacPlayerBar(store: store)
                .background {
                    GeometryReader { proxy in
                        Color.clear.preference(key: NativeMacPlayerHeight.self, value: proxy.size.height)
                    }
                }
                .padding(.leading, leadingInset)
                .padding(.horizontal, 20)
                // This is a persistent content rail, so keep the capsule out from under it.
                .padding(.bottom, 12)

            if let notice = model.notice {
                NativeMacNoticeView(notice: notice, dismiss: model.dismissNotice)
                    .padding(.leading, leadingInset)
                    .padding(.bottom, playerHeight + 24)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
                    .id(notice.id)
            }


        }
        .environment(\.nativeMacPlayerClearance, playerHeight + 28)
        .onPreferenceChange(NativeMacPlayerHeight.self) { height in
            if abs(playerHeight - height) > 0.5 { playerHeight = height }
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            // The official player is mounted off screen. Its bridge diagnostics are useful
            // when debugging, but a normal listener should not see implementation chatter
            // such as an event from a superseded document. Keep this space for a real service
            // outage, where Reconnect is an actionable control.
            if !model.serviceConnected {
                SwiftUI.HStack(spacing: 8) {
                    SwiftUI.Spacer()
                    SwiftUI.Image(systemName: "wifi.slash")
                    SwiftUI.Text(model.debugMode ? model.status : model.isConnecting
                        ? "Connecting to Goosic…" : "The playback service is unavailable.")
                        .lineLimit(2).font(.callout)
                    if model.isConnecting { SwiftUI.ProgressView().controlSize(.small) }
                    SwiftUI.Button("Reconnect", action: model.connect)
                        .disabled(model.isConnecting)
                    SwiftUI.Spacer()
                }
                .padding(10)
                .padding(.leading, leadingInset)
                .background(.thinMaterial)
            } else if model.sessionExpired, let account = model.activeAccount {
                // Guest pages must not pass for the account's own: say whose session ended
                // and offer the way back.
                SwiftUI.HStack(spacing: 10) {
                    SwiftUI.Image(systemName: "person.crop.circle.badge.exclamationmark")
                        .foregroundStyle(SwiftUI.Color.orange)
                    SwiftUI.Text("Your YouTube Music session for \(account.displayName) ended, so Goosic is showing guest content.")
                        .font(.callout)
                        .lineLimit(2)
                    SwiftUI.Spacer()
                    SwiftUI.Button("Sign In Again") { model.signInAgain() }
                        .disabled(model.accountOperationInProgress)
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 8)
                .padding(.leading, leadingInset)
                .padding(.top, 28)
                .background(.thinMaterial)
            } else if model.debugMode {
                // Debug mode shows how Goosic works inside: the status line and what the
                // web player last said. With it off these go only to the log.
                SwiftUI.VStack(alignment: .leading, spacing: 2) {
                    SwiftUI.Text(model.status).lineLimit(1)
                    SwiftUI.Text(model.hostStatus).lineLimit(1).foregroundStyle(.secondary)
                }
                .font(.caption2.monospaced())
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 12)
                .padding(.vertical, 4)
                .padding(.leading, leadingInset)
                .padding(.top, 28)
                .background(.thinMaterial)
            }
        }
        }
    }

    @SwiftUI.ViewBuilder
    private var detailContent: some SwiftUI.View {
        if let entity = model.detail {
            NativeMacEntityView(entity: entity, store: store)
        } else {
            switch model.route {
            case .home, .explore, .charts, .moodsAndGenres, .newReleases, .history:
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
        case .history: "What you played recently"
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
    @SwiftUI.Environment(\.goosicReduceMotion) private var reduceMotion
    @State private var rendered: (file: URL, image: CGImage)?

    var body: some SwiftUI.View {
        SwiftUI.Color.clear
            .overlay {
                if let file, let rendered, rendered.file == file {
                    SwiftUI.Image(decorative: rendered.image, scale: 1)
                        .resizable()
                        .scaledToFill()
                        .id(file)
                        .transition(.opacity)
                }
            }
            .overlay {
                if let rendered, rendered.file == file {
                    SwiftUI.Color(nsColor: .windowBackgroundColor)
                        .opacity(ArtworkBackdropRenderer.tintOpacity)
                }
            }
            .clipped()
            .ignoresSafeArea()
            .allowsHitTesting(false)
            .accessibilityHidden(true)
            .animation(reduceMotion ? nil : .easeInOut(duration: 0.6), value: file)
            .task(id: file) {
                rendered = nil
                guard let file else { return }
                let image = await ArtworkBackdropRenderer.backdrop(for: file)
                guard !Task.isCancelled, let image else { return }
                rendered = (file, image)
            }
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
        // Only what differs is written. This runs on every SwiftUI update, and setting a
        // window property to the value it already has still asks AppKit for another layout
        // pass, which fed a layout loop that ended in the "Update Constraints in Window" crash.
        if !window.titlebarAppearsTransparent { window.titlebarAppearsTransparent = true }
        if window.titleVisibility != .hidden { window.titleVisibility = .hidden }
        if !window.styleMask.contains(.fullSizeContentView) { window.styleMask.insert(.fullSizeContentView) }
    }
}

/// Width of the floating sidebar, so page layout begins beside it while the detail column is
/// drawn full-width underneath. Zero when the sidebar is hidden.
private struct NativeMacLeadingInsetKey: SwiftUI.EnvironmentKey {
    static let defaultValue: CGFloat = 0
}

private struct GoosicReduceMotionKey: SwiftUI.EnvironmentKey {
    static let defaultValue = false
}

extension SwiftUI.EnvironmentValues {
    var goosicReduceMotion: Bool {
        get { self[GoosicReduceMotionKey.self] }
        set { self[GoosicReduceMotionKey.self] = newValue }
    }

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
struct NativeMacFixedSymbol: SwiftUI.View {
    let name: String
    let glyphSize: CGFloat
    let width: CGFloat
    let height: CGFloat

    var body: some SwiftUI.View {
        SwiftUI.Image(systemName: name)
            .resizable()
            .scaledToFit()
            .frame(width: glyphSize, height: glyphSize)
            .modifier(NativeMacSymbolTransition(symbol: name))
            .frame(width: width, height: height)
    }
}

/// Animate only control state changes, never the playback clock or the surrounding layout.
struct NativeMacSymbolTransition: SwiftUI.ViewModifier {
    let symbol: String
    @SwiftUI.Environment(\.goosicReduceMotion) private var reduceMotion

    func body(content: Content) -> some SwiftUI.View {
        content
            .contentTransition(reduceMotion ? .identity : .symbolEffect(.replace))
            .animation(reduceMotion ? nil : .smooth(duration: 0.22), value: symbol)
    }
}

/// The same small press response for transport, panel, and volume controls in every player.
struct NativeMacPlayerButtonStyle: SwiftUI.ButtonStyle {
    @SwiftUI.Environment(\.goosicReduceMotion) private var reduceMotion

    func makeBody(configuration: Configuration) -> some SwiftUI.View {
        configuration.label
            .scaleEffect(configuration.isPressed && !reduceMotion ? 0.88 : 1)
            .opacity(configuration.isPressed ? 0.72 : 1)
            .animation(reduceMotion ? nil : .spring(duration: 0.22, bounce: 0.25),
                       value: configuration.isPressed)
    }
}

private struct NativeMacSidebar: SwiftUI.View {
    static let width: CGFloat = 210

    @ObservedObject var store: NativeMacModelStore
    @SwiftUI.Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @SwiftUI.State private var confirmSignOut = false

    private var model: GoosicAppModel { store.model }

    var body: some SwiftUI.View {
        SwiftUI.VStack(spacing: 0) {
            sidebarList
                .mask(sidebarScrollMask)
            accountButton
        }
        // The split view draws the sidebar's glass and sets its width; nothing is layered here.
        .onAppear {
            if !model.usesDebugSidebarFixture { model.loadUserPlaylists() }
        }
        // The sidebar is created before the asynchronous account snapshot returns. Re-read when
        // the account appears (or changes) so an initially empty sidebar is not mistaken for an
        // account with no playlists.
        .onChange(of: model.activeAccount?.id) { _, _ in
            if !model.usesDebugSidebarFixture { model.loadUserPlaylists(force: true) }
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
                likedMusicRow
                libraryRow(.artists, title: "Artists", icon: "music.mic")
                libraryRow(.albums, title: "Albums", icon: "square.stack")
                row(.history, icon: "clock.arrow.circlepath")
                    .disabled(model.activeAccount == nil)
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
                    .foregroundStyle(SwiftUI.Color.goosicPink)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("sidebar.route.\(route.rawValue)")
        .accessibilityAddTraits(selected ? .isSelected : [])
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
                    .foregroundStyle(SwiftUI.Color.goosicPink)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selected ? .isSelected : [])
        .listRowBackground(
            RoundedRectangle(cornerRadius: 6)
                .fill(selected ? SwiftUI.Color.primary.opacity(0.09) : SwiftUI.Color.clear)
                .padding(.horizontal, 10)
        )
    }

    /// Liked Music, named as its page names it. Every link to `LM` or `VLLM` lights this row.
    private var likedMusicRow: some SwiftUI.View {
        let selected = model.detail?.isLikedMusic ?? false
        return SwiftUI.Button {
            model.show(.likedMusic)
        } label: {
            SwiftUI.Label {
                SwiftUI.Text("Liked Music").fontWeight(selected ? .semibold : .regular)
            } icon: {
                NativeMacFixedSymbol(name: "heart.square", glyphSize: 16, width: 22, height: 22)
                    .foregroundStyle(SwiftUI.Color.goosicPink)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(model.activeAccount == nil)
        .accessibilityAddTraits(selected ? .isSelected : [])
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
        .accessibilityIdentifier("sidebar.playlist.\(playlist.id)")
        .accessibilityAddTraits(selected ? .isSelected : [])
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

    /// The account menu, as Windows has it beside its sidebar: switch accounts, browse as a
    /// guest, add another, sign in again when a session has ended, or sign out.
    private var accountButton: some SwiftUI.View {
        SwiftUI.Menu {
            NativeMacAccountMenuItems(model: model, confirmSignOut: $confirmSignOut)
        } label: {
            SwiftUI.HStack(spacing: 10) {
                NativeMacAccountAvatar(
                    url: model.activeAccount?.avatarUrl,
                    fallback: String(model.activeAccountLabel.prefix(1)).uppercased()
                )
                SwiftUI.VStack(alignment: .leading, spacing: 1) {
                    SwiftUI.Text(model.activeAccountLabel).font(.subheadline.weight(.semibold)).lineLimit(1)
                    SwiftUI.Text(accountDetail)
                        .lineLimit(1)
                        .font(.caption2)
                        .foregroundStyle(model.sessionExpired ? SwiftUI.Color.orange : SwiftUI.Color.secondary)
                }
                SwiftUI.Spacer()
                SwiftUI.Image(systemName: "chevron.up.chevron.down")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .contentShape(Rectangle())
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .padding(.horizontal, 8)
        .padding(.bottom, 8)
        .accessibilityLabel("Account: \(model.activeAccountLabel)")
        .accessibilityIdentifier("sidebar.account")
        .alert(
            "Sign out of \(model.activeAccountLabel)?",
            isPresented: $confirmSignOut
        ) {
            SwiftUI.Button("Cancel", role: .cancel) {}
            SwiftUI.Button("Sign Out", role: .destructive) { model.signOut() }
        } message: {
            SwiftUI.Text("Goosic forgets this account and clears its sign-in from this Mac.")
        }
    }

    private var accountDetail: String {
        if !model.serviceConnected { return "Offline" }
        if model.activeAccount == nil { return "Browsing as a guest" }
        if model.sessionExpired { return "Session ended — sign in again" }
        return model.activeAccount?.email ?? "Signed in"
    }
}

/// The account menu's entries, shared by the sidebar and the Settings page.
struct NativeMacAccountMenuItems: SwiftUI.View {
    let model: GoosicAppModel
    @SwiftUI.Binding var confirmSignOut: Bool

    private var busy: Bool {
        !model.serviceConnected || model.accountOperationInProgress
            || model.playbackTransition != .idle || model.isAdvertisement
    }

    // Menus are built from their own content, so the icon style is set here rather than
    // inherited from the window; without it macOS drops every menu icon.
    var body: some SwiftUI.View {
        SwiftUI.Group { items }.labelStyle(.titleAndIcon)
    }

    @SwiftUI.ViewBuilder
    private var items: some SwiftUI.View {
        if model.sessionExpired {
            SwiftUI.Button("Sign In Again…", systemImage: "arrow.clockwise.circle") { model.signInAgain() }
                .disabled(busy)
            SwiftUI.Divider()
        }
        if !model.accounts.isEmpty {
            SwiftUI.Section("Accounts") {
                SwiftUI.ForEach(model.accounts, id: \.id) { account in
                    SwiftUI.Button {
                        model.switchAccount(to: account.id)
                    } label: {
                        SwiftUI.Label(
                            account.displayName,
                            systemImage: account.id == model.activeAccountId ? "checkmark.circle.fill" : "person.crop.circle"
                        )
                    }
                    .disabled(busy || account.id == model.activeAccountId)
                }
            }
        }
        if model.activeAccount != nil {
            SwiftUI.Button("Browse as a Guest", systemImage: "person.crop.circle.badge.questionmark") {
                model.browseAsGuest()
            }
            .disabled(busy)
        }
        SwiftUI.Button(
            model.accounts.isEmpty ? "Sign In with Google…" : "Add Another Account…",
            systemImage: "person.badge.plus"
        ) { model.signIn() }
        .disabled(busy)
        if model.activeAccount != nil {
            SwiftUI.Divider()
            SwiftUI.Button("Sign Out…", systemImage: "rectangle.portrait.and.arrow.right", role: .destructive) {
                confirmSignOut = true
            }
            .disabled(busy)
        }
        SwiftUI.Divider()
        SwiftUI.Button("Account Settings…", systemImage: "gearshape") { model.navigate(to: .settings) }
    }
}

private struct NativeMacAccountAvatar: SwiftUI.View {
    let url: String?
    let fallback: String

    var body: some SwiftUI.View {
        NativeMacCachedImage(url: url, maxPixels: 68) { image in
            if let image {
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
        NativeMacCachedImage(url: url, maxPixels: 36) { image in
            if let image {
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
/// speaker button. Panel toggles remain visible as the controls wrap in a narrow window.
/// Every control is a plain glyph on the glass; volume expands into a small inner capsule.
private struct NativeMacPlayerBar: SwiftUI.View {
    @ObservedObject var store: NativeMacModelStore
    @SwiftUI.Environment(\.goosicReduceMotion) private var reduceMotion
    @SwiftUI.State private var scrubPosition: Double = 0
    @SwiftUI.State private var isScrubbing = false
    @SwiftUI.FocusState private var progressFocused: Bool
    @SwiftUI.State private var statusVisible = false
    @SwiftUI.State private var volumeVisible = false
    @SwiftUI.Environment(\.openWindow) private var openWindow
    @SwiftUI.State private var progressHovered = false
    @SwiftUI.State private var progressHoverGeneration = 0

    private var model: GoosicAppModel { store.model }
    private var busy: Bool { model.accountOperationInProgress || model.playbackTransition != .idle }
    private var canControl: Bool { model.currentTrack != nil && model.serviceConnected && !busy }
    private var canAdjustVolume: Bool { !busy && !model.isAdvertisement }
    /// Keep the player compact while listening. The timeline is the one interaction that needs
    /// additional space, so it alone reveals the full scrubber and elapsed/remaining times.
    /// This prevents a newly selected song from making the whole chrome jump in height.
    private var playerExpanded: Bool {
        progressHovered || isScrubbing
    }
    private var showsTimes: Bool { playerExpanded }

    var body: some SwiftUI.View {
        // Music's proportions: transport at the leading edge, a fixed-width now-playing cluster,
        // and the panel buttons at the trailing edge, in a compact capsule rather than one that
        // stretches with the window.
        SwiftUI.ViewThatFits(in: .horizontal) {
            SwiftUI.HStack(spacing: 18) {
                transport
                nowPlaying
                    .frame(minWidth: 120, maxWidth: 370)
                SwiftUI.Spacer(minLength: 0)
                trailingControls
            }
            // Reserve the capsule's full content width. With a short title, a fixed-size
            // HStack shrinks to its intrinsic width and SwiftUI centers that whole row,
            // moving the transport away from the leading edge.
            .frame(width: 708)

            SwiftUI.HStack(spacing: 10) {
                compactTransport
                nowPlaying
                    .frame(minWidth: 0, maxWidth: .infinity)
                trailingControls
            }
            .frame(minWidth: 440)

            SwiftUI.VStack(spacing: 6) {
                nowPlaying
                SwiftUI.ViewThatFits(in: .horizontal) {
                    SwiftUI.HStack(spacing: 4) {
                        transport
                        SwiftUI.Spacer(minLength: 0)
                        trailingControls
                    }
                    SwiftUI.VStack(spacing: 4) {
                        transport
                        trailingControls
                    }
                }
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
        // Music's compact width when there is room, narrower when the inspector takes some. A
        // fixed width here, beside the sidebar and inspector columns, was wider than the window's
        // minimum, and AppKit crashed re-solving a layout that could never fit.
        .frame(minWidth: 0, maxWidth: 740)
        .modifier(NativeMacPlayerGlass())
        .shadow(color: .black.opacity(0.18), radius: 16, y: 8)
        .animation(reduceMotion ? nil : .spring(duration: 0.28, bounce: 0.12), value: playerExpanded)
        .onChange(of: model.currentTrack?.id) { _, _ in
            isScrubbing = false
            progressFocused = false
        }
        .onChange(of: model.isSeekable) { _, seekable in if !seekable { isScrubbing = false } }
    }

    private var transport: some SwiftUI.View {
        SwiftUI.HStack(spacing: 2) {
            glyph(model.shuffle ? "Turn shuffle off" : "Turn shuffle on", "shuffle", size: 15, active: model.shuffle, subtle: true, action: model.toggleShuffle)
            glyph("Previous track", "backward.fill", size: 17, action: model.previous)
                .disabled(!canControl || model.isAdvertisement)
            playPauseButton
            glyph("Next track", "forward.fill", size: 17, action: model.next)
                .disabled(!canControl || model.isAdvertisement)
            glyph(model.repeatMode.label, model.repeatMode == .one ? "repeat.1" : "repeat", size: 15,
                  active: model.repeatMode != .off, subtle: true, action: model.cycleRepeatMode)
        }
    }

    private var compactTransport: some SwiftUI.View {
        SwiftUI.HStack(spacing: 2) {
            glyph("Previous track", "backward.fill", size: 17, action: model.previous)
                .disabled(!canControl || model.isAdvertisement)
            playPauseButton
            glyph("Next track", "forward.fill", size: 17, action: model.next)
                .disabled(!canControl || model.isAdvertisement)
        }
    }

    private var playPauseButton: some SwiftUI.View {
        SwiftUI.Button(action: model.togglePause) {
            SwiftUI.ZStack {
                NativeMacFixedSymbol(name: model.isPaused ? "play.fill" : "pause.fill",
                                     glyphSize: 19, width: 34, height: 30)
                    .opacity(busy ? 0 : 1)
                if busy { SwiftUI.ProgressView().controlSize(.small) }
            }
            .frame(width: 34, height: 30)
            .contentShape(Rectangle())
        }
        .buttonStyle(NativeMacPlayerButtonStyle())
        .accessibilityLabel(model.isPaused ? "Play" : "Pause")
        .help(busy ? "Preparing playback" : model.isPaused ? "Play" : "Pause")
        .disabled(!canControl)
    }

    /// The artwork and the song, with the progress line directly beneath both, from the
    /// artwork's leading edge to the end of the text, as Music draws it.
    private var nowPlaying: some SwiftUI.View {
        SwiftUI.VStack(alignment: .leading, spacing: 3) {
            SwiftUI.HStack(spacing: 10) {
                NativeMacExpandableArtwork(
                    url: model.currentTrack?.thumbnail,
                    size: 34,
                    enabled: model.currentTrack != nil
                ) { model.setFullPlayerOpen(true) }
                SwiftUI.VStack(alignment: .leading, spacing: 1) {
                    SwiftUI.Text(model.currentTrack?.title ?? "Nothing playing")
                        .font(.body.weight(.semibold)).lineLimit(1)
                    SwiftUI.Text(busy ? "Preparing playback…" : model.nowPlayingSubtitle)
                        .font(.callout).foregroundStyle(.secondary).lineLimit(1)
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
        // The times join the line at either end while it is hovered or dragged. They stay on the
        // line's own row, inside the capsule, so only the line shortens and nothing else moves.
        return SwiftUI.HStack(spacing: 6) {
            if showsTimes {
                SwiftUI.Text(GoosicAppModel.timeText(position))
                    .transition(.opacity)
            }
            SwiftUI.GeometryReader { proxy in
                SwiftUI.ZStack(alignment: .leading) {
                    Capsule().fill(.primary.opacity(0.15))
                    Capsule().fill(.primary.opacity(0.6))
                        .frame(width: max(proxy.size.width * position / total, 0))
                }
                .frame(height: showsTimes ? 4 : 3)
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
                            progressFocused = false
                        }
                )
            }
            if showsTimes {
                SwiftUI.Text("−" + GoosicAppModel.timeText(max(total - position, 0)))
                    .transition(.opacity)
            }
        }
        .font(.caption2.monospacedDigit())
        .foregroundStyle(.secondary)
        .frame(height: 10)
        .opacity(model.isSeekable ? 1 : 0.45)
        .onHover(perform: scheduleProgressHover)
        .animation(reduceMotion ? nil : .easeOut(duration: 0.15), value: showsTimes)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Playback position")
        .accessibilityValue("\(model.elapsedText) of \(model.durationText)")
        .accessibilityAdjustableAction { direction in
            guard model.isSeekable, !busy else { return }
            let step = direction == .increment ? 10.0 : -10.0
            model.seek(to: min(max(position + step, 0), total))
        }
        .focusable(model.isSeekable && !busy)
        .focused($progressFocused)
        .focusEffectDisabled()
        .overlay {
            if progressFocused {
                Capsule().stroke(SwiftUI.Color.goosicPink.opacity(0.5), lineWidth: 1)
                    .allowsHitTesting(false)
            }
        }
        .onExitCommand { progressFocused = false }
        .onMoveCommand { direction in
            guard model.isSeekable, !busy else { return }
            if direction == .left { model.seek(to: max(position - 10, 0)) }
            if direction == .right { model.seek(to: min(position + 10, total)) }
        }
    }

    /// Small enter/exit hysteresis prevents the scrubber's own expansion from producing a
    /// leave-enter loop when the pointer sits on its edge.
    private func scheduleProgressHover(_ hovering: Bool) {
        progressHoverGeneration &+= 1
        let generation = progressHoverGeneration
        let delay = hovering ? 0.24 : 0.14
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) {
            guard generation == progressHoverGeneration else { return }
            progressHovered = hovering
        }
    }

    /// Volume is an overlay with no layout footprint. Its opening never changes the bar's
    /// fitting choice, height, or the artwork and transport positions.
    private var trailingControls: some SwiftUI.View {
        SwiftUI.HStack(spacing: 6) {
            moreMenu
            glyph(model.lyricsVisible ? "Hide lyrics" : "Show lyrics", "quote.bubble", size: 16, active: model.lyricsVisible, action: model.toggleLyrics)
            glyph(model.queueVisible ? "Hide Up Next" : "Show Up Next", "list.bullet", size: 16, active: model.queueVisible, action: model.toggleQueue)
            volumeToggle
        }
        .fixedSize()
        .opacity(volumeVisible ? 0 : 1)
        .allowsHitTesting(!volumeVisible)
        .accessibilityHidden(volumeVisible)
        .overlay(alignment: .trailing) {
            if volumeVisible {
                SwiftUI.HStack(spacing: 8) {
                    // Keep More available above the covered panel buttons, including their actions.
                    moreMenu
                    volumeControl
                }
                .fixedSize()
                .transition(reduceMotion ? .identity : .opacity.combined(with: .scale(scale: 0.96, anchor: .trailing)))
            }
        }
        .zIndex(1)
        .animation(reduceMotion ? nil : .easeOut(duration: 0.18), value: volumeVisible)
    }

    private var volumeToggle: some SwiftUI.View {
        glyph(volumeVisible ? "Hide volume controls" : "Show volume controls",
              model.isMuted ? "speaker.slash.fill" : "speaker.wave.2.fill", size: 16) {
            volumeVisible.toggle()
            progressFocused = false
        }
        .accessibilityValue(volumeVisible ? "Expanded" : "Collapsed")
        .accessibilityAction(named: SwiftUI.Text(model.isMuted ? "Unmute" : "Mute")) {
            if canAdjustVolume { model.toggleMuted() }
        }
        .contextMenu { muteMenuItem }
    }

    private var volumeControl: some SwiftUI.View {
        SwiftUI.HStack(spacing: 8) {
            SwiftUI.Slider(value: Binding(
                get: { model.isMuted ? 0 : model.volume },
                set: { model.setVolume($0) }
            ), in: 0...1)
            .controlSize(.small)
            .tint(SwiftUI.Color.primary)
            .frame(width: 110)
            .accessibilityLabel("Volume")
            .accessibilityValue("\(Int((model.isMuted ? 0 : model.volume) * 100)) percent")
            .disabled(!canAdjustVolume)
            volumeToggle
        }
        .padding(.leading, 12)
        .padding(.trailing, 4)
        .padding(.vertical, 3)
        .modifier(NativeMacGlassCapsule())
        .onExitCommand { volumeVisible = false }
        .contextMenu { muteMenuItem }
    }

    private var muteMenuItem: some SwiftUI.View {
        SwiftUI.Button(model.isMuted ? "Unmute" : "Mute",
                       systemImage: model.isMuted ? "speaker.wave.2.fill" : "speaker.slash.fill",
                       action: model.toggleMuted)
            .disabled(!canAdjustVolume)
    }

    private var moreMenu: some SwiftUI.View {
        SwiftUI.Menu {
            SwiftUI.Group { moreMenuItems }.labelStyle(.titleAndIcon)
        } label: {
            NativeMacFixedSymbol(name: "ellipsis", glyphSize: 15, width: 30, height: 30)
                .foregroundStyle(model.sleepTimerActive ? SwiftUI.Color.goosicPink : SwiftUI.Color.primary)
                .contentShape(Rectangle())
        }
        .menuStyle(.button)
        .buttonStyle(NativeMacPlayerButtonStyle())
        .menuIndicator(.hidden)
        .fixedSize()
        .help("More")
        .accessibilityLabel("More playback actions")
        .popover(isPresented: $statusVisible) {
            SwiftUI.VStack(alignment: .leading, spacing: 8) {
                SwiftUI.Text("Playback status").font(.headline)
                SwiftUI.Text(model.status)
                if model.isAdvertisement { SwiftUI.Text("Advertisement · Seeking and track changes are unavailable.") }
            }
            .font(.callout).padding(18).frame(width: 300)
        }
    }

    @SwiftUI.ViewBuilder
    private var moreMenuItems: some SwiftUI.View {
            if let track = model.currentTrack {
                NativeMacTrackMenuItems(track: track, model: model, includePlay: false)
                SwiftUI.Divider()
            }
            NativeMacSleepTimerMenu(model: model)
            muteMenuItem
            SwiftUI.Button("Import Timed Lyrics…", systemImage: "text.badge.plus") {
                NativeMacLyricsImport.open(model: model)
            }
            .disabled(model.currentTrack == nil)
            SwiftUI.Button("Lyrics", systemImage: "quote.bubble", action: model.toggleLyrics)
            SwiftUI.Button("Up Next", systemImage: "list.bullet", action: model.toggleQueue)
            SwiftUI.Button("Mini Player", systemImage: "pip.enter") {
                openWindow(id: NativeMacMiniPlayer.windowID)
            }
            SwiftUI.Button("Full-Screen Player", systemImage: "arrow.up.left.and.arrow.down.right") {
                model.setFullPlayerOpen(true)
            }
            .disabled(model.currentTrack == nil)
            if model.debugMode {
                SwiftUI.Divider()
                SwiftUI.Button("Playback Status…", systemImage: "info.circle") { statusVisible = true }
            }
    }

    private func glyph(_ title: String, _ name: String, size: CGFloat, active: Bool = false,
                       subtle: Bool = false, action: @escaping () -> Void) -> some SwiftUI.View {
        SwiftUI.Button(action: action) {
            NativeMacFixedSymbol(name: name, glyphSize: size, width: 30, height: 30)
                .foregroundStyle(active ? SwiftUI.Color.goosicPink : subtle ? SwiftUI.Color.secondary : SwiftUI.Color.primary)
                .contentShape(Rectangle())
                .background(active ? SwiftUI.Color.primary.opacity(0.12) : .clear, in: Circle())
                .animation(reduceMotion ? nil : .easeOut(duration: 0.18), value: active)
        }
        .buttonStyle(NativeMacPlayerButtonStyle())
        .accessibilityLabel(title)
        .accessibilityAddTraits(active ? .isSelected : [])
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
            content.glassEffect(.regular, in: .capsule)
        } else {
            content.background(.regularMaterial, in: Capsule())
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

private struct NativeMacPlayerHeight: SwiftUI.PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = max(value, nextValue()) }
}

private struct NativeMacPlayerClearanceKey: SwiftUI.EnvironmentKey {
    static let defaultValue: CGFloat = 112
}

extension SwiftUI.EnvironmentValues {
    var nativeMacPlayerClearance: CGFloat {
        get { self[NativeMacPlayerClearanceKey.self] }
        set { self[NativeMacPlayerClearanceKey.self] = newValue }
    }
}

private struct NativeMacSearchHeaderHeight: SwiftUI.PreferenceKey {
    static let defaultValue: CGFloat = 0

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = max(value, nextValue())
    }
}

private struct NativeMacSearchView: SwiftUI.View {
    @SwiftUI.Environment(\.nativeMacPlayerClearance) private var playerClearance
    @ObservedObject var store: NativeMacModelStore
    @SwiftUI.Environment(\.nativeMacLeadingInset) private var leadingInset
    @SwiftUI.Environment(\.goosicReduceMotion) private var reduceMotion
    @SwiftUI.FocusState private var fieldFocused: Bool
    @SwiftUI.State private var headerHeight: CGFloat = 124

    private var model: GoosicAppModel { store.model }
    private var suggestions: [String] {
        RecentSearches.suggest(model.recentSearches, model.query)
    }

    var body: some SwiftUI.View {
        SwiftUI.ZStack(alignment: .top) {
            if model.submittedQuery.isEmpty {
                browseContent
            } else {
                NativeMacCatalogPage(
                    key: model.currentSearchKey,
                    title: "Search",
                    subtitle: "Results for “\(model.submittedQuery)”",
                    state: model.state(for: model.currentSearchKey),
                    model: model,
                    contentTopInset: headerHeight
                )
            }

            floatingControls
                .zIndex(1)

            if fieldFocused && !model.submittedQuery.isEmpty && !suggestions.isEmpty {
                recentSearches(suggestions, title: "Recent searches")
                    .padding(16)
                    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
                    .padding(.horizontal, 24)
                    .padding(.leading, leadingInset)
                    .padding(.top, headerHeight)
                    .zIndex(2)
            }
        }
        .onAppear {
            if store.searchFocusRequest > 0 { fieldFocused = true }
        }
        .onChange(of: store.searchFocusRequest) { _, _ in fieldFocused = true }
    }

    /// Keep the scroll viewport full height. Only the scrollable content is inset beneath
    /// these floating controls, so results can pass behind their material without a header band.
    private var floatingControls: some SwiftUI.View {
        NativeMacGlassGroup {
            SwiftUI.VStack(spacing: 16) {
                searchField
                    .frame(maxWidth: 620)
                    .frame(maxWidth: .infinity)
                filterBar
            }
        }
        .padding(.horizontal, 24)
        .padding(.leading, leadingInset)
        .padding(.top, 18)
        .padding(.bottom, 12)
        .background {
            SwiftUI.GeometryReader { proxy in
                SwiftUI.Color.clear.preference(key: NativeMacSearchHeaderHeight.self, value: proxy.size.height)
            }
        }
        .onPreferenceChange(NativeMacSearchHeaderHeight.self) { measured in
            if abs(headerHeight - measured) > 0.5 { headerHeight = measured }
        }
    }

    private var searchField: some SwiftUI.View {
        SwiftUI.HStack(spacing: 10) {
            SwiftUI.Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
            SwiftUI.TextField("Search music", text: Binding(
                get: { model.query }, set: { model.query = $0 }
            ))
            .textFieldStyle(.plain)
            .focused($fieldFocused)
            .onSubmit(submit)
            if !model.query.isEmpty {
                SwiftUI.Button {
                    model.query = ""
                    model.search()
                    fieldFocused = true
                } label: {
                    SwiftUI.Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
                .help("Clear search")
                .accessibilityLabel("Clear search")
            }
            SwiftUI.Button("Search", action: submit)
                .buttonStyle(.plain)
                .foregroundStyle(SwiftUI.Color.goosicPink)
                .disabled(model.query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .modifier(NativeMacSearchGlass())
    }

    /// Let the system draw and animate the macOS segmented control. Every selection still
    /// uses its own upstream query and cache; narrow columns can scroll to all six segments.
    private var filterBar: some SwiftUI.View {
        SwiftUI.ViewThatFits(in: .horizontal) {
            filterPicker
            SwiftUI.ScrollView(.horizontal, showsIndicators: false) {
                filterPicker
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.bottom, 12)
    }

    private var filterPicker: some SwiftUI.View {
        SwiftUI.Picker("Search category", selection: Binding(
            get: { model.searchFilter },
            set: { model.selectSearchFilter($0); fieldFocused = false }
        )) {
            SwiftUI.ForEach(CatalogSearchFilter.allCases, id: \.self) { filter in
                SwiftUI.Text(filter.rawValue).tag(filter)
            }
        }
        .pickerStyle(.segmented)
        .controlSize(.large)
        .labelsHidden()
        .tint(Optional<SwiftUI.Color>.none)
        .fixedSize(horizontal: true, vertical: false)
        .transaction { if reduceMotion { $0.animation = nil } }
    }

    private var browseContent: some SwiftUI.View {
        SwiftUI.ScrollView {
            SwiftUI.VStack(alignment: .leading, spacing: 30) {
                if !model.recentSearches.isEmpty {
                    recentSearches(fieldFocused ? suggestions : model.recentSearches, title: "Recently searched")
                }
                SwiftUI.VStack(alignment: .leading, spacing: 16) {
                    SwiftUI.Text("Browse categories").font(.title3.weight(.semibold))
                    SwiftUI.LazyVGrid(columns: [GridItem(.adaptive(minimum: 200), spacing: 20)], spacing: 20) {
                        browseCard(.explore, symbol: "globe", color: .blue)
                        browseCard(.charts, symbol: "chart.line.uptrend.xyaxis", color: .orange)
                        browseCard(.moodsAndGenres, symbol: "theatermasks", color: .purple)
                        browseCard(.newReleases, symbol: "sparkles", color: .pink)
                    }
                }
            }
            .padding(24)
            .padding(.top, headerHeight)
            .padding(.leading, leadingInset)
            .padding(.bottom, 120)
        }
    }

    private func recentSearches(_ queries: [String], title: String) -> some SwiftUI.View {
        SwiftUI.VStack(alignment: .leading, spacing: 14) {
            SwiftUI.HStack {
                SwiftUI.Text(title).font(.title3.weight(.semibold))
                SwiftUI.Spacer()
                SwiftUI.Button("Clear", action: model.clearRecentSearches)
                    .buttonStyle(.plain)
                    .foregroundStyle(SwiftUI.Color.goosicPink)
            }
            SwiftUI.ScrollView(.horizontal, showsIndicators: false) {
                SwiftUI.HStack(spacing: 16) {
                    SwiftUI.ForEach(queries, id: \.self) { query in
                        SwiftUI.Button {
                            model.searchAgain(query)
                            fieldFocused = false
                        } label: {
                            SwiftUI.HStack(spacing: 12) {
                                SwiftUI.Image(systemName: "clock.arrow.circlepath")
                                    .font(.title2).foregroundStyle(.secondary)
                                    .frame(width: 36, height: 36)
                                SwiftUI.Text(query).lineLimit(2)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                            }
                            .padding(16)
                            .frame(width: 240, height: 80)
                            .background(.primary.opacity(0.06), in: RoundedRectangle(cornerRadius: 12))
                            .contentShape(RoundedRectangle(cornerRadius: 12))
                        }
                        .buttonStyle(NativeMacPlayerButtonStyle())
                        .accessibilityLabel("Search for \(query)")
                    }
                }
            }
        }
    }

    private func browseCard(_ route: GoosicRoute, symbol: String, color: SwiftUI.Color) -> some SwiftUI.View {
        SwiftUI.Button { fieldFocused = false; model.navigate(to: route) } label: {
            SwiftUI.ZStack(alignment: .bottomLeading) {
                RoundedRectangle(cornerRadius: 12)
                    .fill(LinearGradient(colors: [color.opacity(0.7), color], startPoint: .topLeading, endPoint: .bottomTrailing))
                SwiftUI.Image(systemName: symbol)
                    .font(.system(size: 64, weight: .light))
                    .foregroundStyle(.white.opacity(0.3))
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing)
                    .padding(18)
                SwiftUI.Text(route.title).font(.title3.weight(.bold))
                    .foregroundStyle(.white).padding(18)
            }
            .frame(height: 150)
            .contentShape(RoundedRectangle(cornerRadius: 12))
        }
        .buttonStyle(NativeMacPlayerButtonStyle())
        .accessibilityLabel(route.title)
    }

    private func submit() { model.search(); fieldFocused = false }
}

private struct NativeMacEntityView: SwiftUI.View {
    let entity: GoosicEntityReference
    @ObservedObject var store: NativeMacModelStore

    private var model: GoosicAppModel { store.model }

    // Back, Share and More live in the window toolbar, as in Music; the page is only content.
    var body: some SwiftUI.View {
        NativeMacCatalogPage(
            key: .entity(entity),
            title: entity.kindLabel,
            subtitle: "YouTube Music",
            state: model.state(for: .entity(entity)),
            model: model
        )
        .padding(.top, 20)
        .onAppear { model.loadUserPlaylists() }
    }
}

/// The toolbar's More menu for an open page: how its list is sorted, and what can be done to
/// it. Editing is offered only on a playlist this account owns — following someone else's puts
/// it in the library while leaving every edit refused.
private struct NativeMacPageMenu: SwiftUI.View {
    let entity: GoosicEntityReference
    let model: GoosicAppModel

    // Menus are built from their own content, so the icon style is set here rather than
    // inherited from the window; without it macOS drops every menu icon.
    var body: some SwiftUI.View {
        SwiftUI.Group { items }.labelStyle(.titleAndIcon)
    }

    @SwiftUI.ViewBuilder
    private var items: some SwiftUI.View {
        SwiftUI.Menu {
            if case .playlist = entity {
                SwiftUI.Picker("Sort By", selection: SwiftUI.Binding(
                    get: { model.trackSortOrder },
                    set: { model.trackSortOrder = $0 }
                )) {
                    SwiftUI.ForEach(TrackSortOrder.allCases) { order in
                        SwiftUI.Text(order.label).tag(order)
                    }
                }
                .pickerStyle(.inline)
            }
            if let page = model.state(for: .entity(entity)).page, let first = page.tracks.first {
                SwiftUI.Divider()
                SwiftUI.Button("Play Next", systemImage: "text.line.first.and.arrowtriangle.forward") {
                    for track in page.tracks.reversed() { model.playNext(track) }
                }
                .disabled(model.currentTrack == nil)
                SwiftUI.Button("Start Radio", systemImage: "dot.radiowaves.left.and.right") {
                    model.startRadio(from: first)
                }
            }
            if let playlist = model.ownedPlaylist(for: entity),
               !model.isSaved(playlist: playlist.id) {
                SwiftUI.Divider()
                SwiftUI.Button("Rename…", systemImage: "pencil") { model.beginRenaming(playlist) }
                SwiftUI.Button("Edit Description…", systemImage: "text.alignleft") {
                    // The page carries no description to start from, so the field starts
                    // empty rather than prefilled with the header line, which is not one.
                    model.beginEditingDescription(playlist, current: "")
                }
                SwiftUI.Menu("Who Can See This", systemImage: "eye") {
                    SwiftUI.ForEach(PlaylistPrivacy.allCases) { privacy in
                        SwiftUI.Button(privacy.label, systemImage: privacy == .private ? "lock" : (privacy == .public ? "globe" : "link")) {
                            model.setPlaylistPrivacy(playlist, to: privacy)
                        }
                    }
                }
                SwiftUI.Divider()
                SwiftUI.Button("Delete Playlist…", systemImage: "trash", role: .destructive) {
                    model.playlistPendingDeletion = playlist
                }
            }
            if case .playlist(let id) = entity, model.activeAccount != nil,
               model.isSaved(playlist: id) {
                SwiftUI.Divider()
                SwiftUI.Button("Remove from Library", systemImage: "minus.circle") {
                    let title = model.state(for: .entity(entity)).page?.title ?? "playlist"
                    model.setSaved(false, playlist: id, title: title)
                }
                .disabled(model.libraryOperationInProgress)
            }
            if let url = NativeMacLinks.url(for: entity) {
                SwiftUI.Divider()
                SwiftUI.Button("Copy Link", systemImage: "link") { NativeMacLinks.copy(url) }
            }
        } label: {
            SwiftUI.Image(systemName: "ellipsis")
        }
        .menuIndicator(.hidden)
        .help("More")
        .disabled(model.libraryOperationInProgress)
    }
}

struct NativeMacQueuePanel: SwiftUI.View {
    @ObservedObject var store: NativeMacModelStore
    @SwiftUI.Environment(\.goosicReduceMotion) private var reduceMotion

    private var model: GoosicAppModel { store.model }

    var body: some SwiftUI.View {
        SwiftUI.VStack(alignment: .leading, spacing: 0) {
            // "From Liked Music · 99 songs": where the queue came from, then what is left.
            // "From Liked Music · 99 songs": where the queue came from, then what is left.
            SwiftUI.Text(model.upNextText)
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .padding(.horizontal, 18)
                .padding(.bottom, 8)

            SwiftUI.HStack {
                SwiftUI.Button("Clear Up Next", action: model.clearUpcoming)
                    .disabled(!model.canClearUpcoming)
                if model.canUndoClear {
                    SwiftUI.Button("Undo", action: model.undoClearUpcoming)
                        .accessibilityLabel("Undo clearing Up Next")
                }
            }
            .padding(.horizontal, 18)
            .padding(.bottom, 8)

            if model.queue.tracks.isEmpty {
                SwiftUI.ContentUnavailableView(
                    "Queue is empty",
                    systemImage: "list.bullet",
                    description: SwiftUI.Text("Choose a song or video to start a queue.")
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                SwiftUI.List {
                    SwiftUI.ForEach(Array(model.queue.tracks.enumerated()), id: \.offset) { index, track in
                        queueRow(track, index: index)
                            .listRowInsets(SwiftUI.EdgeInsets())
                            .listRowSeparator(.hidden)
                            .listRowBackground(
                                index == model.queue.currentIndex
                                    ? SwiftUI.Color.primary.opacity(0.08) : SwiftUI.Color.clear
                            )
                    }
                    .onMove { source, destination in
                        model.moveQueueItems(from: source, to: destination)
                    }
                    SwiftUI.Color.clear.frame(height: 110).listRowSeparator(.hidden)
                }
                .listStyle(.plain)
                .scrollContentBackground(.hidden)
            }
        }
        .frame(maxHeight: .infinity, alignment: .top)
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.2), value: model.queue.currentIndex)
    }

    private func panelHeader(title: String, subtitle: String) -> some SwiftUI.View {
        SwiftUI.HStack(alignment: .firstTextBaseline, spacing: 10) {
            SwiftUI.VStack(alignment: .leading, spacing: 2) {
                SwiftUI.Text(title).font(.title3.weight(.bold))
                SwiftUI.Text(subtitle).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            SwiftUI.Spacer()
            SwiftUI.Button(action: model.toggleQueue) {
                SwiftUI.Image(systemName: "xmark")
                    .frame(width: 26, height: 26)
            }
            .buttonStyle(.plain)
            .help("Close queue")
            .accessibilityLabel("Close queue")
        }
        .padding(.horizontal, 18)
        .padding(.top, 58)
        .padding(.bottom, 12)
        .overlay(alignment: .bottom) { SwiftUI.Divider() }
    }

    private func queueRow(_ track: GoosicTrack, index: Int) -> some SwiftUI.View {
        let isCurrent = index == model.queue.currentIndex
        return SwiftUI.Button {
            model.play(track, queueIndex: index)
        } label: {
            SwiftUI.HStack(spacing: 10) {
                NativeMacPanelArtwork(url: track.thumbnail, size: 40)
                SwiftUI.VStack(alignment: .leading, spacing: 2) {
                    SwiftUI.Text(track.title)
                        .font(.subheadline.weight(isCurrent ? .semibold : .regular))
                        .lineLimit(1)
                    SwiftUI.Text(track.artist)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                SwiftUI.Spacer(minLength: 4)
                if isCurrent {
                    SwiftUI.Image(systemName: "speaker.wave.2.fill")
                        .font(.caption)
                        .foregroundStyle(SwiftUI.Color.goosicPink)
                } else {
                    SwiftUI.Text(track.duration)
                        .font(.caption2.monospacedDigit())
                        .foregroundStyle(.tertiary)
                }
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 8)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .contextMenu {
            NativeMacTrackMenuItems(
                track: track, model: model, includePlay: false, queueIndex: index
            )
            SwiftUI.Divider()
            SwiftUI.Button("Move Up", systemImage: "arrow.up") {
                model.moveQueueItems(from: IndexSet(integer: index), to: index - 1)
            }.disabled(index == 0)
            SwiftUI.Button("Move Down", systemImage: "arrow.down") {
                model.moveQueueItems(from: IndexSet(integer: index), to: index + 2)
            }.disabled(index == model.queue.tracks.count - 1)
        }
        .accessibilityValue(isCurrent ? (model.isPaused ? "Current track, paused" : "Playing") : "")
    }
}

/// A short message about something just done, shown above the player bar and then gone.
private struct NativeMacNoticeView: SwiftUI.View {
    let notice: GoosicNotice
    let dismiss: () -> Void

    var body: some SwiftUI.View {
        SwiftUI.HStack(spacing: 8) {
            SwiftUI.Image(systemName: notice.isError ? "exclamationmark.circle.fill" : "checkmark.circle.fill")
                .foregroundStyle(notice.isError ? SwiftUI.Color.orange : SwiftUI.Color.goosicPink)
            SwiftUI.Text(notice.text).font(.callout).lineLimit(2)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
        .modifier(NativeMacPlayerGlass())
        .shadow(color: .black.opacity(0.15), radius: 10, y: 4)
        .onTapGesture(perform: dismiss)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isButton)
        .accessibilityLabel("\(notice.text). Dismiss")
        .accessibilityAction { dismiss() }
        .onAppear {
            NSAccessibility.post(
                element: NSApplication.shared,
                notification: .announcementRequested,
                userInfo: [
                    .announcement: notice.text,
                    .priority: notice.isError
                        ? NSAccessibilityPriorityLevel.high.rawValue
                        : NSAccessibilityPriorityLevel.medium.rawValue,
                ]
            )
        }
    }
}

/// The inspector that holds Lyrics and Up Next, switched by a segmented control at its top as
/// Music's is. The player bar's two buttons open it on either side.
private struct NativeMacNowPlayingInspector: SwiftUI.View {
    @ObservedObject var store: NativeMacModelStore

    private var model: GoosicAppModel { store.model }

    var body: some SwiftUI.View {
        SwiftUI.VStack(spacing: 10) {
            SwiftUI.Picker("Show", selection: SwiftUI.Binding(
                get: { model.lyricsVisible ? 0 : 1 },
                set: { choice in
                    if choice == 0, !model.lyricsVisible { model.toggleLyrics() }
                    if choice == 1, !model.queueVisible { model.toggleQueue() }
                }
            )) {
                SwiftUI.Text("Lyrics").tag(0)
                SwiftUI.Text("Up Next").tag(1)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .padding(.horizontal, 16)
            .padding(.top, 8)

            if model.lyricsVisible {
                NativeMacLyricsPanel(store: store)
            } else {
                NativeMacQueuePanel(store: store)
            }
        }
    }
}

private struct NativeMacLyricsPanel: SwiftUI.View {
    @ObservedObject var store: NativeMacModelStore
    var body: some SwiftUI.View { NativeMacLyricsView(store: store, immersive: false) }
}

private struct NativeMacPanelArtwork: SwiftUI.View {
    let url: String?
    var size: CGFloat = 42

    var body: some SwiftUI.View {
        NativeMacCachedImage(url: url, maxPixels: Int(size * 2)) { image in
            if let image {
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
    @SwiftUI.Environment(\.goosicReduceMotion) private var reduceMotion
    @SwiftUI.State private var hovered = false

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

/// The queue and lyrics are a persistent edge column, not a floating popover. Keeping the
/// leading separator while letting the material run to the window edge matches Music's browsing
/// layout and leaves the player visually behind the panel instead of turning it into a card.
private struct NativeMacNowPlayingPanelSurface: SwiftUI.ViewModifier {
    func body(content: Content) -> some SwiftUI.View {
        // Liquid Glass works for compact controls, but over a tall scrolling rail it refracts
        // catalog artwork into bright blobs that compete with every label. A system material
        // plus an adaptive veil keeps the same depth while preserving legibility.
        content
            .background(SwiftUI.Color(nsColor: .windowBackgroundColor).opacity(0.76))
            .background(.regularMaterial)
            .overlay(alignment: .leading) {
                SwiftUI.Rectangle()
                    .fill(SwiftUI.Color.primary.opacity(0.12))
                    .frame(width: 1)
            }
            .shadow(color: .black.opacity(0.16), radius: 18, x: -8)
    }
}

private struct NativeMacDownloadsView: SwiftUI.View {
    @SwiftUI.Environment(\.nativeMacPlayerClearance) private var playerClearance
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
                .disabled(model.downloadsLoading || !model.serviceConnected)
                if let failure = model.downloadsFailure {
                    SwiftUI.Label(failure, systemImage: "exclamationmark.triangle")
                        .foregroundStyle(.secondary)
                }
                if model.downloadsLoading {
                    SwiftUI.ProgressView("Reading imported files…")
                } else if model.downloadedTracks.isEmpty && model.downloadsFailure == nil {
                    SwiftUI.ContentUnavailableView("No imported music", systemImage: "music.note",
                        description: Text("Import finalized files from your previous Goosic library to listen here."))
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
            .padding(.bottom, playerClearance)
        }
    }
}

private struct NativeMacSettingsView: SwiftUI.View {
    @SwiftUI.Environment(\.nativeMacPlayerClearance) private var playerClearance
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
                        SwiftUI.Button(model.serviceConnected ? "Connected" : model.isConnecting ? "Connecting…" : "Reconnect", action: model.connect)
                            .disabled(model.serviceConnected || model.isConnecting)
                            .modifier(NativeMacSettingsButtonStyle(prominent: !model.serviceConnected))
                    }
                }

                if NativeMacUpdater.shared.isAvailable {
                    settingsCard("Updates", systemImage: "arrow.triangle.2.circlepath") {
                        settingsRow(
                            title: "Goosic updates",
                            detail: "Check for a newer macOS alpha.",
                            systemImage: "arrow.down.circle.fill",
                            tint: .goosicPink
                        ) {
                            SwiftUI.Button("Check for Updates…") { NativeMacUpdater.shared.check() }
                                .modifier(NativeMacSettingsButtonStyle(prominent: false))
                        }
                    }
                }

                settingsCard("Playback", systemImage: "play.circle.fill") {
                    settingToggle(
                        "Autoplay",
                        detail: "Keep the music going with related recommendations.",
                        systemImage: "infinity",
                        isOn: SwiftUI.Binding(get: { model.autoplay }, set: { model.setAutoplay($0) })
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
                            set: { model.setArtworkBackground($0) }
                        )
                    )
                    SwiftUI.Divider().opacity(0.45)
                    settingToggle(
                        "Hide Explicit Songs",
                        detail: "Leave songs marked explicit out of lists and shelves.",
                        systemImage: "e.square",
                        isOn: SwiftUI.Binding(get: { model.hideExplicit }, set: { model.setHideExplicit($0) })
                    )
                    SwiftUI.Divider().opacity(0.45)
                    settingToggle(
                        "Reduce Motion",
                        detail: "Skip decorative animation, on top of the system setting.",
                        systemImage: "figure.walk.motion",
                        isOn: SwiftUI.Binding(get: { model.reduceMotion }, set: { model.setReduceMotion($0) })
                    )
                    SwiftUI.Divider().opacity(0.45)
                    settingsRow(
                        title: "Open Goosic To",
                        detail: "The page shown when Goosic starts.",
                        systemImage: "house",
                        tint: .goosicPink
                    ) {
                        SwiftUI.Picker("Open Goosic to", selection: SwiftUI.Binding(
                            get: { model.startPage },
                            set: { model.setStartPage($0) }
                        )) {
                            SwiftUI.Text("Where I left off").tag("last")
                            SwiftUI.Text("Home").tag("home")
                            SwiftUI.Text("Library").tag("library")
                            SwiftUI.Text("Liked Music").tag("liked")
                        }
                        .labelsHidden()
                        .fixedSize()
                    }
                }

                settingsCard("Appearance", systemImage: "circle.lefthalf.filled") {
                    settingsRow(title: "Theme", detail: "Choose an appearance or follow your Mac.",
                                systemImage: "paintpalette", tint: .goosicPink) {
                        SwiftUI.Picker("Theme", selection: Binding(
                            get: { model.theme }, set: { model.setTheme($0) }
                        )) {
                            ForEach(GoosicTheme.allCases, id: \.self) { theme in
                                Text(theme.label).tag(theme)
                            }
                        }
                        .labelsHidden()
                    }
                }

                settingsCard("Integrations", systemImage: "square.stack.3d.up.fill") {
                    settingToggle(
                        "Discord Status",
                        detail: "Show the song playing on your Discord profile while Discord is open.",
                        systemImage: "gamecontroller.fill",
                        isOn: SwiftUI.Binding(get: { model.discordStatus }, set: { model.setDiscordStatus($0) })
                    )
                }

                settingsCard("Advanced", systemImage: "wrench.and.screwdriver.fill") {
                    settingToggle(
                        "Debug Mode",
                        detail: "Show technical details, status messages, and full error text.",
                        systemImage: "ladybug.fill",
                        isOn: SwiftUI.Binding(get: { model.debugMode }, set: { model.setDebugMode($0) })
                    )
                    SwiftUI.Divider().opacity(0.45)
                    settingsRow(
                        title: "Log",
                        detail: "Everything Goosic shows is logged, with nothing private in it.",
                        systemImage: "doc.text.magnifyingglass",
                        tint: .goosicPink
                    ) {
                        SwiftUI.Button("Open Log Folder") {
                            guard let directory = Diagnostics.logDirectory else { return }
                            try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
                            NSWorkspace.shared.open(directory)
                        }
                        .modifier(NativeMacSettingsButtonStyle(prominent: false))
                    }
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
            .padding(.bottom, playerClearance)
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
                if active && model.sessionExpired {
                    SwiftUI.Button("Sign In Again…", systemImage: "arrow.clockwise.circle") { model.signInAgain() }
                        .disabled(accountChangesDisabled)
                }
                if active {
                    SwiftUI.Button("Browse as a Guest", systemImage: "person.crop.circle.badge.questionmark") {
                        model.browseAsGuest()
                    }
                    .disabled(accountChangesDisabled)
                }
                // Signing out and removing are the same thing, as on Windows: Goosic forgets the
                // account and clears its sign-in from this Mac.
                SwiftUI.Button(active ? "Sign Out" : "Remove Account", systemImage: "person.crop.circle.badge.minus", role: .destructive) {
                    model.removeAccount(account.id)
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
    func body(content: Content) -> some SwiftUI.View {
        content
            .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 20))
            .overlay(RoundedRectangle(cornerRadius: 20).stroke(.primary.opacity(0.08), lineWidth: 1))
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

extension SwiftUI.Color {
    /// Goosic's accent, used where Music uses its red. `Color.accentColor` is not it: that reads
    /// the system accent and ignores the window's tint, which is how hearts came out blue.
    static let goosicPink = SwiftUI.Color(red: 1.0, green: 0.02, blue: 0.32)
}

#if DEBUG
/// Canvas host for the complete native shell. It intentionally renders the model without
/// calling `connect()`, so previews never start the Rust child or touch account WebKit state.
@MainActor
private struct NativeMacAppPreview: SwiftUI.View {
    @SwiftUI.StateObject private var store = NativeMacModelStore()
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
/// Lets the artwork backdrop continue under the floating sidebar on macOS 26.
private struct NativeMacBackgroundExtension: SwiftUI.ViewModifier {
    @SwiftUI.ViewBuilder
    func body(content: Content) -> some SwiftUI.View {
        if #available(macOS 26.0, *) {
            content.backgroundExtensionEffect()
        } else {
            content
        }
    }
}

/// A flexible toolbar space, so the items after it sit at the trailing edge. Only macOS 26 has
/// one; before it the items stay where `.automatic` puts them.
struct NativeMacTrailingSpacer: SwiftUI.ToolbarContent {
    var body: some SwiftUI.ToolbarContent {
        if #available(macOS 26.0, *) {
            SwiftUI.ToolbarSpacer(.flexible)
        }
    }
}
#endif
