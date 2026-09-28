# GoosicReborn

GoosicReborn is a native rewrite of Goosic around a Rust playback authority. The Linux shell is a
GTK 4 application in Rust; the older SwiftCrossUI shell remains a reference during migration. Rust
owns the versioned playback authority and the anonymous read-only catalog; each shell talks to it
over newline-delimited JSON on a private stdio channel. Account-scoped content reads run inside
the active WebKit profile and the ported reader keeps its GPL-3.0 notice; see Licensing below.
The transition to one native shell per operating system is documented in
[the native-shell migration plan](docs/NATIVE_SHELL_MIGRATION.md) and [the Linux shell notes](docs/LINUX_SHELL.md).

## What works today

Version 0.2.3 distributes the native Windows x64 WinUI app as a setup executable and portable
ZIP. From 0.2.0 an installed copy updates itself: Settings checks GitHub Releases, verifies the
new Setup against the release's checksums, and installs it. Home is laid out as YouTube Music
lays it out, with Quick picks as a compact song list, and Settings adds a start page, hiding
explicit songs, reduced motion, close to tray, launch at startup, now-playing notifications,
remembered window placement and an efficiency mode. 0.2.1 adds editing your own playlists —
choosing songs, removing them, and moving them — and Save to playlist from the player bar.
0.2.2 brings the details of Apple Music, Spotify and YouTube Music: a like button, volume
slider, sleep timer and mini player; search led by the top result; Moods & genres; sort and
find in playlists; a library grid; pages that open instantly from the last copy; Discord
status; and a debug mode with a log. See [the release notes](docs/RELEASE_0.2.2.md). 0.2.3 is a
quick fix release: the keyboard's next and previous keys work, the volume slider follows an
audio taper, and Setup shows both licences ([notes](docs/RELEASE_0.2.3.md)).
Setup creates a Start menu shortcut and uninstaller, bundles .NET and the Windows App SDK, and
installs WebView2 if missing (internet required). It is unsigned, so Windows may display an
unknown-publisher warning. Download it from
[GitHub Releases](https://github.com/AnalogicGoose/GoosicReborn/releases).
The platform notes below otherwise describe the older Swift shell. The native Windows shell
supports official WebView2 playback, sign-in, personal catalog, queue, lyrics and media controls;
local downloaded-file playback and legacy preference import remain unavailable. Only Windows
x64 binaries are distributed in 0.2.3; macOS/Linux playback acceptance is not claimed.

- **Live catalog.** Home, Explore, Charts, Moods & genres, New releases, and Search read the real YouTube Music catalog through Rust. Home understands song shelves as well as artwork carousels and loads continuation pages instead of stopping after the first response. Albums, playlists, and artists open to their real track lists.
- **Personal content.** On macOS and in the GTK Linux shell, signed-in Home and Library read inside the active account's WebKit profile. Linux adds playlist creation and management, song likes, save-to-library and add-to-playlist. A native live read of Home, playlists, liked songs and the owned-playlist list passed. A user completed sign-in in the Flatpak after its account storage was made writable; restart, authenticated reads and mutations still need a packaged live check. Cookies never cross the service protocol. See [the content parity map](docs/CONTENT_PARITY.md) and [the feature parity audit](docs/FEATURE_PARITY.md).
- **Real playback.** Playing any song row claims the `officialWebView` lease from Rust and loads that video in the single web host — WKWebView on macOS, WebKitGTK on Linux. Advertisements are reported as informational markers and are never bypassed. The GTK Linux shell has been heard playing official and local audio; its packaged build still needs a full account and hidden-window session check.
- **A real transport.** Elapsed and total time, seeking, volume and mute, and autoplay to the next queued track — all reflecting what the player confirms, never what was requested. Goosic's queue overrides the official app's own "up next", so it never plays something you did not choose.
- **Preferences that persist.** Volume, mute, autoplay, shuffle, repeat, the queue panel, and the screen you were on are stored by Rust and restored on launch. Preferences from a previous Goosic install can be imported; the old data is read, never changed, and credentials are never carried over.
- **Enforced ownership.** `goosic-core` allows one playback owner at a time, scopes transitions to a generation, and requires strictly increasing sample sequences. Switching to a downloaded file quiesces the official host first; switching away stops the local renderer first.
- **Appearance.** System, light, or dark, applied through the toolkit so it works on every backend and survives a restart. Imported from a previous Goosic install along with the rest of the preferences.
- **Lyrics.** Synced lyrics from LRCLIB, with the current line highlighted as the track plays, falling back to plain text and saying plainly when a track has none.
- **Shuffle and repeat.** Off / all / one, plus shuffle that never picks the track it is already on. Both persist, and both are carried over from a previous Goosic install.
- **Radio.** When a queue runs out, playback continues with the radio that follows the last track, and any playing track can seed a new one. This is the previous Goosic's "auto radio", and the imported preference maps onto autoplay, so it stays off for anyone who had it off.
- **Artwork.** Albums, playlists, artists, and tracks show their real cover art, fetched and cached by the shell over an anonymous, host-restricted HTTPS session.
- **Native material.** The sidebar, queue, and now-playing surfaces sit on a real platform material: `NSGlassEffectView` on macOS 26+, `NSVisualEffectView` on macOS 14–25, and a plain background anywhere else. It is a background leaf that never wraps the controls, so buttons and their accessibility stay native.
- **Legacy downloads.** The Downloads screen can import finalized `.webm` files from the previous Goosic install without copying or deleting them. Rust decodes them into a private WAV cache and the local host plays only that decoded file — AVFoundation on macOS, GStreamer on Linux; no downloader, yt-dlp, or account cookie is involved.

## Crates and apps

- `goosic-protocol` — the Codable/serde-compatible 0.3.0 request, response, catalog, settings, downloads, accounts, and event envelopes, and the conformance fixtures every shell is held to: canonical lines, lines that must be refused, lines that must be tolerated, and whole conversations with the service.
- `goosic-core` — the playback authority: one owner, lease generations, increasing sample sequences, account-change resets, harmless advertisement markers.
- `goosic-catalog` — read-only YouTube Music access, split into a pure parser and a guest-only HTTP client. It answers what exists, never who may play.
- `goosic-settings` — durable preferences, and the reversible, credential-free import from a previous Goosic install.
- `goosic-accounts` — durable account profiles, metadata only: no WebKit, cookie, or credential integration.
- `goosic-lyrics` — LRCLIB lookups and LRC parsing; no account, no key, no credentials.
- `goosic-downloads` — read-only legacy media indexing plus WebM/Opus-to-WAV decode caching; it contains no downloader or account-cookie path.
- `goosic-service` — one request per stdin line, one response per stdout line, with no diagnostics on stdout.
- `goosic-shell-support` — the NDJSON client and platform-neutral rules for sign-in navigation, bridge validation, media projection, catalog conversion and queue selection. The GTK Linux shell consumes it; the Swift shell keeps copies held to the same tests. It links no UI, WebView, cookie, audio or secure-storage dependency.
- `apps/goosic-linux` — the native GTK shell on Linux, with official WebKitGTK playback, GStreamer local playback, account profiles, MPRIS, background mode and a Flatpak manifest.
- `apps/goosic-swift` — the shell: routed navigation, live catalog screens, search with filter tabs, entity detail pages, a queue and now-playing bar, and the official playback host. It builds on macOS against AppKit and on Linux against GTK 4. Every platform seam has a real Linux implementation behind it: WebKitGTK for official playback, GStreamer for decoded local files, MPRIS for the system media controls, and per-account network sessions for sign-in. Windows keeps the stubs.

## Architecture

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the ownership, catalog, and wire contracts, and [docs/LEGACY_COMPATIBILITY.md](docs/LEGACY_COMPATIBILITY.md) for the migration, storage, and licensing boundaries. [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md) declares which protocol version each shell speaks and which service it is paired with — today, exactly one version, demanded exactly by both sides. [docs/SHELL_CONTRACT.md](docs/SHELL_CONTRACT.md) records what the Swift shell's `Core` held and what of it moved to Rust, and [docs/LINUX_SHELL.md](docs/LINUX_SHELL.md) is the design of the GTK shell that replaces the Swift build on Linux.

## Working in this repository

Branches follow a five-branch model — `main` for deployments, `development` as the trunk, and one long-lived branch per platform. Which one a change is cut from depends on whether it would be wrong to leave out on another platform; [docs/BRANCHING.md](docs/BRANCHING.md) has the rule and the reasoning. [AGENTS.md](AGENTS.md) is the short version for AI coding agents, along with the invariants that are not open to change; [CLAUDE.md](CLAUDE.md) exists only to point Claude Code at it, so there is one file to keep current instead of two. Merges down the branch tree are automatic, and CI builds Rust on all three platforms, the Swift package on macOS and on Linux, and — wherever `apps/goosic-linux` exists — the Rust GTK shell, for every push.

## Prerequisites

Rust 1.88+ and Cargo everywhere. The Rust workspace is portable and needs nothing else: `rusqlite` is bundled and `ureq` uses rustls, so there is no system SQLite or OpenSSL to install.

A distribution-packaged Rust splits apart what `rustup` ships as one toolchain, and an editor is the first thing to notice. `rust-analyzer` resolves its sysroot from `rustc --print sysroot` — `/usr` on a packaged install — and reports ``can't load standard library, try installing `rust-src` sysroot_path=/usr`` when the standard-library sources are not there. Without them it cannot see `core`, so `Option` stops resolving and every `None` arm is read as a new binding (``Variable `None` should have snake_case name``) and macros like `matches!` fail to expand (`expected bool, found ()`). Those diagnostics are the missing sysroot, not the code: `cargo check` stays clean throughout. `cargo fmt` and `cargo clippy` are separate packages on the same installs.

| Distribution | Install |
| --- | --- |
| Fedora | `sudo dnf install rust-src rustfmt clippy` |
| Debian/Ubuntu | `sudo apt install rust-src rustfmt` (clippy ships in `rust-clippy`) |
| Arch | `sudo pacman -S rust-src` (`rust` already carries rustfmt and clippy) |

Keep them at the same version as `rustc`; a mismatched `rust-src` produces the same errors it was meant to fix. `rustup` users get all three with `rustup component add rust-src rustfmt clippy`.

The Swift shell is built in the Swift 6 language mode and needs Swift 6.0+ and, per platform:

| Platform | Also needs |
| --- | --- |
| macOS 14.0+ | Xcode's toolchain; nothing further |
| Linux | Development headers for GTK 4, WebKitGTK 6.0, GLib and GStreamer — `gtk4-devel webkitgtk6.0-devel glib2-devel gstreamer1-devel gstreamer1-plugins-base-devel` on Fedora, `libgtk-4-dev libwebkitgtk-6.0-dev libglib2.0-dev libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev` on Debian. `CGtk`, `CWebKitGTK`, `CGLib` and `CGStreamer` resolve them through `pkg-config`. GStreamer also needs its runtime plugins (`gstreamer1-plugins-good`, `gstreamer1-plugin-libav`) to decode anything, and WebKitGTK plays through the same ones |
| Windows 10/11 | Visual Studio Build Tools with the C++ workload, for `link.exe` and the Windows SDK; the Windows App Runtime **1.5-preview1**, which the WinUI bindings require by version and which a newer 1.5 does not satisfy; and Git for Windows, whose bash runs the symlink repair below |

Swift package resolution needs network access the first time because SwiftCrossUI is pinned to the official `0.9.0` tag.

The Make targets always set `SCUI_DEFAULT_BACKEND` explicitly — `AppKitBackend` on macOS, `GtkBackend` on Linux. Leaving it unset is not equivalent: SwiftCrossUI's `DefaultBackend` then names every platform's backend target, and SwiftPM pulls `swift-winui`'s C targets into the build graph, which fail on a non-Windows host looking for `wtypesbase.h`.

A distribution-packaged Swift has an editor-only failure of its own, and like the `rust-src` one above it accuses the code rather than the packaging. Fedora's `swift-lang` ships a `sourcekit-lsp` that resolves no C module declared by a SwiftPM target: `import CGtk` reports `No such module`, and so does a six-line package whose only dependency is a zlib shim. `swift build` and `make test` succeed throughout, because the compiler is not the component at fault. If the editor underlines an import that plainly compiles, the fix is a toolchain, not a setting: install an official one from [swift.org](https://www.swift.org/install/linux/) — `swiftly` places it under `~/.local/share/swiftly` without disturbing the packaged one — and point the Swift extension at its `bin` directory with `swift.path`. Put that same toolchain on `PATH` for `make` as well. The editor and the Makefile share one build directory, and two different toolchains writing to it invalidate each other's artifacts on every switch, which reads as an editor that recompiles the world each time it is opened.

## Build, run, and test

```sh
make test           # Rust workspace tests plus the Swift test target, all offline
make test-rust-live # opt-in: hits music.youtube.com to check the catalog parser against reality
make build-swift    # builds the shell for the host platform
make run-swift      # builds the service and launches the shell against it
make build-linux    # builds the service and native GTK shell on Linux
make test-linux     # builds the service and tests the native GTK shell
make run-linux      # launches the GTK shell with that service
make package-macos  # builds Goosic.app, the download a tester installs
```

`make run-linux` is the native Linux development path. It builds both Rust programs and points the
shell at that exact service executable. The installable build uses the GNOME 50 Flatpak manifest
under `apps/goosic-linux/packaging/flatpak/`; [the Linux notes](docs/LINUX_SHELL.md) show its build
steps and the live acceptance checks still required. `make run-swift` keeps the older shell available
as a comparison during migration.

A build for someone who is not working on Goosic is a *test build*, not a deployment:
[docs/RELEASING.md](docs/RELEASING.md) explains how one is cut, and what each operating system
asks a tester to click past when the download is not signed by a paying developer.

Windows uses the scripts under `scripts/` instead, because the Makefile's `uname` branch does not cover it and `make` is not part of the toolchain:

```bat
scripts\windows-build.bat   :: builds the shell
scripts\windows-test.bat    :: runs the Swift test target
scripts\windows-run.bat     :: builds the service and launches the shell against it
```

They exist to carry three pieces of setup that are not obvious from any failure they cause. `vcvars64.bat` is called for `link.exe` and the Windows SDK, with the Visual Studio Installer directory added to `PATH` first so it can find `vswhere`; without that it aborts early and `swiftc` reports a missing `link` tool and then an unloadable standard library, neither of which names the cause. `SDKROOT` is set explicitly, because the toolchain installer writes it machine-wide and any shell opened beforehand inherits a stale environment. And Windows refuses to create symlinks without Developer Mode, so dependencies are checked out with `core.symlinks=false` and repaired afterwards by `scripts/windows-fix-symlinks.sh`, which the scripts run for you between resolving and building.

The shell connects to the service on launch, so Home loads without any manual step. The sidebar button remains the way back if a transport failure drops the child process.

### The GTK shell on Linux

`apps/goosic-linux` replaces the Swift build on Linux and is its own Cargo workspace. It needs Rust 1.92+ and the development headers for GTK 4.20+, WebKitGTK 6.0, GStreamer and libsoup 3 — `gtk4-devel webkitgtk6.0-devel gstreamer1-devel gstreamer1-plugins-base-devel libsoup3-devel` on Fedora — plus the same GStreamer runtime plugins as above. Until the Make targets exist, it runs against a service built from the root workspace:

```sh
cargo build -p goosic-service
cd apps/goosic-linux
GOOSIC_SERVICE_PATH=../../target/debug/goosic-service cargo run
cargo test
```

Closing its window keeps it playing; `Ctrl+Q` quits. Packaging it as a Flatpak, and the tools that needs, are in [docs/LINUX_SHELL.md](docs/LINUX_SHELL.md), which also lists what the shell still lacks.

To drive the authority without a shell at all, feed it compact JSON lines. Its stdout is protocol-only; diagnostics, if any, go to stderr.

```sh
cargo build -p goosic-service
echo '{"protocolVersion":"0.4.0","requestId":"1","command":"catalog.search","payload":{"query":"daft punk","filter":"songs"}}' \
  | ./target/debug/goosic-service
```

## Current limitations

- **Linux account writes need live validation.** The GTK shell's read-only Home, playlist, liked-song and owned-playlist calls passed against an existing signed-in profile. Playlist creation, saving, renaming, privacy, deletion, likes and add-to-playlist are wired to the same reader but have not been tried against a real account. Exact playlist-entry removal and reordering still need the upstream per-entry identifier in the catalog projection.
- **No new downloads.** This migration deliberately imports and plays only finalized legacy files. Explicit Premium-only downloading is not implemented, so the app never claims to create a new offline file.
- **The packaged Linux player needs full-session checks.** The GTK shell has played official and local audio on a live desktop, and the Flatpak builds offline, runs its tests, and connects to its bundled service. A complete sign-in, a track ending while the window is hidden, and playback on other desktop and graphics setups remain unverified. The older Swift Linux host remains a conformance reference and has not been heard playing officially.
- **Decoded audio is stored as data, not cache.** Rust's WAV cache sits in the per-user data directory rather than the cache directory, so it is swept into backups and ignored by tools that free cache space. The move is planned for every platform in one change; see [docs/LINUX_SHELL.md](docs/LINUX_SHELL.md).
- **The Swift Windows shell has playback stubs.** The native WinUI shell uses WebView2 for official playback under Rust authority; it has no local-file playback host yet.
- **Windows preferences cannot be imported.** WebView2 keeps local storage in LevelDB rather than SQLite, and no reader for it exists here.
- **Windows distribution is x64.** Native ARM64 packaging has not been validated for this release.
- Catalog pages are clamped to one protocol frame; a clamped page says so rather than presenting a partial list as complete.

`goosic-service` is a private, one-process-per-app, single-client child reached through inherited stdin/stdout. It is not a daemon or socket service; stdio must never be shared or multiplexed. Generation is freshness authorization within that boundary. Future multiplexing requires an unforgeable per-client capability and active-owner authorization before account resets.

## Migration phases

The rewrite from the previous Goosic:

1. **Done** — protocol/core/service authority and the native shell.
2. **Done** — the official WebView host and its origin-checked bridge: WKWebView on macOS and WebKitGTK in the native Linux shell. The Swift Linux reference has not been heard playing.
3. **Done** — the live catalog, search, and real playback from the catalog.
4. **Done** — durable preferences and the legacy preference import.
5. **Done** — read-only legacy downloaded-media import, Rust decode cache, and local-file playback: AVFoundation on macOS, GStreamer on Linux.
6. **Done (macOS and Linux)** — account profiles with isolated WebKit stores and system media controls; native platform material on macOS.
7. **Done** — catalog artwork.
8. **Built (macOS and native Linux)** — authenticated Home and Library reads through the active WebKit profile; native Linux reads passed a credentialed live check.
9. **Built (macOS and native Linux)** — private playlist and album pages through that profile.
10. **In progress** — Linux playlist creation, likes, saves and owned-playlist management are built; exact item edits and live mutation validation remain.
11. **Next** — the remaining mutation surface and full packaged Linux acceptance.

The move to native shells, from [the migration plan](docs/NATIVE_SHELL_MIGRATION.md):

1. **Done** — the contract frozen: classification, conformance fixtures, and the compatibility manifest.
2. **Done** — `goosic-shell-support`: the transport and the platform-neutral rules in Rust.
3. **In progress** — the native macOS shell.
4. **Built, acceptance pending** — the GTK 4 shell and its offline Flatpak on Linux.
5. **Implemented** — the WinUI 3 shell for Windows with an x64 installer and portable ZIP.
6. **Waiting** — removing SwiftCrossUI, once every replacement shell passes conformance and packaging.

## Licensing

The original GoosicReborn code is MIT (`LICENSE`). Files that carry a GNU General Public
License header — currently `apps/goosic-swift/Sources/GoosicSwift/Resources/PersonalCatalog.js`,
ported from the previous Goosic — are GPL-3.0-or-later, with their original authors' copyright
preserved. Because the shell links that program in, the application as distributed is under
GPL-3.0 (`LICENSE-GPL-3.0`); the MIT grant continues to apply to the files that do not carry
the GPL header.
