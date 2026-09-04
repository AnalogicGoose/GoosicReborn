# GoosicReborn

GoosicReborn is a native rewrite of Goosic on a Rust authority plus a SwiftCrossUI shell. Rust owns the versioned playback authority and the read-only catalog; the shell talks to it over newline-delimited JSON on a private stdio channel. No legacy GPL source is copied here.

## What works today

- **Live catalog.** Home, Explore, Charts, Moods & genres, New releases, and Search read the real YouTube Music catalog through Rust, as an anonymous guest. Albums, playlists, and artists open to their real track lists.
- **Real playback.** Playing any song row claims the `officialWebView` lease from Rust and loads that video in the single WKWebView host. Advertisements are reported as informational markers and are never bypassed.
- **A real transport.** Elapsed and total time, seeking, volume and mute, and autoplay to the next queued track — all reflecting what the player confirms, never what was requested. Goosic's queue overrides the official app's own "up next", so it never plays something you did not choose.
- **Preferences that persist.** Volume, mute, autoplay, the queue panel, and the screen you were on are stored by Rust and restored on launch. Preferences from a previous Goosic install can be imported; the old data is read, never changed, and credentials are never carried over.
- **Enforced ownership.** `goosic-core` allows one playback owner at a time, scopes transitions to a generation, and requires strictly increasing sample sequences. Switching to a downloaded file quiesces the official host first; switching away stops the local renderer first.
- **Native material.** The sidebar, queue, and now-playing surfaces sit on a real platform material: `NSGlassEffectView` on macOS 26+, `NSVisualEffectView` on macOS 14–25, and a plain background anywhere else. It is a background leaf that never wraps the controls, so buttons and their accessibility stay native.
- **Legacy downloads.** The Downloads screen can import finalized `.webm` files from the previous Goosic install without copying or deleting them. Rust decodes them into a private WAV cache and macOS AVFoundation plays only that decoded file; no downloader, yt-dlp, or account cookie is involved.

## Crates and apps

- `goosic-protocol` — the Codable/serde-compatible 0.2.0 request, response, catalog, and event envelopes.
- `goosic-core` — the playback authority: one owner, lease generations, increasing sample sequences, account-change resets, harmless advertisement markers.
- `goosic-catalog` — read-only YouTube Music access, split into a pure parser and a guest-only HTTP client. It answers what exists, never who may play.
- `goosic-settings` — durable preferences, and the reversible, credential-free import from a previous Goosic install.
- `goosic-downloads` — read-only legacy media indexing plus WebM/Opus-to-WAV decode caching; it contains no downloader or account-cookie path.
- `goosic-service` — one request per stdin line, one response per stdout line, with no diagnostics on stdout.
- `apps/goosic-swift` — the macOS shell: routed navigation, live catalog screens, search with filter tabs, entity detail pages, a queue and now-playing bar, and the official playback host.

## Architecture

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the ownership, catalog, and wire contracts, and [docs/LEGACY_COMPATIBILITY.md](docs/LEGACY_COMPATIBILITY.md) for the migration, storage, and licensing boundaries.

## Prerequisites

Rust 1.88+ and Cargo; Swift 5.10+ and macOS 14.0+ for the Swift shell. Swift package resolution needs network access the first time because SwiftCrossUI is pinned to the official `0.9.0` tag. The SwiftCrossUI package exposes optional non-macOS backend dependencies; the Make targets select `AppKitBackend` explicitly for this macOS shell.

## Build, run, and test

```sh
make test          # Rust workspace tests plus the Swift test target, all offline
make test-rust-live # opt-in: hits music.youtube.com to check the catalog parser against reality
make run-swift     # builds the service and launches the shell against it
```

The service accepts compact JSON lines such as `{"protocolVersion":"0.2.0","requestId":"1","command":"catalog.search","payload":{"query":"daft punk","filter":"songs"}}`. Its stdout is protocol-only; diagnostics, if any, go to stderr.

The shell connects to the service on launch, so Home loads without any manual step. The sidebar button remains the way back if a transport failure drops the child process.

## Current limitations

- **No account.** The catalog is browsed as a guest, so Library has nothing personal to show and there is no sign-in. Account profiles and persistence are phase 4.
- **No new downloads.** This migration deliberately imports and plays only finalized legacy files. Explicit Premium-only downloading is not implemented, so the app never claims to create a new offline file.
- **macOS only.** Windows and Linux playback hosts remain explicit stubs so no renderer can bypass Rust's authority.
- **The imported theme is not applied.** A legacy theme preference is stored and shown, but the shell does not style itself from it yet.
- **Windows preferences cannot be imported.** WebView2 keeps local storage in LevelDB rather than SQLite, and no reader for it exists here.
- Catalog pages are clamped to one protocol frame; a clamped page says so rather than presenting a partial list as complete.

`goosic-service` is a private, one-process-per-app, single-client child reached through inherited stdin/stdout. It is not a daemon or socket service; stdio must never be shared or multiplexed. Generation is freshness authorization within that boundary. Future multiplexing requires an unforgeable per-client capability and active-owner authorization before account resets.

## Migration phases

1. **Done** — protocol/core/service authority and the native shell.
2. **Done (macOS)** — the official WebView host and its origin-checked bridge; Windows and Linux remain stubs.
3. **Done** — the live catalog, search, and real playback from the catalog.
4. **Done** — durable preferences and the legacy preference import.
5. **Done (macOS)** — read-only legacy downloaded-media import, Rust decode cache, and AVFoundation local-file playback.
6. **Next** — account profiles, system media controls, theming, and production packaging.
