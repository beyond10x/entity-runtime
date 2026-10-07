---
sidebar_position: 4
title: Typed refusals
description: Every kernel, definition and store refusal kind, what it means, and whether to repair, gather evidence, reload or escalate.
source: "Written by hand against CoreError::kind and DefinitionError::kind (crates/entity-core/src/error.rs) and ShellError::kind (crates/entity-shell/src/lib.rs); entity-runtime-docs fails when a kind in those functions is missing from this page"
---

# Typed refusals

A refusal means the runtime understood the request and declined to produce or store the change.
It is ordinary control flow for an agent integration, not a partial failure: no refused rule
leaves assigned fields behind, no refused invariant emits events, no revision conflict overwrites
the winning state and no failed atomic batch commits a prefix.

Match the `kind` (or the Rust variant), never the message: messages may be reworded. The `entity`
command prints a refusal as JSON on standard output with exit `1`.

## Kernel refusals

`CoreError` in Rust; in JSON, `kind` plus the fields named below.

| `kind` | Meaning | Typical response |
|---|---|---|
| `entity_not_registered` | no validated definition for `(entity, version)` | load the intended definition set |
| `entity_mismatch` | the instance was created under another definition | refuse the caller-supplied instance |
| `subject_mismatch` | a prepared operation was continued with another subject | restart from the subject the preparation named |
| `unknown_state` | the instance claims a state its definition does not declare | repair the store or the migration that produced it |
| `revision_exhausted` | another revision cannot be represented | stop; do not wrap or reset the history |
| `operation_not_found` | the definition declares no such operation | inspect the operations and replan |
| `invalid_transition` | the operation is not legal from the current state (`state`) | reload and choose a legal operation |
| `validation` | fields, arguments or a service response break their schema (`errors`, each with a `path`) | repair every path |
| `precondition_failed` | the observed facts contradict an operation rule (`rule`, `reason`) | choose another operation or escalate |
| `precondition_unobservable` | the rule needs facts nobody observed (`unresolved`) | gather every path in `unresolved` |
| `invariant_violation` | the resulting entity would be invalid | do not bypass it; fix the model or the input |
| `invariant_unobservable` | the result's validity depends on missing facts (`unresolved`) | gather or model the evidence |
| `template` | a template path cannot resolve at run time (`expression`) | repair the definition or input; never substitute null |
| `increment_overflow` | a `set` increment's exact sum is outside what the field's kind holds (`operation`, `field`, `value`, `amount`, `range`) | do not retry with the same amount; the field is not wrapped or rounded |
| `definition` | the definition was refused; `defect` is the first defect's kind, `defects` all of them | fix the definition before exposing it |

The service rules ([service semantics](../concepts/service-semantics.md)) add:

| `kind` | Meaning |
|---|---|
| `refused` | the selected outcome declares a refusal (`outcome`, `error`, `reason`) |
| `no_outcome_selected` | no branch applies to this input |
| `outcome_unobservable` | a branch guard could not be answered (`unresolved`); no later branch is tried |
| `unspecified_move_source` | no branch claims this state for the operation |
| `identity_mismatch` | the identity field no longer derives to the storage address (`field`, `id`, `address`) |
| `creation_identity_unavailable` | a derived creation's fields cannot supply an address |
| `fulfillment_required` | a `service/3` branch needs host-supplied field actions (`fields`) |
| `fulfillment_keys_mismatch` | the supplied field actions differ from the branch's (`missing`, `extra`) |
| `required_field_removal` | a field action asked to remove a required field |

`precondition_failed` and `precondition_unobservable` are deliberately different:

```json
{ "kind": "precondition_failed", "rule": "large_refunds_need_a_human",
  "reason": "refunds above 5000 cents require a human actor" }
```

The facts were present and the policy said no.

```json
{ "kind": "precondition_unobservable", "rule": "reviewed",
  "unresolved": ["$fields.review_score"] }
```

The policy could not answer, because the evidence is missing. An agent should not handle the two
the same way.

## Store refusals

`entity-shell` (and with it the `entity` command, a generated CLI and the MCP tools) reports a store
refusal with these kinds; Rust callers of a provider match `StoreError` variants.

| `kind` | `StoreError` | Meaning | Response |
|---|---|---|---|
| `revision_conflict` | `RevisionConflict` | the stored revision differs from the expectation | reload and decide again |
| `record_conflict` | `RecordConflict` | the record id already names different bytes | investigate; never switch to a new id silently |
| `store_unreachable` | `Unreachable` | the provider could not be reached | retry or follow the declared offline policy; never treat as absent |
| `store_backend` | `Backend` | the provider itself failed | report an operational error |
| `not_found` | — | no stored subject has that id | check the id and entity type |
| `invalid_recording` | — | the recording envelope is incomplete or invalid | supply complete provenance |

The `entity` command prints a store refusal as
`{"refused": true, "by": "store", "kind": …, "detail": …}`; an `eventlog-providers` build adds
`eventlog_provider` for a refusal from the Eventlog File provider. MCP tool results carry the same
`kind` with `by` naming the refusing boundary (`kernel`, `store` or `input`).

The recorded stores of `entity-store`'s asynchronous ports add `AsyncStoreError::Forked` (a
subject two merged branches both decided for; join its heads with `BatchAction::Merge`) and
`BatchExceedsReadBounds` (the write would exceed the handle's read bounds; divide the batch).
[Storage and replay](../concepts/storage.md) explains both.

## Definition defects

A refused definition reports every defect, each with one of these kinds.
[The definition language](./definitions.md#refusals-at-registration) explains the `kernel/1` ones.

- `kernel/1`: `empty_entity_name`, `zero_version`, `empty_lifecycle`, `empty_lifecycle_state`,
  `unknown_initial_state`, `duplicate_lifecycle_state`, `empty_operation_name`, `no_transitions`,
  `empty_from_states`, `unknown_from_state`, `unknown_to_state`, `ambiguous_transition`,
  `unknown_set_field`, `empty_event_type`, `invalid_field`, `constraint_not_applicable`,
  `invalid_rule`, `unknown_relation_target`, `invalid_template`, `duplicate_definition`,
  `semantics_key_not_available`.
- `set` increments and clears, under every semantics: `increment_target_invalid`,
  `increment_amount_invalid`, `increment_on_create`, `clear_target_invalid`, `clear_flag_invalid`,
  `clear_on_create`, `set_assignment_conflict`.
- Service outcomes: `empty_outcome_name`, `duplicate_outcome`, `ambiguous_default_outcome`,
  `duplicate_wrong_state_outcome`, `wrong_state_on_create`, `wrong_state_with_selector`,
  `wrong_state_with_state_guard`, `wrong_state_unreachable`, `guard_state_outside_move`,
  `creates_effect_on_operation`, `missing_creates_effect`, `refusal_mutates_state`,
  `unobservable_outcome`, `unknown_outcome_state`, `response_field_unknown`,
  `outcome_response_incomplete`.
- Service identity and relations: `identity_field_unknown`, `identity_field_not_addressable`,
  `relation_via_unknown`, `relation_via_wrong_shape`, `relation_carrier_optionality`,
  `relation_target_missing`, `relation_second_owner`, `relation_carrier_wrong`,
  `relation_field_claimed_twice`.
- Service values and conditions: `map_key_not_text`, `map_value_missing`, `union_tag_collides`,
  `union_variant_missing`, `quantifier_bind_invalid`, `quantifier_over_not_collection`,
  `quantifier_body_scope`, `condition_too_deep`, `compare_operand_not_addressable`,
  `scale_unnamed`, `scale_empty`.
- Optional values and field actions (`service/2`, `service/3`): `conditional_argument_invalid`,
  `conditional_target_invalid`, `conditional_target_conflict`, `fulfillment_on_create`,
  `fulfillment_field_unknown`, `fulfillment_identity_field`, `fulfillment_set_conflict`,
  `fulfillment_conditional_set_conflict`, `fulfillment_presence_mismatch`.
