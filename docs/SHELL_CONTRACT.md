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
and prove it handles it. `crates/goosic-protocol/fixtures/` holds twenty-three of them across
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
`decode_response` decides whether a frame is a response worth routing at all — it parses and it
speaks this shell's protocol version — and `timeout_for` is the per-command deadline table lifted
out of `ServiceClient.swift`. `ServiceClient` puts them together: a private child process, a
writer thread so a full pipe can never stall a UI thread, a reader that routes each answer to
its request by id, and one deadline per request.

Routing by id is not a detail. The first version of this crate compared each frame against the
request most recently sent and treated a mismatch as a stream that had lost its place. That was
a faithful port of the Swift client as it then stood, and it was already wrong for where the
service was going: the macOS branch moved catalog and lyrics reads off the service's main loop,
so answers now arrive in the order their work finished, and a slow browse no longer holds a pause
behind it. A client that insisted on order would rebuild the queue the service had just taken
apart. The Swift client was rewritten the same way on that branch, and the Rust client mirrors
its concurrency tests one for one so the two are held to the same behaviour.

The distinction that no test anywhere covered before is now a method with a name:
`TransportError::invalidates_connection`. Two failures leave the connection usable. A remote
error is the authority answering correctly — refusing a stale generation is a *response*. A
timeout is one request giving up, and its late answer, arriving for an id nobody is waiting on,
is dropped rather than treated as corruption; tearing the service down for it is how one
unanswered catalog read used to stop playback. Everything else — an undecodable line, a foreign
protocol version, an oversized frame, end of file — means the stream is gone, and every request
still waiting gets that same answer instead of waiting for a service that no longer exists.

Porting it also pinned a boundary that was a comment in one file: the newline counts towards
the frame limit, so the largest accepted frame is one byte shorter than the maximum. That is
exactly the kind of arbitrary edge a second implementation lands on by one, and it now fails a
test instead of a user's session.

## Why single lines were not enough

A response line pins a shape. It cannot pin the thing a shell actually has to get right, which
is what the service answers *given what was asked before*. Owner conflict, stale generation and
non-monotonic sample are not shapes at all: each of them only exists as the second half of a
conversation, and a fixture holding one response line in isolation would let a client that
never tracked the lease pass.

`fixtures/exchanges.json` holds ten scripted conversations. Each starts from a service that has
just been launched, and every request in it must produce exactly the response recorded beside
it. They were generated by driving the real service rather than written by hand, for the same
reason the encoding fixtures were, and doing it that way settled three questions the prose
version of this contract left open.

A successful claim *increments* the generation. The number a shell sends is never the number it
must send next, and a host that keeps reusing what it claimed with is refused on its very first
sample. That is one line of code in the authority and the single most likely thing for a new
shell to get wrong.

When two refusals both apply, the owner check wins. A second host claiming a held lease is told
`ownerConflict` whether its generation is current or stale — which matters because a client
that received `generationMismatch` would refresh the generation and try again forever, while
`ownerConflict` tells it the truth: someone else is playing.

A sample sequence must *strictly* increase, so an equal one is refused as firmly as a lesser
one — the case a client guarding with `<` instead of `<=` gets wrong, and exactly what a retry
after a lost response sends. A refused sample also does not consume the sequence, so the retry
that follows can still succeed.

The advertisement exchange is there as an invariant rather than as a likely bug: reporting a
marker leaves the owner and the generation untouched, so a shell cannot treat one as a stop and
hand the lease away in the middle of an advertisement.

Two things did not become fixtures, and the reasons are worth recording. A **timeout** is not
an exchange: it is the absence of a response, so nothing about it can be written as a line the
service produces. It lives in `goosic-shell-support` as `timeout_for` with its own tests, and it
is a client rule rather than a protocol one. And `CoreError::InvalidOwner` turns out to be
unreachable through the service: a claim naming `none` as its owner is refused as
`invalidRequest` by the dispatcher before the authority is consulted. The authority is right to
keep the check — it has other callers, and a rule that is only enforced one layer up is a rule
waiting to be bypassed — but no conversation can produce that code today, so no fixture claims
one can.

## Which rules may move first

The rule above — that nothing moves before it has tests where it currently sits — turns step two
into an audit before it is a port. Running that audit produced a cleaner answer than expected,
and one that decides the order of the work.

**Being testable and being movable turn out to be the same property.** Every rule that has
already been lifted out of an object and into a namespace has tests, and every rule still living
inside `GoosicAppModel` does not. That is not a coincidence or an oversight by whoever wrote the
tests: a rule reachable only by constructing a 1,978-line `@MainActor` observable object is a
rule nobody can write a cheap test for, so nobody did. The same property that makes it hard to
test is the one that makes it hard to move.

Ready to move, because the Swift suite already holds them to their behaviour: `isAllowedLoginURL`
and `sanitizeMetadata` and the polling decision in `AccountLoginValidation`; the now-playing
projection and the command availability in `SystemMediaPlayback`; `OfficialBridge.rejectionReason`;
the artwork host allow-list and the FNV-1a cache key; and the catalog DTO conversion, which carries
thirty tests of its own.

The exception proves the rule and is worth naming, because it is the template for everything
still trapped. `indexAfter(_:wrapping:)` lives on the model and is nonetheless a pure function of
its arguments plus shuffle and repeat, so it can be called without driving anything — and it has
five tests, covering the deliberate-next wrap, repeat-all, repeat-one, shuffle never picking the
track it is already on, and a single-track queue. It is a rule that happens to sit in the wrong
file, and it can move as it stands.

Not ready, and each for a different reason. `isValidVideoID` is three lines and has no test that
names it; it is exercised only through the hosts, which is not the same thing. Seek and volume
clamping are trivial arithmetic tangled up with status strings and host dispatch in the same
method, so there is nothing to move until the clamp is separated from what it does afterwards.
And `receive(_:)` is the largest: it decides whether a renderer's report is believable and then
acts on it in one body, so its rule half cannot be tested without its effect half running.

`next()`, `previous()` and `advanceAfterEnd()` are not in either list because they are not pure
rules at all. They orchestrate: choose an index, claim a lease, tell a host. Under the plan's own
restructuring rule they stay native, and the part of them that is a rule — the index choice — is
`indexAfter`, which is already covered.

One thing found during the audit that should not be "fixed" by whoever reads the code next. The
model's `receive(_:)` guards look like a weaker duplicate of `rejectionReason`: they check the
owner, generation, video id and finiteness, but not the bridge version, the token, the sequence
or the volume range. They are not a hole. Both platform hosts apply the full `rejectionReason`
before forwarding anything to the model, so the model's guards are a second, deliberately cheaper
layer behind a complete one. Deleting either copy would remove a layer that is doing work.

## What moved, and where the port deliberately differs

Every rule the audit called ready now lives in `goosic-shell-support`, held by the same cases the
Swift suite holds its original to — ported test by test, with each Swift test's name kept
recognisable so the two can be read side by side. That covers the sign-in navigation policy,
including the country-domain shape check a Nicaraguan sign-in forced; the official bridge's event
shape, its rejection rules and the scripts it injects; the system media projection and command
availability; the catalog conversions; the artwork host allow-list and cache key; the lyric
highlight; and route, filter, repeat and theme identity. `indexAfter` moved as the template said it
could.

The rules the audit called not ready moved as well, because extracting them was the point of the
audit rather than a reason to stop at it. The seek and volume clamps, the model's cheap sample
guard, the end-of-track decision and the fresh-page volume reconciliation each became a function
with a name and tests, where before each was a few lines inside a method that also changed state
and talked to a host.

What did not move is presentation, and it stays out on purpose: route titles and glyphs, repeat and
theme labels, the colour scheme a theme maps to, the material a window draws. Those are how a thing
is shown, and each shell localises and draws its own. `MaterialSurfaceKind` stays in the Swift shell
for the same reason — it chooses between AppKit materials, and no other shell has anything to
choose between.

A faithful port is the default, so the places this one is not faithful are recorded, each being a
decision somebody should be able to find later.

The Swift model coalesces preference saves made within a second of each other through a `merge`
that copies six of the eight fields. Toggling shuffle or cycling repeat within a second of a volume
drag is therefore never saved. The Rust version carries every field and has a test named for the
case. The bug is still present in the Swift shell, on `development` and on the macOS branch alike.

`isValidVideoID` used Unicode `isLetter` and `isNumber`, so eleven accented letters passed. A video
id is URL-safe base64, and a guard in front of a URL the host builds should not be more generous
than the thing it guards, so the Rust version accepts ASCII only. The seek and volume clamps used
`min(max(…))`, which lets a NaN through to the player; the Rust versions refuse a non-finite request
instead. `durationSeconds("1::2")` returned 62, because Foundation's `split` drops empty fields;
the Rust version refuses it, which is what the Swift comment says the function does. Shuffle drew
random positions until it missed the current one; the Rust version draws from the other positions
directly, which is the same distribution and cannot spin.

One difference runs the other way, and the port had to work to keep it. Foundation's
control-character set includes Unicode format characters — zero-width spaces, and the
bidirectional overrides that can make a display name read as something it is not — while Rust's
`char::is_control` does not. The port spells that category out rather than approximating it, so a
name carrying a right-to-left override is refused in Rust exactly as it is in Swift.

One change reached the protocol. The Swift DTOs decode a catalog row of a kind they do not
recognise as `unknown` and show it inert. The Rust `CatalogItemKind` refused it, and under the
transport above an undecodable frame closes the connection — so a newer service that started
returning podcasts would have taken playback down in any Rust shell over a single row.
`CatalogItemKind` now has an `Unknown` variant that the service never produces, and the tolerance
fixtures carry the case, so every future shell is held to it.

Three of the ported pieces have newer versions on the macOS branch. They were ported from
`development` because that is what ships and what the Swift tests describe: the sign-in completion
timeout, which that branch raises to three minutes; the completion script, which it rewrites around
the page's own signed-in flag; and the observer's choice of media element. The same branch adds
rules that do not exist on `development` at all — volume synchronisation, navigation policy,
catalog freshness and request ledgering. When it lands, each of those is a change to this crate as
well. Until the Swift shell consumes the crate instead of its own copies, every change to one of
these rules has to be made in both places, because the shared test cases are the only thing keeping
the two in step.

## What comes next

Which protocol version each shell speaks, and what happens when it meets a service that speaks
another, is declared in [COMPATIBILITY.md](COMPATIBILITY.md). It records one thing this document
implies without saying: the Swift shell keeps its own hand-copied copy of the version constant,
and nothing verifies it against `goosic-protocol`.

The Swift shell becomes the first client to run against the fixtures, which turns it from the
specification into the first conformance reference — which is what the plan needs it to be
before it can be deleted.

The rules named above move to `goosic-shell-support` only after they have tests where they
currently sit. Moving an untested rule and testing it afterwards proves the new copy is
self-consistent, not that it still does what the shipped shell does.
