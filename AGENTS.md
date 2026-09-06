# AGENTS.md

Instructions for AI coding agents working in this repository. Read this before making a
change; it is short because everything in it is load-bearing.

GoosicReborn is a native rewrite of Goosic: a Rust playback authority plus a SwiftCrossUI
shell, built for macOS, Linux, and Windows out of one repository. Rust decides what is
allowed; the shell renders and asks.

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
  `development`. This covers everything in `crates/`, `goosic-protocol`, shared Swift views,
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
make test            # Rust workspace tests + Swift tests, offline and deterministic
make test-rust       # Rust only
make test-swift      # Swift only
make build-swift     # build the shell for the host platform
make run-swift       # build the service and launch the shell against it
make test-rust-live  # opt-in; hits music.youtube.com. Do not run it in a normal check.
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
- **No GPL source is copied into this repository.** The previous Goosic is read for
  compatibility of formats and storage keys only — see
  [docs/LEGACY_COMPATIBILITY.md](docs/LEGACY_COMPATIBILITY.md). The legacy import reads old
  data and never modifies or deletes it, and never carries credentials over.
- **No downloader.** `goosic-downloads` imports finalized legacy files and decodes them. It
  contains no yt-dlp path and no account-cookie path, and must not grow one.
- **A clamped catalog page says so.** Never present a partial list as complete.

## 4. Where platform-specific code lives

The directory says which platform a file belongs to, so nobody has to infer it from a name:

```
Sources/GoosicSwift/
    Core/                 compiled everywhere; no #if os(...) belongs here
    Platform/macOS/       AppKit, WebKit, AVFoundation, MediaPlayer
    Platform/Linux/       GTK 4, WebKitGTK
    Platform/Stubs/       every platform without a real implementation
```

Files under `Platform/` carry their platform as a suffix — `OfficialPlaybackHost+macOS.swift`,
`OfficialPlaybackHost+Unsupported.swift`. That is not decoration and not a second way of
saying what the directory already says: SwiftPM derives each object file from the source's
base name, so one target cannot hold two files called `OfficialPlaybackHost.swift`, and the
build fails with `multiple producers` rather than anything that names the real cause.

Each file still opens with the `#if os(...)` that makes it true. The directory is where a
reader looks; the guard is what the compiler obeys. Keeping both means a misplaced file
fails to compile instead of silently vanishing from a platform.

Platform work belongs in these seams, behind the existing abstractions, not scattered through
the screens:

| Concern | Core | Platform | State |
| --- | --- | --- | --- |
| Playback host abstraction | `PlatformPlaybackHost.swift` | — | shared; the extension point |
| Official (web) playback | `OfficialBridge.swift` | `OfficialPlaybackHost+*.swift`, `WebKitSurface+Linux.swift` | macOS real, Linux written but never yet heard, Windows stubbed |
| Local decoded-file playback | `LocalPlaybackEvent.swift` | `LocalPlaybackHost+*.swift` | macOS real, others stubbed |
| System media controls | `SystemMediaPlayback.swift` | `SystemMediaControls+*.swift` | macOS real, others stubbed |
| Window material | `MaterialSurfaceKind.swift` | `MaterialSurface+*.swift` | macOS real, plain elsewhere |
| Account WebKit profiles | `AccountLoginModel.swift` | `AccountLoginHost+*.swift` | macOS real, others stubbed |

The `Core` column is the half that decides things and the `Platform` column is the half that
talks to an operating system. Rules, wire shapes, and validation belong in `Core` — those are
the parts a test can reach on any machine, and splitting them out is what makes `may this
play` have one answer rather than one per platform.

A stub reports the limitation. It must never produce sound or silently succeed, because that
would let a renderer escape Rust's authority. When you implement one for a platform, keep the
others' behaviour unchanged.

The Linux official host is the one entry above that compiles and passes its tests without
anyone having heard it play. Treat "written" as exactly that: the wire contract is covered by
`OfficialBridgeTests`, but nothing has yet confirmed that WebKitGTK reaches the player, so a
report that it does not work is a bug to investigate rather than a surprise. Its account
profiles are not isolated either — `bind(profile:)` says so out loud instead of pretending,
because a caller that believes it is playing under an account would be playing under guest
storage.

Two portability rules bite far from their cause: `URLSession` needs a
`#if canImport(FoundationNetworking)` import off Darwin, and swift-corelibs-xctest aborts the
whole run on a `@MainActor`-isolated `XCTestCase` subclass — put the isolation on the
individual test method instead.

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
