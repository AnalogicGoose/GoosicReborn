# Goosic 0.2.10

Artists, front and center.

Windows x64 is available as Setup and a portable ZIP. macOS remains a universal alpha for
Apple silicon and Intel Macs running macOS 14 or later. No Linux download is included.

## Artist pages open with the artist

Their picture, their colour.

An artist page used to be a heading over a list, and its only picture was borrowed from an
album or a song. It now opens the way Music's does: the artist's own banner across the page,
the name and monthly audience over its foot, and three round buttons under them. About shows
the artist's description and subscriber count, Play plays the top songs (and pauses them when
they are already playing), and Subscribe follows the artist when an account is signed in. The
whole page takes the banner's colour, darkened until white text reads over it.

Scroll, and the page slides over the picture: the banner stays where it is, blurs, and then
dissolves into the artist's colour, so by the time the name has gone the page is that colour
alone. An artist with only a square picture shows it as a circle instead, as the previous
Goosic did. The banner and every fact come from YouTube Music's own header for the artist, now
asked for at full resolution; when it has no picture at all, Goosic shows the colour rather
than a song's cover. Reduced transparency, increased contrast and reduced motion are respected,
and on Windows so is Efficiency mode.

On a Mac, Shuffle is in the page's ⋯ menu and skips songs that Hide Explicit hides. On
Windows, the page has its own Copy Link and ⋯ buttons at the top right, the ⋯ menu holds Play,
Shuffle, Start Radio and Play Next, and Shuffle is also on Play's right-click menu.

## Go to Artist goes to the artist

From any menu, every time.

Go to Artist and Go to Album in the full player opened the page behind it, so nothing seemed to
happen. They now close the full player first, and from the mini player they bring the main
window forward. Both stay in the menu whenever the song names an artist or album, dimmed when
there is no link, so the menu no longer changes shape from song to song. Songs played from an
album or artist page now carry their artist and album too, so Go to Artist no longer
disappears for them.

## Windows: a More menu in the full player

The playing song's actions, within reach.

The full player had Like but no menu. It now has More beside Like, as the macOS one does, and
the song menus everywhere follow the macOS menus item for item, with rounded rows highlighted
in Goosic's colour. Choosing Downloads again while it was still loading no longer leaves it
loading forever. A mouse wheel over a row of cards scrolls the page smoothly again, with the
system's own distance and acceleration.

## Volume that sounds even

Half way sounds like half way.

The macOS volume slider sent its position straight to the player, so half way up was already
nearly full volume. Both platforms now share one audio taper, so a saved volume opens at the
same position everywhere, and on a Mac dragging the slider no longer stutters.

The macOS changes were built and tested in CI and inspected in the running app. The Windows
changes were built and tested on a Windows runner; the artist page and the full player's menu
still need a look on a Windows PC.
