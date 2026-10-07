---
format: aep.planning-md/3
id: story:set-clears-an-optional-field
kind: story
status: active
title: 'A set: assignment clears an Optional field'
owner: entity-runtime
refs:
- provider: github
  reference: beyond10x/entity-runtime#54
relations:
- decomposes: epic:ess-lowering-entity-core-features
- serves: vision:O2
- depends_on: story:set-increments-a-numeric-field
- depends_on: story:operation-writes-an-optional-field-from-an-optional-argument
scope:
- confidence: inferred
  path: crates/entity-core/src/definition.rs
- confidence: inferred
  path: crates/entity-core/src/error.rs
- confidence: inferred
  path: crates/entity-core/src/lib.rs
- confidence: inferred
  path: crates/entity-core/src/replay.rs
- confidence: cited
  path: crates/entity-core/src/runtime.rs
- confidence: cited
  path: crates/entity-core/src/validation.rs
- confidence: inferred
  path: docs/design/kernel-v0.1.md
- confidence: inferred
  path: docs/design/service-semantics-v0.1.md
revision: 12
transitions:
- {from: "draft", to: "proposed", at: "2026-10-07T02:42:40Z", actor: "human:timo", revision: 11}
- {from: "proposed", to: "active", at: "2026-10-07T02:42:40Z", actor: "human:timo", revision: 12}
---
# A `set:` assignment clears an Optional field

## Outcome

An operation (`kernel/1` `set:` and `service/1` outcome `set:`) may assign `{cleared: true}` to a
field that is not `required`. After the operation the field is absent from the instance, the
decision's record says so, and replay folds it to the same bytes.

## Why

ESS refuses `{cleared: true}` outside a creation's `sets:` as `CLEARED` with
`Needs::EntityCore("a removal a definition states; `Remove` is a host-selected action only")`
(ess `crates/generate/ess-entity-runtime/src/subset.rs:305-310` at `a81a8729d`). A creation can
already leave a field absent (ESS `tests/lowering.rs::a_creation_that_clears_an_optional_field_leaves_it_absent_without_a_host_slot`);
an operation cannot remove a value it set before.

## Acceptance

- Named ESS scenarios in the core domain, written and validated before the implementation: an
  operation clears a present Optional field; clearing an already absent field is accepted and
  changes nothing about that field; the event templates after `set` see the field as absent.
- Uses the typed-assignment form `story:set-increments-a-numeric-field` decides (today
  `{cleared: true}` is a legal object-literal template, `crates/entity-core/src/runtime.rs:2775-2781`).
- The scenarios state whether clearing an already absent field adds it to the record's `removed`
  (a fulfillment `Remove` does, `runtime.rs:1322-1323`), whether `{cleared: true}` is admitted in a
  creation's `set`, and whether a field with a declared `default` may be cleared.
- A field that is both cleared and a `set_if_present` target on one outcome is refused at
  registration (this story owns that conflict rule).
- Registration refuses `{cleared: true}` on a `required` field and `{cleared: false}`, each as a
  named `DefinitionError` (the registration refusal type, `crates/entity-core/src/error.rs:13`) with its path.
- Invariants run after the clear (step order unchanged: invariants after `set`), so an invariant
  that needs the field refuses the operation and the caller's instance is unchanged.
- Replay folds the cleared field identically to `execute` (invariant 2), shown by a test.
- Requirement rows in `docs/requirements.md` pinned by live tests; a `CHANGELOG.md` line.

## Out of scope

Increment and Optional-from-Optional writes (sibling stories); a generic delete of an instance
(invariant 4 forbids it).

## Scope

Derived 2026-10-06 by `aep:story-scoper` at `7926ec45`. Every line is **cited** (read from the story or
the tree) or **inferred** (a reading that could be wrong).

- **Primary surface:** `crates/entity-core`: step 8 (`set:` resolution) of `decide_with_fulfillments` and `set:` registration in `validation.rs` — cited
- **Files:** `crates/entity-core/src/runtime.rs:1263-1268` (step 8 loop: `resolve_template` then `new_fields.insert`; a clear becomes a removal here) — cited
- **Files:** `crates/entity-core/src/runtime.rs:1269, 1314-1323, 1386-1395, 1420` (the `removed` set, today filled only by `OperationFieldAction::Remove`; feeds `materialize_event` 2691-2724 and `DecisionRecord.removed`) — cited
- **Files:** `crates/entity-core/src/runtime.rs:1444-1495` (`Branch.set`, `Branch::implicit`, `Branch::outcome`; change only if the `set` value type changes), `:1002-1020`, `:1127-1148` (step docs) — inferred
- **Files:** `crates/entity-core/src/validation.rs:405-426` (`kernel/1` `operations.<op>.set.<field>` loop) and `:548-575` (`service/1` outcome `set` loop in `validate_outcome_expressions`): the `required`-field and `{cleared: false}` refusals — cited
- **Files:** `crates/entity-core/src/validation.rs:267-281`, `runtime.rs:904-907` (creation outcome `set` shares `OutcomeDefinition.set`; `{cleared: …}` on a creation must be refused or defined) — inferred
- **Files:** `crates/entity-core/src/definition.rs:691-697`, `748-750` (`set` docs, and the type if assignments become a typed enum); `lib.rs:134-142` (re-exports, only for a new public type); `error.rs` (new `DefinitionError` variants at the enum tail, `kind()`, `Display`) — inferred
- **Files:** `crates/entity-core/src/replay.rs:311-317` (`rehydrate` refuses any `removed` on a `kernel/1` event), `:700-745` (`operations_that_would_have_produced` compares only `changed`), `:203-218` (doc) — inferred; `replay` (113-201) needs no change — inferred
- **Tests:** a new file under `crates/entity-core/tests/` — inferred
- **ESS:** new files under `ess/scenarios/core/`; `ess/domains/core.yaml`, `ess/components/core.yaml`, `ess/generated/model.json`, `checks/ess-conformance/src/core.rs` unchanged (existing generic `Execute`, `Register`, `Replay`, `ReplayReturned`, `Rehydrate`) — cited for the scenarios, inferred for the rest
- **Documents:** `docs/design/kernel-v0.1.md` §3.3 (102-131), §10.1 (467-582) if `rehydrate` changes (`req-check` refuses an `R-nn` no design mentions, `scripts/check-requirements.py:62-75`); `docs/design/service-semantics-v0.1.md:211` if the type changes; `website/docs/guide/definitions.md:106-120` — inferred
- **Coordinator-owned, not unit scope:** `CHANGELOG.md`, `docs/requirements.md` (R-number handed out in the brief), `ess/ess-inputs.yaml`, `ess/coverage.json`, `ess/generated/suite.json` (regenerated once after merge), `docs/ess/core-traceability.md` — cited (every sibling edits them)
- **Confidence:** medium — the story named no file; two design choices move lines: a typed assignment versus spotting `{cleared: …}` inside a `Value`, and whether "replay" includes legacy `rehydrate`
- **Would collide with:** `story:set-increments-a-numeric-field` (high: same lines, same `set` value-type decision), `story:operation-writes-an-optional-field-from-an-optional-argument` (medium–high: step 8, `replay.rs` candidate loop); the text stories only on `validation.rs` (other functions) and the `error.rs` enum tail
- **Safety fact:** `DomainEvent.removed` and `DecisionRecord.removed` are skipped when empty (`runtime.rs:85-87, 159-161`); `replay` compares the whole record (`replay.rs:192`); the entity-store record check does not depend on semantics (`crates/entity-store/src/lib.rs:322-343`). A definition without a clear produces byte-identical records provided the embedded definition snapshot (`runtime.rs:1407`) serializes existing `set` templates unchanged. Proof level 2, unproven.

### Not established

- `{cleared: true}` is a legal object-literal template today (`resolve_template` passes it through, `runtime.rs:2775-2781`); `git grep cleared` finds no use in this repository; consumers (aep, aep-service, atlas, bench) not checked.
- Whether clearing an already absent field adds it to `removed` (a fulfillment `Remove` does, `runtime.rs:1322-1323`).
- Whether an Optional field with a declared `default` may be cleared (`validation.rs:845-851`).
- Registration refusals are `DefinitionError` variants in the code, not `ValidationError`.
