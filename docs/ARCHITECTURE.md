# Architecture and non-negotiable contracts

## Data flow

`SwiftCrossUI shell -> GoosicServiceClient -> goosic-service (NDJSON) -> goosic-core | goosic-catalog | goosic-lyrics | goosic-settings | goosic-downloads | goosic-accounts -> goosic-protocol`

The shell requests transitions; it never decides whether a playback transition is valid. `goosic-core` is the single authority and has no UI, WebView, network, cookie, or audio dependencies. The service owns one authority instance for its process lifetime.

`goosic-catalog` is a separate, read-only concern: it answers *what exists* and never *who may play*. Catalog commands are dispatched before the authority and cannot alter ownership, generation, or sequence. It is the only crate that performs network I/O, and it is split into a pure parsing layer (unit-tested against fixtures) and a thin HTTP client (covered by `#[ignore]`d live tests).

`goosic-lyrics` is a third read-only lookup, against [LRCLIB](https://lrclib.net) — an open database needing no account or key. The previous Goosic also carried Genius and Musixmatch; neither is migrated, because Genius requires scraping rendered HTML and Musixmatch requires a user token, and a token is a credential this migration does not carry over. Which line is highlighted is *not* computed here: that belongs to the shell, which is the side that knows the playback position moment to moment, and one copy cannot drift from another.

`goosic-settings` owns durable preferences for the same reason: the shell is a renderer, and persistence should not be reimplemented per platform. `goosic-downloads` indexes and decodes local media, and `goosic-accounts` stores account metadata only — it has no WebKit, cookie, or credential integration, because the platform UI owns the actual profile data. All three are dispatched before the authority and cannot change who owns playback.

`goosic-service` is private to one app process and one client: the Swift shell launches it as a child and communicates over inherited stdin/stdout. It is not a daemon or socket endpoint, and those streams must never be shared or multiplexed. Generation provides freshness authorization within this single-client boundary. If a future design multiplexes clients, it must first add an unforgeable per-client capability and require active-owner authorization before allowing account resets.

## Wire contract (protocol 0.3.0)

Every request is one JSON object per line:

```json
{"protocolVersion":"0.3.0","requestId":"r-1","command":"playback.claim","payload":{"owner":"officialWebView","generation":0}}
```

Every request produces exactly one response line. A response has `ok: true` and a payload, or `ok: false` and a structured `{code,message}` error. Playback commands are `hello`/`handshake`, `state.get`, `account.change`, `playback.claim`, `playback.release`, and `playback.sample`. Owners are exactly `none`, `officialWebView`, and `localDownloadedFile`; online playback means only `officialWebView`.

Catalog commands are `catalog.search` (`query`, `filter`), `catalog.browse` (`catalogId` is a route name), `catalog.album`, `catalog.playlist`, `catalog.artist`, and `catalog.radio` (`catalogId` is the upstream identifier; for radio, the seed video id). Each answers with a `catalog` payload: a page of shelves and/or an ordered track list of flat, normalized rows. Only rows carrying a `videoId` are playable.

Lyrics commands are `lyrics.get`, which takes a `lyrics` query of title, artist, album, and length and answers with a `lyrics` payload of timed or untimed lines.

Settings commands are `settings.get`, `settings.set` (a partial `preferences` patch; absent fields are left alone), and `settings.importLegacy`. Each answers with a `settings` payload.

Downloads commands are `downloads.list`, `downloads.importLegacy`, and `downloads.prepare`. The first two inspect finalized files already present in the user's previous Goosic media directory; they never start a downloader. `downloads.prepare` is accepted only while Rust's active owner is `localDownloadedFile` and its generation matches, then decodes the source WebM/Opus into Rust's private WAV cache and returns that path to the macOS AVFoundation host.

### Frame budget

Requests are capped at 64 KB. A catalog page is clamped by the service to 192 KB serialized — first by structural caps, then by dropping rows — and sets `truncated` when anything was removed, so a partial list is never presented as complete. The shell accepts responses up to 256 KB and gives `catalog.*` commands a longer wait than playback commands, because they reach a third-party service.

## Playback authority contracts

1. At most one non-`none` owner is active.
2. Claims, releases, and samples carry the current generation. A successful claim and release advance the generation, invalidating stale clients.
3. Sample sequence values must strictly increase within one playback generation; each successful claim/release and account change resets sequence to zero.
4. Account change clears the owner, advances generation, replaces the account identifier, and resets sequence.
5. `advertisement` (and any other marker) is metadata. An accepted marker does not release ownership, change generation, or turn into an error/teardown.
6. Stable error codes identify invalid owner/request, owner conflict/mismatch, missing owner, generation mismatch, and non-monotonic samples.

## Artwork

Catalog rows carry a thumbnail URL, and the shell fetches those itself rather than routing them through Rust. Two reasons: the service protocol is strictly serial, so a dense screen would serialize twenty downloads behind one another; and images are presentation, not authority.

SwiftCrossUI's `Image` reads its source synchronously while computing layout, so it is never handed a remote URL — `ArtworkCache` downloads off the main thread, writes to a local cache, and `Image` only ever opens a local file. A half-written file cannot be picked up mid-download, because each is written beside its destination and renamed.

The fetch is deliberately narrow:

- **HTTPS only, and only from the hosts YouTube Music serves artwork from.** The match is on a label boundary, so `evilgoogleusercontent.com` is refused. A catalog response cannot point the shell at an arbitrary server.
- **An ephemeral session with cookie storage disabled**, so a thumbnail request can never become an authenticated one.
- Bounded size and concurrency, and a failure is remembered so a broken image is not retried on every layout.

## Platform material

Screens stay portable SwiftCrossUI; only the material behind them is platform-specific. `MaterialSurfacePlatformResolver` is a pure function from platform and OS major version to a backend, so selection is testable without a window:

| Platform | Backend |
| --- | --- |
| macOS 26+ | `NSGlassEffectView` (Liquid Glass) |
| macOS 14–25 | `NSVisualEffectView` |
| Windows | reserved for a WinUI backdrop |
| Linux | plain background; GTK 4 has no equivalent vibrancy primitive |
| anything else, or an unknown version | plain background |

Two rules keep this from leaking into behaviour. The material is applied with `.background(…)`, so it is a leaf *behind* the content and never wraps it — controls and their accessibility stay native. And its host view returns `nil` from `hitTest`, so it cannot swallow a click meant for a button above it.

Unsupported versions fall back rather than failing: there is no private feature toggle and no reimplemented blur.

## Shell backends

The shell is one SwiftCrossUI target that compiles for more than one backend. Everything platform-specific lives behind `#if os(macOS)` with a complete `#else` branch, so the non-macOS build is a real build rather than a broken one: `OfficialPlaybackHost`, `LocalPlaybackHost`, `AccountLoginHost`, `MaterialSurface`, and `SystemMediaControls` each ship a stub whose surface matches the macOS type. When a stub drifts from that surface the shell stops compiling off macOS, which is the intended signal — the stubs are part of the contract, not scaffolding.

The backend is chosen at build time through `SCUI_DEFAULT_BACKEND`, and the Make targets always set it: `AppKitBackend` on macOS, `GtkBackend` on Linux. It has to be explicit. SwiftCrossUI's `DefaultBackend` otherwise names every platform's backend target and, although each carries a platform condition, SwiftPM still resolves `swift-winui` into the build graph and tries to compile its C targets, which need Windows headers.

A stub build browses, searches, and renders the transport, but produces no audio. That is deliberate: a renderer that played without claiming a Rust lease would be a hole in the ownership model, so the absence of a host is expressed as a host that refuses rather than as an unguarded fallback.

## Official playback host (macOS)

One `WKWebView` renders the official player, in its own website data store, and is the only place online audio is produced. Three things about that host are load-bearing and easy to break:

- **User agent.** YouTube Music refuses to run under WKWebView's bare agent and shows "not optimized for your browser" instead. The configuration names a Safari version so the default agent is a complete Safari string, which is what this engine is.
- **Gesture gating.** The user's click happens in native UI and does not cross into the web view, so `mediaTypesRequiringUserActionForPlayback` is cleared. Without it the host's own play request is blocked and no media element ever starts.
- **Observer identity.** The bridge nonce, lease generation, and requested video id are injected into each load's user script rather than passed as URL parameters, because the official app rewrites its own location and drops query items it does not recognize. Injected values are encoded as JSON string literals.

Bridge events are accepted only when the version, nonce, generation, and video id all match the active load, the sequence advances, and the reported position, duration, and volume are possible. A rejection says which check failed; an opaque rejection is unactionable.

The official app runs its own "up next" queue. When it follows that queue to a video Goosic did not request, the observer pauses it and the host reports the move; Goosic then plays its own next track. Goosic owns the queue, so the app never plays something the user did not choose.

## Local downloaded-file host (macOS)

The local renderer is one `AVAudioPlayer` over a decoded WAV path returned by Rust. It is not a WebView and does not receive cookies or network URLs. Rust's local lease is claimed before preparation; the official WebView is paused and invalidated before that claim, and the local player is stopped synchronously before a release or owner switch. AVFoundation-confirmed play, pause, position, and end events carry the local Rust generation and an independent monotonic sequence.

## Security boundary

The protocol carries identifiers, playback metadata, and public catalog metadata only. Cookies, credentials, bridge tokens, signing keys, and downloaded media URLs must stay in platform-secure storage and never be logged or placed on stdout. WebView implementations must validate origin and generation before forwarding events to this authority.

Catalog reads are anonymous by construction: the client sends no cookies, no `Authorization`, and no account headers, and a test asserts its request context carries none. The anonymous `visitorData` token upstream returns is held in memory for the process lifetime to keep results stable; it is never persisted or written to stdout. A signed-in surface (a real library) is therefore not reachable through this client — it needs the official web view.
