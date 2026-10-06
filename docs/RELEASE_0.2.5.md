# Goosic 0.2.5

Windows x64 is available as Setup and a portable ZIP. macOS is a universal alpha for Apple
silicon and Intel Macs running macOS 14 or later. This release includes the shipped 0.2.4
macOS UI and lyric improvements. No Linux download is included.

## Playback corrections

Previous and Next require a valid current song and queue entry; pressing them while idle no
longer starts an unrelated song. Previous restarts the first queue entry unless Repeat All
is selected. Manual Next moves forward even with Repeat One. Windows serializes competing
track-change requests. Launching Windows Goosic again restores the existing instance, including
when its window was closed to the tray while music kept playing. The new process redirects
activation before it can create another service or playback host.

The bridge selects the active media element instead of an ended advertisement left in the
page. Advertisement handoff cannot masquerade as a song ending. Volume and mute remain
available during advertisements, and scrolling over volume adjusts it. A queued playing
sample cannot undo a listener's pause request. Sleep-timer pauses remain intentional near
a song's end, and partial cached lists retain their incomplete counts until refreshed.

Windows adds player hover, progress, cover, mode-button and time-label transitions, respecting
system animations, Reduce Motion and Efficiency mode. macOS retains its native control,
lyrics and artwork animations. Live playback and visual acceptance still require device testing.

## Downloads and installation

[Windows Setup](https://github.com/AnalogicGoose/GoosicReborn/releases/download/v0.2.5/Goosic-0.2.5-windows-x64-setup.exe)
installs the app, a Start menu shortcut and an uninstaller. WebView2 is installed when missing
and requires internet. [Windows portable ZIP](https://github.com/AnalogicGoose/GoosicReborn/releases/download/v0.2.5/Goosic-0.2.5-windows-x64.zip)
can be unpacked and started with Goosic.Windows.exe. Setup is unsigned and Windows may show
an unknown-publisher warning. SHA256SUMS.txt covers both Windows downloads.

[macOS universal ZIP](https://github.com/AnalogicGoose/GoosicReborn/releases/download/v0.2.5/Goosic-0.2.5-macos-universal.zip)
contains Goosic.app; move it to Applications. The bundle is signed ad hoc, without Developer
ID notarization. After attempting the first launch, allow it through System Settings → Privacy
& Security → Open Anyway if macOS blocks it. SHA256SUMS-macos.txt covers the archive.

The Windows updater offers this stable release. The existing signed macOS alpha feed is
unchanged: this build must be downloaded manually because its update-signing key is held on
the release Mac and is unavailable to GitHub Actions. The bundle retains that feed and public
key for future signed updates. No replacement key is generated.
