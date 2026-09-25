# Feature parity with Apple Music, Spotify and YouTube Music

This document records an audit of the Windows shell against Apple Music, Spotify and YouTube
Music, what the audit changed, and what the macOS and Linux shells need to do to reach the same
place. [CONTENT_PARITY.md](CONTENT_PARITY.md) answers a different question, whether Reborn can
read everything the previous Goosic could. This one is about whether it behaves the way people
expect a music app to behave. The bar was set on purpose at Apple's level of detail, because the
gaps it found were small things that each made the app feel unfinished rather than large missing
screens.

The work landed in two layers. Anything that changes what the service says, or a rule with no
platform in it, is on `development` and every shell already has it. Everything else is how the
Windows shell presents it, and it is described here as behaviour, so a shell on another
platform can reproduce the behaviour in its own toolkit rather than port WinUI code.

## Shared: already on every platform

**Search leads with YouTube Music's top result.** An unfiltered search answers with a
`musicCardShelfRenderer` holding its best guess, usually the artist the query names, followed by
a few of that artist's songs. The parser skipped it, so searching an artist's name listed other
artists but not that one. `search_page` now emits a first shelf titled *Top result*, laid out as
a list: the featured item, then its songs. Those songs omit their artist on the wire because the
card above them names it, so the card's artist is written back into each one; without that they
read as songs by nobody. They are not repeated under *Songs*.

**Podcast episodes are videos, not songs.** An episode row carries a video id, so it used to fall
through to `Song` and appear in the *Songs* shelf of an unfiltered search.

**Moods & genres opens, and the protocol is now 0.4.0.** The page is two grids of
`musicNavigationButtonRenderer` buttons, which the parser did not read, and opening one needs a
browse id *and* parameters, which the request payload had no field for. The buttons now become
items of a new kind, `category`, with an optional `color` (`#RRGGBB`, the stripe YouTube Music
gives each button). The id packs the browse id and parameters together, separated by `|`, and
the new `catalog.category` command takes that id back; only the service ever splits it. A
category opens as an ordinary page of carousels, so any shell that can draw Home can draw it.
Because shells and service must speak the same version string, the constant moved in
`goosic-protocol`, in the fixtures, in the Windows shell and in the Swift shell's hand-copied
`goosicProtocolVersion`. [COMPATIBILITY.md](COMPATIBILITY.md) has the details.
`goosic-shell-support` maps a category to `CardAction::Show(EntityReference::Category(id))`, and
`CatalogKey::Category` caches it, so the Linux shell gets navigation for free.

**Artwork from `gstatic.com` is allowed, and Home ends where YouTube Music ends it.** Both
landed shortly before the audit. Liked Music and the yearly Recap draw their art from
`gstatic.com`, which the allow-list refused, so their cards were blank. And YouTube Music now
answers past the last page of Home with an empty tab frame, which `PersonalCatalog.js` reported
as an unrecognised response; it is now the end of the feed.

## Windows: behaviour to reproduce on macOS and Linux

Each item says what the listener sees and the rule behind it. The Windows file is named so the
implementation can be read, not so it can be copied.

**A like button beside the song.** The player bar and the full player have a heart that fills
once the song is liked (`MainWindow.xaml`, `ShellViewModel.Account.cs`). It is disabled for a
guest, whose like would be stored nowhere. The catalog does not say whether a song is liked, so
at sign-in the shell reads the first page of Liked Music (`VLLM`) through the personal reader and
treats those songs as liked; opening Liked Music teaches it the rest. The toggle shows what
YouTube Music accepted, not the click, so a refused like does not stay lit. Alt+Shift+B, Spotify's
shortcut, likes the song playing.

**Volume in reach.** An inline slider sits in the player bar; the button's popover remains for the
narrow layout, where the slider is hidden.

**A page's Play button controls its own list.** While the list a page queued is playing, the
button reads *Pause* and pauses and resumes it instead of starting it again. The shell remembers
which route started the queue (`ShellViewModel.Queue.cs`, `SetQueueSource`).

**The queue says where it came from.** *Up next* reads "From Liked Music · 99 songs" or "From
Song radio · 25 songs" rather than a bare count.

**Lists that are not finished say so.** A list with a continuation cursor still pending counts
as "100+ songs", not "100 songs". That is the same invariant as a clamped page: a partial list
must not present itself as complete.

**A page uses its own cover.** Playlists and Liked Music carry a header thumbnail; the hero uses it
and falls back to the first song's art only when there is none. Liked Music had been showing the
cover of whatever song happened to be first.

**Playlist rows name their album** in a column of their own, as Spotify's and Apple Music's do.
Album pages leave it off, since every row shares one.

**Sort and find in a playlist.** Playlists and Liked Music offer *Sort* (custom order, title,
artist, album, duration) and a *Find in playlist* box (`ShellViewModel.Sorting.cs`). Both are
views over the rows already loaded and never write to the playlist. Play and a clicked row queue
what is on screen, in the order on screen. Rows that load later join in their sorted place.
Editing a playlist first returns it to its own order and clears the filter, because edits
address rows by position.

**Recent searches.** The search box offers the last eight queries when it opens and narrows them
as you type. They live in the shell's own local preferences and never reach the service or
YouTube Music.

**A sleep timer** in the player's More menu: 15, 30, 45 or 60 minutes, or the end of the current
song (`MainWindow.SleepTimer.cs`). It pauses rather than quitting, so the queue is where it was.
"End of this song" is honoured where a natural end would otherwise advance.

**A mini player.** The full player in a small always-on-top window, from the More menu or a
button in the full player. It is the same layout, not a second one to keep in step; lyrics and
fill-the-screen step aside while it is small and come back as they were. On macOS the
equivalent is a floating `NSPanel`; on GNOME a small window with the keep-above hint.

**Moods & genres as tiles.** A shelf whose items are all categories is drawn as a wrapping grid of
tiles with the category's colour as a stripe, and without scroll arrows. A tile opens the
category page through `catalog.category`.

**Artists are round**, in cards and in list rows.

**Smaller corrections.** The sidebar says *Liked Music*, as the page does. Search offers a
*Videos* filter. The full player's seek bar is white over the artwork rather than the system
accent colour, which is red on many machines. Settings is a centred column. Ctrl+Comma opens
Settings.

## Windows: after the audit

A second round followed the audit, again Windows-first and described here as behaviour.

**Pages open instantly from the last copy.** Measuring showed every visit fetched its page afresh,
from a fifth of a second to over a second, with another two and a half seconds whenever the
account's reader page had to start. The Windows shell now keeps the last copy of each page it
showed (`Service/PageCache.cs`), in memory and on disk per account profile, shows it at once,
and replaces it only if the fresh answer differs. The copy leaves out the continuation cursor,
which changes on every answer and would go stale. A failed refresh leaves the copy standing. The
copies hold only catalog metadata and are deleted when the profile signs out. The library's main
pages are read in the background after sign-in, so even a first visit is instant, and identical
personal reads in flight are shared. A shell on another platform should do the same; the
measurement that justified it is in the log, which now records how long each read took.

**The library is a grid.** A library page is one grid that scrolls down, as Apple Music's is:
artists as circles with centred names, albums and playlists as rounded squares, with no heading
and no sideways arrows. Later parts of the page join the same grid.

**Liked Music is one page.** The library's Songs tab, the Liked Music card on Home and any other
link to `LM` or `VLLM` open the same Liked Music page, with its cover, sort and find, and light its
sidebar row. Before, each opened a plainer copy of the same list.

**Library artists are the artists of the library.** `FEmusic_library_corpus_artists` is the
account's subscriptions; the artists of the songs in the library are
`FEmusic_library_corpus_track_artists`. The Windows shell had the two swapped, so Artists was empty
for most accounts. The Swift shell had the same mistake and is fixed on `development`.

**Debug mode and a log.** Settings has an Advanced section with a debug switch and a button that
opens the log folder. With debug off, errors are one plain sentence and notices about how Goosic
works inside -- a clamped page, waits during advertisements or account changes, the web player's
own messages -- are only written to the log. With it on, they are shown too, and errors carry the
exception. Everything shown on screen is logged. The clamped-page notice is one of those details, as
[AGENTS.md](../AGENTS.md) now records: a partial list still says so in its count, and a page of
shelves is no longer clamped at all. It used to lose every shelf past the twelfth, so a mood page
such as Chill showed twelve of its sixteen; the service now sends the shelves that fit in one
frame with a cursor of its own for the rest, which the shell follows as the listener scrolls,
as it follows YouTube Music's cursors. The service keeps no state: the cursor names the page and
the shelf to resume from, and carries upstream's own cursor so the page continues past its last
shelf as before.

**The page fades under the title bar**, so scrolled titles never run into the window buttons.

## Deliberately not done

Some features every one of those apps has are out of reach by design, and should be declined
rather than half-built:

- **Downloading playlists for offline listening.** Reborn has no downloader, and
  `goosic-downloads` must not grow one (see [AGENTS.md](../AGENTS.md)). *Downloads* imports
  files the previous Goosic finalised.
- **Crossfade, gapless playback, an equaliser and a sound-quality choice.** Official playback is
  YouTube Music's own web player running inside the account's web view. The shell reports on it
  and asks it to play, pause, seek and change volume; it does not own the audio pipeline these
  features need, and reaching into it would bypass the authority rather than use it.
- **Casting to speakers and TVs.** Same reason: the stream belongs to the web player.

## Worth doing next

- **Mood chips on Home.** YouTube Music puts *Energize*, *Relax* and similar chips above Home.
  They are `chipCloudChipRenderer`s whose browse endpoint is `FEmusic_home` plus parameters, so
  they fit the `category` kind as it stands; the work is parsing them in `goosic-catalog` and
  `PersonalCatalog.js` and giving shells a chip row to draw them in.
- **Search suggestions as you type**, from YouTube Music's `music/get_search_suggestions`. That
  is a new anonymous catalog command.
- **An artist's About section and "Fans also like".** The second often arrives as an ordinary
  carousel already; the first needs the header's description.

## For the macOS shell

The Swift shell compiles with the 0.4.0 constant and decodes `category` items as `.unknown`, so a
mood tile is visible but inert until it learns the kind. To finish Moods & genres there: add a
`category` case to `GoosicCatalogKind`, read the optional `color`, draw a shelf of categories as
a grid, and open one with `catalog.category`. The rest of the list above is presentation and can
follow in any order; the like button and *Up next* source are the two people will notice first.

## For the Linux shell

`goosic-shell-support` already turns a category into a `Show` action and caches it under
`CatalogKey::Category`, so the GTK shell needs only to draw the grid and send `catalog.category`
when it builds its catalog views. Everything else above is presentation for it to reproduce with
GTK widgets.
