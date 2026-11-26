<!-- channel: experimental -->
# Goosic 0.4.7 beta 2

More colour, fewer silences. An experimental release.

This is the second beta of 0.4.7, and the first release named the way Apple names its own: a
version, then its beta. It reaches copies whose update channel, in Settings, is set to
Experimental. Copies on Stable stay on 0.4.6. Switching back to Stable never downgrades.

## The full player takes the cover's colours

The background moves with the music.

On Mac and Windows, the full player's background is now made from the playing song's own
cover: blurred, slowly turning, and as bright and saturated as the artwork itself, instead of a
generic gradient. It stops moving when the window is hidden or when Reduce Motion is on.

## Clear glass controls and bigger lyrics

Lighter on top, easier to read.

The full player's buttons, volume and the Like and More buttons now sit on clear glass that lets
the background show through. Lyrics beside the cover are larger, the same size on both
platforms, and fill the full height of the window, fading out gently toward the bottom.

## The song you chose, and everyone on it

Collaborations keep every artist.

A song by several artists now shows all of them rather than only the first. And when YouTube
Music swaps a song for its music video before it starts, Goosic now plays the song, as the web
app does, instead of going round in circles and skipping it.

## A stalled play recovers

Play means play.

When the official player drops a play request mid-load, Goosic now asks once more, gives a
silent load a deadline instead of waiting forever, shows that the song is loading, and writes
what happened to the log. This applies on Mac and Windows.

## macOS: full screen without the white band

The window stays clear edge to edge.

In full screen, the Mac's title bar area no longer turns into an opaque white strip above the
library and the full player. The toolbar keeps every one of its controls.

## What still needs a live check

These changes pass automated tests on both platforms. The Windows background and glass have been
checked by the build only, not by eye, and play recovery has not yet had long listening sessions.
If something misbehaves, the stable channel is one setting away.

## Downloads and installation

This is a pre-release. On a Mac running 0.4.6 or 0.4.7 with Experimental selected, Goosic offers
it through the usual update prompt. On Windows, copies from before this beta cannot read a beta
version, so install this one from the download below once; later betas then arrive through the
usual update prompt.

The x64 installer and portable ZIP are available below. Extract the portable ZIP before
launching. The installer is unsigned. If SmartScreen intervenes, choose More info, then Run
anyway.

The universal macOS alpha ZIP supports Apple silicon and Intel Macs. Move Goosic to
Applications. The app is signed ad hoc. If macOS refuses the first launch, use System Settings
→ Privacy & Security → Open Anyway.

SHA-256 checksums accompany the Windows and macOS downloads.
