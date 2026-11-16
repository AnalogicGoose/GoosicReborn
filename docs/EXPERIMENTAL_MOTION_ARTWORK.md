# Experimental motion artwork

This experiment evaluates real Apple Music album animations alongside Goosic's existing
YouTube Music playback. It is not a production release or a guarantee of artwork availability.
The macOS full and mini players offer an off-by-default Experimental Animated Artwork switch.
Enabling it sends the current title, artist and album to Apple's public catalog and reads a
matching album page. There is no Apple account, web-player token or third-party proxy.
The listener can turn it off immediately; system and app Reduce Motion also disable it.

The native resolver follows the approach studied in Sung at commit
2edcaff81ce96363c4f9de275944a2ce53770772: search, check recording metadata, read the album header's
square video artwork, then inspect the HLS manifests for a bounded MP4. This prototype is more
conservative: it requires exact normalized title, artist and album plus duration within five
seconds, or one unambiguous album when album metadata is absent. It does not guess original
albums for singles. Missing, ambiguous and failed matches retain Goosic's original still cover.
Apple's undocumented page structure can change, and this experiment does not establish permission
to redistribute Apple's artwork. Apple's service terms remain a release consideration.

Requests are anonymous, HTTPS-only, restricted to three exact Apple hosts and checked again on
redirect. Metadata is bounded to 2 MiB and a video to 16 MiB. Only unencrypted, finite HLS with
one MP4 resource is accepted. The video must be a square H.264 track, at most 1080 pixels and
30 frames per second, no longer than 60 seconds, with no audio tracks. Artwork uses a temporary
file which is removed when the view releases it; there is no artwork export or offline music
library. Muting is defence in depth; files containing audio are rejected before player creation.
Track changes cancel stale publication. Hiding the view or window releases the decoder, and
pausing music pauses the artwork. Failures are logged to stderr without media URLs or track data.

Shared transport limits live in goosic-shell-support and the Swift Core copy has matching tests.
The macOS experiment is native AVFoundation presentation. Windows still uses its Image in
MainWindow.FullPlayer.cs; Linux still uses its static artwork cache and GTK picture in the
platform/linux branch. Neither has this experimental renderer. They require native adoption
before a cross-platform feature release, including matching and decoder tests, motion and hidden
window acceptance. No production parity is claimed for this macOS experimental build.

Verification includes make test, a packaged application, and fixture tests for matching, page and
manifest parsing and URL restrictions. Actual Apple retrieval and visible animation must be
reported separately from compilation. For a live check, enable the switch in the full player,
play a known album track with motion artwork, then test pause, resume, track change, hiding the
window and Reduce Motion. No match should affect music playback or replace its still cover.

Build the manual-install universal app with `sh tools/package-motion-experiment-macos.sh` using
Xcode and a Rust toolchain with both Apple targets installed. It packages as Goosic Motion
Experiment.app with its own bundle identifier and no updater feed, so it can sit beside the
regular application. It still uses Goosic's existing account storage; run only one copy at a
time. Enable Experimental Animated Artwork at the bottom left of the full player. This build
is opt-in and is not offered through the normal update feed.

The initial Apple network check resolved Blinding Lights from After Hours (Deluxe), obtained a
real MP4 in 3.6 seconds and passed the native no-audio and video-format validation. This is one
live retrieval result, not a catalog coverage estimate or visual acceptance of the player.
