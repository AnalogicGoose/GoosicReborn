# Branching model

GoosicReborn is one product built for three platforms out of one repository. Most of the
code — every Rust crate, the protocol, and the majority of the Swift shell — is shared, and
only a thin layer at the edge is platform-specific: the playback hosts, the system media
controls, the window material, and the WebKit profile plumbing. The branching model exists
to keep that shared core from being forked three ways by accident.

## The five long-lived branches

| Branch | Role | Receives from | Never receives from |
| --- | --- | --- | --- |
| `main` | Deployment. What has shipped. | `development`, at a deployment | Anything else |
| `development` | Integration. The working trunk. | Cross-platform work branches, and `platform/*` when a platform slice is done | — |
| `platform/macos` | macOS-only work | `development`, and its own work branches | Another `platform/*` |
| `platform/linux` | Linux-only work | `development`, and its own work branches | Another `platform/*` |
| `platform/windows` | Windows-only work | `development`, and its own work branches | Another `platform/*` |

`main` is not the trunk. Nothing is branched from it and nothing is merged into it except a
deployment, so its history is a list of releases rather than a list of changes. Day-to-day,
`development` is the branch that matters: it is the one that must always build and pass its
tests, because it is what every other branch is cut from and re-synced against.

The three `platform/*` branches are long-lived on purpose. A platform port is not one
change; it is a sequence of them that stays incomplete for a while, and merging half a port
into `development` would leave the trunk claiming support it does not have. They collect the
port until it is coherent, then land in one piece.

```
main            ─────────────────────────────●──────────────────────●  deployments only
                                            ╱                      ╱
development     ──●────●──────────●─────────●────────●────────────●   the trunk
                   ╲    ╲        ╱           ╲      ╱
                    ╲    ●──────●  feature/…  ╲    ╱   (cross-platform work)
                     ╲                         ╲  ╱
platform/linux        ●────●────●───────────────●     (Linux-only work)
                            ╲  ╱
                             ●●  feature/linux/…
```

## Where to cut a branch from

One question decides it: **would this change be wrong to leave out on another platform?**

If the answer is yes — the protocol, a Rust crate, a shared Swift view, a document, the build
files — it is cross-platform work. Cut from `development`, merge back to `development`. It
reaches the platform branches when they sync down.

If the answer is no, because the change only exists to satisfy one operating system's API,
cut from that `platform/<os>` branch and merge back to it.

Uncertainty resolves toward `development`. A cross-platform change that lands there is
available to all three platforms immediately; a shared change made on `platform/linux`
is invisible to the other two until that branch lands, and by then the same file has usually
been edited on `platform/macos` too. That is the one way this model produces a genuinely
painful conflict, and cutting from `development` when unsure is what avoids it.

### The rule that follows from this

**A `platform/*` branch should not be the first place a shared file changes.** If Linux work
needs a new field in `goosic-protocol` or a change in `goosic-core`, that need is not
Linux-specific — the other platforms will want the same field. Make that change on a branch
off `development`, land it, sync it down, and then build the Linux-only part on top of it.

The practical test is the diff: if a `platform/linux` branch is touching `crates/`, stop and
ask whether that hunk belongs on `development` instead. It usually does. Files under
`crates/` that are genuinely platform-gated (a `#[cfg(target_os = ...)]` block) are the
exception, not the pattern.

## Work-branch names

Long-lived branches are the five above. Everything else is short-lived, named for what it
does, and deleted after it lands.

| Cut from | Name | Example |
| --- | --- | --- |
| `development` | `feature/<slug>` | `feature/authenticated-catalog` |
| `development` | `fix/<slug>` | `fix/queue-drops-last-track` |
| `platform/<os>` | `feature/<os>/<slug>` | `feature/linux/playback-hosts` |
| `platform/<os>` | `fix/<os>/<slug>` | `fix/macos/material-on-sonoma` |
| `main` | `hotfix/<slug>` | `hotfix/service-spawn-path` |

The `<os>` segment is not decoration. It is what tells a reader — and a merge — that the
branch is aimed at `platform/<os>` and not at the trunk, at the moment when the two look
alike from the outside.

`<os>` is exactly `macos`, `linux`, or `windows`: the same spelling as the platform branch,
lowercase, no version numbers.

## Merge direction

Merges go two ways, and only these ways:

**Down, constantly.** `development` → each `platform/*`. Do this whenever `development`
moves, not when the port is finished. A platform branch that has not seen the trunk in three
weeks is a merge conflict being saved up. This is a merge, never a rebase: the platform
branches are shared, and rewriting their history breaks everyone else's copy.

**Up, when coherent.** `platform/*` → `development`, once the slice is complete enough that
the trunk can honestly claim it. Not per commit — per feature. `development` → `main`, only
at a deployment.

Sideways merges — `platform/linux` into `platform/macos` — are never correct. The two share
nothing that `development` does not already carry, and a sideways merge drags one platform's
half-finished port into the other's history. If both platforms need the same thing, that
thing belongs on `development`.

## What CI checks

`.github/workflows/ci.yml` runs on every push and pull request, and it exists because of a
specific failure mode this repository has: a change can be correct on the platform you are
sitting at and broken on another, and nobody finds out until someone with that machine is
free. The Swift 6 language mode landed that way — two errors that only a Mac could see, each
costing a round trip through a colleague.

Three jobs. The Rust workspace builds and tests on Linux, macOS, and Windows, because it is
portable by construction and there is no excuse for it to break anywhere. The GTK 4 shell
builds and tests on Linux. The AppKit shell builds and tests on macOS.

Windows is marked `continue-on-error`. The shell has never been built there and the hosts are
stubs, so the job reports the state without blocking a merge on work nobody has started. When
`platform/windows` has real work, that flag comes off.

The rule this makes enforceable is the one `development` already had: it must always build
and pass. Before it was an intention; now the trunk is red or green in public.

## Automatic propagation

Merges *down* are mechanical and were being done by hand, which is how `platform/macos` and
`platform/windows` once sat three weeks behind the trunk. A workflow does them now. What is
never automatic is promotion — nothing is merged *toward* `main`, because deciding that
something is ready to ship is a judgement, not a chore.

The cascade follows the branch names, which is what makes it need no configuration:

| A push lands on | It is merged into |
| --- | --- |
| `main` | `development` |
| `development` | `platform/macos`, `platform/linux`, `platform/windows`, and every `feature/<slug>` / `fix/<slug>` |
| `platform/<os>` | every `feature/<os>/<slug>` and `fix/<os>/<slug>` |

Each step triggers the next, so a hotfix reaches every branch in the repository through one
push and no further instructions.

`main` is in that table because of hotfixes. A hotfix is cut from `main`, fixed, and merged
back to `main` — and at that moment the trunk does not have it. Merging `main` into
`development` by hand is the step everyone forgets, and forgetting it is expensive in a
specific way: nothing breaks, the fix simply disappears at the next deployment, when
`development` overwrites `main` with a history that never learned about it. The bug returns
and looks new.

After an ordinary deployment the same merge is a no-op — `main` holds nothing `development`
lacks — so it costs nothing and stays silent. The step only does work in the case where
skipping it would quietly lose a fix.

This is the point where the naming convention stops being documentation and starts being
executable. A branch called `fix/macos/thing` is a child of `platform/macos`; one called
`fix/macos-thing` is a child of `development`, and the hyphen is the whole difference. Name a
branch wrong and the robot syncs it from the wrong parent.

Three rules keep the cascade from becoming noise or damage:

**A conflict opens a pull request; it never forces anything.** A merge between a platform port
and a change to the core is exactly the kind of thing a person has to resolve, and a robot
that guesses is worse than one that stops. There is no `push --force` anywhere in this: a
platform branch's own installers, scripts, and configuration survive by construction.

**Branches already contained in their parent are skipped.** A finished work branch still exists
until someone deletes it, and merging into it produces a commit nobody will ever read and a CI
run nobody will read either.

**Stale branches are skipped.** A branch with no activity for weeks is not waiting for updates.

The propagated push is a normal push, so CI runs on the result — and it should, because merging
a parent into a branch that has its own work produces a tree that has never been compiled. That
is not waste: it is the check that tells you your in-progress branch still builds against the
trunk. What the tree-hash skip removes is the *other* case, where the merge changed nothing.

## Skipping work that is provably identical

CI compares the git tree hash of what it is about to build against the trees that have already
passed. If it matches, the jobs are skipped; the bytes are identical, so the answer is known.

The hash is of the *content*, not of where it came from, and that distinction is the whole
design. A merge result is not the branch that was tested: it is that branch combined with
whatever the target had gained meanwhile, and that combination may never have been compiled.
Trusting a green light because of a branch's ancestry would skip exactly the runs worth doing.

The repository has already produced both cases in one afternoon. Four branches — a fix, the
trunk, and two platform branches — ended up sharing one tree hash and were compiled four
times. In the same batch, a merge into the Linux playback work produced a new tree: the first
that combined WebKitGTK, the shared bridge, and the Swift 6 mode. Content hashing skips the
first three and builds the fourth.

## Deployment

A deployment is `development` → `main`, and nothing else reaches `main`. Before it:
`make test` passes, the three platform branches have been synced down and none is mid-port
in a way the release claims otherwise, and the README's "What works today" and "Migration
phases" describe what actually ships. Tag the merge commit on `main` with the version.

A hotfix is the single exception to "nothing is cut from `main`": cut `hotfix/<slug>` from
`main`, fix the one thing, merge it to `main`, and then merge `main` back into
`development` immediately so the fix is not lost at the next deployment.

## What this looks like in practice

Porting the macOS playback hosts to Linux is Linux-only work: the WebKitGTK host and the
local renderer exist to satisfy one platform's API. It is cut from `platform/linux` as
`feature/linux/playback-hosts` and merges back there. When the Linux shell can actually play
sound, `platform/linux` merges into `development`, and the README stops saying audio is
macOS-only.

Authenticated catalog reads are the opposite: `goosic-catalog` is shared and every platform
needs them. That is `feature/authenticated-catalog` off `development`, and the platform
branches pick it up when they sync down.
