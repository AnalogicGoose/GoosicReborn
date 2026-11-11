# Goosic 0.4.0

A sidebar that settles in, and a player that streams.

Windows x64 is available as Setup and a portable ZIP. macOS remains a universal alpha for
Apple silicon and Intel Macs running macOS 14 or later. No Linux download is included. This
release changes the Windows app and the Mac app, and moves the protocol between the app and its
service to 0.5.0, so each app runs only with the service it ships with.

## Windows: the sidebar docks when there's room

Navigation stays put on a wide window.

From about 1,000 pixels wide, the sidebar now opens docked beside the page, as Apple Music for
Windows does, and the page and player make room for it. On a narrower window the icon rail is
still the default, and the menu button opens the full list over the page. Docked, the sidebar
sits flush against the window's edge; the rail and the overlay keep their floating, rounded look.

## Windows: a darker, deeper look

Black glass over a near-black page.

The sidebar, the floating search and player bars, and the menus are now a black acrylic. The page
behind them is #1C1C1C with #282828 cards, the greys Apple Music for Windows uses, so artwork
stands out and the chrome steps back. Contrast themes still get solid surfaces.

## Windows: choose how clear the glass is

From solid to see-through, your call.

Settings › General › Appearance has a new Material transparency slider. Slide it left and the
sidebar, player bar and menus turn solid; slide it right and more of the page shows through.
The middle is the look as designed, so nothing changes until you move it. It never goes fully
clear, so labels always stay readable over album art. When Windows' transparency effects are off,
the slider says so instead of moving nothing.

## Windows: a sidebar that's yours

Hide what you don't use. Pin what you do.

Edit Sidebar, from the Sidebar actions button or Settings › General, lets you hide any Discover
or Library destination and All Playlists. Search, Home and Settings always stay, so the way back
is never hidden. Pin playlists, albums, artists and songs from their menus; pins appear under
Home and can be reordered by dragging, or with Move Up and Move Down from the keyboard.

## Windows: Settings, reorganised

Find a setting by what it's about.

Settings opens on a list of categories (Account, General, Playback, Windows, Updates and
Advanced), each with a short description, laid out like Windows 11 Settings. A breadcrumb at the
top takes you back to the list.

## Downloads are gone

Everything plays from YouTube Music.

Goosic no longer has a Downloads page, on Windows or on the Mac. It only ever listed the files a
previous Goosic had saved, and could not make new ones. Those files are left exactly where they
are; Goosic simply stops reading them. On the Mac this also removes the separate player that
played them, so every song plays through the official YouTube Music player.

The Windows changes were built and tested in CI and looked at in the running app, including the
transparency slider at both ends. The Mac changes build and pass their tests in CI, but nothing
on the Mac has been checked by hand yet; the missing Downloads page and playback without the old
local player need a hands-on look on a Mac.
