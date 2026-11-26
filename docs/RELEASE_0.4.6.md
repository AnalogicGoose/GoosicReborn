<!-- channel: experimental -->
# Goosic 0.4.6

One player, everywhere. An experimental release.

This is the first release on the experimental channel. It is a pre-release: copies on the stable
channel are not offered it, and stay on 0.4.5 until the next stable release.

## Choose how new you want it

Stable, or first in line.

Settings now has an update channel on macOS and Windows. Stable, the default, offers tested
releases only. Experimental also offers pre-releases like this one, with new features before
they settle. Switching back to stable never downgrades: Goosic keeps what it has until a stable
release passes it.

## The same queue on Mac and Windows

Search starts a station.

Playing a song from search, a shelf or an artist page now starts a station from it on both
platforms, and an album or playlist plays in order. Up Next, shuffle, repeat, radio, Clear and
Undo are decided by one shared engine instead of two copies that had drifted apart, which is
why a search result on the Mac used to queue only the songs beside it.

## One set of rules underneath

Same answer, whichever computer.

The volume curve, the time readout, seeking, when a song has finished, which artwork may load,
the cover's shape, the artist page's colour, sign-in checks, the system media controls and the
page scripts now come from one shared library on both platforms. On Windows the player's clock
now reads 2:05 rather than 02:05, and the media overlay turns Next and Previous off when there
is nothing to skip to, as on the Mac.

## Advertisements hand back cleanly

The song picks up where it was.

When an advertisement ends and leaves the song paused, Goosic now asks it to play again on
Windows too, once, and never against your own pause. A song also has to be heard playing before
its end can move the queue on, on both platforms.

## What still needs a live check

These changes are covered by automated tests on both platforms, not yet by long listening
sessions. Sign-in, advertisement handoffs and the media overlay on Windows are the places to
watch. If something misbehaves, the stable channel is one setting away.

## Downloads and installation

This is a pre-release. Copies on 0.4.5 are not offered it automatically; download it below, or
choose the experimental channel after installing it to keep receiving experimental releases.

The x64 installer and portable ZIP are available below. Extract the portable ZIP before
launching. The installer is unsigned. If SmartScreen intervenes, choose More info, then Run
anyway.

The universal macOS alpha ZIP supports Apple silicon and Intel Macs. Move Goosic to
Applications. The app is signed ad hoc. If macOS refuses the first launch, use System Settings
→ Privacy & Security → Open Anyway.

SHA-256 checksums accompany the Windows and macOS downloads.
