---
format: aep.planning-md/1
id: story:provider-conformance
kind: story
status: implemented
title: Black-box suites a provider runs against itself, and a broken provider they are checked against
summary: One suite, in the crate that owns the traits, run against every implementation and against a deliberately wrong one.
relations:
- decomposes: epic:the-shell
revision: 4
---
# Story: Black-box suites a provider runs against itself

## Outcome

Somebody writing a fourth provider — for their own database, outside this repository — can find out
whether it is correct without reading how the memory store does it. They call one function, and it
tells them which case they fail and where.

## Context

R-85 says every provider answers alike. Nothing enforced that: three implementations of a trait
diverge quietly, and the first place it shows is an adopter's data. The suite has to live in the
crate that owns the traits and travel to each implementation, because a suite that lives next to one
provider becomes a description of that provider's behaviour.

## Acceptance

`entity_store::conformance::run(&mut store)` returns a report of every case; `memory`, `file` and
`sqlite` all pass it; an instance nobody stored is reported **absent** rather than as an error; a
stale write is refused the same way by all three; a refused commit leaves no trace in any of them;
and the suite is run against a provider that is deliberately wrong, which it both catches and
localises — it names the failing case rather than condemning the provider. R-85, R-101, R-102.

## Out of Scope

Performance or concurrency characteristics — the suite says what a provider *answers*, not how fast
or under what contention. A published `entity-store-testkit` crate for out-of-tree providers; the
suite is public API of `entity-store` and that is enough until somebody outside asks.

## Open Questions

None outstanding.
