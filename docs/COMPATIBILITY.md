# Compatibility manifest

This document declares which protocol each shell speaks and which service it is paired with. It
exists because the migration replaces one shell with three, and three shells shipped on their
own schedules will eventually be asked to talk to a service that is not the one they were built
against. A shell must fail saying so rather than guessing, and a reader must be able to find out
which combinations are supposed to work without reading three codebases.

It describes what is true today. Where today's answer is narrower than the plan's eventual one,
that is recorded as a limit rather than smoothed over — a manifest that describes an intention
is worse than none, because it is believed.

## What ships today

| Shell | Status | Protocol | Service pairing |
| --- | --- | --- | --- |
| `apps/goosic-swift` (macOS) | unreleased, in-tree | `0.3.0` exactly | none bundled; resolved at launch |
| `apps/goosic-swift` (Linux) | unreleased, in-tree | `0.3.0` exactly | none bundled; resolved at launch |
| `apps/goosic-swift` (Windows) | stubs only | `0.3.0` exactly | none bundled; resolved at launch |
| `apps/goosic-linux` (GTK 4) | not started | — | — |
| `apps/goosic-windows` (WinUI 3) | not started | — | — |

Product version is `0.1.0`, from the Cargo workspace. There are no packages, so there are no
per-platform artifact revisions yet and nothing to publish a release manifest about. The
`1.4.0+3` scheme in [the migration plan](NATIVE_SHELL_MIGRATION.md) describes where this table
is going, not what it holds.

## The compatibility rule

**A shell and a service are compatible when their protocol versions are equal, string for
string.** There is no range, no minimum, and no negotiation. Both sides enforce it and they
enforce it in opposite directions, which is what makes the pairing safe rather than merely
declared.

The service rejects any request whose `protocolVersion` is not its own, answering
`unsupportedProtocolVersion` with the version it expected. It does this before dispatching,
so an incompatible client cannot reach the playback authority even once. The shell independently
rejects any *response* whose version is not its own, and treats that as a failure that
invalidates the connection rather than as one bad answer — `TransportError::ProtocolVersionMismatch`
in `goosic-shell-support`, which reports `invalidates_connection`. Neither side trusts the other
to have checked.

The check happens at the first exchange, not lazily. A shell sends `hello` as soon as it
connects, before it loads preferences or accounts, so a mismatched pair fails while the window
is still showing "Connecting" rather than halfway through a session.

## Why equality and not a range

The plan asks for a declared protocol *range* per shell, and this manifest declares a single
version instead. That is deliberate: the code implements equality, and a manifest that declared
`>=0.3.0, <0.4.0` would be describing a tolerance no line of code provides. A shell handed a
`0.4.0` service today refuses it, correctly, and would refuse it just as flatly if this document
promised otherwise.

There is also nothing yet to be tolerant of. A range earns its complexity when two supported
versions exist at once — when a Linux package lags a service change by a release, say, and both
must work. Until then a range would be untested tolerance, which is the kind that turns out not
to work on the day it is first needed.

When that day comes the change is small and it is named here so nobody has to rediscover it: the
comparison in `goosic-service`'s `handle_request`, the one in `goosic-shell-support`'s
`accept_response`, and this table. The [exchange fixtures](../crates/goosic-protocol/fixtures/exchanges.json)
already contain an `unsupported-protocol-version` conversation, so a change from equality to a
range has to update a recorded answer rather than pass silently.

## Where the version numbers actually live

`goosic-protocol` is the source of truth: `PROTOCOL_VERSION` in `crates/goosic-protocol/src/lib.rs`.
The service and `goosic-shell-support` both read that constant, so neither can drift from it.

The Swift shell does not. It carries its own `goosicProtocolVersion` in
`apps/goosic-swift/Sources/GoosicSwift/Core/Protocol.swift`, hand-copied, with nothing verifying
the two agree. They agree because someone kept them in step. This is the same class of gap that
`Protocol.swift`'s hand-written DTOs represent, and it closes the same way — when the Swift
shell runs against the conformance fixtures, whose canonical lines carry the version, a drifted
constant stops being invisible. Until then, treat the Swift copy as a copy.

Each future shell will add a third and fourth copy of that constant. That is acceptable only
because the fixtures make a wrong copy fail a test; a shell that declares a version nobody checks
is a shell that will one day claim compatibility it does not have.

## The pairing gap

Nothing is bundled today. The Swift shell resolves its service from `GOOSIC_SERVICE_PATH`, and
falls back to whatever `goosic-service` is first on `PATH`. In a development checkout that is the
service you just built, which is why it has never caused trouble. It also means the shell has no
idea what it launched, and the manifest cannot honestly say a shell ships with a particular
service revision, because no shell ships at all.

The protocol check is what stands in for that pairing, and it is weaker in one specific way worth
being clear about: it catches an incompatible *protocol*, not an incompatible *build*. Two
services with the same protocol version and different behaviour are indistinguishable to a shell.
That is tolerable while the service is built from the same commit as the shell, and it stops
being tolerable the moment packages exist — which is why the plan puts a bundled private service
inside every package, and why this section will be replaced rather than amended when they do.

## What a released shell will have to declare

When packaging exists, each published artifact adds a row carrying its platform, its artifact
revision, the product version it belongs to, the protocol version it speaks, and the exact
version and checksum of the service binary inside it. A service or protocol change then becomes
a coordinated release: CI runs every supported shell against the new service, this table changes,
and the affected packages ship together unless the protocol genuinely did not move.

Until any of that exists, this document has one job, and it is the honest one: to say that there
is exactly one protocol version, that both sides demand it exactly, and that no shell yet carries
a service of its own.
