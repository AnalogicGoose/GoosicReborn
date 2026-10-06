# Goosic 0.2.6

Windows x64 is available as Setup and a portable ZIP. macOS remains a universal alpha for
Apple silicon and Intel Macs running macOS 14 or later. No Linux download is included.

## Windows volume correction

The main player now has one volume control. Its existing slider opens in a small horizontal
capsule over the Lyrics and Playing Next buttons, matching the macOS interaction. Clicking
the speaker again or pressing Escape closes it without resizing the player or moving its
artwork and transport controls. Covered buttons cannot receive clicks or keyboard focus;
More keeps Lyrics, Playing Next and mute reachable, and the speaker context menu also offers
mute. Scrolling over the slider adjusts volume, and muting displays zero while retaining the
chosen level for unmute. The capsule fade respects system animations, Reduce Motion and
Efficiency mode.

The Windows single-instance activation fix and the playback, pause, cache and animation
corrections from 0.2.5 remain included. The macOS app is rebuilt with version 0.2.6 and build
number 4; its native playback and UI are unchanged by this Windows correction. Live visual
and assistive-technology acceptance of the volume capsule still needs device testing.

## Downloads and installation

[Windows Setup](https://github.com/AnalogicGoose/GoosicReborn/releases/download/v0.2.6/Goosic-0.2.6-windows-x64-setup.exe)
installs the app and Start menu shortcut. [Windows portable ZIP](https://github.com/AnalogicGoose/GoosicReborn/releases/download/v0.2.6/Goosic-0.2.6-windows-x64.zip)
can be unpacked and started with Goosic.Windows.exe. Setup is unsigned and may show an
unknown-publisher warning. SHA256SUMS.txt covers both Windows downloads.

[macOS universal ZIP](https://github.com/AnalogicGoose/GoosicReborn/releases/download/v0.2.6/Goosic-0.2.6-macos-universal.zip)
contains Goosic.app; move it to Applications. It is signed ad hoc without Developer ID
notarization. After the first launch attempt, use System Settings → Privacy & Security →
Open Anyway if macOS blocks it. SHA256SUMS-macos.txt covers the archive.

The Windows updater offers this stable release. macOS requires a manual download: its
existing signed alpha feed remains unchanged because the private signing key is held on the
release Mac and is unavailable to GitHub Actions. No replacement key is generated.
