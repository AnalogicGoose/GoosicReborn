# Goosic 0.4.3 Linux alpha 1

Goosic, at home on the Linux desktop.

This is the first Goosic download for Linux: a native GTK 4 app, packaged as a Flatpak, built on
the same Rust playback authority as the Mac and Windows apps. It is an alpha. Some of what it
does has been heard and seen on a real desktop, and some has only been built and tested; the
section on live checks says which is which. macOS and Windows stay on Goosic 0.4.3.

## A native Linux app

Built for GTK, not ported to it.

Goosic on Linux is written in Rust against GTK 4 and WebKitGTK, without libadwaita, so it takes
your desktop's own theme: Breeze on Plasma, Adwaita on GNOME. It browses and searches the
catalog, opens albums, artists and playlists, signs in with your account, and plays through
YouTube Music's official player under Rust's control. Advertisements are reported, never skipped.
Your desktop's media panel shows the song and its controls, a status icon sits in the tray where
your desktop has one, and closing the window keeps the music playing until you quit.

## Frosted glass

Depth, drawn by GTK itself.

The sidebar, the lyrics and queue panel, the floating player, the search field and the Full
Player's controls are now frosted glass. GTK has no CSS blur, so Goosic blurs what lies behind
each surface inside GTK's own renderer and lays a soft veil, highlight and rim over it. High
contrast turns the glass into solid, crisp surfaces.

## Color from the cover

The Full Player wears the song.

Behind the Full Player and the Mini Player, a slowly drifting field of color is drawn from the
playing cover: its five most common colors, arranged on a six-by-six mesh. The same cover always
makes the same arrangement. Reduced motion holds it still, and high contrast replaces it with a
solid background. The design matches the Mac's.

## Lyrics, word by word

Follow every word as it is sung.

When a song's lyrics carry real word timing, each word lights up as it is sung. Songs timed by
line light the whole line, and Goosic never guesses at word timing it doesn't have. Lines around
the active one soften with distance and come sharp when you point at them or scroll, and Resume
lyrics brings the view back. Import Timed Lyrics applies your own timing file to the playing song
for this session only. It is never uploaded.

## Mini Player

Your music, in a corner.

A compact 320 by 470 window with the Full Player's colors, glass and controls. The cover steps
back when you pause. Close it, and the music keeps playing.

## Your song on Discord

Share what you're listening to.

Turn on Discord status in Settings, and the song you're playing appears on your Discord profile.
Goosic talks only to the Discord app already running on your computer, which publishes the
status for you. Goosic never sees your Discord account. The Flatpak can reach Discord installed
natively or as a Flatpak.

## The song you chose plays

No more jumping to the next row.

When YouTube Music swaps in another version of the song you chose, such as the music video for
the audio track, that version plays and Goosic still shows your choice. A song the player won't
play at all is dimmed with a note and skipped, and a pause you press during the swap holds. On
Linux, a refused song can still be added to a new queue or radio; that filtering is not in this
alpha.

## Updates that find you

New builds, without the search.

A few seconds after launch, Goosic checks GitHub Releases for a newer Linux build, and Settings →
Updates checks on request. When one is available, Goosic opens its download in your browser. It
never installs anything by itself, and the check carries no account or cookie.

## What still needs a live check

Tests pass; your desktop decides.

On a Plasma desktop with Intel graphics, earlier builds of this shell have played a real track
through the official player, reported the advertisement in front of it, answered the media panel,
served the tray menu, and kept playing with the window closed. A sign-in has been completed inside
the Flatpak. These have not yet been checked on a live desktop: staying signed in after a restart
of the Flatpak, account changes such as editing playlists, a whole song ending with the window
hidden, the new glass and color backdrops on a real compositor, word-by-word lyrics against a song
that has word timing, the Mini Player during playback, Discord status with a running Discord, an
update offered from a published release, and a real swapped or refused song. GNOME, Xfce, COSMIC
and NVIDIA graphics are untested. One installed session recorded a crash in WebKit's network
process whose effect on playback is unknown. Please report what you find with your distribution,
desktop and graphics hardware.

## Downloads and installation

Linux is offered as an x86_64 Flatpak bundle, `Goosic-0.4.3-linux-alpha.1-linux-x86_64.flatpak`.
It needs Flatpak and the Flathub remote; the GNOME 50 runtime is installed from Flathub on first
install if it is missing. The bundle is not signed, and it is not on Flathub.

```sh
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
sha256sum -c SHA256SUMS-linux.txt
flatpak install --user Goosic-0.4.3-linux-alpha.1-linux-x86_64.flatpak
flatpak run io.github.analogicgoose.Goosic
```

Run the checksum command in the folder holding both downloads. After installation, Goosic also
appears in your desktop's application menu. To install a later alpha, download it and run
`flatpak install --user --reinstall` with its file name; your settings and sign-ins are kept.
To remove Goosic, run
`flatpak uninstall --user io.github.analogicgoose.Goosic`; add `--delete-data` to remove its
settings and sign-ins as well.

The Flatpak can read the previous Goosic's data in `~/.local/share/com.github.ivasy.ytubic` to
import its preferences, read-only. It keeps its own settings and sign-ins under
`~/.var/app/io.github.analogicgoose.Goosic/`, separate from any development build.
