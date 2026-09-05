# CLAUDE.md

The instructions for working in this repository live in **[AGENTS.md](AGENTS.md)**. Read it
before making a change; it is short, and everything in it is load-bearing.

@AGENTS.md

It is kept in `AGENTS.md` rather than here so that every assistant reads the same file. Two
copies would drift, and the half that drifts is always the one someone forgot to update.

If you are about to write code, three things from that file decide what you do before you
touch anything:

1. **Which branch this belongs on.** Ask whether the change would be wrong to leave out on
   another platform. Yes means it is cross-platform: cut from `development`. No, because it
   only exists to satisfy one operating system's API, means it belongs on that
   `platform/<os>`. When unsure, choose `development`.
2. **What is not yours to change.** `goosic-core` is the single playback authority, the
   service's stdout is protocol-only, catalog reads are anonymous, credentials never cross
   the protocol, advertisements are reported and never bypassed, and no GPL source is copied
   here.
3. **`make test` must pass**, and CI repeats it on Linux, macOS, and Windows. Code behind a
   `#if` is invisible to the compiler you are running, so a green local build proves less
   than it looks.
