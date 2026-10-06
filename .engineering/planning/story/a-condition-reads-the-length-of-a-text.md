---
format: aep.planning-md/3
id: story:a-condition-reads-the-length-of-a-text
kind: story
status: implemented
title: A condition reads the length of a text
owner: entity-runtime
refs:
- provider: github
  reference: beyond10x/entity-runtime#54
relations:
- decomposes: epic:ess-lowering-entity-core-features
- serves: vision:O2
scope:
- confidence: cited
  path: crates/entity-core/src/runtime.rs
- confidence: cited
  path: crates/entity-core/src/validation.rs
- confidence: inferred
  path: crates/entity-core/tests/service_values.rs
- confidence: inferred
  path: docs/design/kernel-v0.1.md
- confidence: inferred
  path: docs/design/service-semantics-v0.1.md
revision: 13
transitions:
- {from: "draft", to: "proposed", at: "2026-10-06T09:42:08Z", actor: "human:timo", revision: 10, decided_on: {"recorded":{"review_outcome":1}}}
- {from: "proposed", to: "active", at: "2026-10-06T09:42:08Z", actor: "human:timo", revision: 11, decided_on: {"recorded":{"review_outcome":1}}}
- {from: "active", to: "implemented", at: "2026-10-06T11:36:51Z", actor: "human:timo", revision: 13, decided_on: {"recorded":{"test_result":1,"review_outcome":5,"verification":1}}}
---
# A condition reads the length of a text

## Outcome

A rule condition (precondition, invariant, outcome guard) may compare the length of a text
— a field, an argument or a nested property of type `string` — where length is the count of
Unicode scalar values. It is usable in every predicate position where `count` of an array or a map
is usable today.

## Why

ESS refuses `<text>.count` in a guard as `TEXT_COUNT` with `Needs::EntityCore("the length of a
text; `count` reads arrays and maps only")` (ess
`crates/generate/ess-entity-runtime/src/subset.rs:317-325` at `a81a8729d`). ESS decided the
meaning: characters are Unicode scalar values, no normalization (ESS
`docs/design/string-alphabet-and-length.md`, operator decision 2). In entity-core the path walk
addresses only arrays and maps (ESS's design cites `crates/entity-core/src/runtime.rs:2947-2972` at
`718a702`; re-locate at the current head).

## Acceptance

- Named ESS scenarios in the core domain, written and validated before the implementation: a
  precondition refuses a text one scalar value over the bound and admits one at the bound; a text
  holding a composed and a decomposed `é` counts 1 and 2 respectively; an absent Optional text is
  handled the way `count` of an absent array is handled today (stated in the scenario); an
  invariant and an outcome guard each read a text's length, one scenario per position, so every
  predicate position the Outcome names is exercised.
- The form is the address `<path>.count` that the ESS lowering emits (ESS
  `docs/design/string-alphabet-and-length.md` § 8), a third collection address beside array and map
  `count` under `service/1` (`crates/entity-core/src/runtime.rs:2945-2970`), not a new operator.
- The run-time arm is keyed on a declared `string` field, not on the value: paths registration does
  not type (undeclared `additional_fields` roots, `json`, `additional_properties`, union payloads,
  binder paths; `validation.rs:1386`, `:1479`, `:1485`, `:1501-1503`, `runtime.rs:2803`) keep
  resolving to nothing, so no replayed decision changes (R-97). A test shows one such path.
- The length address on a reference that is not a text, array or map is refused at registration as
  a named `DefinitionError` (the registration refusal type, `crates/entity-core/src/error.rs:13`) with its path (invariant 5: every path is checked).
- Same inputs, same decision bytes (invariant 2); no new dependency in `entity-core`
  (`crates/entity-core/tests/purity.rs` pins the dependency list).
- Requirement rows in `docs/requirements.md` pinned by live tests; a `CHANGELOG.md` line.

## Out of scope

The alphabet constraint (`story:a-text-field-declares-its-alphabet`); grapheme or byte length.

## Scope

Derived 2026-10-06 by `aep:story-scoper` at `7926ec45` (= `origin/main`). Every line is **cited** (read
from the story or the tree) or **inferred** (a reading that could be wrong).

- **Primary surface:** the `service/1` path walk in `crates/entity-core`: `<text>.count` becomes a third collection address beside `<array>.count` and `<map>.count` — cited (story; ESS `docs/design/string-alphabet-and-length.md` § 8, "an entity-core length address")
- **Files:** `crates/entity-core/src/runtime.rs:2945-2970` `collection_address` gains a text arm (Unicode scalar values via `chars().count()`); doc comments `:2855-2860` (`lookup`), `:2880-2883` (`walk`) — cited
- **Files:** `crates/entity-core/src/validation.rs:1417-1476` `walk_field_path`, `service` block: admit `count` on a declared `string`, refuse any segment after it; `:1510-1512` refuses `.count` on every other scalar kind today — cited
- **Symbols:** `collection_address`, `walk`, `lookup`, `walk_field_path`, `FieldKind::String` — cited
- **Unchanged:** `Condition`, `Comparison`, `CONDITION_OPERATORS` (`definition.rs:1047-1415`), `evaluate_condition` (`runtime.rs:2039`), `evaluate_compare` (`:2329`), `condition_needs_subject` (`:1979-2018`) — inferred
- **Unchanged:** absent Optional text; `lookup` returns nothing at `runtime.rs:2872`, the path an absent array takes — cited
- **Tests:** `crates/entity-core/tests/service_values.rs` § 10.6 (`:1372-1470`) — inferred
- **ESS scenarios:** new `ess/scenarios/core/service1-text-count-*.yaml`, like `service1-collection-count.yaml` — inferred
- **ESS inputs:** `ess/ess-inputs.yaml` `service1-*` block `:149-181` — cited
- **ESS derived:** `ess/coverage.json` (single `review` string at `:3`), `ess/generated/suite.json` — inferred
- **ESS unchanged:** `ess/domains/core.yaml`, `ess/generated/model.json` — inferred
- **Documents:** `docs/requirements.md:209-232` (extend R-148 at `:219` or add a row); `CHANGELOG.md:5` — cited
- **Also likely:** `docs/design/service-semantics-v0.1.md:1679-1695` (§ 10.6), `docs/design/kernel-v0.1.md:389-401`, `docs/ess/core-traceability.md:79` — inferred
- **Coordinator-owned, not unit scope:** `CHANGELOG.md`, `docs/requirements.md` (R-number handed out in the brief), `ess/ess-inputs.yaml`, `ess/coverage.json`, `ess/generated/suite.json` (regenerated once after merge), `docs/ess/core-traceability.md`, `website/docs/guide/definitions.md` and `docs/guide/definitions.md` (under the docs overhaul) — cited
- **Confidence:** medium — code sites read, but the address-vs-operator choice and how the run-time arm is keyed are not decided in the story
- **Would collide with:** any unit changing `runtime.rs` `lookup`/`walk`/`collection_address` or `validation.rs` `validate_reference_path`/`walk_field_path` (no sibling does); textually every unit adding core ESS scenarios or requirement rows: `ess/coverage.json` (certain), `ess/ess-inputs.yaml`, `ess/generated/suite.json`, `docs/requirements.md`, `docs/ess/core-traceability.md`, `CHANGELOG.md`
- **Safety fact:** today `<text>.count` resolves to nothing at run time (no string arm at `runtime.rs:2953-2969`; `walk` fails `as_object()` at `:2907`) and registration refuses it on a declared `string` (`validation.rs:1510-1512`). Registration does not type undeclared roots under `additional_fields` (`:1386`), `json` (`:1479`), `additional_properties` (`:1485`), union payloads (`:1501-1503`) or binder paths (`runtime.rs:2803`, `field: None`). An arm keyed on the value turns those from `Unknown` into a number, which can change a replayed decision (R-97). Proof level 2, unproven.

### Not established

- Address (`keys.count`, as the ESS lowering emits) or operator; an operator adds `definition.rs:1103-1415`, `runtime.rs:1979-2018`, `:2039+`.
- Error type: registration refuses bad references as `DefinitionError::InvalidRule`/`InvalidTemplate` (`validation.rs:2016-2031`), not `ValidationError`.
- Whether the run-time arm is keyed on the value or on a declared `string` (see safety fact).
- Whether `enum` and `ref` fields get `.count`; assumed `string` only.
- `kernel/1` excluded (inferred: `count` is `service/1` only, `runtime.rs:2894`, `validation.rs:1432`).

## Scope as landed (wave 1, merged at `9e071211`)

From the implementor's confirmation table; corrections to the drafted Scope are shown, not deleted.

- `runtime.rs` path walk (`lookup`, `walk`, `collection_address`): confirmed; a `checked` flag that turns false past a union's content key was needed — keying on a declared `string` alone was **not enough** (union payloads are typed by tag at run time).
- `validation.rs` `walk_field_path` and the registration path check: confirmed; also the projection-key and binder checks (`:1462-1532`).
- `Condition`, `evaluate_condition`, `evaluate_compare`, `condition_needs_subject` unchanged: confirmed. **Correction:** `CONDITION_OPERATORS` is at `definition.rs:960`, not in 1047-1415.
- Tests: `service_values.rs` § 10.6 runs `:1371-1489` (drafted `:1372-1470`); the new section is at `:1491`. Two attack files added: `tests/adversary_text_count.rs`, `tests/security_text_count.rs`.
- ESS: nine `ess/scenarios/core/service1-text-count-*.yaml`; `coverage.json` changed only its `review` line and new entries; `suite.json` regenerated.
- Documents: R-160, R-161 (`docs/requirements.md`); rows added at `docs/design/kernel-v0.1.md:402` and `docs/ess/core-traceability.md:84` (drafted: edit `:79`); `docs/design/service-semantics-v0.1.md` § 10.6 table and § 11 rows; `CHANGELOG.md`.
- `error.rs`: **not touched** (refusals reuse `InvalidRule` and `QuantifierBodyScope`).
