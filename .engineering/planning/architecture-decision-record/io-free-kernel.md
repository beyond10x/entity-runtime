---
format: aep.planning-md/1
id: architecture-decision-record:io-free-kernel
kind: architecture-decision-record
status: proposed
title: The kernel does no IO and the shell does all of it
summary: No clock, ids, files, network or randomness in entity-core; time and identity are arguments; a Decision is the only output.
relations:
- decides: epic:kernel
revision: 2
---
# ADR: The kernel does no IO and the shell does all of it

## Status

Proposed 2026-08-25. Implemented as the 0.1 kernel; acceptance is the operator's.

## Context

Rules about how an object may change are usually enforced in the same code that loads and stores
it, which makes them untestable without a database and unreplayable without the original clock.
The proof of concept this repository grew from set one rule — *command + state → events; events +
state → new state* — and refused to let the core see a clock, an identifier generator, a filesystem,
a network or a random source.

## Decision

`entity-core` performs no IO. Timestamps, identifiers and everything else the world knows enter as
arguments; a `Decision { instance, events }` is the only output; the caller's instance is never
mutated. The property is enforced by a banned-token scan over the kernel's sources and by pinning
its dependency list (`crates/entity-core/tests/purity.rs`), not by convention.

## Alternatives

* **Inject a `Clock`/`IdGenerator` trait into the kernel** — replayable in principle, but every
  call site becomes a place to forget the injection, and `$now` in a template becomes possible.
  Rejected: an operation that needs a time declares an argument.
* **Let the kernel own persistence behind a repository trait** — the ORM shape. Rejected: it makes
  the kernel's tests need a store and its decisions depend on one.

## Consequences

Every shell must gather time and identity before calling the kernel (the `entity` command does).
Event envelopes (`event_id`, `recorded_at`, correlation, causation, actor) are the shell's, and a
reference envelope type is a story. Optimistic concurrency is possible — the kernel numbers
revisions — but is a provider's job.
