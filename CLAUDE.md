# CLAUDE.md

The instructions for working in this repository live in **[AGENTS.md](AGENTS.md)**. Read it
before making a change; it is short, and everything in it is load-bearing.

@AGENTS.md

It is kept in `AGENTS.md` rather than here so that every assistant reads the same file. Two
copies would drift, and the half that drifts is always the one someone forgot to update.

If you are about to write code, these things from that file decide what you do before you
touch anything:

1. **Which branch this belongs on.** Ask whether the change would be wrong to leave out on
   another platform. Yes means it is cross-platform: cut from `development`. No, because it
   only exists to satisfy one operating system's API, means it belongs on that
   `platform/<os>`. When unsure, choose `development`.
2. **What is not yours to change.** `goosic-core` is the single playback authority, the
   service's stdout is protocol-only, catalog reads are anonymous, credentials never cross
   the protocol, advertisements are reported and never bypassed, and code ported from the old
   Goosic keeps its GPL notice.
3. **Tidy the clone when the branch list gets noisy.** Merges propagate automatically and
   finished branches are pruned from the remote, so local branches whose upstream is gone pile
   up. `AGENTS.md` has the one-liner; it uses `git branch -d`, which refuses to delete
   anything unmerged.
4. **No job is done on one platform.** A change a person can see or hear is finished only
   when both macOS and Windows have it: the shared rule on `development`, then each one's part
   on its own `platform/<os>` branch. The only exemption is a change that exists solely for one
   operating system's API. Linux is out of scope for now and is not ported to unless the user
   asks. Section 8 of `AGENTS.md` has the steps.
5. **`make test` must pass**, and CI repeats it on Linux, macOS, and Windows. Code behind a
   `#if` is invisible to the compiler you are running, so a green local build proves less
   than it looks.
