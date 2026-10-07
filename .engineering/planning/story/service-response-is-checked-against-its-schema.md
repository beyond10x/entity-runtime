---
format: aep.planning-md/3
id: story:service-response-is-checked-against-its-schema
kind: story
status: draft
title: A service response is checked against its declared schema
owner: entity-runtime
relations:
- serves: vision:O2
- informed_by: review-result:er-w2-u1-adversary-pass-1
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: inferred
  path: crates/entity-core/src/error.rs
- confidence: cited
  path: crates/entity-core/src/replay.rs
- confidence: cited
  path: crates/entity-core/src/runtime.rs
- confidence: cited
  path: crates/entity-core/tests/adversary_text_alphabet.rs
- confidence: inferred
  path: crates/entity-core/tests/service_semantics.rs
- confidence: inferred
  path: crates/entity-runtime-docs/src/status.rs
- confidence: cited
  path: docs/design/service-semantics-v0.1.md
- confidence: inferred
  path: docs/ess/core-traceability.md
- confidence: cited
  path: docs/requirements.md
- confidence: inferred
  path: ess/coverage.json
- confidence: inferred
  path: ess/ess-inputs.yaml
- confidence: inferred
  path: ess/generated/suite.json
- confidence: inferred
  path: ess/scenarios/core/service-response-is-checked-against-its-schema.yaml
- confidence: inferred
  path: website/data/status.json
- confidence: inferred
  path: website/docs/concepts/guarantees.md
- confidence: inferred
  path: website/docs/concepts/service-semantics.md
- confidence: inferred
  path: website/docs/status.md
revision: 4
---
# A service response is checked against its declared schema

## Outcome

A `service/1` branch's response is validated against the operation's declared `response` schema
before the decision is returned, as `docs/design/service-semantics-v0.1.md:708` already says;
a response field stricter than the value it is filled from refuses the decision by name.

## Why

Wave 2's adversary pass (review-result `er-w2-u1-adversary-pass-1`) confirmed a pre-existing gap:
`materialize_response` (`crates/entity-core/src/runtime.rs:1695`) never checks the response, so a
declared `max_length` was not enforced at the base `c07f1c10`, and the new `alphabet` key is not
either. Cases pinning today's behaviour live in `crates/entity-core/tests/adversary_text_alphabet.rs`.

## Acceptance

- Response fields are checked against the declared schema; the two pinned cases are rewritten to
  assert the refusal.
- Stored decisions whose responses would now be refused still replay: replay re-validates the
  stored definition and re-decides (`replay.rs:131`), so the design states whether the check
  applies to replay or only to new decisions, and a test pins it.
- Requirement row and `CHANGELOG.md` line.

## Scope

Derived 2026-10-07 by `aep:story-scoper` at `cfcba172` (= `origin/main`). Every line is **cited** (read from the story or the tree) or **inferred** (a reading that could be wrong).

- **Primary surface:** `crates/entity-core/src/runtime.rs`, step 14 (the declared response) — cited
- **Files:** `crates/entity-core/src/runtime.rs:1694-1709` `materialize_response` gains the response schema and semantics and calls `validate_object_under` — cited
- **Files:** its two callers, `runtime.rs:971` in `service_create` (`definition.create.response`) and `:1398-1403` in `decide_with_fulfillments` (`operation.response`) — cited
- **Files:** `crates/entity-core/tests/adversary_text_alphabet.rs:88-140`; the two `*_answered_unchecked_until_responses_are_checked` cases are rewritten to assert the refusal — cited
- **Files:** `crates/entity-core/src/replay.rs:113-200` `replay` re-runs `create` and `decide_before_load`, so it picks up the check with no edit; it changes only if the design exempts replay — cited
- **Also likely:** `crates/entity-core/tests/service_semantics.rs` near `:1156` and `:1235`, for the test that pins replay behaviour — inferred
- **Also likely:** `crates/entity-core/src/error.rs` `CoreError` (`:1111`), only if "by name" means a new variant rather than `CoreError::Validation` with a `response.<field>` path — inferred
- **Reads, no edit expected:** `crates/entity-core/src/validation.rs:2486` `validate_object_under`; `checks/ess-conformance/src/core.rs:115` already maps `Validation` — inferred
- **ESS:** a new `ess/scenarios/core/service-response-is-checked-against-its-schema.yaml` (an `Execute` returning `error_kind: validation` with paths), listed in `ess/ess-inputs.yaml` near `:147`, hashed in `ess/coverage.json`, `ess/generated/suite.json` regenerated — inferred
- **Documents:** `docs/design/service-semantics-v0.1.md:705-717` § 5.3 (must say whether replay applies the check) and its test table at `:1804` — cited
- **Documents:** `docs/requirements.md:235` R-162 (drops "Not yet enforced"; a pinned test rename lands in the same commit) and `:213` R-142; `CHANGELOG.md:5` — cited
- **Documents:** `docs/ess/core-traceability.md:73` (R-142 row) — inferred
- **Documents:** `crates/entity-runtime-docs/src/status.rs:372-377` moves from planned to shipped and regenerates `website/docs/status.md:86` and `website/data/status.json:310`; `website/docs/concepts/service-semantics.md:260-266` ("Known limitation") and `website/docs/concepts/guarantees.md:99-104` ("Planned") become false — inferred
- **Confidence:** high for the code; the replay half is open in the design
- **Would collide with:** the entity-core #54 wave on `runtime.rs` (its operation `set_if_present` likely reaches `Branch` at `:1448-1495` and `decide_with_fulfillments`, which holds `:1398`), on `replay.rs`/`error.rs` if touched, and on any signature change to `validate_object_under`; `story:binder-elements-carry-their-declaration` on `runtime.rs` at file level only; every wave on `CHANGELOG.md`, `docs/requirements.md`, `ess/ess-inputs.yaml`, `ess/coverage.json`, `ess/generated/suite.json`. No overlap with `entity-eventlog` or `entity-store/src/projection.rs` — inferred
- **Safety fact:** `kernel/1` is untouched: only `service_create` (`runtime.rs:714`, gated on `has_service_semantics`) and `Branch::outcome` (`:1491`) carry `responds`; `Branch::implicit` sets `responds: None` (`:1469`). Level 2, unproven

### Not established

- Replay: the acceptance says stored decisions "still replay" and leaves to the design whether the check applies to replay. An exempt replay threads a mode flag through `create` (`runtime.rs:660`), `decide_before_load` (`:1037`) and `continue_with` (`:408`), widening the footprint.
- "Refuses by name": `CoreError::Validation` with a path, or a new variant (which adds `error.rs` and `website/docs/reference/refusals.md`, checked by `kinds.rs`).
- Whether `responds_if_present` values (inserted at `:1707`) are in scope.
- Reachability through ESS lowering is unshown (the informing review calls it "a guess I have not shown").
- The scenario may extend `service-responses-must-be-declared-and-complete.yaml` instead of a new file.
