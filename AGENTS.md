# AGENTS.md

Instructions for AI coding agents working in this repository. Read this before making a
change; it is short because everything in it is load-bearing.

GoosicReborn is a native rewrite of Goosic: a Rust playback authority with SwiftUI/AppKit
on macOS, GTK 4 in Rust on Linux, and WinUI 3 on Windows. SwiftCrossUI is removed. Rust decides
what is allowed; each native shell renders and asks. See [docs/NATIVE_SHELL_MIGRATION.md](docs/NATIVE_SHELL_MIGRATION.md)
and [docs/LINUX_SHELL.md](docs/LINUX_SHELL.md).

## 1. Check what branch you are on first

This repository uses a five-branch model, documented in full in
[docs/BRANCHING.md](docs/BRANCHING.md). The part you must not get wrong:

- `main` is **deployment only**. Never commit to it, never branch from it (except a
  `hotfix/<slug>`), never merge into it outside a deployment. The single exception is
  `.github/workflows/ci.yml`: `main` is the default branch, only the default branch writes
  the build cache, and only its own copy of a workflow is what a scheduled run executes, so
  the file has to be current there. Nothing else earns that exception.
- `development` is the trunk. Cross-platform work is cut from it and merges back to it.
- `platform/macos`, `platform/linux`, `platform/windows` hold work that exists only to
  satisfy one operating system's API.

Before you write anything, answer: **would this change be wrong to leave out on another
platform?**

- **Yes** → it is cross-platform. Work on `feature/<slug>` or `fix/<slug>` cut from
  `development`. This covers everything in `crates/`, `goosic-protocol`, shared shell rules,
  the Makefile, and every document.
- **No**, it exists only because of one OS's API → work on `feature/<os>/<slug>` cut from
  `platform/<os>`, where `<os>` is `macos`, `linux`, or `windows`.

If you are unsure, choose `development`. A shared change made on a platform branch is
invisible to the other two until that branch lands, which is how this repository would get a
three-way conflict in the same file.

**A platform branch should not be the first place a shared file changes.** If Linux work
seems to need a new field in `goosic-protocol`, that need is not Linux-specific: land it on
`development` first, sync it down, then build the platform-specific part on top.

Never merge one `platform/*` into another. Never rebase a long-lived branch — they are
shared, and rewriting their history breaks every other copy.

## 2. Commands

```sh
make test            # Rust tests everywhere + native Swift tests on macOS
make test-rust       # Rust only
make test-swift      # Swift only
make build-swift     # build the native macOS shell
make run-swift       # build the service and launch the shell against it
make test-rust-live  # opt-in; hits music.youtube.com. Do not run it in a normal check.
make package-macos   # the download a tester installs; see docs/RELEASING.md
```

`make test` must pass before you hand work back. CI repeats it on Linux, macOS, and
Windows for every push — see [docs/BRANCHING.md](docs/BRANCHING.md) — so a change that only
compiles on the platform you are working on will be caught, but it is cheaper to notice it
yourself: platform-specific code inside a `#if` is invisible to the compiler you are running. On Linux, `cargo fmt` and `cargo clippy`
need separate packages on a distribution-packaged Rust — see the README's Prerequisites.

## 3. Invariants that are not yours to change

These come from [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), which is the authority. If a
task seems to require breaking one, stop and say so instead of working around it.

- **`goosic-core` is the single playback authority.** One owner at a time, scoped to a
  generation, with strictly increasing sample sequences. The shell requests transitions; it
  never decides whether one is valid. `goosic-core` has no UI, WebView, network, cookie, or
  audio dependency — do not add one.
- **`goosic-service` stdout is protocol-only.** One request per stdin line, one response per
  stdout line. Diagnostics go to stderr. It is a private, single-client child process reached
  through inherited stdio — not a daemon, not a socket, never shared or multiplexed.
- **Catalog reads are anonymous by construction.** No cookies, no `Authorization`, no account
  headers; a test asserts this. `goosic-catalog` answers *what exists*, never *who may play*.
- **Credentials never cross the protocol and never reach stdout.** Cookies, bridge tokens,
  signing keys, and media URLs stay in platform-secure storage.
- **Advertisements are reported, never bypassed.** They are informational markers.
- **Code from the previous Goosic keeps its notice.** The previous Goosic is GPL-3.0 and may
  be ported here directly — reuse working code rather than rewriting it — but a ported file
  carries the GPL header and the original authors' copyright, and the combined program is
  distributed under GPL-3.0 (`LICENSE-GPL-3.0`); see
  [docs/LEGACY_COMPATIBILITY.md](docs/LEGACY_COMPATIBILITY.md). Porting does not relax the
  other invariants: ported fetch code runs where the cookies already live, inside the account's
  WebKit profile, never through the service protocol. The legacy import reads old data and
  never modifies or deletes it, and never carries credentials over.
- **No downloader.** `goosic-downloads` imports finalized legacy files and decodes them. It
  contains no yt-dlp path and no account-cookie path, and must not grow one.
- **A clamped catalog page says so.** Never present a partial list as complete. A page with
  more shelves than one frame holds is paged, not clamped: the service sends what fits and a
  cursor for the rest. A list that is still partial says so in its count ("100+ songs"); the
  sentence explaining why is a detail, shown in debug mode and always logged.

## 4. Where platform-specific code lives

Each native shell owns its platform integrations:

```
apps/goosic-swift/Sources/GoosicSwift/
    Core/                 macOS model, protocol DTOs and tested rules
    Platform/macOS/       SwiftUI, AppKit, WebKit, AVFoundation, MediaPlayer
apps/goosic-linux/         GTK 4, WebKitGTK, GStreamer, MPRIS
apps/goosic-windows/       WinUI 3, WebView2, Windows media controls
```

Swift platform files keep unique base names (`OfficialPlaybackHost+macOS.swift` and
`OfficialPlaybackHost+Preview.swift`) because SwiftPM derives object names from them. The
preview hosts are guarded by `GOOSIC_PREVIEW_NO_WEBKIT`; they refuse playback rather than
producing sound outside Rust authority. No portable Swift UI or Linux/Windows Swift host
should be reintroduced.

Rules, wire shapes and validation remain separate from native screens. The protocol and Rust
shell-support crate are the shared contract, while account profiles, windows and audio hosts
belong to each operating system. A platform-only change stays in that app; a shared rule change
lands on `development` first.

Those platform-neutral rules now also live in Rust, in `goosic-shell-support` — login
navigation policy, bridge event validation, media projection, catalog conversion, queue
selection and the rest; the crate documentation has the table. Until the Swift shell consumes
that crate, the two copies are held to the same test cases and nothing else keeps them in step,
so a change to one of those rules is a change to both, in the same commit.

A stub reports the limitation. It must never produce sound or silently succeed, because that
would let a renderer escape Rust's authority. When you implement one for a platform, keep the
others' behaviour unchanged.

Linux publishes its media controls over MPRIS, which is a bus interface rather than a system
API, so two rules follow from that and not from taste. A method is declared in the
introspection XML only when Goosic can honour it, because a declared method that refuses at
runtime becomes a dead button in every panel on the desktop. And `Position` is deliberately
left out of `PropertiesChanged`, as the specification asks: a player that announced every tick
would wake every panel several times a second. Neither the MPRIS adapter nor the macOS one
decides anything — a command from the bus is rechecked against
`SystemMediaCommandAvailability` before it reaches the model, so a remote client cannot ask for
a transition the app itself would refuse.

The GTK shell has played official and local audio on a live desktop. Its packaged account and
hidden-window acceptance checks remain documented in `docs/LINUX_SHELL.md`; compilation alone
must never be reported as a live playback check.

Two portability rules bite far from their cause. `URLSession` needs a
`#if canImport(FoundationNetworking)` import off Darwin. And swift-corelibs-xctest discovers
tests by casting method signatures, so `@MainActor` anywhere in one is fatal: not only on the
`XCTestCase` subclass but on an individual test method too, which the earlier version of this
note got wrong. The failure is `Could not cast value of type '... -> @Swift.MainActor () throws
-> ()'` followed by signal 6, and it takes the entire run with it rather than the one test.

Leave the method unisolated and put the work in a `MainActor.run` body, which then has to be
`async` — and remember the other half of the rule: that body must not touch `self`, so a
fixture becomes `static`.

### The GTK shell on Linux

`apps/goosic-linux` is the native GTK 4 shell on `platform/linux`. It plays through the Rust
service, reads personal content inside the account's WebKitGTK profile, and builds as a Flatpak.
[docs/LINUX_SHELL.md](docs/LINUX_SHELL.md) records what has been verified and what still needs a
real account or desktop session. When you work on it, these are settled and not yours to reopen
without asking:

- It is its own Cargo workspace, not a member of the root one. CI runs
  `cargo test --workspace` on macOS and Windows, and a member that needs GTK would fail there.
- It links `goosic-shell-support` and `goosic-protocol` and nothing else from `crates/`. It
  runs the `goosic-service` binary installed beside it, found by path and never on `PATH`;
  it never links `goosic-core`.
- It uses GTK 4 without libadwaita, so it takes each desktop's theme rather than GNOME's.
- Its application ID is `io.github.analogicgoose.Goosic`. Storage stays under `goosic`, and
  the media-player bus name stays `org.mpris.MediaPlayer2.goosic`.
- Closing the window keeps the process and the music running; quitting is explicit.
- It ships as a Flatpak on the GNOME runtime, which is what makes GTK 4.20 its floor.

It lives on `platform/linux`. Anything it needs from `goosic-shell-support` or the protocol
lands on `development` first.

## 5. Keep the working copy tidy

Merges down the branch tree are automatic and finished branches are pruned from the remote
weekly, so a local clone accumulates branches whose upstream no longer exists. Clear them at
the start of a session, or any time the branch list stops being readable:

```sh
git fetch --prune
git for-each-ref --format='%(refname:short) %(upstream:track)' refs/heads \
  | awk '$2 == "[gone]" { print $1 }' \
  | xargs -r git branch -d
```

Two details matter. `-d` refuses to delete a branch whose commits are not already merged, so
if something was never integrated it survives and says so — never reach for `-D` to make the
error go away, because that is exactly the case worth reading. And `for-each-ref` is used
instead of parsing `git branch -vv` because the latter marks the current branch with an
asterisk, which ends up in the branch name and produces a confusing failure.

This only touches your own clone. Deleting anything on the remote is the pruning workflow's
job, and it only removes branches whose commits already live in their parent.

## 6. Conventions

- **Documentation is written, not suggested.** Markdown under `docs/`, the README, and this
  file are edited directly. If a code change makes a document wrong, fix the document in the
  same change.
- **Prose over bullets in `docs/`.** Existing documents explain *why* a contract exists, not
  just what it is. Match that.
- **Commit messages** state what changed and why it had to change, in the imperative. No
  co-authorship, attribution, or session trailers.
- **Rust** is edition 2021, `max_width = 100` (`rustfmt.toml`). Tests live beside the code
  they cover; live network tests are `#[ignore]`d.
- **Swift is built in the Swift 6 language mode.** Concurrency errors are errors, not warnings.
  Do not reach for `@unchecked Sendable` to silence one — the single existing use, on
  `GoosicServiceClient`, is a claim about a serial queue that the compiler cannot verify, and it
  needs to stay the only one. In tests, a `MainActor.run` body must not touch `self`; make the
  fixture `static` instead.
- **English** for all code, comments, documents, and commit messages.

## 7. Shared UI requirements for every platform

Before changing any screen, control, layout, theme or animation on macOS, Windows or Linux,
read [docs/UI_DESIGN.md](docs/UI_DESIGN.md). It is the shared product design contract, including
native adaptation, visual hierarchy, behavior, accessibility, performance and acceptance cases.
macOS work also reads [docs/MACOS_UI_GUIDELINES.md](docs/MACOS_UI_GUIDELINES.md).

Use native controls to produce the same product outcome. Do not introduce an opaque header or
divider behind floating search controls, resize the player for temporary volume UI, drop
essential narrow-layout actions, or invent a different brand or interaction on one platform.
Read the implementation-gap audit: existing divergence is work to align, not a design precedent.
A user-approved design correction updates the shared specification in the same change, and
platform differences must be recorded there. Shared documentation belongs on development;
native implementation follows the platform branch rules above. Respect system and app motion,
transparency and contrast settings. Compilation and tests do not establish visual acceptance;
report actual inspection and leave live testing to the user when requested.
