---
format: aep.planning-md/3
id: story:binder-map-elements-read-their-size
kind: story
status: draft
title: A declared map element under a quantifier binder reads its size
relations:
- serves: vision:O2
- informed_by: story:binder-elements-carry-their-declaration
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: crates/entity-core/src/definition.rs
- confidence: inferred
  path: crates/entity-core/src/runtime.rs
- confidence: cited
  path: crates/entity-core/tests/adversary_text_count.rs
- confidence: cited
  path: crates/entity-core/tests/security_text_count.rs
- confidence: inferred
  path: crates/entity-core/tests/service_semantics.rs
- confidence: cited
  path: crates/entity-core/tests/service_values.rs
- confidence: inferred
  path: crates/entity-store/src/projection.rs
- confidence: inferred
  path: crates/entity-yaml/tests/kernel_bytes.rs
- confidence: cited
  path: docs/design/service-semantics-v0.1.md
- confidence: inferred
  path: docs/requirements.md
- confidence: inferred
  path: ess/coverage.json
- confidence: inferred
  path: ess/ess-inputs.yaml
- confidence: cited
  path: ess/scenarios/core/service1-quantifier-map-count-recorded-replay.yaml
- confidence: inferred
  path: ess/service-semantics/domains/service.yaml
revision: 4
---
# A declared map element under a quantifier binder reads its size

## Outcome

Under `service/1`, `$g.count` on a binder element whose declaration is a `map` answers the map's
size, as it does outside a quantifier, and other keys on such an element answer nothing unless the
map declares them; decisions recorded before the change still replay to the same bytes.

## Why

Split from `story:binder-elements-carry-their-declaration`. Changing the binder reading of a
declared map made two decisions the 0.29.0 kernel recorded stop replaying (member value 1 read as
size 6; a member read through a union payload read as nothing), because a record carries no rule
version. The precedent for a reading that must stay replay-safe is a definition-snapshot key,
`number_observation` (R-146), which is a definition-type change: consumer surface, a coordinated
migration.

## Acceptance

- A replay-safe mechanism is designed and accepted before code: either a definition-snapshot key
  that selects the new reading for new definitions only, or another mechanism the design shows
  replays every record the old reading produced.
- The four cases named after this story assert the size reading, and the two recorded-replay cases
  and `service1-quantifier-map-count-recorded-replay` still pass.
- Consumers that pin `entity-core` are named in the migration if a definition type changes.

## Prior work

The first round of `story:binder-elements-carry-their-declaration` built the size reading and was
withdrawn for the replay break; its patch, `build/scratch/u5-full-declaration.patch`, is kept in
the worktree archive of that unit (`er-def-u5`), with the recorded-replay cases that showed the
break.
