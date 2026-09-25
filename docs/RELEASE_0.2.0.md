# Goosic 0.2.0 for Windows

Download `Goosic-0.2.0-windows-x64-setup.exe` and run it. Setup installs Goosic for the current
user, or updates an existing 0.1.0 installation in place; account profiles and preferences are
kept. The portable ZIP is also available: extract the whole archive and run
`Goosic.Windows.exe` without moving it away from its companion files.

**Goosic now updates itself.** 0.1.0 has no updater, so this release is installed by hand once.
From 0.2.0, Settings → Updates checks GitHub Releases, and an installed copy also checks shortly
after launch. Installing downloads the release's Setup, refuses it unless it came from this
repository's release downloads and its SHA-256 matches the release's `SHA256SUMS.txt`, runs it,
and reopens Goosic when it finishes. The portable ZIP reports new releases but cannot install
them in place.

What changed since 0.1.0:

- **Home looks like YouTube Music.** Quick picks and other song shelves are compact lists of rows
  in columns of four rather than large album cards, and a guest's Home keeps the page's own shelf
  order.
- **Volume no longer crackles.** Changing the volume while a song played muted and unmuted it on
  every step of the slider; now only the level changes.
- **Lyrics load more reliably.** Lookups use the song's real artist, album and length instead of
  the text under its title, fall back to search when LRCLIB's exact lookup is slow, accept plain
  lyrics from search, and retry once on their own instead of waiting for the panel to be reopened.
- **New settings.** The page Goosic opens on (Home, Library, Liked Music, or the last page),
  hiding explicit songs, and reduced motion, shared with the other shells' preferences; and for
  Windows, close to tray, launch at startup, silent now-playing notifications, remembered window
  size and placement, and an efficiency mode that asks Windows to run Goosic with EcoQoS.
- **An icon.** Goosic uses the macOS app's icon for its window, taskbar, Start menu, tray and
  uninstaller entry.

Discord status is present in Settings but disabled in this build: it needs a registered Discord
application, which this release does not yet have.

.NET and the Windows App SDK are bundled. Setup installs Microsoft's WebView2 Evergreen Runtime
if it is missing, requiring internet access. The portable ZIP requires WebView2 separately. The
application targets Windows 10 build 17763 or later and was tested on the build host, not a
clean installation of every supported Windows version. Native ARM64, macOS, and Linux
installers are not included.

The installer is unsigned, so Windows may show an unknown-publisher or SmartScreen warning.
`SHA256SUMS.txt` lists the Setup and ZIP; checksums verify integrity, not the publisher. The
in-app update path — downloading, verifying and silently installing a newer release — cannot be
exercised until a release after this one exists.

Build the installer with `apps/goosic-windows/build-installer.ps1 -Version 0.2.0`, which also
writes `SHA256SUMS.txt`, using the repository's Rust and Windows .NET/SDK toolchains and Inno
Setup 6. License files and third-party notices travel in the package. Corresponding
application source is available at the GitHub release tag. The bundled service speaks protocol
`0.3.0` exactly; the new shelf layout and preferences are optional fields within it.
