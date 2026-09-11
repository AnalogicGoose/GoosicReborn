# Legacy compatibility and migration boundary

GoosicReborn's original code is MIT, and the previous Goosic is GPL-3.0. Its code may be ported here directly when it already does the job — the owner has chosen functionality over rewriting — on two conditions: the ported file keeps the GPL header and the original authors' copyright, and the combined program is distributed under GPL-3.0 (`LICENSE-GPL-3.0`). The first such port is the account-scoped InnerTube reader in `PersonalCatalog.js`, which now also carries the mutation layer from `src/lib/innertube/mutations.ts` — the like, subscription, and playlist-edit calls, including the checks that catch an edit upstream refuses while answering HTTP 200. The second is the full-screen player's procedural background: the palette extraction and seeded mesh from `src/components/layout/now-playing-background.tsx`, its motion from `src/index.css`, and the cover-upscaling rule from `src/components/shared/thumbnail.tsx`, now in `Core/NowPlayingMesh.swift` and drawn by `Platform/macOS/NativeMacFullPlayer.swift`. The previous Goosic wrote that background independently from the technique described by `frigopedro/Apple-Music-Background`, which carries no license, and the port keeps it that way: no code from that repository is used. Everything below about formats and storage keys still applies to the legacy *data* import, which reads old data and never modifies, deletes, or carries credentials from it.

This applies to the catalog too. `goosic-catalog` is written against the public YouTube Music web-client request contract — endpoint, client identity, and the stable filter page parameters — not against legacy source. Its parser deliberately does not mirror the legacy traversal: it collects renderer nodes by key anywhere in the response rather than following fixed paths, which is both an independent implementation and more resilient to upstream layout changes.

## Retained identifiers and storage keys

The app identity remains `goosic`; the historical preference keys remain unchanged: `ytm-theme`, `ytm-settings`, `ytm-layout`, and `ytm-track-source`. Playback bridge/session identifiers that existing clients may emit remain `goosic_generation`, `goosic_autoplay`, `goosic_volume`, `goosic_muted`, `goosic-player-generation`, `goosic-player-video-id`, `goosic-player-sequence`, `goosic-player-autoplay`, `goosic-player-volume`, and `goosic-player-muted`. The authority's new wire names (`accountId`, `owner`, `generation`, and `sampleSequence`) are additive and versioned; they do not silently rename those legacy keys.

## What the import actually does

The Downloads screen can also import finalized `.webm` files from the legacy `offline-media/stream` directory. It references those files in place, skips empty or explicitly invalid entries, and never deletes or copies the previous media. A request to play one must first hold Rust's `localDownloadedFile` lease; Rust decodes the WebM/Opus source into its own WAV cache, and macOS AVFoundation opens only the returned decoded path. This build does not implement new downloads, yt-dlp, or cookie-backed extraction.

The previous app kept its preferences in the web view's `localStorage`, which on macOS and Linux is a WebKit SQLite database. `goosic-settings` reads it as follows:

- The database and any write-ahead log are **copied before they are opened**, so a partial or failed import cannot alter the old app's state. A test asserts the legacy file is byte-identical and its modification time unchanged after a read.
- Only the documented preference keys are read. Caches (`ytubic-query-cache`, the per-track cover cache) are skipped.
- The values this build understands are adopted: theme, volume, mute, and the legacy "auto radio" setting, which is the same intent as autoplay. Everything else is stored **verbatim under its original key name**, so nothing is renamed, nothing is lost, and a later build can adopt a setting this one has no home for.
- **Credentials are never carried over.** The legacy `ytm-settings` document holds a Last.fm session key inline, and the store also holds a Musixmatch token and a visitor id under their own keys. Credential-shaped fields are stripped at any depth, credential-only keys are never read, and a test asserts none of them can reach the settings file.

Windows is not covered: WebView2 stores local storage in a LevelDB directory rather than a SQLite database, and no reader for it exists here.

Cookies and account stores under the legacy application-support directory are never opened at all.

## Safe migration

Migration reads legacy state without deleting it, writes the new representation only after successful validation, and records enough information to roll back to the old state. Account changes clear the active playback lease before any new host starts. A failed migration leaves legacy data untouched. No migration step should infer entitlement from a Premium flag or bypass the official playback path.

## Contract summary

- Online playback is the official YouTube Music web player (future WKWebView/WebView2/WebKitGTK host), including ordinary advertisements and restrictions.
- Playback has exactly one owner at a time; owner transitions are generation-scoped.
- Samples are strictly increasing within each generation; stale generations cannot control playback.
- Advertisement markers are normal metadata and never errors or teardown.
- Explicit local downloads are separate from online playback; account cookies never enter download tooling.
- Credentials, cookies, bridge secrets, and signing material are never committed, logged, or sent on the service stdout protocol.
