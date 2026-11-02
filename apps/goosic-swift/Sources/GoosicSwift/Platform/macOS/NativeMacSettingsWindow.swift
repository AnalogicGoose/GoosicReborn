#if os(macOS)
import AppKit
import SwiftUI

/// Goosic's Settings window, which answers ⌘, and the account menu, as Music's does: a toolbar of
/// tabs over labelled groups of checkboxes and pop-up menus. It is a window of its own rather than
/// a page, so changing a setting never takes the main window away from what is playing.
///
/// Every change applies at once and the shared model saves it, as macOS settings windows do, so
/// there is no Cancel or OK behind which an edit could be lost.
struct NativeMacSettingsWindow: SwiftUI.View {
    enum Tab: String {
        case general, playback, accounts, advanced
    }

    /// The selected tab, kept in defaults so the account menu can open the window on Accounts.
    static let tabKey = "goosic.settings.tab"

    @ObservedObject var store: NativeMacModelStore
    @SwiftUI.AppStorage(Self.tabKey) private var tab: Tab = .general

    private var model: GoosicAppModel { store.model }

    var body: some SwiftUI.View {
        SwiftUI.TabView(selection: $tab) {
            NativeMacGeneralSettings(store: store)
                .tabItem { SwiftUI.Label("General", systemImage: "gearshape") }
                .tag(Tab.general)
            NativeMacPlaybackSettings(store: store)
                .tabItem { SwiftUI.Label("Playback", systemImage: "play.circle") }
                .tag(Tab.playback)
            NativeMacAccountSettings(store: store)
                .tabItem { SwiftUI.Label("Accounts", systemImage: "person.crop.circle") }
                .tag(Tab.accounts)
            NativeMacAdvancedSettings(store: store)
                .tabItem { SwiftUI.Label("Advanced", systemImage: "gearshape.2") }
                .tag(Tab.advanced)
        }
        .preferredColorScheme(model.theme.colorScheme)
        .onAppear { model.loadAccounts() }
    }
}

/// The shape every tab shares: right-aligned group labels beside their controls.
private struct NativeMacSettingsPane<Content: SwiftUI.View>: SwiftUI.View {
    @SwiftUI.ViewBuilder let content: Content

    var body: some SwiftUI.View {
        SwiftUI.Form { content }
            .formStyle(.columns)
            .padding(.horizontal, 40)
            .padding(.vertical, 24)
            .frame(width: 560)
            .fixedSize(horizontal: false, vertical: true)
    }
}

/// A checkbox with the sentence that explains it underneath, aligned with its title.
private struct NativeMacSettingsOption: SwiftUI.View {
    let title: String
    let detail: String
    @SwiftUI.Binding var isOn: Bool

    var body: some SwiftUI.View {
        SwiftUI.VStack(alignment: .leading, spacing: 3) {
            // Only the checkmark takes Goosic's pink; pop-ups and buttons stay neutral, as Music's.
            SwiftUI.Toggle(title, isOn: $isOn)
                .toggleStyle(.checkbox)
                .tint(.goosicPink)
            SwiftUI.Text(detail)
                .font(.callout)
                .foregroundStyle(.secondary)
                .padding(.leading, 20)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

private struct NativeMacGeneralSettings: SwiftUI.View {
    @ObservedObject var store: NativeMacModelStore
    private var model: GoosicAppModel { store.model }

    var body: some SwiftUI.View {
        NativeMacSettingsPane {
            SwiftUI.Picker("Open Goosic to:", selection: SwiftUI.Binding(
                get: { model.startPage }, set: { model.setStartPage($0) }
            )) {
                SwiftUI.Text("Where I left off").tag("last")
                SwiftUI.Text("Home").tag("home")
                SwiftUI.Text("Library").tag("library")
                SwiftUI.Text("Liked Music").tag("liked")
            }
            .fixedSize()

            SwiftUI.Picker("Appearance:", selection: SwiftUI.Binding(
                get: { model.theme }, set: { model.setTheme($0) }
            )) {
                SwiftUI.ForEach(GoosicTheme.allCases, id: \.self) { theme in
                    SwiftUI.Text(theme.label).tag(theme)
                }
            }
            .fixedSize()
            .accessibilityIdentifier("settings.theme")

            SwiftUI.LabeledContent("Show:") {
                SwiftUI.VStack(alignment: .leading, spacing: 10) {
                    NativeMacSettingsOption(
                        title: "Album art background",
                        detail: "Blur the playing song's artwork behind the window.",
                        isOn: SwiftUI.Binding(
                            get: { model.artworkBackground }, set: { model.setArtworkBackground($0) }
                        )
                    )
                    NativeMacSettingsOption(
                        title: "Hide explicit songs",
                        detail: "Leave songs marked explicit out of lists and shelves.",
                        isOn: SwiftUI.Binding(get: { model.hideExplicit }, set: { model.setHideExplicit($0) })
                    )
                    NativeMacSettingsOption(
                        title: "Reduce motion",
                        detail: "Skip decorative animation, on top of the system setting.",
                        isOn: SwiftUI.Binding(get: { model.reduceMotion }, set: { model.setReduceMotion($0) })
                    )
                }
            }
            .padding(.top, 8)

            if NativeMacUpdater.shared.isAvailable {
                SwiftUI.LabeledContent("Updates:") {
                    SwiftUI.Button("Check for Updates…") { NativeMacUpdater.shared.check() }
                }
                .padding(.top, 8)
            }
        }
    }
}

private struct NativeMacPlaybackSettings: SwiftUI.View {
    @ObservedObject var store: NativeMacModelStore
    private var model: GoosicAppModel { store.model }

    var body: some SwiftUI.View {
        NativeMacSettingsPane {
            SwiftUI.LabeledContent("Playback:") {
                SwiftUI.VStack(alignment: .leading, spacing: 10) {
                    NativeMacSettingsOption(
                        title: "Autoplay",
                        detail: "Keep the music going with related recommendations.",
                        isOn: SwiftUI.Binding(get: { model.autoplay }, set: { model.setAutoplay($0) })
                    )
                    NativeMacSettingsOption(
                        title: "Shuffle",
                        detail: "Play the current queue in a randomized order.",
                        isOn: SwiftUI.Binding(
                            get: { model.shuffle },
                            set: { enabled in if enabled != model.shuffle { model.toggleShuffle() } }
                        )
                    )
                }
            }

            SwiftUI.LabeledContent("Integrations:") {
                NativeMacSettingsOption(
                    title: "Discord status",
                    detail: "Show the song playing on your Discord profile while Discord is open.",
                    isOn: SwiftUI.Binding(get: { model.discordStatus }, set: { model.setDiscordStatus($0) })
                )
            }
            .padding(.top, 8)
        }
    }
}

private struct NativeMacAccountSettings: SwiftUI.View {
    @ObservedObject var store: NativeMacModelStore
    private var model: GoosicAppModel { store.model }

    private var accountChangesDisabled: Bool {
        !model.serviceConnected
            || model.accountOperationInProgress
            || model.playbackTransition != .idle
            || model.isAdvertisement
    }

    var body: some SwiftUI.View {
        SwiftUI.VStack(alignment: .leading, spacing: 12) {
            SwiftUI.Group {
                if model.accounts.isEmpty {
                    SwiftUI.Text("No accounts. Goosic is browsing as a guest.")
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    SwiftUI.List {
                        SwiftUI.ForEach(model.accounts, id: \.id) { account in accountRow(account) }
                    }
                    .listStyle(.bordered(alternatesRowBackgrounds: false))
                }
            }
            .frame(height: 190)

            SwiftUI.HStack {
                SwiftUI.Button(
                    model.accounts.isEmpty ? "Sign In with Google…" : "Add Account…",
                    action: model.signIn
                )
                .disabled(accountChangesDisabled)
                SwiftUI.Spacer()
            }

            SwiftUI.Text("Sign-in data stays inside each account's isolated WebKit profile and never enters the playback service.")
                .font(.callout)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(24)
        .frame(width: 560)
    }

    private func accountRow(_ account: GoosicAccountSummary) -> some SwiftUI.View {
        let active = account.id == model.activeAccountId
        return SwiftUI.HStack(spacing: 10) {
            NativeMacAccountAvatar(
                url: account.avatarUrl,
                fallback: String(account.displayName.prefix(1)).uppercased()
            )
            SwiftUI.VStack(alignment: .leading, spacing: 1) {
                SwiftUI.HStack(spacing: 6) {
                    SwiftUI.Text(account.displayName).fontWeight(.medium)
                    if active && model.sessionExpired {
                        SwiftUI.Text("Session ended").font(.caption).foregroundStyle(.orange)
                    } else if active {
                        SwiftUI.Text("In use").font(.caption).foregroundStyle(.secondary)
                    }
                }
                SwiftUI.Text(account.email ?? account.channel ?? "YouTube Music account")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            SwiftUI.Spacer()
            if active && model.sessionExpired {
                SwiftUI.Button("Sign In Again…") { model.signInAgain() }
                    .disabled(accountChangesDisabled)
            } else if !active {
                SwiftUI.Button("Use") { model.switchAccount(to: account.id) }
                    .disabled(accountChangesDisabled)
            }
            SwiftUI.Menu {
                if active {
                    SwiftUI.Button("Browse as a Guest") { model.browseAsGuest() }
                        .disabled(accountChangesDisabled)
                }
                // Signing out and removing are the same thing, as on Windows: Goosic forgets the
                // account and clears its sign-in from this Mac.
                SwiftUI.Button(active ? "Sign Out" : "Remove Account", role: .destructive) {
                    model.removeAccount(account.id)
                }
                .disabled(accountChangesDisabled)
            } label: {
                SwiftUI.Image(systemName: "ellipsis.circle")
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            .help("Account options")
        }
        .padding(.vertical, 4)
    }
}

private struct NativeMacAdvancedSettings: SwiftUI.View {
    @ObservedObject var store: NativeMacModelStore
    private var model: GoosicAppModel { store.model }

    var body: some SwiftUI.View {
        NativeMacSettingsPane {
            SwiftUI.LabeledContent("Service:") {
                SwiftUI.HStack(spacing: 10) {
                    SwiftUI.Label(
                        model.serviceConnected ? "Connected" : "Offline",
                        systemImage: model.serviceConnected ? "checkmark.circle.fill" : "exclamationmark.triangle.fill"
                    )
                    .foregroundStyle(model.serviceConnected ? .green : .orange)
                    if !model.serviceConnected {
                        SwiftUI.Button(model.isConnecting ? "Connecting…" : "Reconnect", action: model.connect)
                            .disabled(model.isConnecting)
                    }
                }
            }

            SwiftUI.LabeledContent("Diagnostics:") {
                NativeMacSettingsOption(
                    title: "Debug mode",
                    detail: "Show technical details, status messages, and full error text.",
                    isOn: SwiftUI.Binding(get: { model.debugMode }, set: { model.setDebugMode($0) })
                )
            }
            .padding(.top, 8)

            SwiftUI.LabeledContent("Log:") {
                SwiftUI.VStack(alignment: .leading, spacing: 3) {
                    SwiftUI.Button("Open Log Folder") {
                        guard let directory = Diagnostics.logDirectory else { return }
                        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
                        NSWorkspace.shared.open(directory)
                    }
                    SwiftUI.Text("Everything Goosic shows is logged, with nothing private in it.")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .padding(.top, 8)
        }
    }
}

#if DEBUG
#Preview("Settings") {
    NativeMacSettingsWindow(store: NativeMacModelStore())
}
#endif
#endif
