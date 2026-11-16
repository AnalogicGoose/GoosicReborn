<p align="center">
  <img src="assets/logo.png" alt="GoosicReborn Logo" width="128" height="128" />
</p>

<h1 align="center">GoosicReborn</h1>

<p align="center">
  <strong>A fast, native YouTube Music desktop client powered by a single Rust playback authority.</strong><br />
  Tailored native shells: SwiftUI on macOS, WinUI 3 on Windows, and GTK 4 on Linux. Zero Electron.
</p>

<p align="center">
  <a href="https://github.com/AnalogicGoose/GoosicReborn/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/AnalogicGoose/GoosicReborn/ci.yml?branch=main&style=flat-square&label=CI" alt="CI Status" /></a>
  <a href="https://github.com/AnalogicGoose/GoosicReborn/releases/latest"><img src="https://img.shields.io/github/v/release/AnalogicGoose/GoosicReborn?style=flat-square&color=blue" alt="Latest Release" /></a>
  <img src="https://img.shields.io/badge/platforms-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey?style=flat-square" alt="Supported Platforms" />
  <a href="LICENSE-GPL-3.0"><img src="https://img.shields.io/badge/license-GPL--3.0-blue?style=flat-square" alt="License" /></a>
</p>

---

GoosicReborn replaces web wrappers with real native interfaces powered by a dedicated Rust core (`goosic-core`). Rust acts as the single playback authority and anonymous catalog reader, communicating with each platform shell over newline-delimited JSON across a private stdio channel.

- **macOS:** Built with SwiftUI and AppKit, adopting `NSGlassEffectView` material styling and native system media controls.
- **Windows:** Built with WinUI 3 and Windows App SDK, featuring Mica/acrylic materials, compact sidebar docking, and smooth media transport.
- **Linux:** Built with GTK 4 and WebKitGTK, integrating with desktop themes and MPRIS without depending on GNOME's Libadwaita.

---

## Downloads

| Platform | Channel | Format | Notes |
| :--- | :--- | :--- | :--- |
| **Windows** (x64) | **Stable** (`v0.4.0`) | [**Setup Executable (`.exe`)**](https://github.com/AnalogicGoose/GoosicReborn/releases) \| [Portable ZIP](https://github.com/AnalogicGoose/GoosicReborn/releases) | Built with WinUI 3. Self-updating from `v0.2.0+`. |
| **macOS** (Universal) | **Alpha** (`v0.4.0`) | [**Universal `.zip`**](https://github.com/AnalogicGoose/GoosicReborn/releases) | SwiftUI/AppKit. macOS 14+ required. Universal x86_64/arm64 binary. |
| **Linux** (x86_64) | **Development** | Flatpak / Source | Native GTK 4 shell. See [the Linux shell notes](docs/LINUX_SHELL.md). |

*Unsigned builds may trigger an unknown-publisher warning on first launch.*

---

## Features

- **Blazing Fast Native UI:** No Electron bloat or heavy web container UI. Shells render directly with platform-native controls and materials.
- **Live Catalog & Search:** Home shelves, Explore, Charts, Moods & genres, New releases, and instant search results backed directly by YouTube Music.
- **Real Playback Authority:** Rust enforces strictly one playback owner at a time, lease generations, and monotonic sample sequencing.
- **Synchronized Lyrics:** Live lyrics powered by LRCLIB with line-by-line sync and smooth highlighting.
- **Personal Content & Accounts:** Authenticated sessions run inside platform-isolated WebKit stores. Cookies and credentials never touch the service protocol or network stdout.
- **System Media Integration:** Native macOS `MPNowPlayingInfoCenter`, Windows System Media Transport Controls (SMTC), and Linux MPRIS support.
- **Smart Queue & Radio:** Replaces automated web autoplay with a strict local queue that respects what you chose to play, falling back smoothly to infinite auto-radio when your queue ends.
- **Persistent Preferences:** Volume curves (with natural audio tapers), shuffle, repeat, muted states, and window placements survive relaunches.

---

## Shell Architecture

```
                  ┌───────────────────────────────────────────────┐
                  │                 goosic-core                   │
                  │ (Single Playback Authority, Lease Generation) │
                  └───────────────────────▲───────────────────────┘
                                          │
                                   goosic-service
                           (Private NDJSON stdio channel)
                                          │
       ┌──────────────────────────────────┼──────────────────────────────────┐
       │                                  │                                  │
apps/goosic-swift                 apps/goosic-windows                 apps/goosic-linux
 (macOS: SwiftUI)                   (Windows: WinUI 3)                 (Linux: GTK 4)
```

- `goosic-core` — The playback authority: one owner, lease generations, increasing sample sequences.
- `goosic-catalog` — Anonymous, read-only YouTube Music client and parser. Answers what exists, never who may play.
- `goosic-lyrics` — LRCLIB lyric lookups and LRC timestamp parsing.
- `goosic-settings` & `goosic-accounts` — Durable local configuration and profile metadata.
- `goosic-service` — Private, single-client stdio service. Requests in, responses out, zero telemetry.
- `goosic-shell-support` — Platform-neutral translation rules and NDJSON client.

For full architectural invariants, contracts, and boundaries, see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), [docs/UI_DESIGN.md](docs/UI_DESIGN.md), and [docs/SHELL_CONTRACT.md](docs/SHELL_CONTRACT.md).

---

## Building and Testing

### Prerequisites

- **Rust:** 1.88+ with `rust-src`, `rustfmt`, and `clippy`.
- **macOS:** Xcode with Swift 6 (macOS 14+ target).
- **Windows:** .NET 8 / Windows App SDK (see [Windows Build Instructions](apps/goosic-windows/HANDOFF.md)).
- **Linux:** GTK 4.20+, WebKitGTK 6, and GStreamer (or GNOME Flatpak SDK).

### Commands

```sh
make test           # Run Rust tests and native Swift tests on macOS
make test-rust      # Run Rust tests across crates
make test-swift     # Run Swift tests only
make build-swift    # Build native macOS shell
make run-swift      # Build service and launch macOS shell
make build-linux    # Build service and native GTK 4 shell on Linux
make run-linux      # Launch GTK 4 shell against local service
make package-macos  # Package Goosic.app for macOS
```

---

## Release Notes & History

- [Release 0.4.0](docs/RELEASE_0.4.0.md): Windows docked sidebar, black acrylic material, Settings categories, universal macOS alpha.
- [Release 0.3.2](docs/RELEASE_0.3.2.md): Windows track fallback stability and responsive logging.
- [Release 0.3.1](docs/RELEASE_0.3.1.md): Multi-display fixes and artist top songs deduplication.
- [Release 0.3.0](docs/RELEASE_0.3.0.md): Dynamic artist hero banners and unified audio-taper volume curves.
- [Release 0.2.9](docs/RELEASE_0.2.9.md): Compact sidebar rail and native macOS settings window.
- [Browse all past releases](docs/)

---

## Licensing

GoosicReborn is distributed under the GNU General Public License v3.0 ([LICENSE-GPL-3.0](LICENSE-GPL-3.0)). Core components and original GoosicReborn code are licensed under the MIT License ([LICENSE](LICENSE)), while components ported from the legacy Goosic codebase retain their original GPL-3.0 licensing.
