# Goosic 0.4.3

The song you chose, every time.

## macOS: the song you chose plays

No more jumping to the next row.

Choosing a song could start the one listed after it instead, without a word. YouTube Music's
player sometimes replaces a song before it starts, and the Mac took that for the song ending and
moved on. Now, when the player swaps in another version of the same song, such as the music
video for the audio track, that version plays in its place and Goosic still shows the song you
chose. A brief switch while the player loads is ignored. Windows already worked this way.

## macOS: a song that won't play says so

Skipped, and told why.

When YouTube Music will not play a song at all, Goosic now says so by name and moves on, instead
of starting the next song silently. The song is dimmed wherever it is listed, with a note that it
isn't available to play, and is left out of new queues and radio for the rest of the session. You
can still choose it again. This matches Windows.

## A pause that stays paused

Pause means pause, even mid-swap.

When the player swaps a song for another version, pressing pause while Goosic is still working
out what the player switched to used to be undone the moment the new version loaded. The pause
now holds on macOS and Windows, and pressing Play afterwards plays as you'd expect. Windows
already played swapped songs and named refused ones; this is its only change in 0.4.3.

## What still needs a live check

Tests pass; your ears decide.

On a Mac, the song that prompted this fix played through without a skip. The case where the
player swaps in another version, a real refusal, and a pause pressed during a swap have been
checked by tests and builds only, on both platforms. Linux is not part of this release.

## Downloads and installation

Windows offers an x64 installer and portable ZIP. Extract the portable archive before
launching. The installer is unsigned; if SmartScreen intervenes, choose More info, then
Run anyway. SHA-256 checksums accompany the downloads.

The universal macOS ZIP supports Apple silicon and Intel Macs. Move Goosic to Applications.
The app is signed ad hoc; after the first refused launch, use System Settings → Privacy &
Security → Open Anyway. SHA-256 checksums accompany the Mac download.

The signed Sparkle feed lets installed Macs discover this release. Windows installations
check GitHub Releases for the new installer. Linux has no release download in this version.
