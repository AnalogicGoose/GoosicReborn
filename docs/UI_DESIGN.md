# Goosic UI design specification

## Authority, reference and implementation status

This is the shared UI contract for macOS, Windows and Linux. Every agent changing presentation
must read it. The product must have the same information hierarchy, navigation, control order,
interaction outcomes and responsive behavior on all three platforms. Native widgets, fonts,
window decorations and supported materials adapt to the operating system.

The reference is the user's current native macOS UI and corrections through September 30,
2026: artwork-led discovery, grouped navigation, floating playback and search controls,
capsule-shaped search, a native single-selection search picker, and a volume layer that does
not resize the player. Apple Music screenshots supplied by the user establish the requested
control treatment and interaction. They are not a request to copy unrelated Apple content,
branding or features. Private account names, queries and library contents in screenshots are
not reusable fixture data.

This document records the intended shared outcome. The implementation audit at the end records
where current code differs. A guideline being documented does not mean every shell already
implements it. Current user instructions can revise a design choice; update this contract in
the same change so the other platforms inherit that decision. The ownership, credential,
advertisement and licensing invariants in [ARCHITECTURE.md](ARCHITECTURE.md) still govern UI
behavior. [MACOS_UI_GUIDELINES.md](MACOS_UI_GUIDELINES.md) contains only macOS adaptations.

## Composition and visual hierarchy

Goosic is a music browser with a persistent listening surface. Artwork and music titles carry
the page; navigation and playback chrome stay quieter. The window has a leading navigation
area, a scrolling main canvas, an optional trailing Lyrics or Up Next panel, and a player
capsule floating near the bottom of the main canvas. Full player and mini player are alternate
presentations of the same playback state.

Keep the main canvas continuous under page-owned floating chrome. Search and its category
selector float above results, and the player floats above the bottom of the page. Content must
scroll behind their material. An opaque fixed search header, a separator beneath it, or an
extra layout row reserved for the player is prohibited. Reserve initial and final clearance
inside the scrolling content, not by clipping or padding the scroll viewport. Measure the
actual chrome where its size varies. The first result and last actionable row must be fully
reachable without hiding behind a control.

Navigation and an open inspector are persistent regions with their own material. A native
split column or an overlay implementation is acceptable when it preserves readable content,
usable controls and the shared hierarchy. A region boundary between persistent panes, a
settings-row separator, and a song-row hairline are meaningful. A line added to turn floating
search controls into a separate header band is not.

Use restrained depth. Window backdrop, navigation material, content, floating chrome, transient
controls and modal dialogs form distinct layers. Avoid opaque rectangles behind translucent
controls, double material around a capsule, stacked ornamental shadows, or borders around
every catalog card. Interactive overlays must receive input above the covered content;
decorative artwork, fades and backdrops must never intercept input.

## Color, material, typography and shape

The brand accent is the vivid pink used by the current macOS UI: approximately #FF0552, from
its sRGB components 1.0, 0.02, 0.32. Use it consistently for branded navigation icons, primary
music actions and active music states. Current Windows and Linux #F53150 values are a parity
gap, not a second brand choice. Native selection controls may retain their system selection
color; do not replace a real control just to force every selected segment pink. High-contrast
and forced-color modes may override brand colors for legibility.

Use semantic primary, secondary and tertiary text colors. Primary text contains names and
important state; secondary text contains artist, album, counts and descriptive metadata;
tertiary text contains quieter supporting information. Color alone cannot indicate playing,
selected, expired, disabled or error states. System, Light and Dark remain distinct theme
choices. Materials must remain readable over both pale and dark artwork.

Native translucency is the preferred treatment where it is supported. macOS uses Liquid Glass
on macOS 26 and native materials on earlier supported versions. Windows uses its supported
Mica/Acrylic surfaces and theme-aware fallback brushes. GTK uses its native theme and restrained
translucent or solid fills; compositor blur is not assumed. Reduced transparency and contrast
themes use readable solid surfaces. Equal composition and interaction are required even when
the exact optical effect cannot be shared.

Use the platform's UI font and native text scaling. The hierarchy is a large bold page title,
a smaller semibold shelf title, a readable music title, and quieter secondary metadata. Avoid
all-caps paragraphs, decorative typefaces, heavy weights everywhere, or technical subtitles
under every page title. Titles truncate before controls lose space; durations and elapsed times
use aligned or monospaced digits. Full metadata remains available through an accessible name,
detail page or tooltip where appropriate.

Search fields, the transport surface and grouped full-player controls use capsule shapes.
Music artwork and recent-query cards use modest rounded corners; artist portraits and small
individual full-player actions are circular. A playlist or album cover is not a circular avatar.
Maintain aligned icon canvases and reliable hit areas despite different symbol drawing bounds.

### Reference measurements

These are observed macOS design measurements in logical points, not physical-pixel mandates
for every operating system. Preserve their proportions and density through native scaling.
Existing platform deviations belong in the audit rather than becoming silent new defaults.

| Element | Current macOS reference | Shared outcome |
| --- | --- | --- |
| Page horizontal gutter | 24 | Consistent leading edges for headings, shelves and rows |
| Space between shelves | 28 | Clear groups without dividing the canvas into boxed sections |
| Standard carousel artwork | 158 square; video 280 by 158 | Square music covers, circular artists, wide video artwork |
| Card gap | 16–18 | Consistent rhythm and visible grouping |
| Quick-pick rows | 56 high, up to four rows per column | Compact song shelves with horizontal continuation |
| Page / collection title | About 30 / 32 | Strong title above quieter metadata |
| Navigation width | Native range 180–300, ideal about 212 | Readable grouped navigation, responsive to space |
| Inspector width | 320 | A readable secondary pane that leaves main actions reachable |
| Main player maximum width | 740 including padding | A centered compact capsule, not a window-wide footer strip |
| Main player edge clearance | 20 horizontally, 12 below | Visibly floats above the content |
| Main player art / glyph hit canvas | 34 art; 30–34 control canvas | Compact visuals with usable native hit targets |
| Search field maximum width | 620 | Centered pill with adequate space for a query |
| Default macOS window minimum | 920 by 680 | A platform baseline, not a cross-platform minimum |
| Mini-player minimum | 260 by 380 | Compact listening presentation with core transport |

Use platform accessibility sizing when it requires more space. Do not shrink text, hit targets
or glyphs to preserve an arbitrary reference width. Window resize can select a persistent
compact arrangement; opening a temporary control must not select a different arrangement.

## Navigation and account placement

The leading navigation presents Search and Home first, followed by Discover, Library and
Playlists groups. Discover contains Explore, Charts, Moods & genres and New releases.
Library exposes Liked Music, Artists, Albums, History and Downloads. Playlists contains All
Playlists and account playlist entries with their artwork. Account identity and account actions
anchor the bottom of navigation. Where Settings opens differs by platform: Windows keeps it as
the last navigation destination, while on macOS it is not a navigation row and opens from the
account menu and the app menu's Settings command.

The November 2, 2026 sidebar reference adds a compact icon rail and an expanded navigation
panel. Keep the Goosic identity at the top, a menu action below it, Search before Home, and the
account avatar anchored at the bottom in both arrangements. Expanded navigation shows group
labels, destination names and account status. The Goosic name uses regular font weight. Search
is a navigation tab with a magnifying-glass icon and an expanded label, matching macOS; its
query field belongs to the main Search page rather than the sidebar. Selecting Search or using
the search shortcut opens that page and focuses its floating field, preserving the query and
category when returning. The compact Search icon opens the same page without expanding the
sidebar. Compact navigation keeps named, keyboard-reachable icons with tooltips. Windows rail
shortcuts mirror the expanded destinations in the same order: Home; Explore, Charts, Moods &
genres, New releases; Liked Music, Artists, Albums, History, Downloads; All Playlists; Settings.
Search stays above Home. These are individual icon actions with matching glyphs, names and
tooltips, rather than aggregate Discover or Library buttons. Collapsed icons use uniform spacing
without the expanded sidebar's section-heading gaps. New playlist remains available as a named plus icon
when signed in. The destinations scroll at short heights while account identity stays anchored
at the bottom. Every shortcut navigates directly without asking for the same choice again.
Only the menu action expands navigation, starting at its top. Selecting the current destination
closes the panel without reloading it. Use a rounded selection fill and a short leading accent marker, with focus and
hover distinct from selection. Compact icons keep the same restrained visual size as expanded
navigation icons, within full-size hit targets. Align icon centers, brand artwork and the
account avatar on one leading axis; hide label spacing along with its text so neither the
avatar nor the brand artwork is clipped. Keep Search and Home at the same vertical positions
when navigation collapses to icons. Long labels truncate inside their available
column, with the full name accessible. Collapsing navigation changes presentation only: it preserves
the route, query, account, queue, playback position and mounted renderer.

Windows adapts this with WinUI buttons, Fluent glyphs and theme-aware Acrylic, a 64-logical-pixel
rail and an expanded panel of about 280 pixels. The rail is the default on every launch. The
expanded panel opens only through the explicit menu action, overlays the canvas at
every window width, and returns to the rail after a destination is chosen, an outside click,
or Escape. Expansion is temporary and is not restored with window placement. Content and the
player keep their rail-layout positions while the panel is open. The rail reserves enough content
space to keep the player's essential actions reachable. Goosic's Discover and Library groups
retain their actual destinations, and account playlists and creation remain accessible through
the expanded panel. The reference does not add Apple Music destinations or replace the current
floating search and player treatment. Both sidebar states share the same theme-aware Acrylic
and solid accessibility fallback. Expansion uses a brief compositor reveal without stretching
icons or moving the page/player; system Reduce Motion, app Reduce Motion and Efficiency mode
disable it. Adoption on macOS and GTK remains pending.

Selection uses a restrained rounded row highlight and readable emphasis. Distinguish selected
navigation from hover and keyboard focus. The navigation list may scroll while its account
footer remains available; their materials form one continuous region. Avoid an unrelated opaque
footer patch, an unlabeled duplicate navigation control, or accidental toolbar overflow.

Detail navigation has Back at the content's leading edge and relevant Share and More actions
at its trailing edge. A route and an opened album, playlist or artist remain distinct states.
Returning to a route must not unexpectedly reset its search query or create a new queue.
Playlist entries and Liked Music highlight the destination actually being shown.

Account UI distinguishes guest, signing in, signed in, session expired and service offline.
Saved account metadata or a cached personalized feed is not proof of an authenticated session.
Expired account content must not masquerade as a live personal feed. Show an actionable Sign In
Again state, keep guest and personal caches distinct, and preserve the playback authority's
account-switch rules. Premium or advertisement behavior must reflect the active renderer
session; do not imply entitlement from a saved avatar or account name.

Sign-in uses the platform's isolated account profile with visible loading, failure and retry
states. An empty indefinite login window is not an acceptable state. Browser, passkey and
session-transfer limitations must be stated accurately; opening a system browser does not
establish that its cookies have entered an embedded account profile.

## Discovery, artwork, lists and entity pages

Home, Explore, Charts, Moods & genres and New releases share the same catalog vocabulary.
Render shelves in the order returned, including mixed rows and card carousels. A responsive
song shelf such as Quick picks uses compact rows in horizontally scrolling columns, not a
large cover card for every song. Carousel previous/next controls remain associated with their
shelf. Category cards identify a real destination; do not fabricate catalog categories or
metadata to fill empty space.

An artwork card contains its real cover, title and secondary metadata. Artist portraits are
circular and centered; albums and playlists are square; music videos are wide. Fit or crop
art intentionally without stretching its aspect ratio. A placeholder keeps the final artwork's
geometry. Artwork arriving must replace the placeholder without moving titles or controls.
Missing art must not prevent selecting or playing an otherwise valid item.

Song rows align artwork or track number, title and artist, optional album, duration and More.
A liked marker and explicit indicator convey their actual states. Row separators start after
the leading artwork/number rather than cutting through it. Hover can reveal a play action;
playing state has a stable marker that differs from hover. Context menus and keyboard actions
expose the same meaningful operations as the visible row. Do not hide an essential operation
behind pointer hover alone.

A song's menu has the same items, names and order on every platform, wherever the song
appears: Play, Play Next, Add to Queue and Start Radio (Remove from Queue on a queue row); then
Like and Dislike with an account; then Go to Artist and Go to Album; then Add to Playlist and,
on an owned playlist, Remove from Playlist; then Copy Link. Every item carries an icon, since a
menu where only some rows have one reads as ragged, and labels use title case. Go to Artist and
Go to Album stay in the menu whenever the song names an artist or album and are dimmed when
the catalog gave no link, so the menu does not change shape from one song to the next. The
macOS menus are the reference; Windows draws its flyouts with rounded rows highlighted in the
accent colour, which is the nearest native equivalent.

Album and playlist pages lead with cover art, title, relevant metadata and primary Play/Pause
and Shuffle actions. The primary action reflects whether that collection is playing. Ordered
collections launch ordered queues; discovery selections may resolve a station. Sort and Find
belong to collection browsing and must not silently change the playback queue. Song counts
remain honest about partial data, including a '+' where completeness is unknown. Continuation
pages append with their real cursor rather than presenting the first page as the whole list.

An artist page opens with a hero, in the manner of an Apple Music artist page. The artist's
banner fills the content area edge to edge, under the floating toolbar, cropped to fill rather
than stretched and anchored near its top, where faces are. Its height follows the window's
width within a floor and a ceiling, so a narrow window keeps the name and actions on screen and
a wide one does not become all picture. A scrim darkens the picture's lower part enough for
white text, and the page beneath takes the picture's own colour, sampled from it, before fading
into the ordinary page background over the first section; the artist's colour belongs to the
page, not only to the rectangle the picture occupies. The name is centred near the hero's foot
in a large bold weight, with upstream's audience line under it when there is one. Under those
sit three round buttons: About (the description and subscriber count) to the left, a larger
primary Play in the middle that plays the top songs in order, and Subscribe to the right, filled
when the account follows the artist and shown only with an account. Share and More sit at the
hero's top trailing corner; More holds Shuffle, Mix and Copy Link. Top songs and the shelves
follow as on any page.

The banner and every fact come from the artist header the catalog reads, the `artist` field of
an artist page, and from nowhere else. When the header has a square portrait and no banner, the
hero shows the portrait as a large circle over a soft field of its own colours instead, as the
previous Goosic did; with no picture at all the hero is the field of the accent colour, never a
song's cover or a video still. Missing audience, description or subscriber count leave their
place empty rather than being estimated. Reduced transparency replaces the scrim with an opaque
band, reduced motion removes the colour fade's animation, and increased contrast strengthens the
scrim; the name and the three buttons stay readable over any picture.

Owned-playlist operations expose create, rename, description, privacy, remove/reorder songs,
add to playlist and deletion according to actual capability and account state. Destructive,
unrecoverable deletion is confirmed. Reversible queue edits can offer Undo. An unimplemented
platform capability must report its limitation, not silently succeed.

## Search

The search field is a centered pill containing a search icon, query, clear action when needed,
and a submit action. Enter submits; clear returns to discovery/recent-query state. Existing
recent queries are real stored queries, not invented song results. Recently searched cards and
browse destinations give the empty-query page useful content without fake recommendations.

Windows uses a restrained rounded rectangle with an 8-logical-pixel corner radius for the
search field, following the user's Windows-specific correction; macOS retains its capsule.
Placeholder and entered text are vertically centered with the leading glyph and submit action.
The Windows landing page follows the macOS composition: no extra Search title or explanatory
subtitle above discovery; a centered field with a leading search glyph and explicit submit
action; a visible horizontal native single-selection category list; then recent-query cards
and colored browse cards. Recent queries use roughly 240-by-80 cards with a history glyph,
two-line titles and a Clear action in the section heading. Browse cards are about 150 high,
with a restrained color gradient, a large decorative native glyph and a bottom-leading title.
Explore is blue, Charts orange, Moods & genres purple and New releases pink. Their grid adapts
to available space without turning into window-wide strips. Contrast themes use system brushes.

All, Songs, Albums, Artists, Playlists and Videos form one native single-selection category
control. Selected state must be visible and accessible. macOS uses its native segmented picker;
other platforms use an appropriate native selection control or a linked native single-selection
group. A row of independent decorative buttons with no selection semantics is prohibited.
Do not simulate the macOS control with custom underline tabs. The category changes the upstream
query and cache key; it is not a client-side filter of whatever results happen to be loaded.

The field and selector float over the full-height results viewport without a fixed opaque
header or horizontal separator. Initial scroll-content clearance follows their measured height.
Suggestions float as a dismissible secondary layer and do not push results down. All categories
remain reachable at narrow widths. Preserve keyboard focus, query text and the chosen category
through loading and retries.

## Main player and volume interaction

The normal player order is Shuffle, Previous, Play/Pause, Next, Repeat; current artwork and
track title/subtitle above a thin timeline; then More, Lyrics, Queue and Volume. The compact
layout may wrap persistent controls or omit secondary transport from its first row while
keeping its actions available. Lyrics and Queue remain visible in normal narrow layouts.
Do not silently drop them to make the bar fit. The idle state says Nothing playing and invites
choosing a track; it does not invent progress or leave a permanent preparation spinner.

The timeline is restrained during listening. Hover or scrubbing can reveal elapsed/remaining
time and a thicker track without enlarging the whole bar. Drag sends a bounded seek operation;
keyboard and assistive actions also adjust position. While seeking, live ticks must not pull
the thumb away from the pointer. Seeking and track changes honor advertisement and transition
restrictions. A pending command can show progress in its control while preserving the control's
geometry and symbol identity.

Clicking Volume opens a small material capsule as an overlay at the player's trailing side.
It contains the volume slider and a speaker that closes it on a second click. Opening or closing
must not resize the bar, add a row, move artwork or transport, switch the bar's responsive
layout, or open a separate popover window. The capsule temporarily covers panel icons; More
remains reachable in that layer and provides Lyrics and Up Next. Covered controls cannot receive
clicks or create duplicate accessibility targets. Restore the original visible controls on close.
Mute remains available through More, a context menu and accessible actions. Volume value is
synchronized across main, full, mini and system-facing presentations.

Every volume slider follows the same audio taper: the gain is the cube of the slider's position,
the rule `volume_gain` in `goosic-shell-support` states. Loudness is heard logarithmically, so a
slider that moved the gain in a straight line was already far too loud half way up. The player
and the saved preference hold the gain; the slider, its spoken percentage and keyboard or wheel
steps work in positions, which is what lets a saved volume open at the same place on every
platform. A drag stays smooth under the pointer: the knob follows the hand, the player hears it
at a bounded rate, and the player's own report of its volume never pulls the knob back while it
is held.

## Lyrics, queue, full player and mini player

Lyrics and Up Next are secondary listening panes. In the normal shell, the toggles identify
which pane is open and permit closing it. Panels have their own scrolling, a readable heading
and meaningful empty/loading states. Opening a persistent panel changes available page space
once; do not animate every catalog row through a changing width.

The queue identifies its source, count and current track. It supports selection, reordering,
removal and clearing according to the shared model. A row click chooses that queue entry;
rendering the panel must not resolve a new station or replace the queue. Clear and Undo use
real snapshots/revisions. Clear Up Next removes only entries after the current one, preserving
history and playback. Undo remains available for ten seconds and expires on a queue revision
or listening-context change. Clearing invalidates pending radio replies so they cannot refill
the queue. Progress reports do not reorder visible rows.

Synced lyrics emphasize the active line and retain context above and below. Autoscroll follows
that line without resetting the entire lyric view on each clock sample; user exploration must
remain usable. Unsynced text is readable as text. Missing lyrics and failed lookup are distinct
states; a Lyrics action remains discoverable and explains unavailability. Seeking to a synced
line is available to keyboard and assistive users as well as through pointer input. During synced playback, contextual
lines use the Music-like soft blur, reduced opacity, subtle scale and viewport edge fades;
the active line stays sharp and prominent. Manual scrolling suspends following and makes
context sharp for exploration. A compact Resume lyrics action appears only while following
is suspended, without permanently displacing the lyric viewport. Keyboard-focused or hovered
lines stay readable, and increased contrast removes blur and edge fades. Unsynced lyrics stay
sharp. Resume does not start another playback session. The active line can use a subtle glow and
lift, while contextual blur increases with distance. Word highlighting requires real timestamps supplied by a source, including LRCLIB Lyricsfile.
Line-only lyrics highlight the whole active line; never estimate singing timing. Reduced motion uses static emphasis. Local timing
import never changes audio or playback authority. See [LYRICS_TIMING.md](LYRICS_TIMING.md) for
the timing contract and current native adoption.

Full player is an immersive artwork-led surface with a dark readable treatment, even when the
browser is light. Large artwork and track metadata occupy the leading column; Lyrics or Queue
can occupy the trailing column. The timeline and full transport sit below the artwork. Close
and mini-player actions are grouped at the top leading edge, volume at the top trailing edge,
and Lyrics/Queue in a bottom trailing capsule. Top and bottom chrome share material and
appearance. Native toolbar backing must not create an extra light plate behind dark capsules.
Small individual Like and More actions are circular, side by side after the track metadata;
More opens the playing song's menu. Switching presentation preserves the
same session, track, queue, volume, position and mounted playback host. Choosing a page from
the full player's menu, such as Go to Artist, closes the full player before the page opens,
because the page would otherwise load unseen behind it; from the mini player it also brings
the main window back.

Mini player is a compact native listening window with cover, metadata, timeline and core
transport. Large lyric/queue columns can step aside there, with a route back to the full
presentation. Always-on-top behavior is a native capability, not a recreated fake titlebar.
Window close, tray/background and explicit quit follow documented platform preferences;
presentation changes must not accidentally stop or restart playback.

## Settings, feedback and loading

Changes apply as they are made and are saved by the shared model, so there is no Cancel or OK
behind which an edit could be lost. On macOS, Settings lives in its own window, apart from the
main window, so changing one never takes the user away from what is playing. It is the standard
Settings window, reached with ⌘, and from the account menu: a toolbar of tabs
(General, Playback, Accounts, Advanced) over right-aligned group labels with checkboxes and
pop-up menus, as Music's is, with Goosic's accent on the checkmarks only.

Settings use readable groups with a label, concise explanation and native control.
Organize by connection/updates, playback, appearance or startup behavior, integrations,
advanced diagnostics and accounts as appropriate to actual capability. Do not distribute a
platform-only setting to another shell as a dead control. Shared settings such as autoplay,
shuffle, artwork background, explicit filtering, theme and reduced motion keep the same meaning.
Defaults, current values and persistence follow the shared model. Settings groups belong to
the content layer: use native grouped backgrounds or standard materials rather than applying
Liquid Glass to entire cards of explanatory text.

Initial loading uses geometry-matched skeletons. Refresh can keep valid cached content visible
with an honest stale/retry message. Authentication expiry is handled separately from a generic
network failure. Empty results, offline service, unavailable lyrics and unsupported features
have short specific messages and an actionable recovery where one exists. Do not expose raw
WebView errors, protocol codes, bridge tokens or stack traces in normal product flows; technical
detail belongs in Debug Mode and safe logs.

Notices float above the player without covering its controls. Important errors and expiring
sessions are perceivable without relying on a disappearing animation. Do not show success
before a confirmed mutation, turn an advertisement into an unexplained playback failure, or
use saved content to imply a successful refresh.

## Accessibility, motion and performance

The November 2 Apple Music video is a motion reference for page fades,
sidebar hover/selection feedback and native account-menu opening
and closing. Adapt those effects to existing Goosic destinations and controls; the clip does
not request new settings pages, Apple account actions or a different player placement. Added
motion excludes album artwork. Navigation and selection remain usable while effects run,
and system/app reduced motion and Efficiency mode disable added transitions.

The 03:15:05 recording is the corrected motion authority, inspected as 71 uncropped frames at
30 fps. The selected sidebar indicator stretches toward the destination, then its trailing
edge catches up and contracts; it is not a fixed-height line translating between tabs. The
observed transition occupies about 0.47–0.80 seconds. The page title remains stationary while
section headings and individual content items rise and become visible, with smaller starting
offsets for headings than for cards and rows. The content entrance occupies roughly
0.80–1.03 seconds. Windows uses about 22 logical pixels for section headings and 44 for items,
with a 240 ms decelerating arrival, driven by actual view arrival rather than a delayed page-wide
fade or fabricated loading sequence. Rapid selection changes retarget the same indicator.
Skeletons appear only while the page has no content, including during cached or partial
refreshes; they must not remain above already loaded shelves. Independent artwork arrival
effects remain excluded. Reduced motion uses immediate final geometry and full opacity.

Feed navigation prepares the destination before replacing the current canvas. Commit one
complete response and run the arrival once; do not first animate cached shelves and then
clear/recreate them when a refresh completes. The current feed remains usable while fetching.
Newer navigation or an account change invalidates an earlier pending response. Failures have
an honest destination error/retry state instead of replaying the entrance or leaving a skeleton.

Every icon action has a meaningful accessible name, suitable native hit target and keyboard
path. Single-selection controls expose their selection; disabled actions reflect actual
availability. Focus is distinct from selection, visible but restrained, and dismissible where
appropriate. The timeline must not retain a large stuck blue outline. Escape and completed
pointer interaction clear its temporary focus cue. Typing into a field must not trigger
transport shortcuts. Use the platform's standard modifiers and native text editing behavior.
High contrast, reduced transparency, larger text and keyboard-only use are acceptance cases,
not reasons to omit the shared control hierarchy.

Play/Pause changes through native symbol replacement where available, with a modest equivalent
state transition on other systems. Other player controls share press/state feedback. Typical
current macOS control transitions are about 180–280 ms; native timing can adapt. Apply the most
restrictive request from system Reduce Motion and the app preference. Efficiency modes may
suppress decorative motion too. Do not animate layout on playback ticks or resize the whole
page for a small glyph transition. The full player slides up from the bottom edge as one
surface, artwork included, as Music's does, rather than fading or scaling, which exposed clipped
window edges; with reduced motion it appears without the slide.

Fetch, artwork decoding, palette sampling and blur must not block the UI thread. Use bounded
work queues, cached/downsampled artwork and targeted view invalidation. Avoid a whole-shell
refresh for every downloaded image or clock tick. Keep content lazy where it is long. Idle
catalog surfaces may release resources when safe; the active playback host and authority lease
must survive UI navigation, resizing, overlays and presentation switches. Fewer web processes
is not a justification to destroy an authenticated profile or a playing renderer.

## Native adaptation and evidence

| Shared requirement | macOS | Windows | Linux |
| --- | --- | --- | --- |
| Shell and window | SwiftUI/AppKit; native titlebar/split view | WinUI; native window integration | GTK 4; native desktop decorations, without libadwaita |
| Selection control | Native segmented picker | Native single-selection control | Linked native selection group |
| Floating chrome | Liquid Glass / native material | Supported Acrylic/Mica and theme fallback | Native theme with translucent/solid fallback |
| Icon vocabulary | SF Symbols | Native Windows icons | Native symbolic icon theme |
| Motion | Symbol/content transitions | Native/compositor state transitions | Native widget transitions where supported |
| Accessible behavior | Native accessibility semantics | UI Automation semantics | GTK accessibility semantics |

These are implementation seams, not permission to change product composition or action meaning.
A platform without blur still has a floating capsule; a platform without SF Symbols still has
Play/Pause in the same role. Never reintroduce SwiftCrossUI or a shared view runtime to obtain
parity. Shared rules belong in Rust/protocol contracts, and each native shell renders them.

The audit combines static source review, the user's macOS screenshots and corrections, and
synthetic GTK desktop captures from the October 9 parity branch. Earlier Linux observations
came from origin/platform/linux at fc0a31e. Captures establish fixture rendering, not live
Windows/Linux playback or the user's visual acceptance; macOS acceptance remains separate.

| Area | Evidence and present difference | Required parity work |
| --- | --- | --- |
| Floating composition | Mac overlay search/player; Windows layered content/player; GTK Overlay shell | Preserve scrolling beneath chrome and initial content clearance in every shell |
| Search control | Mac segmented Picker; Windows independent filter buttons; GTK parity branch has floating grouped native toggles | Give all shells the same floating pill/selector composition and native single-selection semantics |
| Volume | Mac corrected overlay; Windows fixed speaker capsule; GTK parity branch uses a homogeneous stack to cover utility icons while More and the speaker remain reachable | Keep geometry fixed; verify native assistive-technology behavior |
| Player bar | Mac 740-wide capsule; Windows now the same width, with a Like button the Mac bar does not have, and it stays in place beside an opening panel until the panel would reach it; GTK parity branch has the same capsule width and native controls | Decide whether Like belongs in the bar on every platform; verify GTK text scaling |
| Lyrics | Mac blur, dimming and scale by distance with word-by-word fill; Windows the same without the word fill, having no word timings; GTK parity branch emphasizes synced lines with manual-follow suspension and line seeking | Carry word timings to Windows; adopt contextual effects and word rendering on GTK |
| Accent | Mac uses #FF0552; GTK parity branch adopts that token; Windows #F53150 remains a gap | Align Windows while preserving native/contrast selection exceptions |
| Density | Mac standard art 158; Windows palette card token 196; GTK artwork/card sizing differs | Compare at equivalent scaling and align perceived density to the current reference |
| Navigation/inspector | Mac native split regions; Windows adaptive columns/overlays; GTK parity branch adapts navigation and panels down to a 480-pixel window | Preserve native hit areas and verify text scaling and desktop contrast |
| Full/mini presentation | Mac and Windows immersive players; GTK parity branch has an expanded dark cover/lyrics/queue surface with inspected synthetic captures | GTK mini player and live presentation acceptance remain pending |
| Accessibility/material | Windows has explicit HighContrast brushes; Mac reduced-motion/material paths; GTK native/theme paths | Verify actual keyboard, contrast, transparency and scaling behavior on each desktop |

Primary source map: macOS NativeMacApp.swift, NativeMacCatalogSurface.swift,
NativeMacFullPlayer.swift and NativeMacCachedImage.swift under
apps/goosic-swift/Sources/GoosicSwift/Platform/macOS; Windows MainWindow.xaml,
MainWindow.Layout.cs, MainWindow.Motion.cs and Styles/Palette.xaml under
apps/goosic-windows/Goosic.Windows; Linux src/shell.rs, src/ui.rs, src/player_bar.rs and src/ui.css
on the platform/linux ref above. [CONTENT_PARITY.md](CONTENT_PARITY.md) and
[FEATURE_PARITY.md](FEATURE_PARITY.md) record content and feature capability separately.

## Required implementation and review practice

Before editing, identify the affected design sections and inspect the corresponding controls
in the other shells or their platform branch. A current discrepancy is a parity gap to record,
not a precedent for introducing another interpretation. Keep shared specification changes on
development-based branches and native implementation work on the correct platform branch.
Avoid copying presentation constants into several unrelated files; use each shell's existing
shared theme/layout definitions when applying a token.

Use representative, non-private fixture content: mixed shelves, long names, explicit tracks,
missing artwork, partial pages, empty results, an expired account and a playing queue. Compare
the same states on each platform, while allowing its native font and material rendering.

| Acceptance case | What must be inspected |
| --- | --- |
| Wide / narrow / short window | No clipped essential controls; Lyrics and Queue visible in the normal bar |
| Search before/after submit and scrolling | Pill, single selected category, no opaque header/divider, results pass beneath controls |
| Volume closed/open/closed | Bar bounds and artwork/transport positions unchanged; overlay usable; covered targets inactive |
| Panel and full/mini switches | Correct selected pane; session/queue preserved; no whole-page layout animation |
| Light / dark / pale and dark cover | Readable labels and coherent material, especially full-player top/bottom capsules |
| Keyboard / assistive technology / larger text | Reachable actions, focus and selection semantics, usable timelines, no text-shortcut collision |
| Reduced motion / transparency / contrast | Supported native fallback, stable layout, no lost action or meaning |
| Loading / refresh / offline / expired / partial / ad | Honest state, appropriate recovery, no fake success or stale signed-in claim |

Run the repository's required tests and focused platform checks for changed code. Record what
was actually inspected. Successful compilation and tests do not prove visual parity. If the
user owns live testing, finish the implementation and checks, provide the rebuild instruction,
and explicitly leave visual acceptance pending. Do not silently claim a screenshot match.

The September 30 macOS quality corrections add adaptive collection headers, measured player
clearance, native keyboard row navigation, accessible lyrics with optional following, compact
player volume and a return route, connected theme preferences, recoverable login errors, and
revision-bound queue clear/undo. These are implementation changes, not proof of live visual
acceptance. The native macOS acceptance matrix remains required; Windows and GTK adaptations
must retain the same behavior where their implementation still differs.

The October 9 Linux parity pass is implemented on `feature/linux/windows-parity`, synchronized
with the shared 0.2.7 trunk. Native GTK captures with synthetic content cover expanded cover,
lyrics and queue, responsive browsing and light/dark presentation. Queue editing/undo and
breakpoints have offline tests. Search clearance lives inside the virtualized content, and
volume opens without changing the bar's bounds. These checks do not establish live audio,
account mutation, screen-reader, word-timing or mini-player acceptance; [LINUX_SHELL.md](LINUX_SHELL.md)
records remaining work and the reproducible desktop fixture.
