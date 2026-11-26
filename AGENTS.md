# AGENTS.md

Instructions for AI coding agents working in this repository. Read this before making a
change; it is short because everything in it is load-bearing.

GoosicReborn is a native rewrite of Goosic: a Rust playback authority with SwiftUI/AppKit
on macOS, GTK 4 in Rust on Linux, and WinUI 3 on Windows. SwiftCrossUI is removed. Rust decides
what is allowed; each native shell renders and asks. See [docs/NATIVE_SHELL_MIGRATION.md](docs/NATIVE_SHELL_MIGRATION.md)
and [docs/LINUX_SHELL.md](docs/LINUX_SHELL.md).

## 1. Check what branch you are on first

See [docs/BRANCHING.md](docs/BRANCHING.md) for the branch model.

- `development` is the trunk for shared code, documentation, macOS and Windows. Cut a
  short-lived `feature/<slug>` or `fix/<slug>` from it and open a pull request back into it.
  A single pull request may contain both native shells and the shared code they use.
- `main` is deployment only. Deploy through a pull request from `development`. An urgent
  production fix uses a `hotfix/<slug>` from `main` and is merged back into `development`
  afterwards. The default-branch copy of `.github/workflows/ci.yml` may be kept current
  between deployments so scheduled cache builds use it.
- `platform/linux` is the Linux alpha integration branch. Linux-only work branches target
  it. Shared contracts land on `development` and Linux maintainers merge them in when needed.

Keep long-lived history intact. Update a work branch when its pull request needs it.

## 2. Commands

```sh
make test            # Rust tests everywhere + native Swift tests on macOS
make test-rust       # Rust only
make test-swift      # Swift only
make build-swift     # build the native macOS shell
make run-swift       # build the service and launch the shell against it
make test-rust-live  # opt-in; hits music.youtube.com. Do not run it in a normal check.
make package-macos   # the download a tester installs; see docs/RELEASING.md
sh tools/release.sh <x.y.z>  # a release, end to end; see docs/RELEASING.md
```

`make test` must pass before you hand work back. CI runs the affected suites for pull
requests and integration pushes. When selected, the Rust suite runs on all three operating
systems and the Windows suite tests the presentation rules and builds WinUI. Source behind a
platform-specific `#if` needs a build on that platform. See [docs/BRANCHING.md](docs/BRANCHING.md)
for CI and the README's Prerequisites for the Linux formatting and lint packages.

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
- **No downloads.** Goosic plays online only. There is no downloader, no offline library, no
  `downloads.*` command and no local-file playback owner; the legacy downloads import was removed
  in protocol 0.5.0. Do not reintroduce any of it, and never a yt-dlp or account-cookie path.
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

Those platform-neutral rules live in Rust, in `goosic-shell-support` — login navigation policy,
bridge event validation, media projection, catalog conversion, the listening queue and the rest;
the crate documentation has the table. Both shells call it through `goosic-shell-support-ffi`:
Windows loads the DLL, macOS links the static library, and the queue (`QueueEngine`) and the
player's queue copy come only from there. **A new rule goes into the crate, not into a shell.**
Rules a shell has not yet moved over keep a copy held to the same test cases, and a change to one
of those is a change to the Rust rule and every copy, in the same commit; moving the copy onto the
crate is better still.

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

The GTK shell has played official audio on a live desktop. Its packaged account and
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

Finished branches are pruned from the remote weekly, so a local clone accumulates branches
whose upstream no longer exists. Clear them when the branch list stops being readable:

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
job, and it only removes branches whose commits already live in an integration branch.

## 6. Conventions

- **Documentation is written, not suggested.** Markdown under `docs/`, the README, and this
  file are edited directly. If a code change makes a document wrong, fix the document in the
  same change.
- **Prose over bullets in `docs/`.** Existing documents explain *why* a contract exists, not
  just what it is. Match that.
- **Codex reviews releases, not pull requests.** Do not ask for `@codex review` on a feature
  or fix pull request unless the user asks. It is requested once, on the deploy pull
  request into `main`, whose diff is everything since the last release. See
  [docs/RELEASING.md](docs/RELEASING.md#codex-reviews-the-release).
- **Commit messages** state what changed and why it had to change, in the imperative. No
  co-authorship, attribution, or session trailers.
- **Release notes read like an Apple product page.** This applies to `docs/RELEASE_<version>.md`
  and the GitHub release built from it, not to commits. Open with the version as a headline and
  a one-line tagline that sums up the release. Give each change its own section: a short
  headline, a one-line tagline that says what it means for the person using Goosic, then a
  short, confident paragraph on what changed and why. Lead with the outcome, keep sentences
  short, and leave internals out of the taglines. Every claim must be true and checkable;
  the style is presentation, not hype, so never promise more than the release does, and say
  plainly what still needs a live check. The downloads and installation section stays factual.

  ```
  ## macOS: the full player slides up

  Now playing, front and center.

  The full player now slides up from the bottom edge as one piece, as Music's does, instead
  of fading and scaling, which cut off the top of the window and stuttered.
  ```
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

## 8. No job is done on one platform

Goosic is one product on three systems, and the shells keep their own copies of many rules:
Windows in `apps/goosic-windows/Goosic.Windows/Presentation`, macOS in
`apps/goosic-swift/Sources/GoosicSwift/Core`, Linux through `goosic-shell-support`. A fix made in
one copy reaches no other on its own. That is how Windows gained an audio-taper volume slider
while the Mac kept a linear one, and how a song that YouTube Music swapped before it started was
kept on Windows but skipped on the Mac for two months. So the rule is absolute: **a change a
person can see or hear is not finished until both macOS and Windows have it.** Fixing it on the
platform where it was reported and leaving the other for later is not done; it is half done.

**Linux is out of scope for now.** The GTK shell is not yet a working product, so a change does
not wait for it and an agent does not port to it unless the user asks. Bringing Linux to parity
is a later job of its own; until then, its line in the Parity section says "out of scope for
now". When the user brings Linux into scope, this paragraph is removed and the rule covers all
three shells.

1. **Find the same behavior in the other shells.** Read their code; do not assume. The paths
   in section 4 say where each shell keeps it.
2. **Decide for each one:** it already behaves this way; it needs this change; or it does not
   apply, because the change exists only to satisfy one operating system's API. A Settings
   window is a macOS convention, and Windows keeps a Settings page, so that one does not apply.
   Say which, and why. "Does not apply" is the only exemption, and only for that reason.
3. **Make the change on both macOS and Windows, in the same piece of work.** A rule (a
   curve, an order, a limit, a validation, a timing) lands in `goosic-shell-support` on
   `development`. Include both native implementations and any shared rule changes in the
   same pull request into `development` when they can be reviewed and tested together.
4. **When macOS or Windows cannot be changed now** -- it cannot be built or checked on the machine at
   hand, or the user explicitly says to stop at one -- say so before handing back, and open an
   issue labelled `parity` for it that names the files to change and what done looks like, and
   link it from the pull request. That is a recorded gap, not an exemption: the job stays open
   until the issue is closed. An agent never decides on its own that the other platform can wait.
5. **Record the answer** in the pull request's Parity section, one line per shell: changed,
   with its pull request; already behaves this way; does not apply, and why; or deferred, with
   its issue; for Linux, "out of scope for now". Leaving it blank is not an answer.
