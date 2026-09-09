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

Every request produces exactly one response line, and responses are matched by request id rather than by arrival order. A response has `ok: true` and a payload, or `ok: false` and a structured `{code,message}` error. That distinction used to be theoretical and is now load-bearing. The service answered one request at a time, so a catalog browse waiting on a third-party host held every command queued behind it — pause, seek, release included — for as long as the upstream took, and the client, which read the pipe until its own request's answer appeared, could not have noticed a different answer if one had arrived. A transport control that does nothing for twenty seconds is indistinguishable from a broken player.

Read-only upstream lookups — the `catalog.` and `lyrics.` families — are now answered on a small pool of worker threads instead of the main loop. Membership in that pool is decided by what a command may touch, not by how slow it is: both families take their client by shared reference, keep their own state behind a mutex, and are dispatched ahead of the playback authority precisely because they cannot alter ownership, generation, or sequence. Everything else needs `&mut` access to state the main loop owns, and stays on it. One writer owns stdout and every response passes through it, so concurrency can never interleave two responses inside one line.

The client side matches. Several requests may be outstanding, each completes on its own, and a request that outlives its deadline now fails alone rather than tearing down the child process — the old teardown existed because a reader blocked on an answer that never came could not be recovered any other way, which meant one slow catalog read could take playback down with it. Teardown is now reserved for what genuinely corrupts the stream: an undecodable line, a version mismatch, an oversized frame, or end of file.

Playback commands are `hello`/`handshake`, `state.get`, `account.change`, `playback.claim`, `playback.release`, and `playback.sample`. Owners are exactly `none`, `officialWebView`, and `localDownloadedFile`; online playback means only `officialWebView`.

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

Two portability rules follow from the toolchain rather than from this design, and both are load-bearing because breaking either produces a failure far from its cause. `URLSession` lives in `FoundationNetworking` off Darwin, so any file that fetches over HTTP needs a `#if canImport(FoundationNetworking)` import. And swift-corelibs-xctest discovers tests by casting each method to `(Self) -> () throws -> Void`; a `@MainActor`-isolated method does not carry that type, so an isolated `XCTestCase` subclass aborts the entire run before a single test executes. Isolation therefore goes on the individual test — `func testX() async` wrapping its body in `await MainActor.run` — never on the class.

Under the Swift 6 language mode that pattern gains a second requirement: the body of `MainActor.run` must not reach for anything on `self`, because sending a non-`Sendable` `XCTestCase` across an isolation boundary is a data race the compiler now rejects. Test fixtures a `MainActor.run` body needs are therefore `static` — `Self.makeCache()`, `Self.model(tracks:)` — which keeps the closure free of `self` while leaving the test method itself unisolated, so discovery still works.

A stub build browses, searches, and renders the transport, but produces no audio. That is deliberate: a renderer that played without claiming a Rust lease would be a hole in the ownership model, so the absence of a host is expressed as a host that refuses rather than as an unguarded fallback.

## The official bridge, and what is not platform-specific about it

`OfficialBridge` holds the half of the bridge that does not belong to any WebKit
implementation: the wire shape of a message, the page observer that produces them, the media
session guard, and `rejectionReason` — the function that decides whether an event is
trustworthy at all.

It is separate for one reason. macOS drives a `WKWebView` and Linux will drive a WebKitGTK
`WebKitWebView`, and if each host carried its own copy of that logic there would be two
answers to "may this play" that are free to drift apart. Version checks, token matching,
generation matching, and the strictly increasing sequence are the enforcement of the
authority's contract at the renderer's edge; they are not a detail of an engine.

The JavaScript moves across unchanged because it does not need changing: WebKitGTK exposes
the same `window.webkit.messageHandlers.<name>.postMessage` that WKWebView does, so one
observer serves both.

What stays per-platform is only the transport — creating the view, installing the script, and
receiving the message — plus how the message's origin is established. macOS checks
`WKScriptMessage.frameInfo.securityOrigin` against the allowed host after the fact. WebKitGTK
does not deliver an origin with the message, so the equivalent guarantee has to come from
registering the handler in an isolated script world, where the page's own scripts cannot see
the handler to post to it in the first place.

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

Where each WebKit surface may navigate is decided by `NavigationPolicy` in `Core`, not inside a delegate callback. That placement was bought the hard way. Both policies existed for the whole life of this code with a `decisionHandler` parameter that omitted the main-actor attribute the SDK declares, so neither method was ever adopted as the protocol's optional requirement, WebKit never called either one, and every navigation on both surfaces was allowed. Nothing detected it: not the compiler, which reported a warning about a method that "nearly matches"; not a test, because a rule reachable only through WebKit is a rule nothing can reach. The rules are now pure functions over a URL and a frame, a test asserts the two hosts actually implement the Objective-C selector, and the frame is named rather than inferred from a doubly-optional flag.

Both policies judge the main frame and concede the subframes. The main frame is the document a user reads and types into, so it stays on a known host: the player on `music.youtube.com`, the sign-in window on YouTube's hosts or one of Google's sign-in services on one of its country domains. A subframe is content the allowed page itself embedded, and it is how sign-in serves reCAPTCHA and how YouTube Music serves advertisements — policing those would not make either surface safer, it would stop sign-in from completing and would silently turn a player that reports advertisements into one that blocks them. Popups are refused by both, because neither surface has a use for a second window and, for the player, a second window would be a second media owner.

Catalog reads are anonymous by construction: the client sends no cookies, no `Authorization`, and no account headers, and a test asserts its request context carries none. The anonymous `visitorData` token upstream returns is held in memory for the process lifetime to keep results stable; it is never persisted or written to stdout. A signed-in surface (a real library) is therefore not reachable through this client — it needs the official web view.

Account-scoped catalog reads use `PersonalCatalogHost`, a separate native browser-profile seam.
On macOS it creates an ephemeral view over the active account's persistent
`WKWebsiteDataStore`, performs the browse from the trusted YouTube Music origin, and projects the
answer directly into the small catalog shape the shell renders. The program it runs,
`Resources/PersonalCatalog.js`, is the InnerTube client and shelf parsers ported from the
previous Goosic under GPL-3.0; it builds the request from the page's own `ytcfg` and hashes the
session cookie in-document for authorization, so nothing about the session is exported. The profile's cookies and request
context remain inside WebKit. Raw responses and credentials never cross the Rust service
protocol. Binding another account cancels in-flight reads and clears account-scoped page state,
so a late response cannot paint one user's Library under another user's identity. Unsupported
platform hosts fail explicitly until their native account integration implements the same seam.
