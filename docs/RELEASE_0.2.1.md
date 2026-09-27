# Goosic 0.2.1 for Windows

If you have 0.2.0, Goosic offers this update itself: open Settings → Updates, or wait for the
notice shortly after launch. Otherwise download `Goosic-0.2.1-windows-x64-setup.exe` and run it;
it installs for the current user or updates an existing installation in place, keeping account
profiles and preferences. The portable ZIP is also available: extract the whole archive and run
`Goosic.Windows.exe` without moving it away from its companion files.

What changed since 0.2.0:

- **Edit your playlists.** On a playlist you own, the menu beside its title has **Edit songs**.
  Each row gets a checkbox and a click selects it instead of playing it; the bar above the list
  moves the chosen songs up or down or removes them. Changes appear at once and are sent to
  YouTube Music one entry at a time; if it refuses one, the playlist reloads so what you see
  never differs from the real playlist. Songs are reordered with the up and down buttons —
  dragging is not supported yet.
- **Save to playlist from the player bar.** The "⋯" menu on the player bar had only the queue's
  actions. It now offers like, dislike, Save to playlist, start radio, go to artist or album, and
  copy link for the song that is playing.
- **Moving playlist songs uses YouTube Music's own request.** The request Goosic would have sent
  to move a song named a field YouTube Music does not define. Nothing used it before this
  release; it now names the song the moved one should come before, as the web player does.

Removing and moving songs were built and tested against recorded behaviour, not yet against
a live account in this release. If a move does not land where you expected, the playlist
reloads to show its real order; please report it.

.NET and the Windows App SDK are bundled. Setup installs Microsoft's WebView2 Evergreen Runtime
if it is missing, requiring internet access. The portable ZIP requires WebView2 separately. The
application targets Windows 10 build 17763 or later and was tested on the build host. Native
ARM64, macOS, and Linux installers are not included.

The installer is unsigned, so Windows may show an unknown-publisher or SmartScreen warning.
`SHA256SUMS.txt` lists the Setup and ZIP, and the in-app updater installs a Setup only if it
matches; checksums verify integrity, not the publisher.

Build the installer with `apps/goosic-windows/build-installer.ps1 -Version 0.2.1`, which also
writes `SHA256SUMS.txt`, using the repository's Rust and Windows .NET/SDK toolchains and Inno
Setup 6. License files and third-party notices travel in the package. Corresponding application
source is available at the GitHub release tag. The bundled service speaks protocol `0.3.0`
exactly.
