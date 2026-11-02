# The artist page hero

An artist page opens with the artist's own picture, in the manner of an Apple Music artist page,
rather than a heading over a list. [UI_DESIGN.md](UI_DESIGN.md) is the product contract for what
that looks like; this document is how each shell builds it. The macOS shell has it. Windows and
Linux have the data already, because it arrives in the service's answer, and need only the
native drawing described below.

## Where the data comes from

YouTube Music describes an artist in the page's header, not in its shelves, and the previous
Goosic's artist picture came from a shelf, which is why it was often an album cover or a
low-resolution square. `goosic-catalog` now reads the header itself, in
`crates/goosic-catalog/src/parse.rs` (`artist_header`), and sends it as the optional `artist`
field of the artist page's `CatalogPage`:

```json
"artist": {
  "banner": "https://lh3.googleusercontent.com/…=w2880-h1200-p-l90-rj",
  "portrait": "https://lh3.googleusercontent.com/…=w1080-h1080-l90-rj",
  "audience": "28.9M monthly audience",
  "description": "…",
  "subscribers": "1.62M"
}
```

Every member is optional and the whole field is absent from any page that is not an artist's,
so an older shell ignores it and a newer shell treats each missing member as missing rather
than guessing. `banner` is the wide picture, asked of the image host at 2880 by 1200 whatever
size the header listed; `portrait` is a square picture, which the header carries when it has no
banner or alongside one in its visual variant, asked for at 1080. The text members are
upstream's own, already formatted for display, and are shown as they are. The page's own
`thumbnail` now comes from the same header, so an artist's picture never comes from a shelf.

The protocol version did not change; the field is additive and optional, like `nextCursor`
before it. A shell must keep the header across a continuation: the first page carries it and
later pages do not, which `PageView::appending` in `goosic-shell-support` and
`CatalogPageView.appending` in Swift already do.

## The two shared rules

Two decisions are numbers rather than drawing, and three shells computing them three ways would
give one artist three different pages, so they live in `goosic-shell-support::hero`
(`crates/goosic-shell-support/src/hero.rs`), with a Swift copy in `Core/ArtistHero.swift` held
to the same test cases.

`hero::height(width)` is the hero's height for the width of the content area: the banner's own
2.4 proportion, never shorter than 360 and never taller than 620, in the same units as the
width. A narrow window keeps the name and buttons on the picture; a wide one is not all picture.

`hero::tint(rgba, width, height)` is the colour the page takes. A shell decodes the banner, or
the portrait when there is no banner, into a small RGBA bitmap, 48 pixels wide in proportion is
plenty, with its first row at the picture's top. The rule averages the bottom quarter in linear
light and darkens the result, never lightens it, until its relative luminance is at most 0.18,
which keeps white text over it at 4.5:1. It answers `None` for a bitmap smaller than it claims.
Sample off the UI thread and only once the picture is in the artwork cache; until then the page
uses the default colour, sRGB (61, 13, 31), a deep wine that reads as "no picture yet" rather
than as an artist's colour.

## Drawing it

The hero is the first thing on the page, inside the scrolling content so it scrolls away with
it, and it spans the content area edge to edge: no leading padding, no rounded corners, and it
runs under the transparent toolbar to the window's top. In order from back to front it holds
the page colour; the banner, scaled to fill and anchored at its top, so a taller hero crops from
the foot and keeps faces; a vertical scrim; and the text. The scrim goes from black at 20% at
the top, to clear at 22%, then through the page colour at 25% at the middle and 85% at 78%, to
the page colour at the foot, so the picture ends in the page rather than at an edge. With
increased contrast the stops are 35%, 55% and 95%; with reduced transparency the scrim is an
opaque band of the page colour across the lower half instead.

Over that, centred, sit the artist's name in a 52-point bold white weight, at most two lines and
allowed to shrink to 60% before truncating, upstream's audience line under it in white at 80%,
and the three buttons 14 points below with 26 between them. About and Subscribe are 46-point
circles of white at 16% with a hairline white border and a white symbol; Play is a 72-point
solid white circle with a black play or pause symbol. Play queues the top songs in order from
the first and pauses when those songs are already what is playing. About opens a popover, or a
flyout, with the name, the audience and subscriber count joined by a middle dot, and the
description as selectable text; it is disabled when there is neither. Subscribe is shown only
with an account and is filled when the account follows the artist; without an account its
place is an empty 46-point space, so Play stays centred.

The page behind the content takes the colour too, and the page is drawn in the dark appearance
over it, whatever the app's theme, since its text sits on a dark colour. For the hero's height
the background is the page colour unchanged, so the hero's foot meets the same colour wherever
it has scrolled; below that it deepens toward black at 55% at the bottom of the window. Change
the colour with a 0.45-second ease when it arrives, and without animation when the system or
the app asks for reduced motion. The colour belongs to the page key: clear it when the page
changes so one artist's colour does not linger on the next.

Without a banner, a portrait is drawn as a circle 52 points from the hero's top, its side the
hero's height less 290, between 80 and 240, which keeps it clear of the name at every height.
Without either picture the hero is the page colour alone. A song's cover or a video still is
never used in their place. After the hero, a "Top Songs" heading in the title style precedes
the songs, and the shelves follow as on any page. The page's usual heading is not drawn, because
the hero is the heading. Share and More stay in the toolbar as on every page; on an artist page
More gains Shuffle, which turns shuffle on and plays the top songs from a random one.

## Windows

The work is in `apps/goosic-windows` on a `feature/windows/<slug>` branch cut from
`platform/windows`, after `development` with this change has been synced into it.

Start with the data. Add an `ArtistHeader` record beside `CatalogPage` in
`Service/CatalogModels.cs`, with nullable `Banner`, `Portrait`, `Audience`, `Description` and
`Subscribers` properties named `banner`, `portrait`, `audience`, `description` and
`subscribers` in JSON, and an `[JsonPropertyName("artist")] public ArtistHeader? Artist` on
`CatalogPage`. Wherever the view model appends a continuation, keep the first page's header
when the next one has none, as `PageView::appending` does.

The shell does not link `goosic-shell-support`, so restate `hero::height` and `hero::tint` in a
`Presentation/ArtistHero.cs`, in the style of the other rule files there, and add
`Goosic.Windows.Tests/ArtistHeroTests.cs` with the same five cases as the Rust module: 1200
wide gives 500, 600 gives 360, 2400 gives 620; a 4 by 4 bitmap of (20, 30, 60) keeps that
colour; a white one gives (118, 118, 118); a 4 by 8 bitmap white above and black in its last
two rows gives black; and a 12-byte bitmap claiming 2 by 2 gives null, as does an empty one.
A change to one copy is a change to all three in the same commit.

Draw the hero as the first element of the page's `ItemsRepeater` or list header, so it scrolls
with the songs, with its width bound to the content area's actual width and its height from
`ArtistHero.Height`. An `Image` with `Stretch="UniformToFill"` and `VerticalAlignment="Top"`
inside a clipped `Grid` gives the crop; the scrim is a `LinearGradientBrush` with the stops
above; the round buttons are `Button`s with a `CornerRadius` of half their size. Load the
banner through `ArtworkLoader`, which already caches by URL, and sample the tint from a
`SoftwareBitmap` decoded at 48 pixels wide with `BitmapDecoder` and a `BitmapTransform`, off the
UI thread. Set `RequestedTheme="Dark"` on the page while the hero shows and put the page colour
and its gradient on the page's background. The About details fit a `Flyout`. Check
`UISettings.AnimationsEnabled` and `AccessibilitySettings.HighContrast`, and the app's own
reduce-motion setting, as the other pages do. The page's existing header, which
`ShellViewModel.Account.cs` decides with `ShowPageHeader` and fills from `page.Thumbnail`, is
hidden when the page has an artist header.

## Linux

The work is in `apps/goosic-linux` on a `feature/linux/<slug>` branch cut from `platform/linux`,
after `development` with this change has been synced into it. The GTK shell links
`goosic-shell-support`, so it reads `PageView.artist` and calls `goosic_shell_support::hero`
directly; it restates nothing.

The page is a `gtk::ListView` of `PageRow`s built in `src/pages.rs` and drawn by `row_widget`
in `src/ui.rs`. Add a `PageRow::ArtistHero` variant holding the header, the name, the channel
and the top songs, emitted first for an artist page in place of the usual heading row, and give
`same_row` a case for it so a refresh does not rebuild the picture. Draw it as a `gtk::Overlay`
whose child is a `gtk::Picture` with `ContentFit::Cover` inside a box clipped with
`set_overflow(gtk::Overflow::Hidden)` and sized from `hero::height`; keep the top anchoring by
drawing the texture yourself in a custom widget's `snapshot`, as `src/backdrop.rs` already
does, because `Picture` centres its crop. The scrim is a CSS `linear-gradient` on an overlay
box, written with the page colour, and the buttons are `gtk::Button`s with the `circular` style
class. The row has to break out of the list's usual side margin, so it carries a style class
that removes it.

Load the banner through `src/artwork.rs`'s `ArtworkCache`. Once its file is cached, decode it on
a worker thread at 48 pixels wide with `gdk_pixbuf::Pixbuf::from_file_at_scale`, add an alpha
channel if it has none, and pass the pixel bytes, row by row without the rowstride padding, to
`hero::tint`. Put the colour on the page's background through a small
per-page CSS provider, together with the dark style. Follow `gtk::Settings`'
`gtk-enable-animations` and the app's reduce-motion preference for the colour change, and the
desktop's high-contrast theme for the strong scrim. About is a `gtk::Popover` on its button.

Since compilation is not a visual check on any platform, each shell's change should be looked
at on its own desktop against the two pictures in UI_DESIGN.md's description: a wide banner, and
a page with only a portrait.

## Not yet done

The header's own Shuffle and Mix buttons on YouTube Music start stations with ids of the form
`RDAO…` and `RDEM…`, which the service cannot play: `catalog.radio` starts only from a song. Until
a service request for an artist station exists, Shuffle shuffles the top songs and Start Radio
starts from the first of them. The subscribe button names a channel id that differs from the
artist's browse id; Subscribe uses the browse id, the same id the rest of the app knows the
artist by. And YouTube Music has no "Latest Release" block in the header that Apple Music
shows beside the songs, so none is drawn.
