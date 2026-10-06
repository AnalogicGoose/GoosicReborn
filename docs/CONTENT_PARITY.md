# Content parity with Goosic

GoosicReborn treats the previous Goosic application as a product map, not a source-code
dependency. This document records which music surfaces existed there and where their data now
comes from in Reborn, so a visually finished screen is not mistaken for a complete catalog.

The public discovery surfaces are Home, Explore, Search, Charts, Moods & genres, New releases,
artist pages, album pages, and playlist pages. Their metadata can be read anonymously and stays
inside `goosic-catalog`. Home is paged with YouTube Music's opaque continuation cursor, and its
parser covers artwork carousels, responsive song shelves such as Quick picks, and direct grids.
It reads them in the page's own order, and each shelf carries a `layout`: `list` for song rows,
which YouTube Music shows as a compact grid of rows, and cards otherwise (the default, left off
the wire). The personal reader sets the same field for signed-in Home. Before either existed,
every carousel was listed ahead of every row shelf, and every shelf was drawn as large cards, so
Home did not look like the page it came from. The shell appends each page when the user reaches the end instead of presenting the first browse
response as the whole feed.

The macOS Search page keeps a pill-shaped search field and the native segmented picker for
All, Songs, Albums, Artists, Playlists and Videos floating above its content. macOS draws the
picker's selection and chrome, including its current system appearance on macOS 26. Results scroll
behind the controls without a separate header band or divider; their initial clearance is
measured inside the scrollable content. See [the shared UI design specification](UI_DESIGN.md) and
[the macOS adaptation](MACOS_UI_GUIDELINES.md).
Its initial view shows saved recent queries and cards linking to
the existing Explore, Charts, Moods & genres and New releases routes. Those cards change
presentation, not catalog coverage. The player keeps Lyrics and Queue controls visible when
its controls wrap into additional rows in a narrow content column. Main, full and mini-player
controls share press feedback and symbol replacement transitions; system or app Reduce Motion
turns those animations off. Timeline focus retains keyboard seeking, uses a subtle focus cue,
and clears with Escape or a completed pointer seek. The full player's top controls use the same
dark capsule material as its bottom panel controls. Clicking the main player's speaker opens
a volume capsule layered over the bar's trailing controls; clicking it again or pressing
Escape closes it. This overlay does not participate in layout, so opening it never resizes the
bar or moves the artwork and transport. More stays available in the overlay, with Lyrics and
Up Next actions while their icons are covered. Mute remains in More and in the volume
control's context menu and accessibility actions.

Signed-in Home is different from guest Home. On a platform with a real account host, it is read
inside the active account's WebKit profile so shelves such as Listen again, Mixed for you,
personal mixes, familiar favourites, and recommendations reflect that account. Cookies stay in
WebKit; only normalized public music metadata reaches the shared model. Platforms that do not
yet implement this host report that limitation rather than silently showing guest data as
personalized.

The personal Library has four initial surfaces: playlists, liked songs, albums, and artists.
They use the same active-profile host and support their own continuation cursors. The reader
behind them is the previous Goosic's InnerTube client and shelf parsers, ported as
`PersonalCatalog.js` with their GPL notice, rather than a second parser written from scratch. Playlist and
album detail pages are read through the same account when one is active, because the anonymous
client cannot see a private one and does not report that it cannot: upstream answers a private
browse with an empty page, so a user's own playlist opened looking as though it had no tracks. If
the account cannot answer — the entity may simply be public and not in this library — the anonymous
route still runs, so nothing that used to work stops working. This
restores the content-reading half of the previous Library without moving credentials into Rust
or weakening the service protocol.

Native discovery cards start the selected song and fetch its Up Next station through the
account's personal reader. The reader calls `/next` and projects only the playlist panel;
Listen again and other discovery shelves are never queue inputs. Guest playback uses the
anonymous radio reader. A station keeps its original seed, reader/account identity, and
continuation; request revisions reject replies from a replaced queue. Album and playlist
track lists remain explicit ordered queues. The native sidebar also lists the account's
playlists with their artwork. Library mutation support exists, but complete state derivation
and UI coverage, category sub-pages, saved queues, podcasts, and channel switching remain
follow-up work.

The macOS official renderer receives the saved volume and mute preference with each load.
A document-start media gate applies those values before content playback and guards later
media resets. Advertisement-marked playback remains outside that gate. Native slider changes
update the gate's preference; renderer reports only confirm or reapply it. Executable
JavaScript fixtures cover the ordering and replacement behavior; they do not substitute for
listening through real advertisement transitions in WebKit.

The boundary is intentional. Anonymous catalog reads stay in Rust and are testable on every
platform. Account-scoped reads stay in the native browser profile that already owns the login
session. Credentials, cookies, and raw account responses never enter the NDJSON service protocol.

## Lyrics timing and native adoption

[LYRICS_TIMING.md](LYRICS_TIMING.md) records the shared LRCLIB Lyricsfile reader, optional word
wire fields, source fallback and the Windows/GTK implementation handoff. Word following requires
real timestamps; songs with only line timing emphasize complete lines. The user kept the
Music-like blur/fade/active-line effects and rejected estimated singing timing and a permanent
following switch. The service conversion is shared; native word rendering, manual exploration,
local import and accessibility adoption remain platform work. Do not confuse LRCLIB supporting
the format with verified word-timed song coverage. The documented September 30 sample had no
word arrays, so it does not establish live word-following acceptance.
