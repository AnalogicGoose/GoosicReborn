# Goosic 0.4.1 — Motion Artwork Experiment

Album covers, with a little movement.

## macOS: animated album covers

The cover comes to life when a matching animation is available.

The full player and mini player can now show real Apple Music album animations alongside
Goosic's YouTube Music playback. Enable Experimental Animated Artwork at the bottom left of
the player. The switch is off by default. Lookups send the track title, artist and album to
Apple. Exact metadata and duration checks reject uncertain matches; missing artwork keeps
the original still cover. This is an unofficial experiment with incomplete album coverage,
not a supported Apple integration or a guarantee of continued access.

## Silent by design

The music keeps its own player.

Artwork files containing audio are rejected. Animations pause with the music, respect Reduce
Motion and release their decoder when the player is hidden. The existing music playback and
advertisement handling continue through Goosic's playback authority.

## An app of its own

Try the experiment beside your regular installation.

The download is named Goosic Motion Experiment.app and has its own bundle identifier. It has
no automatic update feed. It shares Goosic's account storage, so run one copy at a time.
Windows and Linux motion artwork remain unimplemented.

## Downloads and installation

The universal ZIP supports Apple silicon and Intel Macs running macOS 14 or later. Extract it
and open Goosic Motion Experiment.app. The app is signed ad hoc, without a Developer ID or
notarization; macOS may require approval in System Settings → Privacy & Security. A SHA-256
checksum accompanies the download. Install and remove it manually.

The Rust workspace and 279 Swift tests passed, with the opt-in network test skipped during the
normal run. A separate live check retrieved Blinding Lights from After Hours (Deluxe) and
passed native video and no-audio validation. The packaged app's signature and architectures are
checked separately. Visible animation, track transitions and hidden-window behavior still need
a manual acceptance check. Apple's terms and artwork rights remain unresolved for a production
feature. Source and experiment details are in docs/EXPERIMENTAL_MOTION_ARTWORK.md.
