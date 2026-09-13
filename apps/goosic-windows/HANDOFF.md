# Windows shell — where this got to, and what is next

Hand this to whoever picks the work up. It is written to be read once, start to finish, before
touching anything.

## Read these first

`AGENTS.md`, then `docs/NATIVE_SHELL_MIGRATION.md` (step 5 is this work) and
`docs/SHELL_CONTRACT.md` (what may and may not be reimplemented per shell). They are short and
load-bearing. The branch is `platform/windows`; PR #2 targets `development`.

## What exists and is verified working

`apps/goosic-windows/Goosic.Windows` — a WinUI 3 application in C#, .NET 9.

- **Chrome matching Apple Music for Windows**, which is the reference the user gave: a 48px icon
  rail with the account avatar pinned bottom, the transport occupying the title-bar row with a
  centred now-playing panel, a named sidebar drawn *over* dimmed content rather than beside it,
  and a right-hand panel shared by Lyrics and Playing Next.
- **Catalog browse** against the live service — shelves of square 168pt artwork, real titles and
  subtitles. Confirmed on screen.
- **Search** — the rail's search control opens the sidebar focused; a query reaches
  `catalog.search` and renders Songs / Artists / Albums shelves. Confirmed on screen.
- **Artwork** — fetched against the same host allow-list and FNV-1a cache key the other shells
  use, cookies disabled, cached under `%LOCALAPPDATA%\Goosic\artwork`. The view is only ever
  handed a local file.
- **Transport to `goosic-service`** — NDJSON framing that survives a pipe splitting anywhere,
  requests correlated by id, per-command deadlines matching `timeout_for`, stderr drained but
  never parsed, protocol version enforced.
- **`crates/goosic-shell-support-ffi`** — the rules over a C ABI, P/Invoked from C#. The shell
  asks it a question with a known answer at startup so a missing library fails visibly.

## What is built but NOT confirmed working

**Playback.** `Service/OfficialPlaybackHost.cs` claims the lease correctly (verified: the
authority refused a wrong generation, and stopped refusing once the contract was followed),
loads `music.youtube.com/watch?v=…` in a WebView2, installs the observer, and validates every
message through the FFI. **No accepted sample has ever been observed**, so the now-playing
panel still reads "Nothing playing" after selecting a track.

Where to look, in order:

1. **The bridge shim.** The shared observer posts through `window.webkit.messageHandlers`, which
   WebView2 does not have. A shim in `EnsureReadyAsync` forwards to `chrome.webview` and
   stringifies the payload. It is unproven — confirm the shim is installed before the observer
   runs, and that `OnWebMessage` fires at all. A breakpoint or a `Debug.WriteLine` in
   `OnWebMessage` answers this in one run.
2. **Whether the page ever plays.** YouTube Music may not autoplay without a user gesture, and
   may show consent or sign-in interstitials. The observer needs a `media` element to exist.
3. **Rejection reasons.** `OnWebMessage` writes every rejection to `Debug.WriteLine` with the
   reason from the rules library. Run under a debugger and read them; they name exactly which
   check failed.

Do **not** "fix" this by loosening the validators. They are shared and correct; if a check
rejects everything, the shim or the load is wrong, not the rule.

## Uncommitted, compiles, never run

The working tree holds ~620 lines not yet committed (`git diff --stat` shows six files):
back-navigation history, play/pause, seek and volume sliders, mute, previous/next, lyrics
loading via `lyrics.get`, an account summary via `accounts.get`, now-playing artwork, and the
bridge shim folded into the same document-created script as the observer (so the observer can
never run before its shim). `dotnet build` succeeds. **None of it has been exercised in the
running app.** Run it, check each control against the service, then commit in pieces with
messages that say what changed and why (see `AGENTS.md` §6).

## Queue, menus and system controls

`ShellViewModel.Queue.cs` owns the queue: entries are clones found by reference, so a track
queued twice stays two entries. It offers play next, add to queue, remove, clear, shuffle
(upcoming entries only, with the original order kept to restore), repeat off/all/one, a radio
through `catalog.radio` that extends itself from its cursor, playing a whole album, playlist or
artist, and `catalog.continue` behind "Load more". Rows and cards have a context menu (right
click, Shift+F10, or the row's "more" button). `SystemMediaControls.cs` publishes the Windows
media overlay and media keys through an unplayed `MediaPlayer`'s transport controls; button
presses go to the same handlers as the transport. Shortcuts: Space, Ctrl+Left/Right,
Ctrl+Up/Down, Ctrl+M, Ctrl+S, Ctrl+R, Ctrl+F, Ctrl+L, Ctrl+Q, Alt+Left, Esc. It renders; the
menus, radio and overlay have not yet been exercised by hand.

## Not started

Sign-in and per-account WebView2 profile isolation, then everything that needs an account:
library, likes, save to playlist, playlist management, all through `PersonalCatalog.js` inside
the authenticated profile. Local downloaded-file playback, the full-screen player, the
artwork-derived mesh gradient behind the now-playing panel, MSIX packaging, running the
protocol fixtures against this shell.

## How to build and run

```pwsh
cd apps\goosic-windows\Goosic.Windows
dotnet build -c Debug
$env:GOOSIC_SERVICE_PATH="C:\DEV\GoosicReborn\target\debug\goosic-service.exe"
.\bin\Debug\net9.0-windows10.0.26100.0\Goosic.Windows.exe
```

`cargo build -p goosic-shell-support-ffi` runs automatically and the DLL is copied beside the
executable. A packaged build will ship `goosic-service.exe` beside the app, which is what
`LocateService` looks for when the environment variable is absent.

## Traps that will cost you a day if you rediscover them

- **WindowsAppSDK 1.6 silently fails on .NET 9.** The XAML compiler exits 1 having written
  *nothing* — no stdout, no stderr, no output file. The project uses **2.4**, which reports real
  errors. If XAML ever fails with only `MSB3073`, suspect the SDK version, not your markup, and
  bisect with a minimal `MainWindow.xaml` before editing anything.
- The `TargetFramework` must name a Windows SDK version present in `Windows Kits\10\References`.
  A missing one fails the same silent way.
- `x:Bind` and `x:DataType` require **public** types; keep constructors internal so they may
  still take the internal service types.
- `x:Bind` resolves during `InitializeComponent()`. Build the view model **before** calling it,
  or every binding reads null and the crash surfaces inside `Microsoft.UI.Xaml.dll` naming none
  of your files.
- `RowDefinition.Height` needs a `GridLength`; an `x:Double` resource throws at parse time.
- A `ResourceDictionary` cannot have implicit items on both sides of a property element — put
  `ThemeDictionaries` first.
- Set `StandardOutputEncoding` / `StandardInputEncoding` / `StandardErrorEncoding` to UTF-8 on
  the service process, or every accented name arrives mangled.
- WinRT `IAsyncOperation` has no `ConfigureAwait`.

## Repository hygiene this work is carrying

`platform/windows` currently carries several fixes that belong on `development` and are red
there: the swift-log pin, an `ETXTBSY` retry, a `State` ambiguity fix and a compiler-crash
workaround in the macOS files, plus the strays listed in the memory note. They were landed here
because Autofix named the failing checks. Someone should lift them to the trunk; this PR going
green does not make `development` green.
