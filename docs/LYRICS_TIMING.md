# Lyric timing and listening effects

Goosic reads LRCLIB's `lyricsfile` response field using the
[Lyricsfile 1.0 draft specification](https://github.com/tranxuanthang/lyricsfile/blob/main/SPECIFICATION.md).
A usable Lyricsfile takes priority over legacy LRC/plain fields. Lookup and search can now
return real word or segment timing automatically when a contributor supplied it; coverage is
not universal. Search prefers usable word timing, then line timing, plain lyrics and
instrumental records. The exact recording lookup still takes priority over search.

Search is also length-aware. LRCLIB lists every recording of a song, and the richest timing is
often a live, extended or video version whose lines land seconds away from the studio track.
When the query carries the track's length, a result within two seconds of it beats every other
result, and richness decides only among those; outside that window the closest length wins and
richness breaks ties. A record that does not state its length is usable but loses to a match, and
a query without a length keeps the older richness-only ranking.

Timing is never presented from a different recording. If the query has a length and the chosen
record's length differs from it by more than three seconds, the lyrics are returned as not synced:
the text is kept, every time and word timing is dropped, and the shell shows them as plain lyrics.
Plain text from the record is used when it has any, otherwise the synced lines are stripped of
their timestamps. The same check runs on the exact lookup's answer when the record carries a
length, since that endpoint matches loosely on its own side. Lyrics that visibly do not follow
along are honest; lyrics highlighted at the wrong moment are not. Instrumental records are
unaffected.

Legacy LRC text is parsed with two details that matter for alignment. An `[offset:N]` tag, in
milliseconds, shifts every line; by the LRC convention a positive value makes lyrics appear
sooner, so it is subtracted from each time and clamped at zero, and only the first such tag
counts. Timestamps with hours, `[h:mm:ss.xx]`, are read as hours; they were once read as
minutes and seconds, which put a ten-minute song's lines at the wrong second. The Lyricsfile
`offset_ms` is not applied: a non-zero value makes the whole Lyricsfile unusable, so the record's
legacy LRC fields (and their `[offset:]` tag) are used instead rather than showing shifted timing
unshifted.

A line carries `atMs`, `text` and, when the provider supplied one, `endMs`: where the line
stops being sung. It is omitted when unknown, and a shell then keeps the line current until
the next one starts rather than guessing. The lyric wire format also carries optional `words`
arrays with absolute `atMs`, optional `endMs` and `text`. Existing line-only responses remain compatible. Supplied start/end pairs drive a
word sweep; a start without an end activates the word at that timestamp. `progress` infers no
missing end from the next word, the next line or text length. On macOS the drawing of a
word-timed line sweeps a start-only word until the next word in the same line begins; the
section on word motion below records that decision and its limits. Songs without real word
timing highlight the complete active line.

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
Context depth now depends on distance from the active line; nearby text remains clearer. On macOS, active words can glow and lift; an advancing highlight follows the current
segment on macOS and Windows.
Only the active line redraws, at up to 60 frames per second. Its presentation position may
interpolate at most one second beyond the last displayed playback position, then freezes until
another update arrives. The current line follows that same interpolated position. The page
confirms the position several times a second, and the lyrics take every confirmation, even
where a progress bar takes only one a second to save redraws: anchored on a value up to a second
old, the sweep stalled once a second and the highlight jumped back on pause. A seek holds its
target for the lyrics until the page confirms it or a second passes, and is not re-confirmed from
each sample while held, which made the clock snap back repeatedly. While the listener drags the
progress bar the lyrics keep following the reported position, because the audio plays on. Pausing, advertisements, disconnection and pending playback transitions
stop that interpolation. It never writes a position, makes a seek or changes Rust authority.
Manual scrolling, hovered/focused text and increased contrast reveal readable text; reduced
motion disables the sweeping and movement, retaining a static active-line emphasis.

On macOS the player's More menu and the lyrics context menu offer Import Timed Lyrics; on
Windows the lyrics context menu, in both the side panel and the full player, does. Choose a
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
Lyricsfile conversion runs in the Rust service and is shared by every shell. Windows now draws word sweeps, supplied-end rests, interlude dots and per-row following.
Linux retains whole-line presentation; local timed-lyrics import is on macOS and Windows.
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
word schedule for a line-timed song. A new visual effect does not create timing data. On
November 16, 2026 the user asked for Music's word motion on word-timed songs, which is a
different case: the words there carry supplied starts, and only the end of each sweep is taken
from the next supplied start.

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
shape and can also be imported on macOS and Windows.

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
| Active-line position | Scroll anchor at 36% of the viewport height, with enough padding above the first row and below the last for either to reach it |
| Following transition | Each row moves on its own critically damped spring, response 0.4 seconds; the column never travels as one block |
| Line emphasis transition | Opacity and blur only: SwiftUI's smooth spring over 0.55 seconds, no overshoot |
| Line change cascade | Rows at or above the new line start together; each row below starts 50 ms after the one above, up to ten rows |
| Line change timing | At the line's own start, scheduled from the interpolated position; a confirmed position up to 250 ms short of a line already shown does not step back |
| Upcoming lines | Opacity `max(0.3, 0.6 - (distance - 1) * 0.07)`, blur `min(3, distance * 0.5)` |
| Past lines | Opacity `max(0.16, 0.34 - (distance - 1) * 0.06)`, blur `min(3, 0.8 + distance * 0.6)` |
| At rest | While nothing is being sung, the finished line and every upcoming line sit at 0.62 opacity with no blur |
| Active glow | White at 0.2 opacity, radius 8 immersive / 5 inspector |
| Line size and place | Unchanged when a line becomes current. An earlier 2-point lift and a 0.97 context scale read as a zoom on every line change, which Music does not have, and were removed on November 16, 2026 |
| Viewport fades | Clear at edges; opaque from 14–84% immersive / 10–86% inspector |
| Word fill | macOS 15 and later: a sweep with a soft leading edge half the font size wide, from 0.4 to 1.0 opacity. macOS 14: the letters brighten in place over the same span |
| Word lift | A word rises 0.05 of the font size as it is sung, eased out over at least a second, and stays raised |
| Held word | A word of two to seven characters swept for a second or longer: its letters swell by up to 12% and glow in turn |
| Start-only word | Swept from its own start to the next word's start in the same line, for at most 2.5 seconds |
| Last word of a line | With no supplied end, swept over a fixed 450 ms |
| Interlude | Three dots, 0.3 of the font size, fill in turn across the stretch, breathe by 8%, and shrink away over its last 600 ms |
| Pointer highlight | The line under the pointer gets a rounded backing at 8% of the text color and stays swept |
| Drawing cadence | Only the line being swept, or one row of dots, updates, capped at 60 frames per second |

### Word motion on macOS

LRCLIB's word-timed records checked on November 16, 2026 carry word starts without word ends,
so a rule that only swept supplied start/end pairs left every word snapping from dim to lit.
The macOS shell therefore draws a start-only word across the span from its own start to the
next word's start in the same line. Both ends of that span are timestamps the provider
supplied, and the word cannot still be the one being sung after the next one has begun. The
span is capped at 2.5 seconds, because a longer wait is more likely a rest than a held note.
A line's last word has nothing after it to bound it, so it takes a fixed 450 ms rather than a
guess at its length when no line end is supplied. Words that start together
settle over 220 ms.

This is `LyricsTiming.sweeps`, and it only shapes drawing. `LyricsTiming.progress` and
`word_progress` are unchanged and still report a start-only word as complete at its start.
A line without `words` gets no sweep, no lift and no partial fill, exactly as before. Windows uses the same sweep spans. `goosic-shell-support::lyrics_motion` now owns
the sweep spans, rest and interlude boundaries, dot drawing values and row-motion parameters.
Swift and C# retain tested native copies until their bridges expose these rules. Linux has
not yet adopted the drawing.

The emphasis on a held word uses the same span, so a word followed by a rest inside its line
can be emphasised as though it were held. The 2.5-second cap, the limit of seven characters and
the low strength of anything under about a second and a half keep that rare and slight, but it
is the one place where the drawing can say more than the data does. Supplied word ends remove
the doubt wherever a provider has them.

From macOS 15 the line being sung is drawn by a text renderer over the glyphs the system
already laid out, which is what lets a word lift, swell and glow without the line wrapping
differently or moving its neighbours. Right-to-left runs brighten as a whole instead of being
swept from the left. The amounts follow the published parameters of the AMLL lyric player,
which documents Music's behavior more closely than anything Apple has published; no code
from it is used.

Interlude dots stand only for stretches the lyrics themselves mark as unsung: the
introduction before a first line that begins four seconds or more into the recording, and a
line the provider left empty for four seconds or more. A rest after a sung line is drawn
as an interlude only when the line carries a supplied end and the next line starts four
seconds or more after it; a line without an end has none, because nothing in the document
says where the singing stopped. A shorter empty line keeps its musical note. The dots fade out outside their stretch and their row
keeps its height, because a row that folded away moved every line below it while the column
was scrolling to the next line.

A line becoming current must not flash. Its text is built from the same word pieces whether
or not it is current, so nothing is swapped or wrapped differently at that moment, and the
drop to the unsung brightness animates with the rest of the line's emphasis instead of
landing at once under an opacity that is still rising. A context line revealed by the pointer
eases over 0.2 seconds, since lines pass under a resting pointer on every line change. The
immersive column keeps 16 points of room at each side inside its scroll view, which clips,
so the dots' swell, a glow and the pointer highlight are not cut off. The widening is applied
after the viewport's fade mask; a mask applied to the narrowed view trims the sides again.

The following motion was measured, not chosen. A recording of Music taken on November 16,
2026 was read frame by frame, at about 55 frames a second, and each row of lyrics tracked
through five line changes. Rows at or above the new line begin to move together, and every
lyric line below begins 50 ms after the one above it. From its own start a row covers a quarter
of the distance in 50 ms, half in 100 ms, three quarters in 170 ms, 90% in 240 ms and 98% in
340 ms, without overshoot. That is a critically damped spring with a response of about 0.4
seconds, and it is what `rowSpring` and `rowDelay` are. Scrolling the column as one block, on
any curve, cannot produce this, and it read as the lyrics stopping and starting.

The macOS shell gets there without giving up its scroll view. On a line change the column
scrolls to the new row at once, and in the same update every row is drawn lower by exactly the
distance scrolled, so nothing has moved. Each row then gives that offset back on its own
spring after its own delay. The distance is the difference between where the two rows are laid
out, which is why the column is padded so that no row's scroll is ever clamped. When either
row has not been laid out, or following resumes on the row already followed, the column moves
as one on the same spring. With reduced motion it does not animate at all.

The other curves follow Apple's own guidance from "Animate with springs" (WWDC 2023): a spring
without bounce for anything that must not overshoot, such as opacity and blur, and a small
bounce where a settle reads as physical, such as position and scale. Thirty frames a second
was visibly stepped for a sweep that moves a few points per frame, which is why the one live
line draws at sixty.

A line that reaches its supplied end, with the next line a second or more away, is at rest
until that line starts. A shorter wait is not drawn: the line stays lit until the next begins,
because dimming it for a fraction of a second showed as the highlight dropping back and
jumping forward on a line change. At rest: nothing is lit,
the line and those still to come are equally readable and sharp, and the column stays where
it is unless an interlude's dots take over as the row it follows. That is what Music does
between lines, and it needs the line's own end; a line without one stays lit until the next
begins, as before. The end also bounds the sweep of the line's last word. Line ends are
scheduled from the interpolated position the same way line starts are, and a confirmed
position up to 250 ms short of a rest already shown does not undo it. Upcoming lines are
brighter and sharper than past ones, matching a recording of Music taken on November 16, 2026.

Windows consumes `endMs` for rests and interludes. Linux retains whole-line presentation;
its hand-built fixtures include `end_ms: None` to preserve unknown ends.

Every line is drawn through one view structure whether or not it is current, because a line
that changed structure on becoming current was replaced outright and its emphasis could not
animate.

Distance is the absolute row-index distance from the active line. An unsynced document uses
sharp text and no guessed current line. Line-only active text stays uniformly highlighted;
it must not look partly sung. Hovered/focused text and manual exploration reveal sharp context.
The line being sung keeps its sweep under the pointer; keyboard focus still shows it whole.
Increased contrast removes softening and edge masks and uses readable static emphasis. Reduced
motion disables the sweep, lift and animated transitions, keeping a static active line. Native
focus and semantic labels must remain available when decorative effects are disabled.

The display clock may interpolate no more than one second beyond the displayed playback
position. A newly confirmed position never lands exactly where the clock had got to, and
stepping to it made every sweep hitch about once a second. When the two are within 0.3 seconds
the clock keeps the position it was showing and runs up to a few percent fast or slow, closing
60% of the gap over the next second; it never runs backwards. A larger difference, such as a
seek or a stall, is taken at once. This is `LyricsTiming.reanchor`. Reset its sample time on a confirmed position update, pause-state change and track
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
no word timing, so live word-following alignment remains unverified. Windows rendering is implemented, but native visual and accessibility acceptance remains
pending. GTK motion and non-macOS import dialogs remain pending native work.

## Windows native motion

The WinUI side panel and full player draw the same sweep spans, supplied-end rests,
introduction and interlude dots, and per-row following as macOS. The follower schedules line
starts and rests from a presentation clock bounded to one second beyond a confirmation.
Pauses and advertisements freeze it. Neither the clock nor the compositor authorizes playback.
The full-player cover settles to 74% scale while paused without changing the controls' layout.
System animation settings, Reduce motion and Efficiency mode disable decorative motion.

Windows brightens one grapheme at a time using XAML text runs rather than the macOS soft-edged
gradient. Those runs cannot transform independently, so Windows has no word lift, held-word
swell or glow. The panel keeps its native 22-point type, the full player its 30-point type;
active text uses the accent in the panel and white in the full player. The Windows backdrop
already used a black veil. These are native adaptations, not evidence of visual acceptance.

Each row starts a critically damped 0.4-second composition spring after its 50 ms stagger.
The scroller jumps to its final offset while row translations hold the old appearance, then
release it. No one has run this port on Windows during this work. Check line changes for a
one-frame flash if the scroll and translation land in different frames; replace following
with an offset-positioned column if reproduced. Also inspect wrapped and bidirectional text,
manual scrolling and Resume, pause/resume, seeks, track changes and accessibility settings
on a real Windows desktop.
