# Lyric timing and listening effects

Goosic reads LRCLIB's `lyricsfile` response field using the
[Lyricsfile 1.0 draft specification](https://github.com/tranxuanthang/lyricsfile/blob/main/SPECIFICATION.md).
A usable Lyricsfile takes priority over legacy LRC/plain fields. Lookup and search can now
return real word or segment timing automatically when a contributor supplied it; coverage is
not universal. Search prefers usable word timing, then line timing, plain lyrics and
instrumental records. The exact recording lookup still takes priority over search.

The lyric wire format carries optional `words` arrays with absolute `atMs`, optional `endMs`
and `text`. Existing line-only responses remain compatible. Supplied start/end pairs drive a
word sweep; a start without an end activates the whole word at that timestamp. No missing end
is inferred from the next word, the next line or text length. Songs without real word timing
highlight the complete active line.

The parser accepts line-only, plain and instrumental Lyricsfiles, preserves word spacing and
Unicode text, and sorts lines stably. Equal line starts and overlapping vocals remain in the
document; the current macOS presentation highlights the most recently started line rather
than several vocalists simultaneously. Invalid word text or timing falls back to highlighting
the complete line. Structurally invalid YAML, unknown versions and nonzero `offset_ms` values
fall back to the legacy LRCLIB fields; the draft has not settled offset semantics. Parsing
rejects duplicate keys, tags, anchors and aliases, limits input to 2 MB and nesting to 16 levels,
and bounds parser events. Outgoing lyrics are bounded by both 900 lines and a 200 KiB document
budget, reserving space for the protocol envelope; any removed lines set `truncated`.

The listening presentation keeps Music-like bold active text, contextual blur and edge fading.
Context depth now depends on distance from the active line; nearby text remains clearer. Active
lines have a subtle glow and lift, and an advancing highlight follows the current segment.
Only the active line redraws at up to 30 frames per second. Its presentation position may
interpolate at most one second beyond the last displayed playback position, then freezes until
another update arrives. Pausing, advertisements, disconnection and pending playback transitions
stop that interpolation. It never writes a position, makes a seek or changes Rust authority.
Manual scrolling, hovered/focused text and increased contrast reveal readable text; reduced
motion disables the sweeping and movement, retaining a static active-line emphasis.

The macOS player's More menu and the lyrics context menu offer Import Timed Lyrics. Choose a
JSON file using the structure in [word-timed-lyrics.json](examples/word-timed-lyrics.json), with
timestamps in milliseconds from the recording's beginning. Word text must concatenate exactly
to the line text, including spaces, punctuation and Unicode text. Timing is absolute, not
relative to the line. For local JSON import, lines must be strictly ordered and words cannot overlap or run into the
next line. A missing word end is allowed and activates that word without estimating its duration. A file is limited to 2 MB, 900 lines, 512 Unicode scalars per line and 128 segments
per line. Invalid imports leave the currently displayed lyrics untouched.

Imports apply to the current listening session and are not saved to the library. The song must
remain the same while choosing a file. A late network lookup cannot replace a successful import
for that song. Imports do not modify the chosen file or audio, and do not contact another lyrics
provider. Precise timings must match this recording; files for a live version or alternate edit
will not align just because the title matches.

Swift and Rust shell-support validate the same local-import and word-progress cases. LRCLIB
Lyricsfile conversion runs in the Rust service and is shared by every shell. Linux and Windows
continue their existing line presentation; optional word fields can be ignored safely until
those native shells adopt word highlighting and local import.
The macOS visual acceptance still needs a real timed song, a line-only song, pauses and seeks,
manual scrolling, short/wrapped/Unicode lines, and the system accessibility settings. Unit tests
and compilation do not establish a screenshot match or live singing alignment.

## Decisions to preserve when adding native shells

This section is the September 30, 2026 handoff for Windows and Linux. Read
[UI_DESIGN.md](UI_DESIGN.md) before changing presentation; that document remains the shared
product design authority. Lyricsfile parsing and source selection are shared Rust service
work, not a new HTTP client to implement separately in each shell. Synchronize development
into the platform branches before native work. Changes to wire fields, shell-support rules,
FFI bindings or this documentation land on development first; native rendering, dialogs and
accessibility integrations follow `platform/windows` or `platform/linux`.

The user explicitly kept the Music-like lyric treatment after the first audit removed too
much of it. Preserve contextual blur, fading, active-line emphasis and smooth following.
The permanent Follow current line switch was removed because it displaced the immersive
viewport. Resume lyrics appears only after manual exploration. The user also rejected the
estimated word sweep: no shell may reintroduce a proportional, evenly distributed or inferred
word schedule for a line-timed song. A new visual effect does not create timing data.

Format support and catalog coverage are separate facts. On September 30, live LRCLIB checks
found no word arrays in the sampled records, including
[Risk It All, record 36872149](https://lrclib.net/api/get/36872149) and
[Out of Touch, record 36840320](https://lrclib.net/api/get/36840320). Several other queried
recordings also had only line timing. This sample is not a statement about every version or
all songs in the database. Do not advertise named songs as word-synced without inspecting
that recording's current response. Every record having a `lyricsfile` field is not evidence
that its `words` arrays are populated. Use synthetic fixtures for repeatable acceptance until
a live recording with valid word timing is verified.

## Shared data and progress contract

`lyrics.get` still returns `payload.lyrics`; adding word timing does not introduce a playback
command or change the service's single-client stdio transport. `synced` describes line timing,
not word coverage. A document can mix line-only and word-timed lines. `source` attributes the
provider and `truncated` reports an incomplete document. Absent or empty `words` means complete
active-line emphasis. Negative line times belong to unsynced text and must never seek or follow.

A word or segment is `text`, `atMs` and optional `endMs`, with milliseconds measured from the
start of the recording. Whitespace and punctuation are part of the supplied text. Concatenate
segments exactly; do not reconstruct them with inserted spaces or tokenize a line to invent
segments. Keep native shaping, wrapping, bidirectional text and grapheme boundaries intact.
Words can represent syllables or longer segments, so a renderer must not assume Latin words.
The fixture in [examples/word-timed-lyrics.json](examples/word-timed-lyrics.json) shows the wire
shape and can also be imported on macOS.

Use `goosic_shell_support::lyrics::word_progress` for Rust consumers. Its Swift copy is
`LyricsTiming.progress` and is held to the same boundary tests. With a supplied end greater
than the start, progress is `clamp((positionMs - atMs) / (endMs - atMs), 0, 1)`. Without an end,
or with a zero-duration segment, progress is zero before the start and one at/after it. A
non-finite position returns zero. Never fill a missing end from the next segment, next line or
track duration. Word progress is presentation only and does not authorize a seek.

Use `goosic_shell_support::lyrics::active_line_index` for the active-line rule. Unsynced text
and positions before the first line have no active line. Otherwise the most recently started
line is current; equal starts select the last line in the service's stable ordering. Lyricsfile
permits simultaneous vocals and overlapping words. Preserve the supplied data even though the
current macOS renderer emphasizes one line at a time. Multi-vocalist highlighting is future
work and must be described separately from ordinary word support.

The provider parser and local-import validator have different jobs. LRCLIB conversion accepts
valid overlapping timing under the draft format and degrades invalid words to line-only text.
`valid_timing` in shell-support, matching Swift `LyricsTiming.valid`, validates the stricter
local JSON import contract. Do not run that local validator over provider responses and
accidentally erase valid overlapping vocals. Preserve the service's malformed-YAML/version/
offset fallback, source ranking, byte/line bounds and truncation marker.

## Rendering and interaction reference

The following values record the current macOS implementation, not replacement native controls
or a requirement to copy SwiftUI APIs. Adapt units, font metrics and supported effects while
preserving the same hierarchy and behavior. The full player uses white text over its dark
artwork surface; the inspector emphasizes its active line with the app accent.

| Effect | macOS reference |
| --- | --- |
| Typography | Bold 30-point immersive text; bold 25-point rounded inspector text |
| Vertical spacing | 26 points immersive; 22 points inspector |
| Active-line position | Scroll anchor at 36% of the viewport height |
| Following transition | Ease in/out, 0.72 seconds immersive; 0.65 seconds inspector |
| Line emphasis transition | Ease in/out over 0.4 seconds on active-line changes |
| Context opacity | `max(0.16, base - distance * 0.04)`, base 0.36 immersive / 0.34 inspector |
| Context blur | `min(3, base + distance * 0.35)`, base 1.1 immersive / 0.8 inspector |
| Active glow | White at 0.2 opacity, radius 8 immersive / 5 inspector |
| Active lift and context scale | Active lift of 2 points; context scale 0.97 with a leading anchor |
| Viewport fades | Clear at edges; opaque from 14–84% immersive / 10–86% inspector |
| Word fill | Existing glyphs brighten from 0.55 to 1.0 opacity using supplied segment progress |
| Drawing cadence | Only the active word-timed line updates, capped at 30 frames per second |

Distance is the absolute row-index distance from the active line. An unsynced document uses
sharp text and no guessed current line. Line-only active text stays uniformly highlighted;
it must not look partly sung. Hovered/focused text and manual exploration reveal sharp context.
Increased contrast removes softening and edge masks and uses readable static emphasis. Reduced
motion disables the sweep, lift and animated transitions, keeping a static active line. Native
focus and semantic labels must remain available when decorative effects are disabled.

The display clock may interpolate no more than one second beyond the displayed playback
position. Reset its sample time on a confirmed position update, pause-state change and track
change, and use the pending seek's displayed position when present. Freeze interpolation on
pause, advertisement, service disconnection or a non-idle playback transition, and after the
one-second limit until another update arrives. Render ticks never publish playback samples,
change queue state or advance the authority's clock. Stop timers when their presentation is
hidden or detached; do not rebuild the complete lyric list, catalog or shell on every frame.

Wheel, trackpad and drag scrolling suspend following without swallowing the native scroll
event. Suspend only for input in that viewport; an unrelated page scroll must not disable
lyrics. Show a compact Resume lyrics action in the viewport without moving its layout.
Resuming, or activating a seekable lyric line, returns to the current-line anchor. Track changes
reset following. Keyboard scrolling and screen-reader exploration must also remain usable;
include them in native acceptance rather than assuming pointer tests cover them.

A synced line is a native action only when seekable, connected, outside account operations and
an idle playback transition. It requests a seek through the model and Rust authority rather
than executing playback directly. Expose the entire line as one accessible action with a
contextual label and current-line value; decorative per-character runs must not become
hundreds of screen-reader elements or announcements. Preserve readable focus, keyboard
activation, pause/seek commands and manual browsing across main/full-player presentation changes.

## Windows implementation handoff

The current WinUI DTO in `apps/goosic-windows/Goosic.Windows/Service/CatalogModels.cs` keeps
only a line's text and start. Add an optional/default-empty segment collection with the exact
wire names and a nullable signed 64-bit `endMs`. Preserve words through the line projection in
`ViewModels/ShellViewModel.cs`; the current `LyricLineViewModel` and `FollowLyrics` logic track
whole lines. Retain lookup identity, request-version checks, source captions, unavailable/
not-found/loading states and `truncated` messaging in `Presentation/LyricsState.cs` and
`Presentation/LyricsLookup.cs`.

`MainWindow.SidePanel.cs`, `MainWindow.FullPlayer.cs` and the lyric templates in
`MainWindow.xaml` are the native presentation entry points. Share one word-progress/following
state between inspector and full player. Use native shaped text and rendering/composition
resources for glyph ranges; do not place independent buttons or separate layout boxes around
each character. Observe system animation/contrast preferences together with app Reduce Motion,
and dispose rendering subscriptions on teardown. Keep the line as a native accessible action.

The Rust word-progress and import-validation helpers are not currently exported through
`goosic-shell-support-ffi`. If Windows needs a callable binding, add it on development with
conformance tests before native work. Until a binding exists, a pure managed equivalent must
be held to the same Rust boundary cases; no WinUI dependency belongs in the shared helper.
Extend `Goosic.Windows.Tests/LyricsStateTests.cs` and `LyricsLookupTests.cs` and add focused
wire/progress/import tests. Unit tests cannot establish live WinUI blur, text shaping or
screen-reader acceptance.

Use a native file picker for the local JSON import route, scoped to the current recording.
Apply it to the current listening session, reject oversized/invalid files without replacing
existing lyrics, and prevent an outstanding lookup from overwriting a successful import.
Do not add persistence, uploads or credentials as a side effect of this parity work.

## Linux implementation handoff

The GTK source examined for this handoff is `origin/platform/linux` at
`fc0a31eec1af7b297c0bbca36d0599b4aa63eae4`; refresh the branch state before implementation.
The files are `apps/goosic-linux/src/lyrics.rs`, `src/side_panels.rs` and the native UI/theme
files. `lyrics.rs` already stores `goosic_protocol::LyricsDocument` and calls shell-support's
active-line rule. Preserve that typed document, including word arrays, through GTK rendering.
When updating protocol dependencies, add `words: Vec::new()` to old Rust `LyricsLine` test
literals; serde defaults preserve old responses but do not supply missing struct-literal fields.

Call the existing shell-support progress/validation functions directly. Keep decision state
outside GTK, as the current lyrics module does, and retain its late-answer rejection and
revision-based updates. Add native follow-suspension/import state without rebuilding the list
on frame ticks. Use GTK/Pango's shaped layout and native accessibility for text ranges and line
actions; use a widget-scoped tick/update callback only while the active line is visible and
animated. Native theme colors, reduced motion/contrast preferences and readable fallbacks must
remain intact. GTK's theme adaptation and the no-libadwaita requirement still apply.

Use a native GTK file dialog and the same current-track/session-only import contract. The
main/full-player rendering paths should share state when a full-window listening surface is
implemented; the existing Linux notes identify that full player as separate open work. This
handoff does not claim that GTK already has the macOS immersive surface or word effects.
The GTK app stays in its separate Cargo workspace and uses the service installed beside it;
do not link playback authority or fetch lyrics directly in the native shell.

## Acceptance and evidence

Use deterministic fixtures for plain lyrics, line-only timing, complete word start/end pairs,
start-only words, empty timed lines, Unicode/punctuation/whitespace, long wrapped lines,
repeated/equal starts and overlapping vocals. Assert complete active-line emphasis when words
are absent, exact activation at a word start, bounded progress before/after an explicit end,
and no inferred duration. Check old responses with no `words` and new words with no `endMs`.

Service tests must cover Lyricsfile priority, word-rich search selection, invalid-word fallback,
malformed/unknown/offset YAML fallback, duplicate keys/aliases/tags/nesting limits, byte/line
truncation and wire roundtrips. Native import tests must keep the current lyrics unchanged
on validation failure, reject a file selected for a track that changed, and protect a successful
import from an outstanding provider response. Keep local-import strictness separate from
valid provider overlaps.

Inspect main/full/compact listening presentations, narrow/short windows, pause/resume, forward
and backward seek, skipped tracks and delayed/disconnected service updates. Verify manual
scroll/Resume does not fight the listener, masks do not hide a focused target, and long or
bidirectional lines retain their layout while fill changes. Verify keyboard activation, Windows
screen readers or Linux AT-SPI tools, increased contrast and reduced motion/transparency on
real native desktops. A compile or screenshot with synthetic text proves neither live audio
alignment nor coverage for a particular recording. Record the actual provider record ID,
recording duration and timing presence whenever live word alignment is accepted.

The shared Rust tests and all 240 Swift tests passed after the September 30 Lyricsfile change,
and both the service and macOS shell built. The sampled live records checked afterward had
no word timing, so live word-following alignment remains unverified. Windows/GTK rendering,
import dialogs and accessibility checks are pending native work, not completed acceptance.
