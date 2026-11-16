# Lyrics motion handoff

This records where the macOS lyric motion work stood on November 16, 2026, for whoever picks it
up next. The behavior itself is specified in [LYRICS_TIMING.md](LYRICS_TIMING.md); this file is
only the state of the work and what is still open. Delete it once those items are closed.

## Where the work is

Everything is on `feature/macos/lyrics-motion`, cut from `platform/macos`, in two commits. The
first changes shared code: an optional `endMs` on a lyric line in `goosic-protocol`, carried
from the Lyricsfile parser in `goosic-lyrics`. The second is the macOS presentation in
`NativeMacLyricsView.swift`, `NativeMacFullPlayer.swift` and `Core/LyricsTiming.swift`, with its
tests and documents.

The branch rules in `AGENTS.md` say a shared file should not first change on a platform branch.
The two commits are separate so the first can be cherry-picked onto a branch cut from
`development`, merged there, and synced down before the macOS commit is proposed against
`platform/macos`. That split has not been done. No pull request is open.

## What was verified and what was not

`make test` passed with 273 Swift tests and 317 Rust tests. On this Mac `make` needs
`DEVELOPER_DIR=/Applications/Xcode-beta.app/Contents/Developer`, because the Command Line Tools
cannot load the `#Preview` macro.

The owner watched each build and reported on it; the assistant never saw the app on screen.
The owner confirmed that the word sweep, the interlude dots, the cover shrinking on pause and
the richer backdrop behave as wanted, that the flicker on line changes is gone, and that the
highlight no longer drops back between lines. The last change, each row moving on its own
spring with a 50 ms delay per line, was built and launched but the owner had not yet reported
on it.

## Open items

The per-row motion scrolls the column at once and holds every row back by the distance
scrolled, then releases each row on its own spring. If the scroll and the hold-back ever land
in different frames, a line change shows a one-frame flash. If the owner reports that, replace
the scroll view's following with a column positioned by offsets alone.

The interlude dots keep their row's height when inactive. Music folds the row away and the
lyrics slide up. Folding was removed because it fought the scroll; with per-row motion in place
it may be safe to bring back.

Music's lyric text is larger relative to its window than ours. The owner was told and has not
asked for it.

The exact LRCLIB lookup wins even when it returns a line-only record and a word-timed record
exists for the same recording. Preferring the word-timed one is a change in `goosic-lyrics` on
`development`.

A start-only word is swept until the next word in its line begins. That reverses the earlier
rule against inferring a word's end, on the owner's instruction to match Music; the limits are
in LYRICS_TIMING.md under "Word motion on macOS". The owner was asked to confirm and did not
answer directly.

Windows drops `words` and `endMs` at its DTO in `Service/CatalogModels.cs` and highlights whole
lines only. Linux draws lines only as well, and its tests build `LyricsLine` values by hand, so
they need `end_ms: None` when that shell takes this protocol version. Neither has a `parity`
issue yet. The sweep span, the rest rule and the row timings are rules, so they belong in
`goosic-shell-support` before a second shell draws them.

## How the motion was measured

The row timings came from reading a screen recording of Music frame by frame: the mean
brightness of each pixel row across the lyric column, per frame, gives an image of position
against time in which every line is a band, and tracking each band through a line change gives
its start and its curve. The recordings were the owner's and are not in the repository.
