# GoosicReborn

GoosicReborn is a native rewrite of Goosic on a Rust authority plus a SwiftCrossUI shell. Rust owns the versioned playback authority and the read-only catalog; the shell talks to it over newline-delimited JSON on a private stdio channel. No legacy GPL source is copied here.

## What works today

- **Live catalog.** Home, Explore, Charts, Moods & genres, New releases, and Search read the real YouTube Music catalog through Rust, as an anonymous guest. Albums, playlists, and artists open to their real track lists.
- **Real playback.** Playing any song row claims the `officialWebView` lease from Rust and loads that video in the single web host — WKWebView on macOS, WebKitGTK on Linux. Advertisements are reported as informational markers and are never bypassed. Only macOS has been heard to play; see the limitations below for what that means on Linux.
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

- `goosic-protocol` — the Codable/serde-compatible 0.3.0 request, response, catalog, settings, downloads, accounts, and event envelopes.
- `goosic-core` — the playback authority: one owner, lease generations, increasing sample sequences, account-change resets, harmless advertisement markers.
- `goosic-catalog` — read-only YouTube Music access, split into a pure parser and a guest-only HTTP client. It answers what exists, never who may play.
- `goosic-settings` — durable preferences, and the reversible, credential-free import from a previous Goosic install.
- `goosic-accounts` — durable account profiles, metadata only: no WebKit, cookie, or credential integration.
- `goosic-lyrics` — LRCLIB lookups and LRC parsing; no account, no key, no credentials.
- `goosic-downloads` — read-only legacy media indexing plus WebM/Opus-to-WAV decode caching; it contains no downloader or account-cookie path.
- `goosic-service` — one request per stdin line, one response per stdout line, with no diagnostics on stdout.
- `apps/goosic-swift` — the shell: routed navigation, live catalog screens, search with filter tabs, entity detail pages, a queue and now-playing bar, and the official playback host. It builds on macOS against AppKit and on Linux against GTK 4. Every platform seam now has a real Linux implementation behind it: WebKitGTK for official playback, GStreamer for decoded local files, MPRIS for the system media controls, and per-account network sessions for sign-in. Windows keeps the stubs.

## Architecture

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the ownership, catalog, and wire contracts, and [docs/LEGACY_COMPATIBILITY.md](docs/LEGACY_COMPATIBILITY.md) for the migration, storage, and licensing boundaries.

## Working in this repository

Branches follow a five-branch model — `main` for deployments, `development` as the trunk, and one long-lived branch per platform. Which one a change is cut from depends on whether it would be wrong to leave out on another platform; [docs/BRANCHING.md](docs/BRANCHING.md) has the rule and the reasoning. [AGENTS.md](AGENTS.md) is the short version for AI coding agents, along with the invariants that are not open to change; [CLAUDE.md](CLAUDE.md) exists only to point Claude Code at it, so there is one file to keep current instead of two. Merges down the branch tree are automatic, and CI builds Rust on all three platforms plus the shell on Linux and macOS for every push.

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

Swift package resolution needs network access the first time because SwiftCrossUI is pinned to the official `0.9.0` tag.

The Make targets always set `SCUI_DEFAULT_BACKEND` explicitly — `AppKitBackend` on macOS, `GtkBackend` on Linux. Leaving it unset is not equivalent: SwiftCrossUI's `DefaultBackend` then names every platform's backend target, and SwiftPM pulls `swift-winui`'s C targets into the build graph, which fail on a non-Windows host looking for `wtypesbase.h`.

A distribution-packaged Swift has an editor-only failure of its own, and like the `rust-src` one above it accuses the code rather than the packaging. Fedora's `swift-lang` ships a `sourcekit-lsp` that resolves no C module declared by a SwiftPM target: `import CGtk` reports `No such module`, and so does a six-line package whose only dependency is a zlib shim. `swift build` and `make test` succeed throughout, because the compiler is not the component at fault. If the editor underlines an import that plainly compiles, the fix is a toolchain, not a setting: install an official one from [swift.org](https://www.swift.org/install/linux/) — `swiftly` places it under `~/.local/share/swiftly` without disturbing the packaged one — and point the Swift extension at its `bin` directory with `swift.path`. Put that same toolchain on `PATH` for `make` as well. The editor and the Makefile share one build directory, and two different toolchains writing to it invalidate each other's artifacts on every switch, which reads as an editor that recompiles the world each time it is opened.

## Build, run, and test

```sh
make test           # Rust workspace tests plus the Swift test target, all offline
make test-rust-live # opt-in: hits music.youtube.com to check the catalog parser against reality
make build-swift    # builds the shell for the host platform
make run-swift      # builds the service and launches the shell against it
```

`make run-swift` is the whole story on both platforms: it builds `goosic-service`, points `GOOSIC_SERVICE_PATH` at it, and launches the shell, which spawns the service itself. On Linux the result is a GTK 4 window, on Wayland or X11 alike.

The shell connects to the service on launch, so Home loads without any manual step. The sidebar button remains the way back if a transport failure drops the child process.

To drive the authority without a shell at all, feed it compact JSON lines. Its stdout is protocol-only; diagnostics, if any, go to stderr.

```sh
cargo build -p goosic-service
echo '{"protocolVersion":"0.3.0","requestId":"1","command":"catalog.search","payload":{"query":"daft punk","filter":"songs"}}' \
  | ./target/debug/goosic-service
```

## Current limitations

- **No signed-in library.** Sign-in and per-account WebKit profiles work, but catalog reads are still anonymous, so Library has nothing personal to show. Authenticated catalog reads are the next slice.
- **No new downloads.** This migration deliberately imports and plays only finalized legacy files. Explicit Premium-only downloading is not implemented, so the app never claims to create a new offline file.
- **Linux audio is written but unheard.** Both playback hosts now exist there — WebKitGTK for the official player, GStreamer for decoded files — and both claim the same Rust leases as their macOS counterparts. What is missing is a person confirming that sound comes out. The local host is the only one with runtime evidence: its tests open a real WAV and read the duration back, which they can do silently because a paused pipeline decodes without touching the audio device. The official host has never been past compiling. Treat a report that Linux does not play as a bug to investigate, not as an expected limitation.
- **Windows has no audio at all.** `OfficialPlaybackHost` and `LocalPlaybackHost` are explicit stubs there. A stub reports the limitation rather than producing sound, so no renderer can bypass Rust's authority.
- **Windows preferences cannot be imported.** WebView2 keeps local storage in LevelDB rather than SQLite, and no reader for it exists here.
- **Two download tests fail on Windows.** `goosic-downloads` builds and 13 of its 15 tests pass there, but path handling assumes Unix syntax and the legacy import returns `InvalidFilename`. CI reports it without blocking, because it is a real bug in shared code rather than an accepted platform limit.
- Catalog pages are clamped to one protocol frame; a clamped page says so rather than presenting a partial list as complete.

`goosic-service` is a private, one-process-per-app, single-client child reached through inherited stdin/stdout. It is not a daemon or socket service; stdio must never be shared or multiplexed. Generation is freshness authorization within that boundary. Future multiplexing requires an unforgeable per-client capability and active-owner authorization before account resets.

## Migration phases

1. **Done** — protocol/core/service authority and the native shell.
2. **Done (macOS)** — the official WebView host and its origin-checked bridge; Windows and Linux remain stubs.
3. **Done** — the live catalog, search, and real playback from the catalog.
4. **Done** — durable preferences and the legacy preference import.
5. **Done (macOS)** — read-only legacy downloaded-media import, Rust decode cache, and AVFoundation local-file playback.
6. **Done (macOS)** — account profiles with isolated WebKit stores, system media controls, and native platform material.
7. **Done** — catalog artwork.
8. **Next** — authenticated catalog reads for a signed-in library, then theming and production packaging.
