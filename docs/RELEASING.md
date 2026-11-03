# Releasing

There are two kinds of build here, and only one of them is a deployment.

A **deployment** is `development` → `main`, described in [BRANCHING.md](BRANCHING.md). It is what
"shipped" means, and it happens when the trunk is coherent on every platform it claims.

A **test build** is a pre-release download for platform acceptance. A deployment can contain a
stable Windows build and an explicitly labeled macOS alpha without claiming Linux acceptance.
The native shells are now integrated on development; releases no longer tag a platform branch.

## Making one

Merge the coherent platform changes into development, run the required tests, and deploy
by merging development into main. Tag that deployment commit with a numeric version such as
`v0.2.10` and push the tag. The release workflow tests the tagged tree on Windows and macOS,
builds the Windows installer and portable ZIP and the universal macOS app, verifies the Mac
bundle signature and architectures, and attaches both checksum files to a draft release.

Inspect the downloads and their hashes before publishing the draft. Publication makes the
Windows updater offer the version. The macOS download remains an alpha until native acceptance
is complete.

Publishing does not reach a Mac. Every installed copy reads `updates/macos-alpha.xml` from
`development`, and CI cannot sign that feed, so a release is not finished until someone on the
release Mac has run Sparkle's `sign_update` on the published ZIP, added an item with a higher
`sparkle:version` to the top of the feed, and landed it on `development`. Skipping this looks like
nothing at all: the feed stayed at 0.2.4 through later releases, and every Mac went on reporting
0.2.4 as the newest version, even one already running something newer. Because the feed is read live, the
correction reaches copies already installed without anyone reinstalling. Without the private
signing key, publish the manual download and leave the signed feed unchanged. Never replace the
key merely to make CI able to sign.

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
