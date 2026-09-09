# The shell contract

This document is step one of [NATIVE_SHELL_MIGRATION.md](NATIVE_SHELL_MIGRATION.md): it
classifies what the Swift shell's `Core` directory actually contains, so that the parts three
shells must agree on can be frozen before any of them is rewritten.

The order matters and it is not bureaucratic. A rule that lives only in Swift source is a rule
that gets reimplemented from reading that source, and a reimplementation that is *close* is
worse than one that is obviously wrong: it passes a demo and diverges under load. Freezing the
contract first is what makes a second and third shell implementable without treating Swift as
the specification.

## What the four categories mean here

A **pure rule** is deterministic, takes no I/O, and would give the same answer on any machine
in any language — may this event be accepted, is this host allowed, what is the cache key for
this URL. These belong in Rust, in the frontend-neutral crate the plan calls
`goosic-shell-support`, because a rule with three copies has three answers as soon as one of
them is edited.

**Transport** is the NDJSON conversation with `goosic-service`: framing, correlating a response
to its request, deadlines, partial buffers, and what to do when the child process dies. It is
identical for every shell and nothing about it is presentational.

**Presentation state** is what a screen is showing right now — the selected route, focus, load
placeholders, panel visibility. It stays native. Moving it into Rust to be tidy would build a
cross-language UI framework nobody asked for, which the plan rules out explicitly.

**Platform integration** is the part that can only be written against one operating system: a
WebView and its origin isolation, an audio renderer, secure profile storage, media keys. It
stays native by definition, and it is where each shell keeps its own security responsibility.

## Where the line falls today

`Protocol.swift` is a hand-written Swift translation of types whose source of truth is the Rust
`goosic-protocol` crate. Nothing verifies the two still agree; they agree because someone kept
them in step. That is the most important thing this step has to fix, because every shell will
make the same translation and each one is a fresh opportunity to drift.

`ServiceClient.swift` is transport and only transport — process spawn, request numbering,
per-command timeouts, a partial-output buffer, and invalidation when the child dies. Its
`timeout(for:)` table is a rule that happens to live inside it.

`AccountLoginModel.swift` and `SystemMediaPlayback.swift` are almost entirely pure rules
already, which is why they were the easy half of the Linux port: `isAllowedLoginURL`,
`sanitizeMetadata`, the polling decision, the now-playing projection and the command
availability all answer questions with no machine in them. `OfficialBridge.swift` is the same
shape — `rejectionReason` and `isValidVideoID` are rules, and the observer and media-session
scripts are data those rules travel with, not code the shell runs.

`ArtworkCache.swift` and `MaterialSurfaceKind.swift` are split down the middle. The host
allow-list, the FNV-1a cache key and the platform-to-backend resolution are rules; the
`URLSession` fetching, the concurrency limit, the disk layout and the material itself are
platform integration.

`Catalog.swift` is mostly the conversion from protocol DTOs into what a list needs, which is a
rule, plus `CatalogLoadState`, which is presentation.

`Models.swift` is the one that is genuinely mixed, and at 1,978 lines it is most of the
problem. It holds forty-one published properties — presentation state, all of it — alongside
seventy-eight methods that include the queue advance, shuffle and repeat selection, seek and
volume clamping, and the validation in `receive(event:)` that decides whether a renderer's
report is worth believing. Those are rules sitting inside an observable object, reachable only
by constructing the whole model. Extracting them is the bulk of step two, and this document
exists so that extraction is a move rather than a rewrite.

`PlatformPlaybackHost.swift` is referenced by nothing and can be deleted.

## What is already pinned, and what is not

The Swift suite runs 108 tests, and they are not spread the way the risk is. Artwork caching
and catalog conversion carry the most; the official bridge, account validation, media
projection and the Linux hosts each carry a handful.

Two gaps matter more than the rest.

**The transport had no tests at all.** Nothing exercised framing, request correlation, a
timeout, a partial frame arriving across two reads, or the death of the child process. Three
shells were about to depend on that behaviour and the only description of it was one Swift file.

**`goosic-protocol` had two tests**: one wire snapshot and one error round trip. It is the
declared source of truth for a contract that three independently written clients will encode
and decode, and two tests do not describe it. The Swift DTOs are unverified against it.

Everything the migration plan lists as a required fixture — request, response, malformed frame,
timeout, owner conflict, stale generation, non-monotonic sample — falls into one of those two
gaps. That is not a coincidence; it is why the plan puts them first.

## How those two gaps were closed

Fixtures belong in Rust beside `goosic-protocol`, and as data rather than as assertions written
into one language's test framework: a shell in any language must be able to read the same case
and prove it handles it. `crates/goosic-protocol/fixtures/` holds twenty-two of them across
three files — lines that must round-trip byte for byte, lines that must be refused, and lines
that are not canonical but must still be accepted — with `conformance.rs` exposing them to any
Rust test and a fourth test asserting that no case is listed as both required and refused.

The canonical bytes were generated from the Rust types rather than written by hand, which is
the only reason they are trustworthy, and doing it that way immediately surfaced three details
a second implementation would have got wrong: an empty payload is `"payload":{}` and not an
omitted key, while `payload`, `error` and `accountId` are written as explicit nulls where a
hand-written client would leave them out.

`goosic-shell-support` now holds the language-neutral half of the transport. `FrameReader`
finds one NDJSON frame in a stream that arrives in whatever sizes the pipe felt like — split
across two reads, three frames in one read, a byte at a time — and refuses a stream that grows
past the limit without ever terminating, which is the shape a desynchronised pipe actually has.
`accept_response` decides whether a frame is the answer to the question that was asked, and
`timeout_for` is the per-command deadline table lifted out of `ServiceClient.swift`.

The distinction that no test anywhere covered before is now a method with a name:
`TransportError::invalidates_connection`. A remote error is the authority answering correctly —
refusing a stale generation is a *response*, and the channel is fine. Everything else means the
stream is out of step, and a client that keeps reading will pair the following answer with the
wrong question. The Swift client already behaved this way; nothing said so.

Porting it also pinned a boundary that was a comment in one file: the newline counts towards
the frame limit, so the largest accepted frame is one byte shorter than the maximum. That is
exactly the kind of arbitrary edge a second implementation lands on by one, and it now fails a
test instead of a user's session.

## What comes next

The Swift shell becomes the first client to run against the fixtures, which turns it from the
specification into the first conformance reference — which is what the plan needs it to be
before it can be deleted.

The rules named above move to `goosic-shell-support` only after they have tests where they
currently sit. Moving an untested rule and testing it afterwards proves the new copy is
self-consistent, not that it still does what the shipped shell does.
