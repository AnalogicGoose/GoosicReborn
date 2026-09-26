# Goosic 0.2.2 for Windows

If you have 0.2.0 or 0.2.1, Goosic offers this update itself: open Settings → Updates, or wait
for the notice shortly after launch. Otherwise download `Goosic-0.2.2-windows-x64-setup.exe` and
run it; it installs for the current user or updates an existing installation in place, keeping
account profiles and preferences. The portable ZIP is also available: extract the whole archive
and run `Goosic.Windows.exe` without moving it away from its companion files.

This release came out of an audit against Apple Music, Spotify and YouTube Music. What changed
since 0.2.1:

**Playing**

- **Like the song that is playing** with the heart on the player bar or in the full player, or
  Alt+Shift+B. Songs you liked before are already shown as liked.
- **Volume** has a slider on the player bar.
- **Sleep timer**: 15 to 60 minutes, or the end of the song, from the player's "⋯" menu. It
  pauses rather than quitting.
- **Mini player**: a small window that stays on top, from the "⋯" menu or the full player.
- A page's **Play** button becomes **Pause** while its list is playing.
- The queue says where it came from: "From Liked Music · 99 songs".
- The full player's position bar is white over the artwork instead of the system accent colour.

**Finding music**

- **Search starts with the top result**: search an artist and the artist comes first, with their
  songs credited to them. Podcast episodes no longer appear among songs, and there is a Videos
  filter. The search box remembers your last eight searches.
- **Moods & genres works**: coloured tiles, each opening that mood's playlists. Long pages such
  as a mood now load every section as you scroll instead of stopping after twelve.
- Artists are drawn as circles.

**Your library**

- **Pages open instantly.** Goosic keeps the last copy of each page and shows it at once, then
  quietly brings it up to date. Your library is read in the background after you sign in.
- **The library is a grid** you scroll down, as in Apple Music.
- **Library → Artists shows your artists.** It had been showing your subscriptions, which are
  empty for most accounts, and the Subscriptions tab showed your artists.
- **Liked Music is one page**: the sidebar, Library → Songs and the card on Home all open it, with
  its own cover, "100+ songs" while more is loading, **Sort** (title, artist, album, duration)
  and **Find in playlist**. Playlists get the same sort and find, and an album column.

**Everything else**

- **Discord status**: turn it on in Settings to show what you are listening to on Discord.
- **Debug mode** in Settings → Advanced. Normally a problem shows as one plain sentence; debug mode
  shows the technical detail too. Everything is written to a log either way, and **Open log
  folder** finds it for a bug report. The log stays on your computer and never records
  passwords, cookies or tokens.
- Settings is centred, Ctrl+, opens it, and the page fades under the title bar instead of running
  into the window buttons.

.NET and the Windows App SDK are bundled. Setup installs Microsoft's WebView2 Evergreen Runtime
if it is missing, requiring internet access. The portable ZIP requires WebView2 separately. The
application targets Windows 10 build 17763 or later and was tested on the build host. Native
ARM64, macOS, and Linux installers are not included.

The installer is unsigned, so Windows may show an unknown-publisher or SmartScreen warning.
`SHA256SUMS.txt` lists the Setup and ZIP, and the in-app updater installs a Setup only if it
matches; checksums verify integrity, not the publisher.

Build the installer with `apps/goosic-windows/build-installer.ps1 -Version 0.2.2`, which also
writes `SHA256SUMS.txt`, using the repository's Rust and Windows .NET/SDK toolchains and Inno
Setup 6. License files and third-party notices travel in the package. Corresponding application
source is available at the GitHub release tag. The bundled service speaks protocol `0.4.0`
exactly; 0.2.1 spoke `0.3.0`, and because shell and service ship together an update replaces
both at once.
