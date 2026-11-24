# Goosic 0.4.4

Let the artwork do the talking.

## The cover, in its true shape

Square songs stay square. Videos stay wide.

Every cover used to be cut to a square, so a music video lost both of its sides in the player bar
and the full-screen player. Goosic now draws the cover at the shape of the picture itself: a song's
square art is unchanged, a video's frame is shown whole, and the title, timeline and controls move
to meet it. An unusually tall or wide picture is kept between 9:16 and 16:9, so it never becomes a
sliver. On macOS and Windows.

## Pink steps aside

The colour now comes from your music.

The pink accent is gone. Icons, the heart, the playing song's marker, the sung lyric line and
slider fills are drawn in a neutral colour instead: white in dark mode, near-black in light mode.
On Windows this also replaces the desktop's accent colour in Goosic's buttons, switches and menus,
so nothing turns red or blue. The browse categories and Settings tiles keep their colours, because
they mark a place rather than a state. On macOS and Windows.

## What still needs a live check

Tests pass; your eyes decide.

Both changes are covered by tests and by builds of each app, and they have not yet been looked at
on a running Mac or PC. A song next to a music video, in light and dark mode, is the check that
matters. Linux is not part of this release.

## Downloads and installation

Windows offers an x64 installer and portable ZIP. Extract the portable archive before
launching. The installer is unsigned; if SmartScreen intervenes, choose More info, then
Run anyway. SHA-256 checksums accompany the downloads.

The universal macOS ZIP supports Apple silicon and Intel Macs. Move Goosic to Applications.
The app is signed ad hoc; after the first refused launch, use System Settings → Privacy &
Security → Open Anyway. SHA-256 checksums accompany the Mac download.

The signed Sparkle feed lets installed Macs discover this release. Windows installations
check GitHub Releases for the new installer. Linux has no release download in this version.
