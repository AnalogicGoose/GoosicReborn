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

`.github/workflows/release.yml` runs from the tagged commit, builds both downloads, and collects
them into a **draft** release. Nothing is public until a person opens the draft and presses
publish. That gate is deliberate: everything else in this repository stays inside it, and a
download does not.

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
