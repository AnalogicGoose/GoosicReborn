# Legacy compatibility and migration boundary

GoosicReborn is an MIT repository. Legacy GPL implementation files are not copied here and must not be linked into these crates or the Swift package. The migration preserves behavior and compatibility contracts through documented identifiers and an explicit, reversible adapter instead of source reuse.

This applies to the catalog too. `goosic-catalog` is written against the public YouTube Music web-client request contract — endpoint, client identity, and the stable filter page parameters — not against legacy source. Its parser deliberately does not mirror the legacy traversal: it collects renderer nodes by key anywhere in the response rather than following fixed paths, which is both an independent implementation and more resilient to upstream layout changes.

## Retained identifiers and storage keys

The app identity remains `goosic`; the historical preference keys remain unchanged: `ytm-theme`, `ytm-settings`, `ytm-layout`, and `ytm-track-source`. Playback bridge/session identifiers that existing clients may emit remain `goosic_generation`, `goosic_autoplay`, `goosic_volume`, `goosic_muted`, `goosic-player-generation`, `goosic-player-video-id`, `goosic-player-sequence`, `goosic-player-autoplay`, `goosic-player-volume`, and `goosic-player-muted`. The authority's new wire names (`accountId`, `owner`, `generation`, and `sampleSequence`) are additive and versioned; they do not silently rename those legacy keys.

## Safe migration

Migration reads legacy state without deleting it, writes the new representation only after successful validation, and records enough information to roll back to the old state. Account changes clear the active playback lease before any new host starts. A failed migration leaves legacy data untouched. No migration step should infer entitlement from a Premium flag or bypass the official playback path.

## Contract summary

- Online playback is the official YouTube Music web player (future WKWebView/WebView2/WebKitGTK host), including ordinary advertisements and restrictions.
- Playback has exactly one owner at a time; owner transitions are generation-scoped.
- Samples are strictly increasing within each generation; stale generations cannot control playback.
- Advertisement markers are normal metadata and never errors or teardown.
- Explicit local downloads are separate from online playback; account cookies never enter download tooling.
- Credentials, cookies, bridge secrets, and signing material are never committed, logged, or sent on the service stdout protocol.
