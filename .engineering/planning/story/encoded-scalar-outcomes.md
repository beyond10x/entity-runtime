---
format: aep.planning-md/1
id: story:encoded-scalar-outcomes
kind: story
status: draft
title: Validate encoded string values and map keys through the pure kernel
owner: ess-evolution-01a089ee
relations:
- depends_on: story:tagged-union-outcomes
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: crates/entity-core
- confidence: cited
  path: crates/entity-surface
- confidence: cited
  path: docs/design/kernel-encoded-outcomes-v4.md
- confidence: cited
  path: docs/requirements.md
revision: 3
---
## Context

ESS evolution requires typed wire values rather than permissive JSON strings. ESS e14d75fb crates/generate/ess-gen/src/types.rs declares exact UUID, padded base64 and decimal-text grammars; current Connectors credential and mutation identifiers use UUID types. ER profile 3 does not enforce these grammars.

## Acceptance

An explicit outcome profile validates UUID, padded base64 and decimal string values and map keys recursively across arguments, state, defaults, events and business errors without changing their bytes, agrees with independently expected ESS wire vectors, and preserves legacy and profile-1/2/3 serialization and reader boundaries.

## Design

Profile 4 adds closed encoding and key_encoding metadata with strict applicability; absent metadata leaves old bytes unchanged and explicit null metadata refuses. Validate the declared wire spelling without normalization, decoded equality or numeric coercion. Keep the kernel IO-free and dependency set unchanged. JSON Schema projects matching patterns and map propertyNames. Decimal arithmetic, unresolved ESS temporal semantics, Binary64, integer key bounds, nested invariants and the full ESS lowerer remain obligations of the overall evolution, not claims of this story. This adds validation metadata for existing scalar types, not a new domain entity.

## Scope

Cited: crates/entity-core/src/definition.rs, validation.rs, outcome.rs and lib.rs own field metadata, admission and evaluation. crates/entity-surface/src/lib.rs owns schema projection. Focused core/surface tests, docs/requirements.md, CHANGELOG.md and a profile design document record the contract. No concurrent implementation is scheduled; this single prerequisite is not a multi-story decomposition requiring a critic panel.

## Verification

Implemented definition/record profile 4, strict encoding and key_encoding metadata, recursive grammar validation and schema patterns/propertyNames. Unknown or null metadata refuses; old profiles refuse supplied metadata. Defaults, event/error outputs and replay retain exact spelling. R-129 and docs/design/kernel-encoded-outcomes-v4.md record the wire contract and remaining semantics.

The affected core/surface package run passed, including existing legacy, named-outcome, collection and union coverage and the new encoding cases. After extending the focused cases for dormant defaults, old map-key admission and replay tampering, all six encoding tests passed in 0.01s. Clippy all-targets with -D warnings, rustdoc with -D warnings, Rust 1.85 all-target compilation, formatting and the existing requirements checker passed (104 requirements, 363 live test functions, zero findings). No full gate ran. No dependency, MSRV or consumer pin changed.

Independent retained Rust probe: er-encoded-probe compares actual ESS e14d75fb generated UUID/base64/decimal schemas to 39 manually expected vectors, each tested through ER as a string and a map key. The UUID vectors also match the actual Connectors 04a7e221 CredentialGenerationId schema generated from a git archive of committed source, excluding concurrent primary edits. Every admitted record keeps its input bytes and replays. The published ER 03ca1caf reader independently proves legacy entity and outcome-profile-1/2/3 definition/record byte equality and refusal of new profile/encoding vocabulary.

Sensitivity: disabling only map-key encoding validation made that probe accept an unhyphenated UUID key and fail (exit 101). The validator was restored byte-for-byte before the final focused tests and comparison passed. The initial new-test compile failure called replay with an incorrect signature; it was corrected to the existing record-slice API. Both failure logs are retained, not represented as successful runs.

Evidence, exact ESS input/output schemas, source snapshot, pinned compiler receipt and Rust probe are retained under /home/timo/.cache/ess-evolution-20260910, with er-encoded prefixes. The story remains draft under the repository policy requiring an explicit operator request for lifecycle moves. This completed validation prerequisite does not complete ESS evolution, decimal arithmetic, temporal/Binary64 semantics, nested invariants, full ESS lowering or Connectors runtime adoption.
