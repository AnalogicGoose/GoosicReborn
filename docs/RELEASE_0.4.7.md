<!-- channel: experimental -->
# Goosic 0.4.7

One player, everywhere. An experimental release.

This is the first release on the experimental channel. It is a pre-release: it reaches copies
whose update channel, in Settings since 0.4.6, is set to Experimental. Copies on Stable stay on
0.4.6 until the next stable release. Switching back to Stable never downgrades.

## The same queue on Mac and Windows

Search starts a station.

Playing a song from search or a shelf now starts a station from it on both platforms, and an
album, a playlist or an artist's songs play in order. Up Next, shuffle, repeat, radio, Clear and
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
its end can move the queue on, on both platforms, and a song played again after it finished moves
the queue on again when it ends.

## What still needs a live check

These changes are covered by automated tests on both platforms, not yet by long listening
sessions. Sign-in, advertisement handoffs and the media overlay on Windows are the places to
watch. If something misbehaves, the stable channel is one setting away.

## Downloads and installation

This is a pre-release. On 0.4.6, choose Experimental under Updates in Settings and Goosic offers
it through the usual update prompt; on anything older, update to 0.4.6 first or download it below.

The x64 installer and portable ZIP are available below. Extract the portable ZIP before
launching. The installer is unsigned. If SmartScreen intervenes, choose More info, then Run
anyway.

The universal macOS alpha ZIP supports Apple silicon and Intel Macs. Move Goosic to
Applications. The app is signed ad hoc. If macOS refuses the first launch, use System Settings
→ Privacy & Security → Open Anyway.

SHA-256 checksums accompany the Windows and macOS downloads.
