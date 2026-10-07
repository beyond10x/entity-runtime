---
format: aep.planning-md/3
id: story:binder-map-elements-read-their-size
kind: story
status: draft
title: A declared map element under a quantifier binder reads its size
relations:
- serves: vision:O2
- informed_by: story:binder-elements-carry-their-declaration
revision: 1
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
