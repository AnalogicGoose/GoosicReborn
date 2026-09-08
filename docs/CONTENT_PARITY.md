# Content parity with Goosic

GoosicReborn treats the previous Goosic application as a product map, not a source-code
dependency. This document records which music surfaces existed there and where their data now
comes from in Reborn, so a visually finished screen is not mistaken for a complete catalog.

The public discovery surfaces are Home, Explore, Search, Charts, Moods & genres, New releases,
artist pages, album pages, and playlist pages. Their metadata can be read anonymously and stays
inside `goosic-catalog`. Home is paged with YouTube Music's opaque continuation cursor, and its
parser covers artwork carousels, responsive song shelves such as Quick picks, and direct grids.
The shell appends each page when the user reaches the end instead of presenting the first browse
response as the whole feed.

Signed-in Home is different from guest Home. On a platform with a real account host, it is read
inside the active account's WebKit profile so shelves such as Listen again, Mixed for you,
personal mixes, familiar favourites, and recommendations reflect that account. Cookies stay in
WebKit; only normalized public music metadata reaches the shared model. Platforms that do not
yet implement this host report that limitation rather than silently showing guest data as
personalized.

The personal Library has four initial surfaces: playlists, liked songs, albums, and artists.
They use the same active-profile host and support their own continuation cursors. Playlist and
album detail pages continue through the existing entity routes after a card is selected. This
restores the content-reading half of the previous Library without moving credentials into Rust
or weakening the service protocol.

The following product areas from the previous Goosic remain separate follow-up work: library
mutations such as liking, following, creating, editing, and deleting playlists; a dedicated
user-playlist sidebar index; category sub-pages; saved queue creation; podcasts; and authenticated
channel switching. None of those are represented as complete merely because their read-only
content can now be displayed.

The boundary is intentional. Anonymous catalog reads stay in Rust and are testable on every
platform. Account-scoped reads stay in the native browser profile that already owns the login
session. Credentials, cookies, and raw account responses never enter the NDJSON service protocol.
