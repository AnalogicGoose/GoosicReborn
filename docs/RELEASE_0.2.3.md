# Goosic 0.2.3 for Windows

A quick fix release on top of 0.2.2. An installed copy offers it in Settings shortly after
launch; otherwise download `Goosic-0.2.3-windows-x64-setup.exe` or the portable ZIP from
GitHub Releases. The protocol is unchanged at `0.4.0`.

## Fixed

**The keyboard's next and previous keys work (#18).** WebView2 registered the playing page as a
media session of its own, and Windows delivered the media keys to that session instead of to
Goosic's. The page deliberately has no next or previous handler, so those two keys did nothing.
Chromium's media-key handling is now switched off in Goosic's WebView2 environment, so every key
reaches the same handler as the buttons in the Windows media flyout.

**The volume slider follows an audio taper.** Loudness is heard logarithmically, so a slider that
moved the gain in a straight line spent most of its travel between loud and louder. The gain is
now the cube of the slider's position, the usual approximation of an audio-taper
potentiometer: half way is about -18 dB, and the bottom of the slider is still silence. The
volume keys step along the same curve. The saved preference is still the gain, so a volume chosen
in 0.2.2 plays at the same loudness; the slider simply shows it further up.

**Setup shows the right licence (#19).** The licence page showed only the GPL. The original
GoosicReborn code is MIT and the files ported from the previous Goosic are GPL-3.0, which makes
the distributed program GPL-3.0; the page now shows both texts and says which applies to what.

## Building

Build the installer with `apps/goosic-windows/build-installer.ps1 -Version 0.2.3`, which also
writes `SHA256SUMS.txt` for the in-app updater.
