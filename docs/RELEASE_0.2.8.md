# Goosic 0.2.8

Windows x64 is available as Setup and a portable ZIP. macOS remains a universal alpha for
Apple silicon and Intel Macs running macOS 14 or later. No Linux download is included.

## Search keeps going

The Songs and Videos search tabs used to stop after twenty results and present them as the
whole list. YouTube Music continues those tabs, and Goosic now follows it: more rows arrive as
the end of the list comes into view, added to the list already shown rather than under a second
"Songs" heading. The Albums, Artists and Playlists tabs, and the All tab, are unchanged; each is
short enough to show its end, and All links to a tab per kind.

On Windows a search's song rows are a shelf of columns that scrolls sideways, so more arrive as
that shelf nears its sideways end, and the shelf keeps its place while they do. The Windows
change was built and tested on a Windows runner and has not yet been tried in the running app.

## macOS: smoother scrolling

Scrolling a long page was visibly behind Apple Music. Two causes accounted for nearly every late
frame, and both are fixed. A pointer resting over a list hovered each row that slid under it,
which made two frames in three late on a 1,500-song playlist; rows now hold their hover state
while the page scrolls, on macOS 15 and later, and the row under the pointer takes it up when
scrolling stops. And the playback position, which moves once a second, redrew the whole window;
it now redraws only the player bar, the full-screen player and the lyrics. Late frames on that
playlist fell from about 67% to under 1%.

## macOS: the artwork background and the app icon

With the artwork background on, a flat dark bar ran across the page under the toolbar and the
sidebar showed plain grey instead of the playing cover. Both now show the cover.

The app icon follows the system icon style on macOS 26: light, dark, clear and tinted. macOS 14
and 15 keep the single icon.

## macOS: when the service stops

If the playback service stops while a song is playing, the song now stops with it. Before, the
web player kept playing with nothing in Goosic deciding whether it should, and a discarded
service could keep running in the background after Goosic replaced it. The track stays selected;
reconnect from the sidebar and press Play to continue, including for a downloaded song. A request
that merely times out no longer counts as losing the service.

The macOS app is rebuilt with version 0.2.8 and build number 6. Its fixes still need a check on
a Mac with a real account: scrolling and hover, the artwork background in light and dark, the
icon in each style, and stopping the service during a song.

## Downloads and installation

[Windows Setup](https://github.com/AnalogicGoose/GoosicReborn/releases/download/v0.2.8/Goosic-0.2.8-windows-x64-setup.exe)
installs the app and Start menu shortcut. [Windows portable ZIP](https://github.com/AnalogicGoose/GoosicReborn/releases/download/v0.2.8/Goosic-0.2.8-windows-x64.zip)
can be unpacked and started with Goosic.Windows.exe. Setup is unsigned and may show an
unknown-publisher warning. SHA256SUMS.txt covers both Windows downloads.

[macOS universal ZIP](https://github.com/AnalogicGoose/GoosicReborn/releases/download/v0.2.8/Goosic-0.2.8-macos-universal.zip)
contains Goosic.app; move it to Applications. It is signed ad hoc without Developer ID
notarization. After the first launch attempt, use System Settings → Privacy & Security →
Open Anyway if macOS blocks it. SHA256SUMS-macos.txt covers the archive.

The Windows updater offers this stable release. macOS requires a manual download: its
existing signed alpha feed remains unchanged because the private signing key is held on the
release Mac and is unavailable to GitHub Actions. No replacement key is generated.
