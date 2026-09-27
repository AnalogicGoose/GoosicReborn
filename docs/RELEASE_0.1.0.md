# Goosic 0.1.0 for Windows

Download `Goosic-0.1.0-windows-x64-setup.exe` and run it. Setup installs Goosic for the current
user, creates a Start menu shortcut, offers a desktop shortcut, and registers an uninstaller
in Windows Installed apps. Re-running setup updates the installation. Uninstall preserves
account profiles and preferences. The portable ZIP is also available: extract the whole archive
and run `Goosic.Windows.exe` without moving it away from its companion files.

This is the Windows x64 release. It includes catalog browsing, search, artist pages with Show
all, official WebView2 playback, sign-in, personal catalog, queue controls, lyrics, media keys,
and persistent preferences. Rust remains the single playback authority. There is no downloader;
Windows local-file playback and legacy preference import are unavailable.

.NET and the Windows App SDK are bundled. Setup installs Microsoft's WebView2 Evergreen Runtime
if it is missing, requiring internet access. The portable ZIP requires WebView2 separately.
The application targets Windows 10 build 17763 or later; this release was tested on the build
host, not a clean installation of every supported Windows version. Native ARM64, macOS, and
Linux installers are not included.

The installer is unsigned, so Windows may show an unknown-publisher or SmartScreen warning.
Compare downloads with `SHA256SUMS.txt` supplied with the release. Checksums verify file integrity;
they do not replace publisher signing. Installation, upgrade, shortcut creation, bundled-service
handshake, and uninstall were exercised in a temporary install directory. Fresh-machine playback
and installation of WebView2 on a machine without it remain unverified.

Build the installer with `apps/goosic-windows/build-installer.ps1 -Version 0.1.0` using the
repository's Rust and Windows .NET/SDK toolchains and Inno Setup 6. The script verifies the
Microsoft signature on the downloaded WebView2 bootstrapper. License files and third-party
notices travel in the package. Corresponding application source is available at the GitHub
release tag. The bundled service speaks protocol `0.3.0` exactly.
