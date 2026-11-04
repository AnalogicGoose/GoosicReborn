# Goosic 0.3.1

Artist pages that stay responsive.

Windows x64 is available as Setup and a portable ZIP. macOS remains a universal alpha for
Apple silicon and Intel Macs running macOS 14 or later. No Linux download is included. This
release changes the macOS app only; the Windows downloads are rebuilt at the new version.

## Artist pages no longer freeze the Mac

On a Mac with a second display, opening an artist page could stop the window responding while
the music played on. The artist's banner took its width from a measurement of the page, and the
window works out its smallest size from the page, so each settled the other in turn. With two
screens at different pixel densities the rounding never came out the same twice, and the window
was laid out again for as long as the page stayed open. The banner now takes the width it is
given, and its height follows from that width, so there is nothing to settle.

## Add to Playlist holds still while a song plays

The Add to Playlist submenu in a song's ⋯ menu was rebuilt with every update from the player, so
while music played it flashed under the pointer and a playlist could only be chosen between two
flashes. It is now rebuilt only when your playlists change.

## Artist pages, tidied

An artist whose top songs YouTube Music also sends as a list of songs shows them once, in Top
Songs, including songs that arrive as the page loads further; albums, singles, videos and
related artists are left as they are. The page's own Play, Shuffle and Play Next use that one
list. And while an artist page loads, it opens in the artist's place at the banner's final
height, instead of showing the ordinary page heading first and jumping when the banner arrives.

The macOS changes were built and tested in CI. The freeze was reproduced on an Apple silicon Mac
with an external display, where 0.3.0 held a full processor core on an artist page and this build
stays idle; the playlist menu and the loading page still need a look with an account signed in.
