# Releasing

There are two kinds of build here, and only one of them is a deployment.

A **deployment** is `development` → `main`, described in [BRANCHING.md](BRANCHING.md). It is what
"shipped" means, and it happens when the trunk is coherent on every platform it claims.

A **test build** is a pre-release download for platform acceptance. A deployment can contain a
stable Windows build and an explicitly labeled macOS alpha without claiming Linux acceptance.
The native shells are now integrated on development; releases no longer tag a platform branch.

## Making one

A release is three commands and one click, run from any clean checkout on the release Mac. Each
command can be repeated: it finds what an earlier run already did and carries on, so a run cut
short by a failed check or a closed laptop is resumed by running it again.

1. **Write `docs/RELEASE_<x.y.z>.md`**, in the style described in AGENTS.md. It can sit
   uncommitted in the checkout; the next step commits it.
2. **`sh tools/release.sh <x.y.z>`** freezes the version, promotes the platform branches, opens
   the deploy pull request and asks Codex to review it. In order: it cuts `release/<x.y.z>` from
   development, runs `tools/bump-version.sh` there and adds the notes; it opens a promotion pull
   request for each of `platform/macos` and `platform/windows` that holds work development lacks;
   it waits for CI on all of them at once and merges each when its checks pass; then it opens
   the deploy pull request from development into main and comments `@codex review` on it.
3. **`sh tools/release.sh ship <x.y.z>`**, once Codex's findings are dealt with, merges the deploy
   pull request when its CI is green. The release workflow sees a version on main that has no tag,
   tags the merge `v<x.y.z>` itself, tests the tagged tree on Windows and macOS, builds the
   Windows installer and portable ZIP and the universal macOS app, verifies the Mac bundle's
   signature and architectures, and attaches both checksum files to a draft release.
4. **Inspect the draft and publish it.** This stays a person's click on purpose: publication makes
   the Windows updater offer the version, and nothing undoes that quietly. The macOS download
   remains an alpha until native acceptance is complete.
5. **`sh tools/release.sh feed <x.y.z>`** puts the release in front of installed Macs. Publishing
   does not reach a Mac: every installed copy reads `updates/macos-alpha.xml` from `development`.
   The command downloads the published ZIP, reads the app's `CFBundleVersion`, signs the ZIP with
   Sparkle's `sign_update` and the private key in this Mac's keychain, adds an item to the top of
   the feed, refuses if its build is not above the newest one there, and merges it into
   `development` when CI is green.

The script merges only after every check on a pull request has passed, and waits for them
itself. The branches have no required checks, so `gh pr merge --auto` would merge at once rather
than wait, which is why it is not part of the release. It works in a temporary worktree and never
touches the checkout it is started from.

Why each step is where it is. The version is frozen before anything is promoted: 0.3.0 was
renamed after its deploy pull request had opened, and the rename cost a second full round of CI.
`tools/bump-version.sh` moves the workspace version in `Cargo.toml` and the workspace's entries in
`Cargo.lock`, and the Windows and macOS packaging defaults; the release workflow reads the tag
and the notes file from the version, so it has nothing to edit. The bump leaves the README's prose
and the protocol's own version, which shares numbers with a release but is not one, and lists
every line that still mentions the old version so a missed default is visible. Tagging in the
workflow rather than by hand means a deployment cannot be merged and then forgotten untagged, and
a push to main that is not a release — the CI workflow kept current there, a hotfix that did not
move the version — finds its version already tagged and builds nothing. A tag pushed by hand
still builds, for the rare release made another way.

The feed item's `sparkle:version` must equal the built app's `CFBundleVersion`, and
`sparkle:shortVersionString` the release version. Sparkle decides by the build number alone: an
item whose `sparkle:version` is higher than the app inside it makes every Mac download the update,
install it, and be offered it again. Skipping the feed looks like nothing at all: it stayed at
0.2.4 through later releases, and every Mac went on reporting 0.2.4 as the newest version, even
one already running something newer. Because the feed is read live, a correction reaches copies
already installed without anyone reinstalling. Without the private signing key, publish the manual
download and leave the signed feed unchanged. Never replace the key merely to make CI able to
sign.

The two halves are built by scripts that also work locally, which is how they are debugged:

- `sh tools/package-macos.sh [version]` — builds `Goosic.app` for Apple silicon and Intel in one
  binary, with the service in `Contents/MacOS` beside the shell and the resource bundle in
  `Contents/Resources`, then zips it. It compiles the app icon from `apps/goosic-swift/Goosic.icon`
  with `actool`, which comes with Xcode 26 or later and not with the Command Line Tools, so select
  that Xcode first. The bundle carries `Assets.car`, from which macOS 26 draws the Dark, Clear and
  Tinted icons, and `Goosic.icns` for earlier releases. Not every Xcode 26 compiles that icon:
  on GitHub's runners `actool` from 26.4 through 26.6 throws on it ("Could not open Goosic.icon")
  while 26.0 to 26.3 and Xcode 27 succeed, so the release workflow pins Xcode 26.3 rather than
  taking the latest stable one. Try a newer Xcode against the icon before moving the pin.
- `.\apps\goosic-windows\package.ps1 [-Version …]` — publishes the WinUI shell with .NET and the
  Windows App SDK inside the folder, puts the release service and the rules library beside the
  executable, and zips it. The folder needs nothing installed: WebView2 is part of Windows.

Each carries its own copy of the service because that is how the shells find it. The macOS shell
looks beside its executable when `GOOSIC_SERVICE_PATH` is unset, which is the case for anything
opened from the Finder; the Windows shell has always looked beside its own executable first.

## Stable and experimental releases

Every copy of Goosic follows one of two update channels, chosen in Settings and saved by the
service with the other preferences: **stable**, the default, and **experimental**. An
experimental release is an ordinary release marked as a GitHub pre-release. It keeps a plain
`x.y.z` version, so the numbers simply rise across both channels and a stable listener never sees
the ones spent on experiments. A suffixed name such as `0.5.0-beta.1` was the alternative and was
refused on purpose: the macOS build number is derived from the version, the Windows Setup file is
named after it, and every installed copy compares versions as plain triples, so a suffix would
have needed a new rule in each of those places and in every copy already installed.

To make one, start `docs/RELEASE_<x.y.z>.md` with the line `<!-- channel: experimental -->`. GitHub
does not render the comment. The release workflow reads it, creates the draft as a pre-release,
and leaves the last stable release marked as latest. After publication, `tools/release.sh feed`
sees the pre-release flag and gives the Mac feed item `<sparkle:channel>experimental</sparkle:channel>`.

Which release a copy is offered is one rule, `goosic_shell_support::updates`, asked by both shells.
Windows reads the release list and lets `choose_release` pick the newest release its channel
allows; macOS tells Sparkle which channels it may install from through `sparkle_channels`, and
Sparkle skips any item whose channel is not allowed. (0.4.6's Mac app does not link the rules
library yet, so it keeps a one-line Swift copy of `sparkle_channels` held to the same cases; the
release after it asks Rust.) Copies older than 0.4.6 allow no channel, read
`releases/latest`, and so stay on stable without any change. Leaving the experimental channel never
downgrades: the copy keeps what it has until a stable release passes it.

## The macOS build number

Sparkle offers an update only when its `sparkle:version` is higher than the installed app's
`CFBundleVersion`, so that number has to rise with every release. It used to be typed into the
release workflow, and nobody raised it after 0.2.9: 0.2.10 and 0.3.0 both came out of CI as
build 7, the same as 0.2.9, so the feed could not offer 0.3.0 until its Mac ZIP was rebuilt by
hand from the tag as build 8 and replaced on the release.

`tools/package-macos.sh` now derives it from the version as MAJOR × 10000 + MINOR × 100 + PATCH,
so 0.3.1 is build 301 and 1.0.0 is build 10000. A higher version always gives a higher build, the
same tag always gives the same build, so a retried workflow run produces an identical app, and
every derived number is above the hand-numbered builds 1 to 8 already in the feed. The scheme
holds while minor and patch stay below 100, and the script refuses a version that breaks that
rather than producing a number that could go backwards. A commit count was the alternative, but
it depends on how much history a checkout has and says nothing a person can read back to a
release.

The release workflow then compares the packaged app's `CFBundleVersion` with the highest
`sparkle:version` in `updates/macos-alpha.xml` and fails if it is not above it, so a collision
stops the run instead of surfacing after publication. `GOOSIC_MACOS_BUILD_VERSION` still
overrides the derivation for the one case it cannot cover: two macOS builds of the same version,
such as a second `-macos-alpha` of one base version, where the later one needs a number above the
derived one.

## Codex reviews the release

Codex reviews a release, not every pull request. Its automatic reviews are switched off for this
repository in the Codex settings (chatgpt.com/codex, Settings → Code review), so feature, fix and
promotion pull requests merge on CI and a person's reading alone. That setting belongs to each
Codex account, not to the repository: anyone whose account is connected to it switches it off in
their own settings, or Codex keeps reviewing every pull request on their behalf. A review on each of those was
the same code read several times on its way to `main` — once in the feature, again in each
promotion — and its findings landed mid-chain, where every fix reopened a pull request and cost
another round of CI.

The deploy pull request is where it reads instead. Its diff, `main...development`, is exactly
what changed since the last release, every platform together, so one review sees the whole of
what is about to ship and nothing that already has. `tools/release.sh` asks for the review when it
opens the deploy pull request, by commenting `@codex review` through `gh`, which posts as the
person running it. It has to come from a person's account linked to Codex: a comment written by a
workflow is not acted on, which is why this runs from the release Mac and not in CI. Run `ship`
only once the review has arrived. A
finding that blocks the release — it breaks what the release notes promise, loses data, or
crosses one of the invariants in AGENTS.md — is fixed on a `fix/` branch into `development`,
which updates the open deploy pull request; ask for another review of the new commits if the fix
is not small. Any other finding goes into a `fix/` branch for the next version, as the next
section says, rather than holding the release.

A pull request that needs a second opinion before then can still ask for one with the same
comment. That is a choice made for that change, not the routine.

## Keeping the release moving

0.3.0 took about an hour and fifty minutes from start to draft, and almost none of that was
work: it was CI running one pull request after another, and green pull requests sitting unmerged
until someone looked. `tools/release.sh` removes both. The freeze and the promotions run their CI
at the same time instead of in a chain, and each merges the moment its checks pass rather than
when someone next looks. The deploy pull request carries the same tree development already
tested, so CI there mostly finds a tree that has passed and skips the work.

Treat a review finding as the next patch unless it blocks the release. A finding on the deploy
pull request, or one that arrives after it is green, that does not break what the release notes
promise goes into a `fix/` branch for the following version. Reopening the deployment for it
costs another full round of CI on every pull request in the chain, which is what added two
rounds to 0.3.0.

## What a tester has to click

Neither build is signed with a paid certificate, so each system asks once. This is worth saying
plainly in the message that goes with the download, because an unexplained "damaged" dialog reads
as a broken app rather than an unsigned one.

**macOS** signs the app ad hoc. The first open is refused; the person then goes to System Settings
→ Privacy & Security, where the refusal is listed, and presses Open Anyway. Dragging the app to
Applications first keeps it out of the quarantine that a Downloads folder applies on every launch.

**Windows** shows SmartScreen's blue "Windows protected your PC" box on the first run: More info →
Run anyway. Unzip the folder somewhere permanent before running it, because Windows runs a program
from inside a zip in a temporary copy that is thrown away, taking the account's sign-in with it.

## What 1.0 requires

A 0.x number tells a person that things may still move under them. 1.0 withdraws that warning,
so it is earned by evidence that the warning is no longer needed, not by a date or a feature
count. Green CI is necessary and nowhere near sufficient: CI proves the code compiles and its
rules hold, and every gate below is something CI cannot see.

**Every platform the release names has passed acceptance.** For macOS that means the acceptance
matrix in [UI_DESIGN.md](UI_DESIGN.md) inspected on a packaged build, after which the download
stops being called an alpha. Linux is either accepted, which means the open rows in
[LINUX_SHELL.md](LINUX_SHELL.md) that belong to "a person" are closed and the release workflow
builds the Flatpak, or the 1.0 notes say plainly that Linux is not part of it. Shipping 1.0 with
Linux left out is honest; implying it is included is not. The rule is the one a deployment
already follows: a release claims only the platforms it has evidence for.

**No known bug is open against a platform the release names.** An issue labelled `bug` is either
fixed on `development`, or moved to a later milestone with a sentence in the release notes
saying what it does and how to avoid it. A fix sitting in an unmerged pull request counts as
open.

**The trunk has been quiet.** The last 0.x release before 1.0 is used for two weeks without a
new `bug` issue against a named platform. A new bug restarts the period after its fix lands.
Releases a few days apart are the sign this has not happened yet, because each one is a
correction of the last.

**An existing installation updates to it.** On a machine running the previous release, the
Windows updater offers 1.0 and installs it, and Sparkle does the same on a Mac, with the account
still signed in afterwards. This is checked on real installs, because the Mac half depends on
the hand-signed feed described above, which nothing in CI exercises. The feed is named
`macos-alpha.xml` and every installed copy reads it by that path, so dropping the alpha label
must not rename or move it: a copy that cannot find its feed never hears of another release.
If a new feed name is wanted, a release first ships the new address and the old feed keeps
pointing at that release.

**Signing is a recorded decision.** Either the builds are signed with paid certificates, so
neither Gatekeeper nor SmartScreen interrupts the first run, or 1.0 ships unsigned and its notes
carry the instructions under *What a tester has to click*. Both are defensible; finding out from
a user's "damaged app" report is not.

**One version everywhere.** The tag, the GitHub release, the version the app reports, and the
workspace `version` in `Cargo.toml` all read `1.0.0`, and the notes live in `RELEASE_1.0.0.md`
beside the earlier ones.

Not every piece of open work is a gate. Moving the macOS and Windows shells onto
`goosic-shell-support` instead of their own copies of the shared rules, and the features under
*Worth doing next* in [FEATURE_PARITY.md](FEATURE_PARITY.md), can follow 1.0, because their absence
does not make the app behave differently from what its notes promise.

## Version names

`v0.1.0-alpha.N` while the platforms are still being finished. The name is the tag, the release,
and what the app reports; keep them the same so a bug report names something findable.

Platform alphas can move ahead of the stable Windows release. The September 30 macOS build is
`v0.2.4-macos-alpha.1`, with notes in [RELEASE_0.2.4_MACOS_ALPHA_1.md](RELEASE_0.2.4_MACOS_ALPHA_1.md).
It is a test build from the source snapshot containing the native macOS changes, not a
deployment to main. Tags containing `-macos-` deliberately do not run the combined release
workflow: build and verify the universal ZIP locally, publish its SHA-256 in
`SHA256SUMS-macos.txt`, then create a GitHub pre-release from the exact source tag. Leave the
stable Windows release as latest so its updater continues to find a Windows installer.

For a local universal build, install both Rust Apple targets and use the selected Xcode's SDK
and compiler. Set `MACOSX_DEPLOYMENT_TARGET=14.0` for the Rust service as well as the shell;
otherwise the service can inherit a newer build-host minimum. Assemble the app outside a
file-provider directory and inspect an extracted copy of the final ZIP; Finder metadata can
invalidate signatures after an earlier signing check. Verify both architectures,
bundle contents, minimum deployment versions and code signing before uploading. Sparkle is
available only when the bundle contains a valid feed URL and public key; a bundled framework
alone does not enable updates. For this alpha, use `GOOSIC_MACOS_BUILD_VERSION=2`; the first
alpha used build 1, so resetting the build number to `0.2.4` would prevent Sparkle from offering
the update. Sign the final ZIP using Sparkle's Keychain-backed `sign_update`, verify it against
the bundled public key, and update `updates/macos-alpha.xml` on development only after the
release assets are public. Never put a private signing key in source, output or the protocol.

A release website should list one entry per base product version and offer macOS and Windows
choices inside it. Preserve each platform's original tag, channel, notes and download URL;
grouping a macOS alpha with a Windows stable release must not describe both as stable or claim
that either received the other's changes. On GitHub, use a short product-version title and
platform sections/download links inside its notes; GitHub's sidebar does not expose custom
platform tabs. Attach later platform builds to the existing version entry rather than creating
another platform-named entry. Preserve platform source tags separately. A version without a platform build should show its
actual availability rather than borrowing an older download under the new version.
