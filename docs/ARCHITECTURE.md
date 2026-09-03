# Architecture and non-negotiable contracts

## Data flow

`SwiftCrossUI shell -> GoosicServiceClient -> goosic-service (NDJSON) -> goosic-core | goosic-catalog -> goosic-protocol`

The shell requests transitions; it never decides whether a playback transition is valid. `goosic-core` is the single authority and has no UI, WebView, network, cookie, or audio dependencies. The service owns one authority instance for its process lifetime.

`goosic-catalog` is a separate, read-only concern: it answers *what exists* and never *who may play*. Catalog commands are dispatched before the authority and cannot alter ownership, generation, or sequence. It is the only crate that performs network I/O, and it is split into a pure parsing layer (unit-tested against fixtures) and a thin HTTP client (covered by `#[ignore]`d live tests).

`goosic-service` is private to one app process and one client: the Swift shell launches it as a child and communicates over inherited stdin/stdout. It is not a daemon or socket endpoint, and those streams must never be shared or multiplexed. Generation provides freshness authorization within this single-client boundary. If a future design multiplexes clients, it must first add an unforgeable per-client capability and require active-owner authorization before allowing account resets.

## Wire contract (protocol 0.2.0)

Every request is one JSON object per line:

```json
{"protocolVersion":"0.2.0","requestId":"r-1","command":"playback.claim","payload":{"owner":"officialWebView","generation":0}}
```

Every request produces exactly one response line. A response has `ok: true` and a payload, or `ok: false` and a structured `{code,message}` error. Playback commands are `hello`/`handshake`, `state.get`, `account.change`, `playback.claim`, `playback.release`, and `playback.sample`. Owners are exactly `none`, `officialWebView`, and `localDownloadedFile`; online playback means only `officialWebView`.

Catalog commands are `catalog.search` (`query`, `filter`), `catalog.browse` (`catalogId` is a route name), `catalog.album`, `catalog.playlist`, and `catalog.artist` (`catalogId` is the upstream identifier). Each answers with a `catalog` payload: a page of shelves and/or an ordered track list of flat, normalized rows. Only rows carrying a `videoId` are playable.

### Frame budget

Requests are capped at 64 KB. A catalog page is clamped by the service to 192 KB serialized — first by structural caps, then by dropping rows — and sets `truncated` when anything was removed, so a partial list is never presented as complete. The shell accepts responses up to 256 KB and gives `catalog.*` commands a longer wait than playback commands, because they reach a third-party service.

## Playback authority contracts

1. At most one non-`none` owner is active.
2. Claims, releases, and samples carry the current generation. A successful claim and release advance the generation, invalidating stale clients.
3. Sample sequence values must strictly increase within one playback generation; each successful claim/release and account change resets sequence to zero.
4. Account change clears the owner, advances generation, replaces the account identifier, and resets sequence.
5. `advertisement` (and any other marker) is metadata. An accepted marker does not release ownership, change generation, or turn into an error/teardown.
6. Stable error codes identify invalid owner/request, owner conflict/mismatch, missing owner, generation mismatch, and non-monotonic samples.

## Security boundary

The protocol carries identifiers, playback metadata, and public catalog metadata only. Cookies, credentials, bridge tokens, signing keys, and downloaded media URLs must stay in platform-secure storage and never be logged or placed on stdout. WebView implementations must validate origin and generation before forwarding events to this authority.

Catalog reads are anonymous by construction: the client sends no cookies, no `Authorization`, and no account headers, and a test asserts its request context carries none. The anonymous `visitorData` token upstream returns is held in memory for the process lifetime to keep results stable; it is never persisted or written to stdout. A signed-in surface (a real library) is therefore not reachable through this client — it needs the official web view.
