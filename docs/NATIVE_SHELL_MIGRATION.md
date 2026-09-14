# Native-shell migration plan

## Decision

GoosicReborn will replace SwiftCrossUI with three native shells and retain one Rust backend. The macOS shell will use SwiftUI and AppKit, the Linux shell GTK 4 written in Rust, and the Windows shell WinUI 3 and WebView2. They are three renderers for one product, not three product forks. Every shell starts one private `goosic-service` child and speaks the existing versioned NDJSON protocol over inherited stdin and stdout.

This is not a rewrite of `goosic-core`, `goosic-service`, or the catalog, settings, downloads, lyrics, and accounts crates. Rust remains the playback authority: a shell may request a transition but Rust decides whether it is valid. The current lease, generation, monotonic sample, advertisement, anonymous-catalog, and credential-boundary invariants remain unchanged.

Removing SwiftCrossUI does not remove native platform work. Each shell owns its window, controls, accessibility, secure account profile, WebView, local audio, media keys, packaging, and signing. Cookies and raw account responses remain in platform profile storage and never traverse the service protocol.

## Target structure

```text
                         shared Rust backend
goosic-protocol ── goosic-service ── goosic-core
                         │
     ┌───────────────────┼────────────────────┐
     │                   │                    │
macOS application   Linux application    Windows application
SwiftUI + AppKit     GTK 4 (gtk4-rs)      WinUI 3
WebKit + AVFoundation WebKitGTK + GStreamer WebView2 + native
     └──── private, versioned NDJSON client ──┘
```

The shared frontend contract is the protocol and conformance fixtures, not a view toolkit. `goosic-protocol` remains the Rust source of truth. Request, response, malformed-frame, timeout, owner-conflict, stale-generation, and non-monotonic-sample fixtures make a replacement shell implementable without treating Swift source as the specification.

## Restructuring rule

The Swift `Core` directory contains both product rules and presentation. Genuinely shared rules move to the frontend-neutral Rust crate `goosic-shell-support`: NDJSON framing, request correlation and deadlines, cache-key rules, navigation decisions, artwork URL validation, and playback-event validation. It communicates with `goosic-service`; it does not link a shell directly to `goosic-core` and takes no UI, WebView, cookie, audio, or secure-storage dependency. The wire DTOs are not duplicated there: `goosic-protocol` already is them.

Do not force UI state into Rust merely to remove Swift. Screens, focus, layout, animation, accessibility, and window state are native-shell code. `WKWebView`, `WebKitWebView`, and WebView2 retain their engine-specific origin isolation and secure-profile responsibility. Native audio hosts must claim the existing Rust lease before producing sound. `PersonalCatalog.js` stays inside the authenticated platform WebView with its GPL notice intact, and legacy import stays read-only and credential-free.

## Migration sequence

1. Freeze the contract. Classify each Swift-core behavior as a pure rule, transport, presentation state, or platform integration. Add Rust tests and black-box protocol fixtures before moving a view. Add a compatibility manifest declaring each released shell's protocol range and bundled-service version.

   *Done.* The classification is [SHELL_CONTRACT.md](SHELL_CONTRACT.md). `goosic-protocol` carries encoding, rejection, tolerance and exchange fixtures — the exchanges are whole conversations with the service, because owner conflict, stale generation and non-monotonic samples only exist as the second half of one. The manifest is [COMPATIBILITY.md](COMPATIBILITY.md); it declares a single exact version rather than a range, because exact equality is what both sides implement. A timeout is not a fixture, since it is the absence of a response; it is a client rule in `goosic-shell-support`.

2. Extract portable support. Create `goosic-shell-support` for the pure shared helpers. Keep presentation state local. Expose a narrow FFI contract to the Swift shell only when a real use proves it valuable; do not create a new cross-language UI framework.

   *Done.* The crate holds the transport — requests routed by id, because the service answers out of order — and every platform-neutral rule from the Swift `Core` directory, each held by the cases its Swift original is held to. No FFI exists: the Swift shell keeps its own copies, and a change to one of those rules is made in both until a real use justifies the boundary. [SHELL_CONTRACT.md](SHELL_CONTRACT.md) lists what moved and the places the port deliberately differs.

3. Complete the macOS SwiftUI shell. Replace remaining SwiftCrossUI-backed components and model observation while retaining the WebKit, AVFoundation, account, and media-control guarantees. Bundle the matching service inside the macOS application.

   *In progress* on `feature/macos/native-swiftui-shell`, where the native macOS shell is being built inside `apps/goosic-swift`.

4. Build `apps/goosic-linux` beside the Swift shell. Start with catalog, search, queue, settings, and protocol transport. Then add WebKitGTK playback and account hosts, local audio, and media controls. The Swift Linux shell is only a temporary conformance reference and is deleted after GTK parity and packaging.

   *Designed, not started.* [LINUX_SHELL.md](LINUX_SHELL.md) records the decisions: GTK 4 through `gtk4-rs` without libadwaita, so the shell follows each desktop's theme; its own Cargo workspace linking `goosic-shell-support`; the application ID `io.github.analogicgoose.Goosic`; a Flatpak on the GNOME runtime; and a process that keeps playing in the background, reachable from every desktop, when its window is closed.

5. Build `apps/goosic-windows` around WinUI 3, WebView2, Windows media controls, and local audio. Do not claim Windows support until it passes the same fixtures and platform-host security checks.

   *Deferred* until the Linux shell lands.

6. Delete `apps/goosic-swift`, SwiftPM, `SCUI_DEFAULT_BACKEND`, and their CI caches only after all replacement shells pass conformance and package checks. Migrate every useful Swift test before deleting it.

   *Waiting.* Because the native macOS shell is being built inside `apps/goosic-swift`, this step becomes the removal of SwiftCrossUI and of the Linux and Windows platform code rather than of the whole package; how the macOS shell is laid out afterwards belongs to step three. The Linux half goes first, when the GTK shell reaches parity and its Flatpak ships.

## Delivery, deployment, and CI

Keep one product release line and protocol policy, but publish independent platform artifact revisions. For example, product `1.4.0` may contain macOS `1.4.0+3`, Linux `1.4.0+5`, and Windows `1.4.0+2`. A Linux-only shell fix publishes only a Linux package; the other platforms keep their current packages. Each package embeds its own compatible private service binary.

A service or protocol change is coordinated: CI tests every supported shell against it, the compatibility manifest changes, and affected packages release together unless the protocol remains backward compatible. A shell fails clearly on an unsupported service version rather than guessing.

The repository currently has CI but no packaging, signing, notarization, or publishing workflow. Add package jobs only after native packages exist: signed and notarized `.app`/DMG on macOS, a Flatpak on the GNOME runtime for Linux, and signed MSIX or installer on Windows. Release jobs consume one tagged revision and publish checksums, artifact revisions, bundled-service versions, and protocol ranges in a signed release manifest.

CI becomes path-aware. Rust, protocol, fixture, release-manifest, and shared packaging changes run all supported shell suites. A change inside one native shell runs its platform suite. The Linux shell's suite builds inside the Flatpak builder on the GNOME runtime, because the hosted runners ship a GTK older than the one it requires. Windows stops being `continue-on-error` when it has a released shell. The content-tree passport may remain, but its seal must cover the exact selected job set.

## Branches and document transition

The five-branch model remains valid. Rust, protocol, fixture, release-manifest, and plan work is cross-platform work from `development`; native shell work comes from `platform/<os>`. A platform branch never introduces a shared protocol change first.

Current documents remain the source of truth for what ships today. Each phase updates the relevant claim: architecture removes SwiftCrossUI constraints, content parity changes when a host genuinely lands, legacy compatibility preserves its GPL and credential guarantees, branching changes when CI and release workflows exist, and the README changes commands and supported-platform statements. This keeps the destination architecture from being represented as shipped work.

Completion requires all three native shells to pass identical protocol fixtures, preserve every security and playback invariant, package a private compatible service, fail safely on service loss or incompatibility, and prove that no platform host can sound without Rust's active lease.
