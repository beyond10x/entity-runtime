---
format: aep.planning-md/1
id: task:fold-refuses-unemitted-event-types
kind: task
status: draft
title: The fold refuses an event whose type no operation emits on its transition
summary: rehydrate checked only the transition for an event type nothing emits; such an event now needs an emitter that produced it
relations:
- decomposes: story:replay-from-events
- serves: vision:O2
revision: 2
---
## Outcome

`rehydrate` refuses an operation event whose type no operation emits on its transition, and a
creation event whose type is not the one `create.emit` names; a definition that emits nothing on
creation folds no history at all.

## Scope

- Cited: `crates/entity-core/src/replay.rs` (`emits_on`, the creation-event emitter check),
  `crates/entity-core/tests/replay.rs`, `docs/design/kernel-v0.1.md` § 10.1, `docs/requirements.md` R-97.

## Acceptance

`an_event_whose_type_no_operation_emits_on_its_transition_is_refused`,
`a_creation_event_of_a_type_the_definition_does_not_emit_on_creation_is_refused` and
`a_history_cannot_begin_for_a_definition_that_emits_nothing_on_creation` pass, and each fails when
its guard is disabled. The AEP fixture histories under `crates/entity-yaml/tests` still fold.

## Authorization

Residual the 2026-09-08 fold hardening documented rather than closed; the operator asked on the same
day for every untracked follow-up to be fixed in this change set.

## Implementation evidence

Both guards were disabled together and the two named tests failed at their variant asserts; restored,
25 replay tests pass. `entity-yaml` fixture tests: 14 passed. Recorded in
`docs/reviews/2026-09-08-full-review.md`. No lifecycle status claim is made by this body.
