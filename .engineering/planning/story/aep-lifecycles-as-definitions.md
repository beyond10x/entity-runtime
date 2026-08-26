---
format: aep.planning-md/1
id: story:aep-lifecycles-as-definitions
kind: story
status: implemented
title: 'Phase 1: the eight AEP lifecycles as entity definitions'
summary: examples/aep/*.yaml, one operation per edge, and an equivalence test against artifacts/lifecycles/*.yaml at the pinned commit.
relations:
- derived_from: epic:drive-engineering-protocols
- depends_on: story:aep-mapping-review
revision: 7
---
# Story: Phase 1 — the eight AEP lifecycles as entity definitions

## Outcome

`examples/aep/*.yaml` holds one definition per `artifacts/lifecycles/*.yaml` in
`engineering-protocols` at the pinned commit, with one operation per edge and no rules yet, and a
test proves the two say the same thing.

## Decided 2026-08-25 — this is built first

The operator inverted the order: this story is built **before** `story:aep-mapping-review` and is
what that story sends. The `depends_on` edge pointing the other way predates the decision and cannot
be removed (the planning CLI adds edges and never deletes them); `story:aep-mapping-review
informed_by story:aep-lifecycles-as-definitions` records the real order.

Consequence: nothing here waits on `engineering-protocols`. The whole story is `examples/` plus a
test in this tree, no manifest of either repository changes, and a refusal costs the examples only.

## Acceptance

For every kind, the set of `(from, operation, to)` edges the definition yields equals the
`transitions` map in the YAML at `79b641c`; `entity validate examples/aep/*.yaml` exits 0 and is
part of `example-check`; the equivalence test reads the upstream YAML from a committed fixture, not
from a sibling checkout — which is also what turns the adoption design's hand-held `79b641c` pin
into something the gate holds.
