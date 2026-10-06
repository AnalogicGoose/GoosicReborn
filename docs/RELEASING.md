# Releasing

There are two kinds of build here, and only one of them is a deployment.

A **deployment** is `development` → `main`, described in [BRANCHING.md](BRANCHING.md). It is what
"shipped" means, and it happens when the trunk is coherent on every platform it claims.

A **test build** is what this document is about: a download handed to a few people so they can use
Goosic on their own machine and say what breaks. It is cut from wherever the code actually is,
which today is not the trunk — the WinUI shell lives on `platform/windows` until the port is
coherent enough to land, so a build that includes Windows is tagged there. That is the honest
shape of a pre-1.0 project, and pretending otherwise by rushing a half-port onto the trunk is the
thing the branching model exists to prevent.

## Making one

Tag the ref that has everything the build claims, and push the tag:

```sh
git tag -a v0.1.0-alpha.2 -m v0.1.0-alpha.2
git push origin v0.1.0-alpha.2
```

`.github/workflows/release.yml` runs from the tagged commit, builds both downloads, and publishes
them as a pre-release. The repository is public, so the release is: the link works for anyone who
finds it, not only the people it is sent to. Pushing the tag is therefore the decision to hand the
build out, and it is the only step here that cannot be taken back -- a deleted release does not
unsend a download someone already has.

The two halves are built by scripts that also work locally, which is how they are debugged:

- `sh tools/package-macos.sh [version]` — builds `Goosic.app` for Apple silicon and Intel in one
  binary, with the service in `Contents/MacOS` beside the shell and the resource bundle in
  `Contents/Resources`, then zips it.
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
