# Branching

| Branch | Purpose |
| --- | --- |
| `development` | Integrates shared code, documentation, macOS and Windows. |
| `main` | Holds deployed versions and remains the default branch. |
| `platform/linux` | Integrates the Linux alpha while it develops separately. |

Start a short-lived `feature/<slug>` or `fix/<slug>` from `development` and open a pull
request back to it. A change that touches shared playback rules and both native shells can
land in one pull request. Cut Linux-only work from `platform/linux` and merge it back there.
Changes to shared contracts go through `development` first.

Update a work branch when its pull request needs new dependencies or has a conflict. Linux
maintainers merge `development` into `platform/linux` when they need upstream changes. Keep
long-lived branch history intact.

## CI

Work branches run CI on pull requests. Push checks run on `development`, `main` and
`platform/linux`, where the integrated trees live. A newer run for the same ref cancels the
older one.

CI tests GitHub's proposed merge. It compares that tree with the PR base and selects suites
from the changed paths. Documentation-only changes skip app builds. A change confined to
one shell runs that shell's checks, except the personal catalog reader runs both macOS and
Windows because both ship it. Changes to shared shell rules or the protocol run the Rust
workspace and all applicable shell suites. When the comparison baseline is unavailable, CI
runs all applicable suites.

The Rust matrix tests on Linux, macOS and Windows. macOS builds and tests Swift against
AppKit. Windows runs its presentation tests and builds WinUI. Where the GTK source is
present, Linux checks the native shell and the Flatpak. GitHub records a `ci-passed/` tag
when CI ran the complete set of applicable build suites and the workflow's success criteria
were met. A later run for that exact Git tree can reuse the result. A full run is always
available through manual dispatch.

The default branch writes shared build caches and runs weekly to refresh them. Feature
branches and pull requests restore those caches. Scheduled and manually dispatched builds
ignore passing-tree tags so the scheduled run can refresh the caches.

## Deployment

Deploy through a pull request from `development` into `main`. Use
[RELEASING.md](RELEASING.md) to freeze the version, request the release review and build
the draft. An urgent production fix can use a `hotfix/<slug>` branch from `main`. Merge the
fix into `main`, then bring it into `development` before the next deployment.

## Pruning

The weekly pruning workflow deletes finished work branches after a week when their exact
commits are already in `development`, `main` or `platform/linux`. Both ends of open pull
requests are protected. Manual runs default to a dry run and show the candidate branch
names and commit IDs.
