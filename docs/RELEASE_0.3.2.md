# Goosic 0.3.2

Songs that play, or say why they can't.

Windows x64 is available as Setup and a portable ZIP. macOS remains a universal alpha for
Apple silicon and Intel Macs running macOS 14 or later. No Linux download is included. This
release changes the Windows app only; the macOS download is rebuilt at the new version with no
change to the Mac app.

## Windows: songs YouTube Music swaps now play

Pick a song and it plays, even when YouTube Music offers another version of it.

Sometimes YouTube Music replaces the song you chose with another version of the same song. The
player accepted the swap and then paused it at 0:00, so the click looked like it did nothing.
The replacement is now loaded under its own identity and plays. A different song is still left
to the queue, as before.

## Windows: a song that can't start tells you

No more silent clicks.

A track that loaded but never started used to sit at 0:00 with nothing to say why. If the page is
still paused after four seconds, Goosic asks it to play once. If nothing is heard after twenty,
the song is marked unavailable and the queue moves on. A song the page cannot load, or that
Goosic cannot start, now says so on screen instead of only in debug mode, and a WebView2 start-up
timeout no longer closes the app. Clicking the song you just chose, while it loads, no longer
restarts the load.

A page that was being replaced could also finish loading just as the next song was chosen and
cause the new song to be skipped. That no longer happens.

## Windows: a quieter log while music plays

Playback leaves less behind.

Goosic's log used to be written to disk from the interface thread about twelve times a second
while music played. It is now written in batches in the background, a playing song is logged when
its state changes and otherwise every five seconds, and a full log is kept as `bridge.log.1`
instead of being deleted, so the record of a song that failed to play is still there after the
next few.

The Windows changes were built and tested in CI, including the rule that decides when a song is
given up on. They have not yet been checked against a signed-in account: substituted versions and
songs that never start need a hands-on look on Windows.
