---
format: aep.planning-md/3
id: story:operation-writes-an-optional-field-from-an-optional-argument
kind: story
status: draft
title: An operation writes an Optional field from an Optional argument
owner: entity-runtime
refs:
- provider: github
  reference: beyond10x/entity-runtime#54
relations:
- decomposes: epic:ess-lowering-entity-core-features
- serves: vision:O2
scope:
- confidence: cited
  path: crates/entity-core/src/definition.rs
- confidence: cited
  path: crates/entity-core/src/error.rs
- confidence: cited
  path: crates/entity-core/src/runtime.rs
- confidence: cited
  path: crates/entity-core/src/validation.rs
- confidence: cited
  path: crates/entity-core/tests/service_binding_boundary.rs
- confidence: cited
  path: docs/design/service-binding-boundary-v0.1.md
- confidence: cited
  path: ess/scenarios/core/conditional-presence-registration-requires-one-typed-optional-leaf.yaml
revision: 11
---
# An operation outcome writes an Optional field from an Optional argument

## Outcome

A `service/2`-or-later operation outcome may declare `set_if_present` — the map that creation
outcomes already carry (`OutcomeDefinition.set_if_present`, `PresentArgument` at
`crates/entity-core/src/definition.rs:930-936`) — so that the field takes the argument's value when
the argument is present and is left as it was when the argument is absent.

Today registration refuses the map on an operation outcome with
`DefinitionError::ConditionalSetOnOperation` (`crates/entity-core/src/validation.rs:659-666`), and
the reviewed scenario `ess/scenarios/core/conditional-presence-registration-requires-one-typed-optional-leaf.yaml`
asserts that refusal in its last step. This story lifts that refusal and applies the map at step 8
of `decide_with_fulfillments` (`crates/entity-core/src/runtime.rs:1263-1268`), reusing the creation
helper `insert_present_arguments` (`runtime.rs:908`, `:1716-1746`).

`kernel/1` and `service/1` are out of scope: they refuse the key outright
(`validation.rs:624-648`), and ESS lowers only to `service/1..3` and already switches to `service/2`
when an operation outcome carries conditional presence (ess
`crates/generate/ess-entity-runtime/src/lib.rs:3082-3097` at `a81a8729d`).

## Why

ESS refuses an `updates:` that writes an Optional field from an Optional input as
`OPTIONAL_UPDATE` with `Needs::EntityCore("`PresentArgument` on an operation write; it covers
creation, event and response members only")` (ess
`crates/generate/ess-entity-runtime/src/subset.rs:326-331` at `a81a8729d`). GitHub
beyond10x/entity-runtime#54.

## Acceptance

- The reviewed scenario above changes from asserting `conditional_set_on_operation` to admitting the
  operation map, regenerated with `task ess-regenerate -- --coverage-review`; new named scenarios in
  the core domain, validated before the implementation: argument present → field takes its value;
  argument absent → field unchanged (present stays present with its old value, absent stays
  absent); the record and event templates after `set` see the result.
- The scenario states whether a `required` target is admitted on an operation; if admitted,
  `validate_conditional_target` (`validation.rs:829-861`) gains an operation variant.
- `service/3`: a field named in both `fulfills` and `set_if_present` on one outcome is refused at
  registration (`validate_fulfillment_outcome`, `validation.rs:484-546`, checks only `set` today).
- Whether `ConditionalSetOnOperation` is removed or kept for a remaining case is decided and stated
  in `CHANGELOG.md` (it is a public variant with a `kind` string).
- `replay()` reruns `decide` for service histories (`crates/entity-core/src/replay.rs:157-186`), so
  a test shows a replayed history with an operation `set_if_present` yields the same bytes as
  `execute` (invariant 2).
- Every definition admitted before this change decides the same bytes after it (no admitted
  definition can reach the new branch today, `validation.rs:659-666`): shown by
  `crates/entity-store/tests/service_2_framing.rs` passing unchanged against its committed record
  fixture, and by `ess-check` passing with no reviewed scenario contract changed except the one
  named above.
- R-150 in `docs/requirements.md:222` amended or a new row added, pinned by a live test;
  `docs/design/service-binding-boundary-v0.1.md` rule 6 (`:331`, `:370-374`) and
  `docs/ess/core-traceability.md:130` updated; a `CHANGELOG.md` line.

## Out of scope

`kernel/1` / `service/1` operations; clearing a field when the argument is absent
(`story:set-clears-an-optional-field` owns `{cleared: true}` and the conflict rule between a cleared
field and a `set_if_present` target on one outcome); increment.

## Scope

Derived 2026-10-06 by `aep:story-scoper` at `7926ec45`. Every line is **cited** (read from the story or the
tree) or **inferred** (a reading that could be wrong).

- **Primary surface:** `crates/entity-core`, the `service/2+` operation-outcome path (validation plus step 8 of `decide_with_fulfillments`) — cited
- **Files:** `crates/entity-core/src/validation.rs:650-675` (`validate_conditional_outcome`'s `set_if_present` loop; the `ConditionalSetOnOperation` push at 659-666) — cited
- **Files:** `crates/entity-core/src/runtime.rs:1263-1268` (step 8 `set` loop; creation version `insert_present_arguments` at :908, helper :1716-1746) — cited
- **Files:** `crates/entity-core/src/runtime.rs:1448-1495` (`Branch` has no `set_if_present` member; `Branch::outcome` 1477-1495 must carry it) — cited
- **Files:** `crates/entity-core/src/error.rs:185-193`, `:545`, `:702-709` (`ConditionalSetOnOperation` variant, `kind`, Display "creation-only") — cited
- **Files:** `crates/entity-core/src/definition.rs:756-758` (doc comment "Creation fields copied…") — cited
- **Files:** `crates/entity-core/tests/service_binding_boundary.rs:693-705` (asserts `ConditionalSetOnOperation`; new tests most likely here) — cited
- **Also likely:** `validation.rs:829-861` (`validate_conditional_target`), `validation.rs:484-546` (`validate_fulfillment_outcome`) — inferred
- **Not changed:** `replay.rs:113-201` (`replay()` reruns `decide_before_load`/`decide`; `rehydrate` refuses every service history, 233-240) — cited
- **ESS:** `ess/scenarios/core/conditional-presence-registration-requires-one-typed-optional-leaf.yaml` (reviewed contract; `task ess-regenerate -- --coverage-review`, Taskfile.yml:38-43); `ess/ess-inputs.yaml` (:24-25), `ess/coverage.json` (:32, :455), `ess/generated/suite.json` — cited
- **ESS:** new scenario files under `ess/scenarios/core/`; `ess/domains/core.yaml`, `ess/generated/model.json`, `checks/ess-conformance/src/core.rs` unchanged — inferred
- **Documents:** `docs/requirements.md:222` (R-150), `docs/design/service-binding-boundary-v0.1.md:319,331,370-374`, `docs/ess/core-traceability.md:81,130`, `CHANGELOG.md:5` — cited
- **Coordinator-owned, not unit scope:** `CHANGELOG.md`, `docs/requirements.md` (R-number handed out in the brief), `ess/ess-inputs.yaml`, `ess/coverage.json`, `ess/generated/suite.json` (regenerated once after merge), `docs/ess/core-traceability.md`, `website/docs/guide/definitions.md` and `docs/guide/definitions.md` (under the docs overhaul) — cited
- **Confidence:** high for the `service/2+` path
- **Would collide with:** `runtime.rs` step 8 (1263-1336) and `Branch` (1448-1495) — any unit changing how an operation's `set:` resolves (`story:set-increments-a-numeric-field`, `story:set-clears-an-optional-field`); `error.rs` `DefinitionError` enum, `kind()`, Display; `validation.rs` outcome/operation `set` validation (405-441, 548-705); `ess/ess-inputs.yaml`, `ess/coverage.json`, `ess/generated/suite.json`, the tail of `docs/requirements.md` (next id R-160), `CHANGELOG.md`, `docs/ess/core-traceability.md` — cited
- **Safety fact:** no definition admitted today reaches the new branch (validation.rs:659-666, 624-648); record framing depends on semantics, not keys (`crates/entity-store/src/asynchronous/encoding.rs:101-104`). Proof level 2, unproven.

### Not established

- Whether `ConditionalSetOnOperation` is deleted or kept.
- Whether any consumer replays records written by a newer build with an older entity-core, which would refuse an operation `set_if_present` snapshot.
