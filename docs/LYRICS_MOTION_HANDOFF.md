# Lyrics motion handoff

The optional shared line end landed in #154 on development. The macOS presentation landed
in #155 on platform/macos. The Windows native port is #158, with GTK fixture compatibility
in #157. Release preparation targets 0.4.1; publication and the signed Sparkle feed remain
with the owner. [LYRICS_TIMING.md](LYRICS_TIMING.md) records behavior and native differences.

The owner previously confirmed the macOS sweep, interlude dots, paused cover, backdrop and
emphasis behavior; macOS per-row following still needs their acceptance. No live Windows
visual check has occurred. On both ports, inspect line changes for a one-frame flash if the
scroll jump and compensating row translations land in different frames. If reproduced,
replace following with offset-only column positioning. Interlude rows retain their height.

The shared Rust lyrics_motion module now owns sweep spans, supplied-end rest boundaries,
interludes and row-motion values. Swift and C# retain tested copies while bridge adoption
remains pending. GTK still draws whole lines; native motion is tracked in parity issue #159.

The exact LRCLIB lookup still wins over a word-timed search result for the same recording.
Changing source ranking is separate shared-service work. Do not promise word timing for
any named song without inspecting the provider's current recording response.
