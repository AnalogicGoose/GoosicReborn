# Goosic 0.2.4 macOS alpha 1

This macOS test build includes the native UI quality corrections and the lyrics improvements
from September 30. The universal app supports Apple silicon and Intel Macs running macOS 14
or later. Windows remains on stable 0.2.3; Windows and GTK adoption of the new lyric effects is
documented separately and is not included in this download.

## Changes

The native SwiftUI/AppKit shell replaces the former SwiftCrossUI shell. Collection headers
adapt to smaller windows, playback controls stay reachable, track rows support keyboard
navigation, and theme preferences apply to the browser. Queue clearing offers revision-bound
Undo, and sign-in and unavailable-content states provide clearer recovery actions.

Lyrics retain the requested Apple Music-style bold active line, contextual blur, edge fades,
glow and smooth following. Manual scrolling reveals readable context and a compact Resume
lyrics action. Reduced motion and increased contrast retain static, readable emphasis.

The shared LRCLIB reader accepts Lyricsfile 1.0 and uses real supplied word or segment
timestamps when available. Line-timed songs highlight the entire active line. There is no
estimated word schedule. Import Timed Lyrics accepts validated JSON for the current listening
session and never uploads it or changes the audio. See [the timing contract](LYRICS_TIMING.md)
for file format, provider fallback and platform adoption details. The live records sampled
for this release had no word timing; live singing alignment remains unverified.

## Install

Download `Goosic-0.2.4-macos-alpha.1-macos-universal.zip`, unzip it and move Goosic to
Applications. The app is signed ad hoc, without a paid Developer ID. If macOS refuses the
first launch, use System Settings → Privacy & Security → Open Anyway after attempting to open
it. The matching checksum is in `SHA256SUMS-macos.txt`.

The macOS alpha channel uses Sparkle updates signed with the existing project update key.
Goosic → Check for Updates can offer this build to earlier Mac alpha installations once the
feed is published. This is build 2, following the first alpha's build 1. It is a pre-release
and does not replace the stable Windows release for Windows updater checks.

## Verification

The Rust tests and all 240 Swift tests passed. Universal packaging additionally checks both
binary architectures, bundle resources and code signing. Tests and packaging do not establish
live playback, word alignment, older-macOS runtime compatibility or visual acceptance on every
supported Mac. Report alpha issues with the version, Mac model and macOS version.
