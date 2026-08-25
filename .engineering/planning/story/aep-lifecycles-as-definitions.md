---
format: aep.planning-md/1
id: story:aep-lifecycles-as-definitions
kind: story
status: draft
title: 'Phase 1: the eight AEP lifecycles as entity definitions'
summary: examples/aep/*.yaml, one operation per edge, and an equivalence test against artifacts/lifecycles/*.yaml at the pinned commit.
relations:
- derived_from: epic:drive-engineering-protocols
- depends_on: story:aep-mapping-review
revision: 3
---
# Story: Phase 1 — the eight AEP lifecycles as entity definitions

## Outcome

`examples/aep/*.yaml` holds one definition per `artifacts/lifecycles/*.yaml` in
`engineering-protocols` at the pinned commit, with one operation per edge and no rules yet, and a
test proves the two say the same thing.

## Acceptance

For every kind, the set of `(from, operation, to)` edges the definition yields equals the
`transitions` map in the YAML at `79b641c`; `entity validate examples/aep/*.yaml` exits 0 and is
part of `example-check`; the equivalence test reads the upstream YAML from a committed fixture, not
from a sibling checkout.
