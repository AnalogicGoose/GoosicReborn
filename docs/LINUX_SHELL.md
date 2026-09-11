# The Linux shell

`apps/goosic-linux` is the Linux application: a GTK 4 program written in Rust that replaces the
SwiftCrossUI build on Linux. It is step four of [the native-shell migration](NATIVE_SHELL_MIGRATION.md).
The plan says *what* replaces what; this document records how the Linux shell is built and why
each choice was made, so that the reasons survive the people who made them.

It is being written slice by slice on `platform/linux`. What exists browses and searches the
catalog, opens albums, artists and playlists, and plays through the official WebKitGTK host under
Rust's lease, with a queue, a now-playing bar, radio, and preferences that are saved and restored.
That playback has been heard rather than only compiled: a scratch harness asked the shell to play a
real track the way a Play button does, the advertisement in front of it was reported and not
skipped, validated samples from the page moved the bar through the song, and PipeWire showed the
WebKit process's uncorked stream. Until the remaining slices land — artwork, the lyrics and queue
panels, downloads, accounts, the media-player interface and background mode — the Swift shell
remains the Linux build, and the sections about those describe a destination.

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
way — and the shell's controls live in a toolbar inside the content. And the shell's own CSS uses
the theme's named colours rather than literal ones, so a theme change reaches everything.

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
does not without an extension, so the icon is a convenience and never the only way back. On GNOME
the shell asks the Background portal to run without a window, and GNOME lists it under Background
Apps with a way to close it. On every desktop, launching the application again presents the
existing window.

The first time the window closes while something is playing, a notification says the application
is still running and how to quit it, so somebody who expected closing to mean quitting is not left
wondering what is holding their speakers. Quitting is explicit — `Ctrl+Q`, the status icon's menu,
the media panel's quit, GNOME's background list — and it releases the Rust lease, stops both
playback hosts and ends the service.

The mechanics follow from that. The window is `hide-on-close`, and the application holds itself
while it runs in the background, so losing its last visible window does not end the process. The
official web view lives as long as the application, not as long as the window: rebuilding it on
reopen would drop the page and the lease with it. While audio is playing, the shell inhibits
suspend through the Inhibit portal and releases the inhibition on pause. It does not stop the screen
from locking; music is not a video.

Two behaviours have to be observed rather than assumed, and each gets a test on real hardware before
this is called done. WebKitGTK may throttle timers in a page whose view is hidden, which would slow
the observer's periodic report; the observer also reports on media events, so end-of-track
detection should survive, but that is an expectation. And the page must keep playing while hidden.
Browsers do not pause background audio, but this is something to hear, not to argue.

## Desktop portals

The shell reaches desktop services through portals wherever one exists, so each desktop supplies
its own implementation: KDE's file picker on Plasma, GNOME's on GNOME.

The colour scheme needs no code: GTK 4.20 reads it from `org.freedesktop.portal.Settings`, under
the `org.freedesktop.appearance` namespace. That is the interface applications call.
`org.freedesktop.impl.portal.Settings`, which is sometimes recommended for this, is the interface
portal *backends* implement, and an application never calls it.

The shell uses the Inhibit portal for suspend while audible, the Background portal to run without a
window, the Notification portal for the one notice above, and OpenURI to open an outside link in the
user's browser. Sign-in never goes there: it stays in the application's own window, under the shared
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
into `/app/bin` with the desktop file, metainfo and icons, and builds offline from vendored crate
sources, as Flatpak requires. A native build remains the development path. No other Linux package
format is planned.

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
        ui.rs              page rows, sidebar, search bar
        player_bar.rs      the now-playing bar
        official_host.rs   WebKitGTK player: script world, bridge handler, per-account session
        login_host.rs      planned: sign-in window under the shared navigation policy
        local_host.rs      planned: GStreamer playbin for decoded files
        mpris.rs           planned: org.mpris.MediaPlayer2 over gio
        status_icon.rs     planned: StatusNotifierItem, where a watcher exists
        portals.rs         planned: Inhibit, Background, Notification, OpenURI
    data/                  planned: desktop file, metainfo, icons, named after the application ID
    packaging/flatpak/     planned: manifest and vendored crate sources
```

The modules are flat rather than grouped into `state/`, `ui/` and `platform/` as first sketched,
because a dozen files did not need folders. What the grouping was meant to protect still holds:
`pages` and `playback` import no GTK, so the decisions they make are tested without a display, and
only `shell` joins them to widgets and hosts.

The shell lives on `platform/linux`, and each slice of it is a `feature/linux/<slug>`. Anything it
needs from `goosic-shell-support` or the protocol is not Linux work: it lands on `development`
first and reaches `platform/linux` through the cascade.

Make targets — `build-linux`, `test-linux` and `run-linux`, the last building the service and
pointing `GOOSIC_SERVICE_PATH` at it the way `run-swift` does — land on `development`, because the
Makefile is shared. Until they do, a development build runs from `apps/goosic-linux` after
`cargo build -p goosic-service` at the repository root, with
`GOOSIC_SERVICE_PATH=../../target/debug/goosic-service cargo run`; its tests find the service in
the same place without the variable. CI builds the
shell inside the Flatpak builder on the GNOME runtime rather than against the runner's own GTK:
`ubuntu-latest` ships a GTK older than 4.20, and building the package users install is the better
test in any case. That job runs when the shell, `goosic-shell-support` or the protocol changes.

## Order of work

The plan's order for step four holds. Transport, catalog, search, queue and settings come first,
because they need no platform host and prove the shell against the service. Then the WebKitGTK
player and the sign-in window, then local audio, then the media-player interface and the status icon,
then running in the background, and the Flatpak last. The test that no host can sound without Rust's
active lease comes with the first host, not after the last. The Linux local host in the Swift shell
already shows how: a paused GStreamer pipeline decodes without opening the audio device, so the test
runs silently in CI.

## Known risks

WebKitGTK's DMA-BUF renderer has been reported to leave pages blank on NVIDIA drivers under
Wayland. The usual escape hatch is `WEBKIT_DISABLE_DMABUF_RENDERER=1`. The shell does not set it by
default. It is tested first on an NVIDIA machine, which the main development machine is, and the
variable is set only if that test fails.

A native build on a distribution whose GTK is older than 4.20, such as the current Ubuntu LTS, would
not follow the system's dark mode. The Flatpak is the supported way to run the shell there.

## When the Swift Linux build goes

Once the GTK shell reaches parity and its Flatpak ships, the Linux half of the Swift package is
deleted: `Platform/Linux`, the `CWebKitGTK`, `CGLib` and `CGStreamer` modules, the Linux Swift CI
job, and the GTK backend setting in the Makefile. The macOS build of that package is untouched by
it. Until then, the Swift Linux shell stays as the conformance reference the GTK shell is compared
against.
