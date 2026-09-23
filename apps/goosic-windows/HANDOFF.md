# Windows shell — state, audit against YouTube Music, and what is left

Written to be read once, start to finish, before touching anything. The last section is a prompt
that can be pasted into another coding agent as-is.

## Settings and the Home feed

Settings has two kinds of switch, stored in two places on purpose. **Listening preferences**
— Hide explicit songs, the page Goosic opens on, Reduce motion — are Rust's (`hideExplicit`,
`startPage`, `reduceMotion` in `goosic-settings`) and shared with every shell. They need
`feature/shared-preferences` from `development`; against a service without it they read as
their defaults and are not saved. The shell now also saves `lastRoute` for "The page I was
on last". **Windows behaviour** — Close to tray, Launch at startup, Now-playing notifications,
Discord status, Remember window, Efficiency mode — lives in `%LOCALAPPDATA%\Goosic\windows-shell.json`
(`Service/ShellPreferences.cs`), except Launch at startup, whose truth is the per-user Run
key so that Task Manager's Startup apps page stays in agreement.

Efficiency mode is Windows' EcoQoS for this process plus no decorative motion. It does not set
WebView2's low-memory target, which Microsoft intends for an inactive WebView; the player's is
always playing. The tray icon is written against `Shell_NotifyIcon` directly (`Service/TrayIcon.cs`).
Discord status talks to the Discord app's local IPC pipe and stays disabled until
`DiscordPresence.ApplicationId` holds a registered Discord application's id. The window, tray
and executable use the macOS app icon, converted to `Assets/Goosic.ico`.

Home follows YouTube Music's layout once `feature/home-feed-layout` is on `development`:
a shelf whose `layout` is `list` (Quick picks) is drawn as columns of four compact rows that
scroll sideways, and every other shelf as cards. Without that field every shelf is cards, as
before.

## Read these first

`AGENTS.md`, then `docs/NATIVE_SHELL_MIGRATION.md` (step 5 is this work) and
`docs/SHELL_CONTRACT.md` (what may and may not be reimplemented per shell). The branch is
`platform/windows`. Work that only exists for Windows stays here; anything in `crates/`, the
protocol, `PersonalCatalog.js` or the docs is cross-platform and lands on `development` first.

## What the app is

`apps/goosic-windows/Goosic.Windows` — WinUI 3 (Windows App SDK 2.4), C#, .NET 9, unpackaged.
It talks to `goosic-service` over NDJSON on stdio, and reaches the shared rules in
`crates/goosic-shell-support` through `crates/goosic-shell-support-ffi` (P/Invoke).

Layout follows the macOS shell: content fills the window on Mica; the sidebar (Discover,
Library, the account's playlists, account row, settings) and the Lyrics / Playing Next panel
float over it as rounded acrylic panels; the player is a floating acrylic pill at the bottom;
rows of cards slide under the panels. The full-screen player fills the window (window full screen
is a separate toggle / F11 inside it) over a moving mesh of the cover's colours.

## Map of the code

| File | Owns |
| --- | --- |
| `MainWindow.xaml` | All views |
| `MainWindow.xaml.cs` | Construction, service wiring, shell-support check |
| `MainWindow.Navigation.cs` / `.Layout.cs` | Routes, back, search; breakpoints, insets, carousels, minimum size |
| `MainWindow.Player.cs` / `.FullPlayer.cs` | Pill, transport, seeking, volume; full-screen player |
| `MainWindow.SidePanel.cs` | Queue and lyrics panel |
| `MainWindow.Menus.cs` / `.Dialogs.cs` / `.Account.cs` | Context menus; confirm and prompt dialogs; account and playlist actions |
| `MainWindow.Keyboard.cs` / `.SystemMedia.cs` | Accelerators; SMTC wiring |
| `Presentation/` | UI-free rules the partials apply: breakpoints and insets, which side panel is open and where focus returns, key bindings and seek/volume arithmetic |
| `ViewModels/ShellViewModel.cs` | Pages (route / search / entity), history, now-playing state, lyrics |
| `ViewModels/ShellViewModel.Queue.cs` | Queue, shuffle, repeat, radio, entity play, continuations, links |
| `ViewModels/ShellViewModel.Account.cs` | Accounts, personal pages, likes, playlists edits |
| `ViewModels/ShellViewModel.Settings.cs` | Preferences via `settings.*`, page flags, volume persistence |
| `ViewModels/NowPlayingMesh.cs`, `Views/MeshBackground.cs` | Cover palette and animated background (GPL port) |
| `Service/OfficialPlaybackHost.cs` | WebView2 player: lease claim, observer, validation, volume, page auto-advance |
| `Service/PersonalCatalogHost.cs` | Signed-in reads/mutations via the Swift shell's `PersonalCatalog.js` |
| `Service/AccountLoginWindow.cs`, `WebProfiles.cs` | Google sign-in in staged per-account WebView2 profiles |
| `Service/SystemMediaControls.cs` | Windows media overlay and media keys |
| `Service/GoosicServiceClient.cs` | Service process and protocol transport |

## Verified in the running app

Browse, search, artwork, playback with accepted samples (including an advertisement reported as a
marker), pause, next, lyrics panel, queue panel, signed-in Home and library (the user signed in),
the floating layout, the pill and its position line. Everything else below compiled and launched
but was not driven by hand — mouse and keyboard were never automated on the user's desktop.

## Audit against the YouTube Music web app

Legend: ✅ works · 🟡 partial · ❌ missing.

| Area | YouTube Music web | Windows shell |
| --- | --- | --- |
| Home, Explore, Charts, Moods & genres, New releases | Shelves | ✅ (`catalog.browse`); signed-in Home via personal reader |
| Home mood chips (Relax, Energize…) | Filter Home | ❌ not exposed by the catalog protocol |
| Search | Results + filters | ✅ results, ✅ filter chips (All/Songs/Albums/Artists/Playlists) |
| Search suggestions / history | As you type | ❌ needs a protocol command (cross-platform) |
| Videos / podcasts / episodes filters | Yes | ❌ |
| Infinite scroll | Automatic | ✅ loads more near the end; "Load more" remains as fallback |
| Shelf "More" (open full shelf) | Yes | ❌ shelf browse endpoints not in the protocol |
| Album page | Tracks, play, shuffle, save | 🟡 numbered rows, cover from the opening card, song count and length; no "save album to library" (needs audio-playlist id) |
| Playlist page | Tracks, edit, reorder, description | 🟡 rename/privacy/delete/remove ✅; reorder ❌; description ❌ |
| Artist page | Shelves, subscribe, shuffle, radio | 🟡 shelves + subscribe ✅; artist shuffle/radio buttons ❌ |
| Library | Playlists/Songs/Albums/Artists/Subscriptions, sort | ✅ sections; sort ❌; Uploads ❌ |
| Liked songs, History | Yes | ✅; remove from history ❌ |
| Play, pause, seek, next, previous | Yes | ✅; previous restarts after 3 s |
| Shuffle, repeat off/all/one | Yes | ✅ persisted in settings |
| Autoplay at queue end | Yes | ✅ appends recommendations after the queue (never replaces it), skipping queued and recently played songs; page's own auto-advance intercepted |
| Queue: view, play next, add, remove, clear | Yes | ✅ Now playing / Up next split; Clear is immediate with Undo |
| Queue reorder by drag | Yes | ✅ drag handle, or Alt+↑/↓ on a row (drag not hand-tested) |
| Start radio | Yes | ✅ signed in: the account's own Up Next via `PersonalCatalog.js` `radio()`; guest: `catalog.radio`. A song card or search result starts a station; only albums, playlists and liked songs play in order (`Presentation/PlaybackOrder`) |
| Like / dislike | Shows current state | 🟡 works, but only ratings set this session show — current like state is never read |
| Save to playlist, new playlist | Yes | ✅ |
| Go to artist / album, share link | Yes | ✅ copy link |
| Not interested, pin to Listen again, song credits | Yes | ❌ |
| Lyrics | YouTube's own, synced when available | 🟡 via `lyrics.get` (third-party source), synced highlighting ✅ |
| "Related" tab in player | Yes | ❌ |
| Volume, mute | Persisted | ✅ persisted in settings, primed before each page so tracks start at the chosen level |
| Keyboard shortcuts | Space, seek, next/prev, like… | 🟡 Space, Shift+←/→ ±10 s, Ctrl+←/→, Ctrl+↑/↓, Ctrl+M/S/R/F/L/Q/B, F11, Alt+←, Esc; no like/dislike keys |
| Media keys / OS overlay | Browser media session | ✅ SMTC |
| Full-screen player with lyrics | Yes | ✅ window-filling, optional OS full screen |
| Accounts: sign in, switch, sign out | Yes | ✅ (sign-in run once by the user) |
| Downloads / offline | Mobile only | ❌ Downloads page is a placeholder; local-file host not built |
| Video mode (song ↔ video) | Yes | ❌ player is a 1 px renderer by design |
| Advertisements | Played | ✅ reported as markers, never skipped (invariant) |

## Playback notes worth knowing

- **Automix is switched off in the renderer.** With it on, YouTube Music fades into its own next
  track some seconds before the requested one ends; the host saw another video, paused, and the
  song lost its ending. `OfficialPlaybackHost.KeepAutomixOffAsync` turns the page's `#automix`
  switch off after each load (bridge.log: `automix "turned off"`). It is behaviour every shell
  wants, so the script belongs in `goosic-shell-support` beside the observer once that crate's FFI
  question is settled.
- **A track replaced before it played is not an end.** The host reads the page's title: the same
  song under another id is kept, anything else is reported as refused, dimmed, and skipped for the
  session. Unreleased tracks on pre-release albums arrive from the catalog with a video id and no
  play count, and are the usual case.
- **Advertisements freeze track changes, seeking and volume**, as on macOS.

## What is left, in priority order

1. **Hand-test** everything marked "not hand-tested": queue drag, volume primer at track start,
   auto-advance when a song ends (`bridge.log` line "the page moved on to its own track"),
   full-screen toggle, sign out/switch, playlist edits, SMTC buttons.
2. **Real like state.** Read `likeStatus` for the playing track in the signed-in profile. This
   needs `PersonalCatalog.js` to grow a read (e.g. from the `next` endpoint) — a cross-platform
   change on `development`, mirrored in the Swift shell.
3. **Downloads.** Local decoded-file host: claim owner `localDownloadedFile`, `downloads.prepare`,
   play the returned path with `MediaPlayer`, report samples like the official host. Route the
   transport to whichever host owns the lease. See `LocalPlaybackHost+macOS.swift`.
4. **Playlist reorder and description**, **save album** (needs the album's audio playlist id from
   the catalog), **artist shuffle/radio** buttons, **remove from history**, **library sort**.
5. **Search suggestions** and **shelf "More"** — both need protocol additions on `development`.
6. **Like/dislike shortcuts**, **Related** tab, **song credits**.
7. **Packaging:** MSIX with `goosic-service.exe` beside the app; run the protocol fixtures.
8. The completion script in `AccountLoginWindow.cs` mirrors the macOS one; replace the stale
   copy in `goosic-shell-support` on `development` and export it through the FFI.

## Build and run

```pwsh
cd apps\goosic-windows\Goosic.Windows
dotnet build -c Debug
$env:GOOSIC_SERVICE_PATH="C:\DEV\GoosicReborn\target\debug\goosic-service.exe"
.\bin\Debug\net9.0-windows10.0.26100.0\win-x64\Goosic.Windows.exe
```

Presentation tests compile `Presentation/*.cs` directly and need no display:

```pwsh
dotnet test apps\goosic-windows\Goosic.Windows.Tests
```

A rule that decides something belongs in `Presentation/`, where a test can reach it; the
`MainWindow.*.cs` partials only apply its answer to controls.

If the app is running, the exe is locked: build with `-p:OutDir=<another folder>\` or close it.
Logs: `%LOCALAPPDATA%\Goosic\logs\bridge.log` (host and path only, never credentials).

## Traps that will cost you a day

- Windows App SDK 1.6 fails silently on .NET 9; the project uses 2.4.
- `x:Bind` needs public types and a model built **before** `InitializeComponent()`.
- A `Slider` marks pointer events handled: attach with `AddHandler(..., handledEventsToo: true)`.
  A `Slider` also needs a 32 px frame — the pill's position line is hand-drawn for that reason.
- A `ScrollViewer` clips content inside its `Padding`; pad the content instead.
- A horizontal-only `ScrollViewer` turns the vertical wheel sideways; carousels forward the wheel.
- `ItemsControl.Padding` never reaches its panel; wrap in a `Border`.
- XML comments cannot contain `--`.
- The official page auto-advances to its own automix when a track ends; the host pauses it and
  treats it as the natural end. Do not loosen the bridge validators to "fix" rejected reports.
- A composition child visual draws above an element's XAML children.
- Heredocs mangle backslashes in this environment; write scripts to files.
- Unpackaged: `WebView2Loader.dll` only lands beside the exe when a RuntimeIdentifier is set.

## Invariants (from `AGENTS.md` — not negotiable)

Rust is the playback authority (claim before sound; one owner; generation-scoped). Service
stdout is protocol only. Catalog reads are anonymous. Credentials never cross the protocol.
Advertisements are reported, never bypassed. Ported GPL code keeps its notice.

## Prompt for the next agent

> You are continuing the native Windows shell of GoosicReborn on branch `platform/windows`
> (repo root `C:\DEV\GoosicReborn`). Read `AGENTS.md`, then `apps/goosic-windows/HANDOFF.md`
> fully. The app is WinUI 3 / C# / .NET 9 in `apps/goosic-windows/Goosic.Windows`. Work through
> "What is left, in priority order" in that file, top to bottom. Rules: keep every invariant in
> the handoff; Windows-only changes stay on `platform/windows`, anything in `crates/`, the
> protocol, `PersonalCatalog.js` or docs goes to a branch cut from `development` first; commit
> in small pieces with imperative messages that explain why; `dotnet build` must succeed and
> `make test` must pass before you hand back; never automate the user's mouse or keyboard —
> ask them to test and read `%LOCALAPPDATA%\Goosic\logs\bridge.log`; update HANDOFF.md's audit
> table as items move. Report what you verified in the running app separately from what only
> compiled.
