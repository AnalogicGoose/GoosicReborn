# The Linux shell

`apps/goosic-linux` is the Linux application: a GTK 4 program written in Rust that replaces the
SwiftCrossUI build on Linux. It is step four of [the native-shell migration](NATIVE_SHELL_MIGRATION.md).
The plan says *what* replaces what; this document records how the Linux shell is built and why
each choice was made, so that the reasons survive the people who made them.

It is being written slice by slice on `platform/linux`. What exists browses and searches the
catalog, opens albums, artists and playlists, and plays through the official WebKitGTK host under
Rust's lease, with a queue, a now-playing bar, radio, and preferences that are saved and restored.
It draws catalog artwork, follows a light, dark or system theme, shows the queue and synced lyrics
beside the page, and has a settings screen that imports the previous Goosic's preferences. A page
the service can continue — Home and the other editorial routes — fetches its next part when the list
reaches the bottom, with a Load more row for a page too short to scroll, and a radio that runs out
follows its own station's cursor instead of seeding a new station from its last track. Only the rows
after the last unchanged one are replaced when a page grows, so appending never moves what is on
screen.

That playback has been heard rather than only compiled: a scratch harness asked the shell to play a
real track the way a Play button does, the advertisement in front of it was reported and not
skipped, validated samples from the page moved the bar through the song, and PipeWire showed the
WebKit process's uncorked stream. Running the same harness against empty storage found something
the Swift port never had to face: on a fresh profile YouTube Music loads the track paused and
waits. The shell therefore answers the first paused report of a load it was asked to play with a
single play request, once, so a pause the user makes afterwards is kept.

Downloaded files play through a GStreamer pipeline of their own, under the `localDownloadedFile`
lease: the Downloads screen lists and imports the previous Goosic's finalized files, the service
decodes one when it is chosen, and the shell switches the lease between the official page and the
file in either direction, quiescing or stopping one renderer before the other may claim. That path
has been exercised end to end too, against a scratch legacy folder: the import found the file, the
service decoded it under the lease, GStreamer played it, and PipeWire showed the stream. A unit test
opens a generated WAV paused and reads its length back, which needs neither a display nor a sound
card.

Accounts are real on Linux now, which the Swift port never managed. Add account releases whatever
plays and opens Google's sign-in in a window of its own, whose cookies land in a staging directory
and are written to SQLite explicitly, since a sign-in kept only in the network process's memory would
be gone at the next launch. Nothing is kept until Rust has stored the account, activated it, and the
staging has been moved into the account's profile; if any of the three fails, the account is removed
again and the staging deleted. Closing the window or letting it time out deletes the staging too —
twice, because WebKit's network process writes into the directory once more as it winds down — and
leftovers are swept at startup. Switching, signing out and removing rebind the player only once Rust
has confirmed, and removing an account deletes its profile after the player has let go of it. The
sign-in window has been opened, shown Google's page, closed and cleaned up under the harness. The
first real sign-in then showed the account in the window and never in the app: the shared
completion check was an older copy that looked for YouTube's `#avatar-btn` rather than YouTube
Music's `ytmusic-settings-button`, and the wait gave up after thirty seconds without noticing
YouTube Music's in-page navigation. The check is now the async one the Swift shell runs, called as an
async function, restarted on every URI change, and never overlapped with itself; against a page
imitating a signed-in YouTube Music it opens the account menu and is accepted, and against a
signed-out one it waits. The GTK shell now reads Home and Library through a muted WebKitGTK view
sharing the active account's network session. An expired session refuses the read rather than
passing guest shelves off as the account's own. A local live read succeeded with an existing
account profile; a fresh sign-in followed by those reads has not yet been checked in the Flatpak.

Both web surfaces judge only the main frame. WebKitGTK asks for a policy on every frame's navigation
without saying which frame it is, and an earlier version of this shell refused every navigation off
the player host — including the frames YouTube Music serves advertisements in, which turns a player
that reports advertisements into one that blocks them. The rule is now applied when the main frame's
load starts or is redirected, where the view's URI is the one being loaded, and a disallowed load is
stopped before it commits; subframes are left alone, and new windows are refused.

The media-player interface, the status icon and background mode are in too, and each was checked
against a real Plasma session rather than only compiled: `gdbus` read the MPRIS properties back as a
panel does and a `PlayPause` sent over the bus paused the page; the status icon registered with the
watcher and served its menu over dbusmenu; closing the window hid it while PipeWire kept the stream
uncorked, raising it brought it back, and the suspend inhibition appeared in the power manager's
list while music played. Everything still missing before this shell replaces the Swift Linux build
is listed under [What is left](#what-is-left).

## Why GTK 4, and why not the alternatives

The deciding constraint is not the look of the application. It is the web engine. Official
playback is the YouTube Music page running inside an embedded WebKit view, with the page observer
injected into an isolated script world, a message handler the page cannot reach, one network
session per account so cookies never mix, and a navigation policy consulted on every load.
Sign-in is a second web view under the same policy. WebKitGTK provides every piece of that, and
the Swift Linux port has already proved it works — the three sign-in failures fixed during that
port were fixed against WebKitGTK and are encoded in the shared rules.

A canvas toolkit such as Slint or Flutter has no web view that can be embedded on Linux.
Embedding one means opening WebKitGTK in a second window anyway, which pays for both toolkits and
keeps the benefit of neither. Qt 6 would bring QtWebEngine, which is Chromium, with different
semantics for script worlds, profiles and navigation: every security argument in
[ARCHITECTURE.md](ARCHITECTURE.md) about the bridge and the login window would have to be made
again for a second engine, from Rust through `cxx-qt`. That would buy a native look on KDE Plasma
and nothing the product needs.

So the shell uses GTK 4 through `gtk4-rs`, WebKitGTK 6.0 through `webkit6`, GStreamer through
`gstreamer-rs`, and D-Bus through `gio` — the same libraries the Swift port already binds by hand.
Because the shell is Rust, it links `goosic-shell-support` directly, with no foreign-function
boundary in the way.

The sluggishness of the SwiftCrossUI build on Linux came from that framework's layout pass, not
from GTK. `GtkListView` and `GtkGridView` recycle a small set of row widgets however long the
list is, which is what makes a thousand-track playlist cheap to scroll.

## Why not libadwaita

Libadwaita imposes GNOME's design language and deliberately ignores GTK themes. The shell is
meant to look at home on every desktop: Breeze on Plasma, which ships a GTK 4 theme, the desktop's
own theme on Xfce and COSMIC, and Adwaita on GNOME, where it is simply GTK 4's default. Plain
GTK 4 inherits whatever the desktop provides; libadwaita would make the application look like a
GNOME application everywhere.

Two costs come with that, and they are accepted knowingly. Libadwaita's adaptive widgets — split
views, breakpoints, toasts, preference pages — are not available, so the equivalents are built from
plain GTK. And following the system's light or dark preference needs GTK 4.20, whose
`gtk-interface-color-scheme` setting reads the desktop settings portal by itself; before 4.20 only
libadwaita did that. The Flatpak runtime makes 4.20 the floor, so this costs nothing in practice.
The choice is binary: initialising libadwaita replaces the stylesheet, so there is no taking a few
of its widgets without taking its look.

Two design rules follow. The window keeps the system's own titlebar rather than a GNOME-style
header bar — drawn by the window manager on X11 and by GTK on Wayland, styled by the theme either
way. The first-release layout follows the Windows shell's composition: a grouped adaptive
sidebar and the Playing Next or Lyrics panel sit over the content, and a compact transport pill
floats at the bottom. Home presents song shelves as horizontally scrolling columns of four
compact rows. GTK uses neutral light and dark palettes with translucent fills and native controls;
WinUI's acrylic blur is not available through ordinary GTK widgets. The shared pink accent is
`#FF0552`. The adaptive and immersive behavior follows [UI_DESIGN.md](UI_DESIGN.md).

## How it talks to Rust

Exactly as every shell does. The shell starts its own `goosic-service` as a private child process
and speaks the NDJSON protocol over inherited stdin and stdout, through `goosic-shell-support`'s
`ServiceClient`. It never links `goosic-core`, and the service is never a daemon or a socket: the
architecture's single-client boundary does not bend for a platform. Completions arrive on the
client's own threads and are handed to the GLib main loop, which is the only place shell state
changes.

The service is found by path. `GOOSIC_SERVICE_PATH` is honoured as a developer's override;
otherwise the shell runs the `goosic-service` installed beside its own executable, which in the
Flatpak is `/app/bin`. It never searches `PATH`, so a packaged shell always runs the service it
shipped with. That closes, for Linux, the pairing gap [COMPATIBILITY.md](COMPATIBILITY.md) records.

Everything the shell decides that has no machine in it comes from `goosic-shell-support`: where the
sign-in window may navigate, whether a bridge event is trustworthy, what the media controls may
offer, which catalog row is playable, what plays next. The shell holds only presentation state and
the code that talks to GTK, WebKitGTK, GStreamer and D-Bus.

## Identity

The application ID is `io.github.analogicgoose.Goosic`: reverse-DNS under the GitHub owner of the
repository, the form Flathub accepts for projects hosted there without a domain of their own. It
is the `GApplication` ID, which is what makes a second launch activate the running instance rather
than start another; the base name of the desktop file, the metainfo file and the icon; the prefix of
the D-Bus object paths; the `DesktopEntry` the media-player interface reports; and the Flatpak's
ID. Portals identify the application by it.

It renames nothing that already exists. Storage stays under `goosic`, as
[LEGACY_COMPATIBILITY.md](LEGACY_COMPATIBILITY.md) requires. The media-player bus name stays
`org.mpris.MediaPlayer2.goosic`, because desktop media panels show that name and the specification
asks for a short one. `com.github.ivasy.ytubic` belongs to the previous Goosic; it is read for the
legacy import and never reused.

## Closing the window

Closing the window does not stop the music. The window hides, and the process keeps running and
playing. It stays visible and controllable through the surfaces each desktop has, because no single
surface exists on all of them.

The media-player interface is the one every desktop has. GNOME, Plasma, Xfce and COSMIC all show the
current track and its controls in their own panels. Two methods become honest once the process
outlives its window — `Raise` presents the window and `Quit` ends the process — so both are
declared, in keeping with the rule that an interface declares only what the application can do.

Where the desktop has a status area, the shell adds an icon to it through StatusNotifierItem, with
a menu to show the window, play or pause, skip, and quit. Plasma, Xfce and COSMIC have one. GNOME
does not without an extension, so the icon is a convenience and never the only way back. The icon
registers whenever a watcher appears, not once at startup, so a panel that starts after Goosic still
shows it. Inside a Flatpak the shell asks the Background portal to run without a window, and GNOME
lists it under Background Apps with a way to close it; a native process needs no such permission.
On every desktop, launching the application again presents the existing window.

The first time the window closes, a notification says the application is still running and how to
quit it, so somebody who expected closing to mean quitting is not left wondering what is holding
their speakers. It says so whether or not music is playing, because the process keeps running
either way. Quitting is explicit — `Ctrl+Q`, the status icon's menu, the media panel's quit, GNOME's
background list — and it ends the service, which takes Rust's lease with it, and both playback
hosts.

The mechanics follow from that. Closing the window hides it, and the application holds itself while
it runs in the background, so losing its last visible window does not end the process. The official
web view lives as long as the application, not as long as the window: rebuilding it on reopen would
drop the page and the lease with it. While audio is playing — an advertisement included — the shell
inhibits suspend through GTK, which goes through the Inhibit portal or the session's power manager,
and releases the inhibition as soon as nothing is audible. It does not stop the screen from locking;
music is not a video.

One of the two behaviours this section said had to be heard rather than argued has been: closing
the window during playback left the page playing, with its stream uncorked. The other is still an
expectation. WebKitGTK may throttle timers in a page whose view is hidden, which would slow the
observer's periodic report; the observer also reports on media events, so end-of-track detection
should survive, but a whole track has not yet been left to finish with the window closed.

## Desktop portals

The shell reaches desktop services through portals wherever one exists, so each desktop supplies
its own implementation: KDE's file picker on Plasma, GNOME's on GNOME.

The colour scheme needs no code: GTK 4.20 reads it from `org.freedesktop.portal.Settings`, under
the `org.freedesktop.appearance` namespace. That is the interface applications call.
`org.freedesktop.impl.portal.Settings`, which is sometimes recommended for this, is the interface
portal *backends* implement, and an application never calls it.

The shell uses the Inhibit portal for suspend while audible, the Background portal to run without a
window, and the Notification portal for the one notice above, each through GTK and GIO, which choose
the portal when one is there. OpenURI is for opening an outside link in the user's browser, and no
screen offers one yet. Sign-in never goes there: it stays in the application's own window, under the shared
navigation policy. The file chooser is used only if a feature needs a path picked, and none does
today — the legacy import reads known locations. The status icon is not a portal; it is
StatusNotifierItem over D-Bus.

## Where things are stored

Every location follows the XDG Base Directory specification, and every crate reads the XDG
variables rather than spelling out a path.

| What | Where | Written by |
| --- | --- | --- |
| Settings and account metadata | `$XDG_CONFIG_HOME/goosic/` | the service |
| Download index | `$XDG_DATA_HOME/goosic/downloads.json` | the service |
| Decoded WAV cache | `$XDG_DATA_HOME/goosic/decoded` today; moving to `$XDG_CACHE_HOME/goosic/decoded` | the service |
| Web profiles, one per account | `$XDG_DATA_HOME/goosic/profiles/<uuid>` | the shell |
| Sign-in staging profiles | `$XDG_DATA_HOME/goosic/staging/<uuid>` | the shell |
| Web engine cache, one per profile | `$XDG_CACHE_HOME/goosic/web/<uuid>` | the shell |
| Catalog artwork | `$XDG_CACHE_HOME/goosic/artwork` | the shell |

Web profiles hold cookies and are user data, never a cache: clearing a cache must not sign anybody
out. Their HTTP cache is a cache, so it lives apart from them.

The decoded WAV cache is in the wrong place, and its own code says so: it describes the directory as
reproducible from the source files, which is the definition of a cache. Decoded audio is large, so
where it sits matters — under data it is swept into backups and ignored by every tool that frees
cache space. The move belongs in `goosic-downloads`, which makes it shared work on `development`, and
it applies to macOS (`~/Library/Caches`) and Windows (`%LOCALAPPDATA%`) in the same change. Nothing in
the old directory needs migrating, since all of it can be decoded again.

Inside the Flatpak those variables point into `~/.var/app/io.github.analogicgoose.Goosic/`. Because
no crate hard-codes a path, the same code lands there with no Flatpak-specific branch. It also
means a Flatpak install and a development build keep separate settings and separate sign-ins. The
previous Goosic's data is read through `$HOME`, so the Flatpak needs read-only access to
`~/.local/share/com.github.ivasy.ytubic` and to nothing broader.

## Packaging

The Linux package is a Flatpak on the GNOME runtime, version 49 or later — the first to carry GTK
4.20. The runtime supplies GTK 4, WebKitGTK 6.0 and GStreamer, so the package ships only Goosic's
own two programs: the shell and the service it launches. One package serves every desktop and every
distribution, which is the point of choosing it.

The permissions are few, and each has a reason. Network access, for the catalog and the player.
Wayland, with X11 as a fallback. The GPU, for WebKit's compositing. The PulseAudio socket, which
PipeWire also serves, for sound. Ownership of `org.mpris.MediaPlayer2.goosic`, for the media panel.
Talking to `org.kde.StatusNotifierWatcher`, for the status icon. And read-only access to the previous
Goosic's directory, for the legacy import. There is no access to the home directory and none to the
host filesystem.

Local playback plays WAV and needs no codec. The official player gets whatever the runtime's
GStreamer can decode, so the GNOME 50 runtime was inspected rather than assumed: it carries GTK
4.22, WebKitGTK 6.0 and GStreamer 1.26, with the Opus, Matroska, VP9, AV1, MP4 and `libav`
plugins. That covers both the Opus-in-WebM and the AAC-in-MP4 streams YouTube Music serves,
without an extension.

The manifest builds the service from the root workspace and the shell from its own, installs both
into `/app/bin` with the desktop file, metainfo and icon, and builds offline from vendored crate
sources, as Flatpak requires. Its test phase runs the Linux shell suite against that installed
service. A native build remains the development path. No other Linux package format is planned.

### Building the Flatpak locally

The toolchain was checked against Flathub on 14 September 2026 rather than taken from memory. Flathub
carries the GNOME runtime at 49 and 50, and the GNOME 50 SDK declares its SDK extensions at the
freedesktop `25.08` branch, so the Rust extension has to be `rust-stable//25.08`; that one ships Rust
1.98.1, above the shell's 1.92 floor. On Fedora, Flathub is added as a system remote, which is why an
earlier `flatpak install --user` answered "No remote refs found for 'flathub'": a user installation
only sees user remotes. Installing without `--user`, or adding Flathub as a user remote first,
both work.

```sh
sudo dnf install flatpak-builder python3-aiohttp python3-tomlkit
flatpak install flathub org.gnome.Platform//50 org.gnome.Sdk//50 \
    org.freedesktop.Sdk.Extension.rust-stable//25.08
# or, for a user installation:
# flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
# flatpak install --user flathub org.gnome.Platform//50 org.gnome.Sdk//50 \
#     org.freedesktop.Sdk.Extension.rust-stable//25.08
```

A Flatpak build has no network, so both workspaces' crates are vendored as source lists generated
from their lock files by `flatpak-cargo-generator.py`, from
[flatpak-builder-tools](https://github.com/flatpak/flatpak-builder-tools); its Python dependencies are
the two packages above. The Rust extension alone is a download of more than half a gigabyte.

```sh
git clone https://github.com/flatpak/flatpak-builder-tools ~/src/flatpak-builder-tools
python3 ~/src/flatpak-builder-tools/cargo/flatpak-cargo-generator.py \
    Cargo.lock -o apps/goosic-linux/packaging/flatpak/cargo-sources-service.json
python3 ~/src/flatpak-builder-tools/cargo/flatpak-cargo-generator.py \
    apps/goosic-linux/Cargo.lock -o apps/goosic-linux/packaging/flatpak/cargo-sources-shell.json
flatpak-builder --user --install --force-clean build-dir \
    apps/goosic-linux/packaging/flatpak/io.github.analogicgoose.Goosic.yml
flatpak run io.github.analogicgoose.Goosic
```

The package has completed a local GNOME 50 SDK build, and its shell connected to the bundled
service when launched in the sandbox. That smoke run did not sign in or play a whole track.

## Workspace and layout

The shell is its own Cargo workspace, with its own `Cargo.toml`, `Cargo.lock` and target
directory. It is not a member of the root workspace, because CI runs `cargo test --workspace` on
macOS and Windows, and a member that needs GTK would fail there; the Swift package is separate for
the same reason. It depends on `crates/goosic-shell-support` and `crates/goosic-protocol` by path,
and on nothing else under `crates/`. It declares Rust 1.92, which `gtk4` 0.11 requires; the root
workspace's 1.88 floor does not apply to it.

It turns on `gtk4`'s `v4_20` feature, the floor the runtime guarantees. That is not a preference:
without a version feature `gtk4` 0.11 hides the `Accessible` interface that `webkit6` implements,
and `webkit6` does not compile. The feature also changes one signature the code depends on — since
GTK 4.12 a list-item factory can build section headers, so its callbacks are handed a plain object
that the page list casts to a `ListItem`.

```
apps/goosic-linux/
    Cargo.toml             its own [workspace]
    src/
        main.rs            GtkApplication and the application ID
        lib.rs             the modules, public where tests and harnesses need to reach them
        service.rs         finding and launching the goosic-service beside the executable
        bridge.rs          carrying service answers onto the main loop
        shell.rs           the window, and the orchestration that ties the rest together
        pages.rs           navigation and catalog load state, decided without GTK
        playback.rs        queue, lease and transport state, decided without GTK
        lyrics.rs          the lyrics panel's lookups and highlight, decided without GTK
        ui.rs              page rows, sidebar, search bar, settings screen
        player_bar.rs      compact/full transport and the fixed-size volume layer
        full_player.rs     expanded cover, lyrics and queue presentation
        backdrop.rs        cropped artwork blur through GTK's render graph
        layout.rs          tested window breakpoints and transport placement
        shell_presentation.rs window layout, shortcuts and sleep-timer orchestration
        side_panels.rs     the queue and lyrics panels
        artwork.rs         anonymous libsoup fetches into the XDG cache, under the shared rules
        theme.rs           gtk-interface-color-scheme, which GTK 4.20 made the way to choose
        official_host.rs   WebKitGTK player: script world, bridge handler, per-account session
        personal_host.rs   signed-in catalog reader inside the active account's profile
        login_host.rs      sign-in window under the shared navigation policy
        web_profile.rs     profile and staging storage, and the main-frame navigation guard
        local_host.rs      GStreamer playbin for decoded files
        mpris.rs           org.mpris.MediaPlayer2 over gio
        status_icon.rs     StatusNotifierItem and its dbusmenu, where a watcher exists
        background.rs      hiding on close, holding the app, Ctrl+Q, inhibit, the Background portal
    data/                  desktop file, metainfo and scalable icon
    packaging/flatpak/     manifest and locked vendored crate source lists
```

The modules are flat rather than grouped into `state/`, `ui/` and `platform/` as first sketched,
because a dozen files did not need folders. What the grouping was meant to protect still holds:
`pages`, `playback` and `lyrics` import no GTK, so the decisions they make are tested without a
display, and only `shell` joins them to widgets and hosts.

The shell lives on `platform/linux`, and each slice of it is a `feature/linux/<slug>`. Anything it
needs from `goosic-shell-support` or the protocol is not Linux work: it lands on `development`
first and reaches `platform/linux` through the cascade.

The shared Makefile provides `build-linux`, `test-linux` and `run-linux`; each builds the service
first, and `run-linux` points the shell at that exact executable. CI keeps the native GTK build in
its Fedora container and also builds and tests the package in the GNOME SDK. The Flatpak job runs
when the Linux shell, `goosic-shell-support` or the protocol changes.

## Order of work

The plan's order for step four held. Transport, catalog, search,
queue and settings came first, because they need no platform host and prove the shell against the
service. Then the WebKitGTK player, local audio, the sign-in window, the media-player interface and
the status icon, and running in the background. The Flatpak builds and launches, but the account and
playback checks below still gate the first public release.

One part of the order was not kept. The test that no host can sound without Rust's active lease was
meant to come with the first host, not after the last, and it did not come at all: each host claims
the lease before it loads, and the harness has heard that ordering work, but nothing fails a build if
a later change breaks it. The GStreamer host now has a test that opens a file paused, which is the
first half of that proof — a paused pipeline decodes without opening the audio device, so it runs
silently in CI — and the rest is listed under [What is left](#what-is-left).

## Known risks

WebKitGTK's DMA-BUF renderer has been reported to leave pages blank on NVIDIA drivers under
Wayland. The usual escape hatch is `WEBKIT_DISABLE_DMABUF_RENDERER=1`. The shell does not set it by
default, and it is set only if a test on NVIDIA hardware fails. That test has not happened: the
machine every check so far ran on loaded Intel's video driver, so the NVIDIA case is still open.

The official WebKit view is one pixel wide and cannot receive a person's volume input, whether the
GTK window is open or in the tray. YouTube Music can reset or replace its media element. An earlier
Linux rule treated the resulting 100% report as a new choice; the shared Rust rule now reapplies
Goosic's chosen volume and mute state when a report drifts. A user then heard brief loud bursts
before that asynchronous correction arrived, both with the window open and in the tray. The Linux
host now injects a page-world guard before the player loads: it caps media volume synchronously
when the page writes it, before a new element plays, and when a new element enters the document.
Goosic's own volume and mute controls update that guard. The whole WebKit view stays muted until
the new document confirms the guard, and remains muted with a terminal warning if WebKit cannot
install it. The Rust report reconciliation still checks the reported state. A live Flatpak playback
check in both window states is still needed. Compilation and a simulated page-element test do not
establish that the installed YouTube Music page uses only those guarded elements.

An installed Flatpak session on 27 September 2026 also logged WebKit's
`WebLoaderStrategy::internallyFailedLoadTimerFired` error and GLib socket warnings. The system
recorded a `SIGSEGV` from `WebKitNetworkProcess` in that Flatpak's control group. The installed
GNOME 50 runtime matched Flathub's current revision when checked. The available log does not show
whether this crash interrupted playback or caused the volume reset, so the WebKit failure needs its
own reproduction and diagnosis.

## What is left

The shell plays through Rust's lease and now has an offline Flatpak build. Signed-in Home and
Library use the active account's WebKitGTK profile, with cookies kept out of the service protocol.
The tests cover page conversion, account-page invalidation, the service client's lease exchanges
against the shared protocol fixtures, and the renderer's
exact owner/generation guard. A live native WebKitGTK read succeeded for signed-in Home, playlists,
liked songs and the owned-playlist picker without sending credentials to Rust's service. What
remains is proving account mutations and playback in a real packaged session, and closing the
product gaps below. Shared crate, protocol, Makefile and CI work lands on `development` first.
After the Flatpak stopped mounting Goosic's own data directory read-only, a user completed a real
sign-in in the sandbox. That has not yet been followed by a restart and authenticated reads there.

The parity work on `feature/linux/windows-parity` synchronizes the shared 0.2.7 backend and adds
an adaptive GTK presentation following [UI_DESIGN.md](UI_DESIGN.md). Navigation becomes an overlay
below 820 logical pixels; the transport stays within a 740-pixel capsule and wraps into two rows
when needed, retaining Lyrics and Playing Next. Search floats above its results, with its native
single-selection category group wrapping at narrow widths. Clearance is represented inside the
virtualized scroll content, so both the first result and last actionable row remain reachable.
The volume slider replaces the utility controls in a homogeneous native stack: opening it does
not change the bar's size or move artwork and transport, and More still offers Lyrics, Queue and
Mute. GTK's portal appearance signals update the palette without guessing from a theme name;
contrast mode uses solid surfaces and suppresses the decorative artwork backdrop.

The full-window player shares the compact player's callbacks and playback state. It shows a
large rounded cover and transport beside lyrics or the editable queue on a wide window, and
keeps controls below the auxiliary pane on a narrow one. Its dark treatment survives a light
browser theme. Opening, closing and resizing never unmount the WebKitGTK playback host. The
playing cover can also form a cropped, blurred browsing backdrop; GTK's render graph performs
the blur, and the saved `artwork_background` setting now has an Appearance control and affects
the browser. This is artwork blur, not a claim that GTK supplies WinUI acrylic compositor blur.

Song menus offer Play next and Add to queue for guests as well as signed-in listeners. Queue
rows have artwork, move and remove actions; the playing occurrence cannot be removed. Clear
upcoming preserves the current entry and its history, invalidates a pending radio reply, and
can be undone for ten seconds while the queue revision and listening context remain unchanged.
Duplicate tracks retain their occurrence cursor when reordered or selected. Synced lyric lines
request bounded seeks through the same shell method as the timeline. Manual wheel or keyboard
exploration suspends following and reveals Resume lyrics; no timing is inferred from line text.
The sleep timer pauses through the existing leased host after 15, 30, 45 or 60 minutes, or at the
end of the chosen song, without advancing the queue or quitting.

The native desktop fixture exercises the actual shell at 1100 and 480 pixels with synthetic
artwork and text in isolated XDG directories. It never signs in or produces sound. Captures cover
browsing, search, light/dark settings, volume, queue, expanded cover and lyrics presentations;
geometry assertions keep the transport in the window and preserve its bounds when volume opens. Pure tests cover queue occurrence preservation, clear/undo expiry
and window breakpoints. These checks establish rendering and offline behavior, not real account,
audio, screen-reader or compositor acceptance. Windows still has a mini player and installer
updates that GTK does not implement, and word-timed listening effects remain separate work. The offline Flatpak build and its service
exchange tests pass on GNOME 50. Plasma's native GTK scrollbar sliders emit minimum-size warnings
during desktop capture; a debugger trace places them in scrolled-window scrollbar measurement,
not the transport slider. They do not fail the fixture or establish compositor acceptance.

| What | Where it belongs | Why it is still open |
| --- | --- | --- |
| The Background portal request while windowless in the installed Flatpak | `platform/linux` | The sandbox smoke run connected to the service, but did not hide a playing window |
| The absence of brief volume bursts in the installed Flatpak | a person | The earlier report-based correction still allowed audible bursts; the new page-world guard needs live playback checks with the window open and in the tray |
| WebKit network-process stability in the installed Flatpak | `platform/linux` | One real session produced a network-process crash and socket warnings; its effect on playback and the trigger are not established |
| An end-to-end test proving neither host can sound without Rust's lease | `platform/linux` | The exact owner/generation guard has a unit test and runs before host calls; the renderer-level refusal still needs a harness |
| Account persistence and authenticated reads after a fresh Flatpak sign-in | a person | A user completed sign-in with writable sandbox storage; restart and account-page checks are still open |
| A whole track ending with the window hidden | a person | WebKitGTK may throttle timers in a hidden view; end of track is expected to survive, unobserved |
| An import from a real previous Goosic install | a person | Only a scratch `.webm` has been imported and played |
| NVIDIA hardware, and GNOME, Xfce and COSMIC sessions | a person | Every live check ran on Plasma with Intel graphics |
| Account-scoped mutations with a real account | a person | Read-only Home, playlists, liked songs and the owned-playlist list passed a live native test; mutations are built but have not been exercised against that account |
| Exact owned-playlist item edits | `platform/linux` | Rename, privacy and delete are present; removing or moving one entry needs its per-entry `setVideoId`, which the current catalog projection drops |
| Word-timed lyrics, listening effects and session-only timing import | `platform/linux`, after development is synchronized | The shared service accepts LRCLIB Lyricsfile; GTK rendering/import remain pending. See [the lyrics handoff](LYRICS_TIMING.md) |
| Saved queues and a native mini player | `platform/linux` | Queue editing and the full player are present; queue persistence and a separate compact window are not |
| Window and playlist context menus | `platform/linux` | Track actions and a checked owned-playlist manager are present; the native window menu is still pending |
| The decoded WAV cache moved from data to cache, on every platform | `development` | Unchanged; see Where things are stored |
| The `goosic-paths` crate from `rescue/native-mac-shell-and-paths` | `development` | Never merged, so path rules are still duplicated per crate |

The credentialed Flatpak mutation/playback and renderer-level lease checks still gate the first
public Linux release. Offline rendering and successful packaging do not close those gates.

A native build on a distribution whose GTK is older than 4.20, such as the current Ubuntu LTS, would
not follow the system's dark mode. The Flatpak is the supported way to run the shell there.

## Native presentation checks

The offline desktop fixture is opt-in because it creates GTK windows. Run it with a deliberately
missing service path and fresh XDG directories, rather than touching real account profiles:

```sh
GOOSIC_SERVICE_PATH=/tmp/goosic-no-service \
XDG_CONFIG_HOME=/tmp/goosic-ui-config XDG_DATA_HOME=/tmp/goosic-ui-data \
XDG_CACHE_HOME=/tmp/goosic-ui-cache GOOSIC_UI_CAPTURE_DIR=/tmp/goosic-ui-captures \
cargo test --manifest-path apps/goosic-linux/Cargo.toml \
    desktop_layout_keeps_controls_inside_the_window -- --ignored --test-threads=1
```

`Ctrl+Shift+F` opens or closes the full player, `F11` changes full-screen presentation, and
`Escape` dismisses it or the current temporary layer. Search is `Ctrl+F`, Lyrics is `Ctrl+L`,
Playing Next is `Ctrl+Shift+Q`, and the sidebar is `Ctrl+B`; `Ctrl+Q` retains its Linux meaning of
Quit. Previous/Next are `Ctrl+Left/Right`, volume is `Ctrl+Up/Down`, seek is `Shift+Left/Right`,
Mute is `Ctrl+M`, Shuffle is `Ctrl+S`, and Repeat is `Ctrl+R`. Space leaves text entry, buttons
and scales to GTK. Settings contains the complete shortcut list.

## Swift Linux retirement

The shared 0.2.7 migration removed the Swift Linux shell, SwiftCrossUI, Linux C shims and the
Swift Linux CI job. GTK is now Linux's native shell; the Rust protocol and conformance fixtures
are its reference, while the native Swift package remains macOS-only. This retirement did not
establish live Linux playback acceptance; the release checks above remain required.
