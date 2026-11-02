# Goosic 0.2.9

Lighter, faster, and easier to get around.

Windows x64 is available as Setup and a portable ZIP. macOS remains a universal alpha for
Apple silicon and Intel Macs running macOS 14 or later. No Linux download is included.

## Windows: a compact sidebar and a new Search page

Everything, one click away.

The expanded sidebar is now a compact icon rail with every destination one click away: Search,
Home, the Discover and Library pages, All Playlists and Settings. The menu button opens the full
navigation as a temporary panel over the page, and choosing a destination, clicking outside or
pressing Escape returns to the rail. Account actions, playlist creation and keyboard access are
unchanged, and short or narrow windows keep the player controls clear.

Search has one field, visible categories, recent searches as cards that can be cleared, and
colored browse cards. Headings and items arrive with a short animation, and the category
indicator stretches toward the new tab before catching up; both respect reduced motion and
Efficiency mode.

## Windows: pages change once

No more flicker between pages.

A page used to appear from its saved copy and then rebuild itself when the fresh answer arrived,
which cut its entrance animation short. Goosic now fetches the destination while the current
page stays on screen and replaces it once. A destination that takes longer than about a third
of a second shows its loading state, so a click on a slow connection is never ignored. If the
fetch fails, the copy Goosic saved earlier is shown instead of an error. Answers that arrive
after you have moved on, or that belong to an account you have switched away from, are thrown
away.

The Windows changes were built and tested on a Windows runner. The sidebar and Search were tried
in the running app; the slow-load, saved-copy and account-switch handling still needs a check on
a Windows PC.

## macOS: Settings in its own window

Change a setting. Keep your place.

Settings is no longer a page in the sidebar. It opens in a window of its own, like Music's, from
the account menu or with ⌘,, with General, Playback, Accounts and Advanced tabs. Changes still
apply as they are made.

## macOS: the full player slides up

Now playing, front and center.

The full player now slides up from the bottom edge as one piece, as Music's does, instead of
fading and scaling, which cut off the top of the window and stuttered. Its moving background is
cheaper to draw, and the cover moves with the panel. The window's close, minimize and zoom
buttons stay in one place on every page, and the sidebar list no longer fades at its edges.

## macOS: less memory

The same music, in a lighter app.

The connection to the playback service kept a small piece of memory for every message it read,
so Goosic grew for as long as it ran; it now releases each one. The hidden YouTube Music page
that plays the audio runs in WebKit's Lockdown Mode, which turns off browser features the player
does not use and roughly halved that page's memory in measurement.

The macOS app is rebuilt with version 0.2.9 and build number 7. Playback with a signed-in
account under Lockdown Mode was tried on one Mac; the slide and the Settings window were
inspected on macOS 27 only.

## Downloads and installation

[Windows Setup](https://github.com/AnalogicGoose/GoosicReborn/releases/download/v0.2.9/Goosic-0.2.9-windows-x64-setup.exe)
installs the app and Start menu shortcut. [Windows portable ZIP](https://github.com/AnalogicGoose/GoosicReborn/releases/download/v0.2.9/Goosic-0.2.9-windows-x64.zip)
can be unpacked and started with Goosic.Windows.exe. Setup is unsigned and may show an
unknown-publisher warning. SHA256SUMS.txt covers both Windows downloads.

[macOS universal ZIP](https://github.com/AnalogicGoose/GoosicReborn/releases/download/v0.2.9/Goosic-0.2.9-macos-universal.zip)
contains Goosic.app; move it to Applications. It is signed ad hoc without Developer ID
notarization. After the first launch attempt, use System Settings → Privacy & Security →
Open Anyway if macOS blocks it. SHA256SUMS-macos.txt covers the archive.

The Windows updater offers this stable release, and the macOS app's built-in updater offers this
build to earlier Mac alphas through its signed feed.
