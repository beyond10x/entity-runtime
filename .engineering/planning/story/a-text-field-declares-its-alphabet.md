---
format: aep.planning-md/3
id: story:a-text-field-declares-its-alphabet
kind: story
status: implemented
title: A text field or argument declares its alphabet
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
- confidence: cited
  path: crates/entity-core/src/validation.rs
- confidence: inferred
  path: crates/entity-surface/src/lib.rs
- confidence: inferred
  path: docs/design/kernel-v0.1.md
revision: 36
transitions:
- {from: "draft", to: "proposed", at: "2026-10-06T11:44:10Z", actor: "human:timo", revision: 33, decided_on: {"recorded":{"review_outcome":3}}}
- {from: "proposed", to: "active", at: "2026-10-06T11:44:10Z", actor: "human:timo", revision: 34, decided_on: {"recorded":{"review_outcome":3}}}
- {from: "active", to: "implemented", at: "2026-10-06T15:49:52Z", actor: "human:timo", revision: 36, decided_on: {"recorded":{"test_result":1,"review_outcome":6,"verification":1}}}
---
# A text field or argument declares its alphabet

## Outcome

A `string` field or argument definition may declare `alphabet: "<characters>"`. A value is valid
only when every Unicode scalar value in it is one of the alphabet's characters; the empty text
satisfies every alphabet. Membership is per scalar value with no normalization and no case folding.
A violation is a value-validation error that accumulates with the others (invariant 6) and names
the path and the first offending character.

## Why

ESS refuses a String newtype's `alphabet:` as `ALPHABET` with `Needs::EntityCore("a condition over
the characters of a text")` (ess `crates/generate/ess-entity-runtime/src/subset.rs:311-316` at
`a81a8729d`). ESS decided the semantics in `docs/design/string-alphabet-and-length.md` § 1.
Entity-core's `FieldDefinition` already carries `min_length` and other string constraints
(`crates/entity-core/src/definition.rs:339-360`); an alphabet is one more constraint of that kind,
not a rule operator.

## Acceptance

- Named ESS scenarios in the core domain, written and validated before the implementation: a value
  inside the alphabet is accepted; one outside it is refused with its path, and the refusal's
  `details` name the first offending character; the empty text is accepted; against an alphabet
  holding only `e` and U+0301, the decomposed `é` (`e` U+0301) is accepted and the composed `é`
  (U+00E9) is refused, naming U+00E9 (ESS `docs/design/string-alphabet-and-length.md` § 1: no
  normalization); an operation argument declaring an alphabet refuses a value outside it the same
  way, with the argument's path, so both the field and the argument case are exercised.
- A Rust test asserts the value-validation error for a two-violation value carries the path and the
  first offending character, not only the path.
- Registration refuses `alphabet:` on a non-`string` field, an empty alphabet and an alphabet that
  repeats a character, each a named `DefinitionError` (the registration refusal type, `crates/entity-core/src/error.rs:13`) with its path (ESS refuses the same three).
- A declared default that violates the alphabet is refused at registration (as
  `defaults-must-satisfy-schema`).
- `#[serde(deny_unknown_fields)]` stays on `FieldDefinition` (invariant 11); a definition without
  `alphabet` serializes to the same bytes as before.
- Requirement rows in `docs/requirements.md` pinned by live tests; a `CHANGELOG.md` line.

## Out of scope

Text length (`story:a-condition-reads-the-length-of-a-text`); regular expressions; nested-newtype
alphabet intersection, which is ESS's to compute before lowering.

## Scope

Derived 2026-10-06 by `aep:story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/entity-core` — the field-definition and value-validation code — cited
- **Files:** `crates/entity-core/src/definition.rs` `FieldDefinition` struct, lines 333-441; the new key goes beside `min_length`/`max_length` at 358-364; the `FieldKind::String` doc at 504 names the string constraints — cited
- **Files:** `crates/entity-core/src/validation.rs`, three functions — cited (git grep `min_length` → 2142, 2198, 2610):
  - `validate_constraint_applicability` 2129-2177: the non-`string` refusal, beside the `min_length/max_length` line at 2142.
  - `validate_field_definition` 2179-2323: the empty-alphabet and repeated-character refusals, beside the `min_length > max_length` check at 2198-2205.
  - `validate_string` 2603-2626: the membership check, which reports the path and the first offending character.
- **Symbols:** `FieldDefinition`, `validate_constraint_applicability`, `validate_field_definition`, `validate_string`, `DefinitionError::ConstraintNotApplicable` / `InvalidField` — cited
- **Default refusal needs no new code:** `validate_field_definition` already sends every declared default through `validate_value` (validation.rs:2311-2322) — cited
- **Arguments are covered by the same path:** operation arguments are `ObjectSchema` of `FieldDefinition`, checked by `validate_value` → `validate_string` (validation.rs:2448-2460) — cited
- **ESS specification:** `ess/scenarios/core/<new>.yaml`, about four new files using the existing `ValidateDefinition` / `Create` / `Execute` commands — cited
- **Via the coordinator:** `ess/ess-inputs.yaml` `scenarios:` list (alphabetical) — cited
- **Via the coordinator:** `ess/coverage.json` scenario list, hash map and its single `review` line (line 3); `ess-check` refuses an inventory change without `--coverage-review` (`checks/ess-conformance/src/main.rs:214`) — cited
- **Via the coordinator:** `ess/generated/suite.json`, regenerated by `task ess-regenerate` — cited
- **No ESS domain change:** `ess/domains/core.yaml`, `ess/components/core.yaml`, `ess/generated/model.json` unchanged; definitions are opaque `DefinitionDocument` strings — inferred
- **No conformance-adapter change:** `checks/ess-conformance/src/core.rs` already exposes `ValidateDefinition`, `Create`, `Execute` and validation `paths` — inferred
- **Also likely:** `crates/entity-core/src/error.rs`, only if the refusals get new `DefinitionError` variants — inferred
- **Also likely:** a new test file `crates/entity-core/tests/<text_alphabet>.rs` — inferred
- **Also likely:** `crates/entity-surface/src/lib.rs` `field_schema` 310-401 and `constraints` 867-891, so OpenAPI/AsyncAPI projections do not drop the alphabet; the acceptance does not ask for it — inferred
- **Not touched:** `crates/entity-graph/src/graph.rs` `collect_field` 202-213; `crates/entity-core/src/registry.rs`; no struct literal of `FieldDefinition` outside `definition.rs` — cited
- **Via the coordinator:** `docs/requirements.md` Schema section (lines 54-67; R-21 at 59, new rows after R-159); `CHANGELOG.md` `## [Unreleased]` — cited
- **Documents:** `docs/design/kernel-v0.1.md` §3.1 (lines 55-62), `docs/ess/core-traceability.md` rows at 50 and 55, `docs/guide/definitions.md:42`, `website/docs/guide/definitions.md:47` — inferred
- **Coordinator-owned, not unit scope:** `CHANGELOG.md`, `docs/requirements.md` (R-number handed out in the brief), `ess/ess-inputs.yaml`, `ess/coverage.json`, `ess/generated/suite.json` (regenerated once after merge), `docs/ess/core-traceability.md`, `website/docs/guide/definitions.md` and `docs/guide/definitions.md` (under the docs overhaul) — cited
- **Confidence:** high for the entity-core surface; medium for the whole story (semantics gating and error-variant choice not stated)
- **Would collide with:** every sibling adding ESS core scenarios (`ess/coverage.json` review line and `ess/generated/suite.json` are certain conflicts; resolve by regenerating after merge), `docs/requirements.md` R-number allocation from R-160, `CHANGELOG.md`, `docs/ess/core-traceability.md`, `docs/design/kernel-v0.1.md`; at file level `definition.rs` and `validation.rs` (siblings cite other ranges), and `error.rs` if variants are added.
- **Safety fact:** decision records embed the full definition; `crates/entity-store/tests/service_2_framing.rs:108-117` pins bytes against `fixtures/service_2_record_3.json`, which carries `"min_length":null`. A new `alphabet: Option<String>` must carry `skip_serializing_if = "Option::is_none"` or every record's bytes change. Proof level 2 (pointed at file:line), unproven.

### Not established

- Whether `alphabet` is admitted under `kernel/1` or only a `service/N` semantics (would add `Semantics` in `definition.rs:105-120` and `entity-store/src/asynchronous/encoding.rs:101-104`).
- Whether "a named `ValidationError`" for registration refusals means reusing `DefinitionError::ConstraintNotApplicable`/`InvalidField` or new variants.
- Whether entity-surface projections must carry the alphabet.
- "First offending character" is only assertable in ESS through the free-text `details` field.

## Scope as landed (wave 2, merged at `da26675c`)

From the implementor's confirmation table; corrections to the drafted Scope are shown, not deleted.

- `definition.rs` `FieldDefinition.alphabet` with `skip_serializing_if` (`:377`): as drafted.
- `validation.rs`: `validate_constraint_applicability` (now `:2191`), `validate_field_definition` (`:2244`), new `validate_alphabet` (`:2404`), `validate_string` (`:2705`); a private `Findings` type caches one membership set per declaration per validation call (added by the cost finding of adversary pass 1).
- **Correction:** `error.rs` was **not** touched; refusals reuse `ConstraintNotApplicable`, `InvalidField` and `SemanticsKeyNotAvailable`.
- `entity-surface/src/lib.rs`: confirmed; `x-alphabet` at `:398`, reference-page entry at `:882`.
- `docs/design/kernel-v0.1.md` § 3.1: new paragraph, lines 70–77. **Correction:** `docs/ess/core-traceability.md` got new rows 86–87 instead of editing rows 50/55.
- Tests: `tests/text_alphabet.rs` (new); attack files `tests/adversary_text_alphabet.rs`, `entity-surface/tests/adversary_alphabet.rs`.
- ESS: five `ess/scenarios/core/service1-text-alphabet-*.yaml`; no domain, component or adapter change.
- Decided: an alphabet is a `service/N` key; `kernel/1` refuses it (`SemanticsKeyNotAvailable`, R-163). An alphabet on a response field registers but is not enforced until `story:service-response-is-checked-against-its-schema`.
