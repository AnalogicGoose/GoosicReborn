# Goosic 0.4.2

Lyrics that keep time.

## Lyrics for the recording you hear

The right words, at the right moment.

Some songs' lyrics ran seconds early or late, because the lookup could choose timing written for
a live, extended or video version. Lyrics are now matched to the length of the recording that is
actually playing, on macOS and Windows. When the closest lyrics belong to a recording more than
three seconds longer or shorter, Goosic shows them as plain text rather than highlight the wrong
line. Offset tags in lyrics files are honoured.

## In step through every pause and seek

The highlight stays with the song.

On macOS, lyrics follow every position the player reports instead of a once-a-second copy, so
the sweep no longer stalls each second and pausing no longer pulls the highlight back. On
Windows, the lyrics no longer stutter for a moment after a seek, and they keep following while
you drag the progress bar.

## Windows: lyrics, drawn like the Mac

The same listening view, on every desktop.

The sung line now sweeps with a soft edge and a gentle glow, and settles in without a dark
flash. Lines move on the same springs as on the Mac, at the same speed, and the interlude dots
breathe in place. Lines in the side panel can be chosen to play from, and the panel uses the
Mac's sizes. Word lift and the swell of held notes remain macOS-only.

## Windows: import timed lyrics

Bring your own timing.

Right-click the lyrics, in the side panel or the full player, and choose Import Timed Lyrics to
load a timed lyrics file for the song that is playing. It uses the same file format and checks
as the Mac, and lasts for the listening session.

## Go to Artist and Go to Album, everywhere

Every song leads somewhere.

Go to Artist and Go to Album now work for the song that is playing and for every row in Up Next
when you are signed in, and Go to Album works on song cards in Home and Explore.

## What still needs a live check

Tests pass; your ears decide.

The Windows lyrics motion and interlude dots have been checked on a live desktop. Lyric timing
after seeks and drags, the import, and the length matching against real songs have been checked
by tests and builds only, as has all of the macOS lyrics work. Linux continues to look lyrics up
by the catalog's length.

## Downloads and installation

Windows offers an x64 installer and portable ZIP. Extract the portable archive before
launching. The installer is unsigned; if SmartScreen intervenes, choose More info, then
Run anyway. SHA-256 checksums accompany the downloads.

The universal macOS ZIP supports Apple silicon and Intel Macs. Move Goosic to Applications.
The app is signed ad hoc; after the first refused launch, use System Settings → Privacy &
Security → Open Anyway. SHA-256 checksums accompany the Mac download.

The signed Sparkle feed lets installed Macs discover this release. Windows installations
check GitHub Releases for the new installer. Linux has no release download in this version.
