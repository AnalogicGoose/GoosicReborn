# Goosic 0.2.7

Windows x64 is available as Setup and a portable ZIP. macOS remains a universal alpha for
Apple silicon and Intel Macs running macOS 14 or later. No Linux download is included.

## The Windows player, rebuilt after the macOS one

The player bar is now the compact capsule the design contract describes: 740 wide at most,
with the position line under the artwork and the title together. It no longer swells when the
pointer crosses it. Pointing at the position line, and only at it, shows the elapsed and
remaining times and thickens the line. Every button dips and springs back when pressed, Play
and Pause trade places as a symbol does on macOS, and Previous and Next nudge in the direction
they point.

Volume has one speaker button that never moves. Clicking it opens a slider capsule around it
and clicking it again, or pressing Escape, closes it. Lyrics, Playing Next and Like step aside
while it is open and More stays reachable beside it. Turning the mouse wheel over the speaker
changes the volume without opening anything. Closing the capsule used to hide the button that
held keyboard focus, which sent focus to the search box and opened recent searches; that no
longer happens. A toggle that was switched off again could stay drawn as on, and no longer does.

The bar keeps its place when Lyrics or Playing Next opens. It moves only when the panel would
reach it, by exactly as far as it has to, and it travels there instead of jumping.

## The page, the full-screen player and lyrics

The page used to fade under the title bar through a dark gradient, which showed as a band
across the top of the window whenever the playing cover coloured the background. The page
itself now thins out there, as on macOS, and the real background shows through.

The full-screen player places its cover and lyrics in the middle of the window as one group,
where the cover used to sit against the left edge.

Lyrics take the macOS listening treatment in the side panel and in the full-screen player. The
line being sung is sharp; the lines around it dim, blur and step back further the further away
they are, and the list fades out at both ends. Pointing at a line sharpens it. Scrolling the
lyrics stops them following the song and sharpens all of them for reading, and Resume lyrics
returns to the line being sung. The word-by-word fill of the macOS shell is not included,
because the Windows lyrics carry no word timings.

Blur and the faded edges are left out in a contrast theme, with Windows transparency effects
off, and in Efficiency mode. All motion respects system animations and Reduce Motion.

The macOS app is rebuilt with version 0.2.7 and build number 5; its native playback and UI are
unchanged by this Windows work. Assistive-technology acceptance of the Windows player still
needs device testing.

## Downloads and installation

[Windows Setup](https://github.com/AnalogicGoose/GoosicReborn/releases/download/v0.2.7/Goosic-0.2.7-windows-x64-setup.exe)
installs the app and Start menu shortcut. [Windows portable ZIP](https://github.com/AnalogicGoose/GoosicReborn/releases/download/v0.2.7/Goosic-0.2.7-windows-x64.zip)
can be unpacked and started with Goosic.Windows.exe. Setup is unsigned and may show an
unknown-publisher warning. SHA256SUMS.txt covers both Windows downloads.

[macOS universal ZIP](https://github.com/AnalogicGoose/GoosicReborn/releases/download/v0.2.7/Goosic-0.2.7-macos-universal.zip)
contains Goosic.app; move it to Applications. It is signed ad hoc without Developer ID
notarization. After the first launch attempt, use System Settings → Privacy & Security →
Open Anyway if macOS blocks it. SHA256SUMS-macos.txt covers the archive.

The Windows updater offers this stable release. macOS requires a manual download: its
existing signed alpha feed remains unchanged because the private signing key is held on the
release Mac and is unavailable to GitHub Actions. No replacement key is generated.
