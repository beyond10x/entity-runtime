---
format: aep.planning-md/3
id: story:set-increments-a-numeric-field
kind: story
status: draft
title: 'A set: assignment increments a numeric field'
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
- confidence: inferred
  path: crates/entity-core/src/error.rs
- confidence: inferred
  path: crates/entity-core/src/lib.rs
- confidence: inferred
  path: crates/entity-core/src/number.rs
- confidence: cited
  path: crates/entity-core/src/replay.rs
- confidence: cited
  path: crates/entity-core/src/runtime.rs
- confidence: cited
  path: crates/entity-core/src/validation.rs
- confidence: inferred
  path: crates/entity-core/tests/replay.rs
- confidence: inferred
  path: crates/entity-core/tests/requirements.rs
- confidence: inferred
  path: crates/entity-core/tests/service_semantics.rs
- confidence: inferred
  path: docs/design/kernel-v0.1.md
revision: 13
---
# A `set:` assignment increments a numeric field

## Outcome

An operation (`kernel/1` `set:` and `service/1` outcome `set:`) may assign a numeric field
`{increment: <n>}`, where `<n>` is a literal number or a template the operation's scope resolves to
a number. The new value is the pre-operation value plus `<n>`, computed inside the decision, so it
is atomic with it: the decision's record carries the resulting value, and replay reproduces it.

## Why

ESS refuses `sets: {field: {increment: n}}` as `INCREMENT` with `Needs::EntityCore("arithmetic over
a stored value")` (ess `crates/generate/ess-entity-runtime/src/subset.rs:299-304` at `a81a8729d`).
Today every `set:` value is a template resolved against the pre-operation fields
(`crates/entity-core/src/definition.rs:694-700`), with no arithmetic.

## Acceptance

- Named ESS scenarios in the core domain, written and validated before the implementation:
  increment by a literal; increment by an argument; the result is revalidated against the field's
  schema after `set` (as `fields_are_revalidated_after_set`), so a `maximum` the sum exceeds is a
  refusal and the caller's instance is unchanged.
- This story decides, once for the epic, how a typed `set:` assignment is told apart from a
  template (today `set` is `BTreeMap<String, Value>` and `{increment: n}` would be read as an object
  template, `crates/entity-core/src/definition.rs:690-697`); `story:set-clears-an-optional-field`
  reuses the decision. The choice and its reason are written in `docs/design/kernel-v0.1.md` § 3.3
  beside the new requirement rows.
- Registration refuses `{increment: …}` on a field that is not `integer` or `number`, on an
  increment template that resolves to a non-number type, on a creation's `set`, and on an Optional
  field that may be absent unless the scenario states what absent plus `n` is. Each refusal is a
  named `DefinitionError` (the registration refusal type, `crates/entity-core/src/error.rs:13`)
  with its path.
- Integer overflow is never a silent wrap: the scenario names the refusal, including under
  `kernel/1`, where an out-of-range sum today fails at step 9 with a wrong-type message
  (`crates/entity-core/src/validation.rs:2488-2497`).
- Replay (`crates/entity-core/src/replay.rs`) folds an incremented field to the same bytes as
  `execute` (invariant 2), shown by a test.
- Requirement rows in `docs/requirements.md` pinned by live tests; a `CHANGELOG.md` line.

## Out of scope

`{cleared: true}` and Optional-from-Optional writes (sibling stories); any other arithmetic
(`decrement` is `increment` by a negative number, if the scenario admits one).

## Scope

Derived 2026-10-06 by `aep:story-scoper` at `7926ec45`. Every line is **cited** (read from the story or
the tree) or **inferred** (a reading that could be wrong).

- **Primary surface:** the `set:` assignment path of `crates/entity-core`: its type, step 8 of execution, its registration checks and the event fold (`git grep -nE '\.set\b' crates/entity-core/src` lists every reader) — cited
- **Files:** `crates/entity-core/src/definition.rs` — `OperationDefinition.set` 690-697, `OutcomeDefinition.set` 748-750; both `BTreeMap<String, Value>`, so today `{increment: n}` is read as an object template — cited
- **Files:** `crates/entity-core/src/runtime.rs` — step 8 of `decide_with_fulfillments` 1263-1268; `Branch` 1446-1500 (`set: &BTreeMap<String, Value>` at 1451, 1466, 1488) — cited; creation loop in `service_create` 904-907 only if the value type changes — inferred
- **Files:** `crates/entity-core/src/validation.rs` — `kernel/1` operation loop 412-427, creation-outcome loop 267-281 (increment at creation refused), `service/1` outcome loop in `validate_outcome_expressions` 565-575 — cited; a new helper beside `field_at` (1898-1925) returning a reference's declared type (`validate_reference_path` 1372-1400 returns `()`) — inferred
- **Files:** `crates/entity-core/src/replay.rs` — `set` loop in `operations_that_would_have_produced` 701-723 (`kernel/1` only; `rehydrate` refuses service definitions at 232-239) — cited
- **Not changed:** `replay` (`replay.rs` 113-217) reruns `decide_before_load`/`select_with`; `changed_fields` (`runtime.rs` 2731-2738) carries the sum with no format change; step 9 (1337-1346) already refuses a sum above `maximum` — cited
- **Also likely:** `crates/entity-core/src/error.rs` (new `DefinitionError` variants at the enum tail ~516, `kind()` ~586, `Display` ~954; a `CoreError` only if overflow is refused at step 8); `crates/entity-core/src/number.rs` (exact sum beside private `Integer::add` 108-140; `serde_json` has `arbitrary_precision`, `Cargo.toml:31`); `crates/entity-core/src/lib.rs:134-142` (only for a new public type) — inferred
- **Tests:** `crates/entity-core/tests/requirements.rs` (beside `fields_are_revalidated_after_set`, :1133), `tests/replay.rs`, `tests/service_semantics.rs` — inferred
- **ESS:** new files under `ess/scenarios/core/` (literal, argument, past `maximum`, overflow, registration refusals); `ess/domains/core.yaml`, `ess/generated/model.json`, `checks/ess-conformance/src/core.rs` unchanged — cited for the scenarios, inferred for the rest
- **Documents:** every new R-id must be mentioned in a `docs/design/*.md` (`scripts/check-requirements.py:62-75`) — cited; `docs/design/kernel-v0.1.md` §3.3 (:122-127) — inferred; R-41 wording (`docs/requirements.md:84`, "every `set` value is a template") — inferred
- **Coordinator-owned, not unit scope:** `CHANGELOG.md`, `docs/requirements.md` (R-number handed out in the brief), `ess/ess-inputs.yaml`, `ess/coverage.json`, `ess/generated/suite.json` (regenerated once after merge), `docs/ess/core-traceability.md`, `website/docs/guide/definitions.md` (under the docs overhaul) — cited
- **Confidence:** high for the code surfaces; medium for `error.rs`, `number.rs` and the test files
- **Would collide with:** `story:set-clears-an-optional-field` and `story:operation-writes-an-optional-field-from-an-optional-argument` on the same `set:` lines (definition.rs 690-697/748-750, runtime.rs 1263-1268/1446-1500, replay.rs 701-723, validation.rs 267-281/412-427/565-575); every unit adding a `DefinitionError` at the `error.rs` tails. Units confined to condition evaluation (runtime.rs 2039-2188, validation.rs 1573-1655) or field definitions (definition.rs 339-448, validation.rs 2179-2331, 2603-2627) share only `error.rs` and the coordinator-owned files — inferred
- **Safety fact:** no existing `set:` value is an object with an `increment` key (git grep here and in local aep `5bd4fe58d`, aep-service `9bc6f64`, atlas `d776ec45`, bench `e3c44fd`), so giving that shape a meaning changes no current definition or record snapshot that `replay` compares (runtime.rs:1748-1753, replay.rs:203-208). Proof level 2, unproven.

### Not established

- How `{increment: …}` is read: detected by shape inside `BTreeMap<String, Value>`, or `set` becomes a typed value. This story makes that decision for the epic; `story:set-clears-an-optional-field` reuses it.
- Where overflow is refused: `kernel/1` integers accept i64 and u64 (validation.rs:2488-2497), `service/1` only i64 (2467-2486); an out-of-range sum fails at step 9 as `Validation` with a wrong-type message in `kernel/1`.
- `binary64` fields (`FieldKind::Binary64`).
- Whether increment needs a `semantics` gate (would add `Semantics`, definition.rs 105-164).
- `$args` references into `json` or additional-fields arguments have no static type at registration.
