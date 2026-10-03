---
format: aep.planning-md/3
id: story:declared-refusal-before-fulfillment-validation
kind: story
status: active
title: Preserve declared refusals when success fulfillment keys are supplied
refs:
- provider: github
  reference: beyond10x/entity-runtime#49
relations:
- serves: vision:O2
- informed_by: story:service-binding-boundary
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: crates/entity-executor/src/lib.rs
- confidence: cited
  path: crates/entity-executor/tests/service_3_fulfillment_retry.rs
- confidence: cited
  path: docs/ess/executor-traceability.md
- confidence: cited
  path: ess/coverage.json
- confidence: cited
  path: ess/ess-inputs.yaml
- confidence: cited
  path: ess/generated/model.json
- confidence: cited
  path: ess/generated/suite.json
- confidence: inferred
  path: ess/scenarios/executor/refusal-ignores-success-fulfillments.yaml
- confidence: inferred
  path: ess/scenarios/executor/subject-refusal-ignores-success-fulfillments.yaml
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-10-03T14:50:47Z", actor: "human:timo", revision: 5}
- {from: "proposed", to: "active", at: "2026-10-03T14:50:47Z", actor: "human:timo", revision: 6}
---
## Context

GitHub issue #49 reports that service guard refusals are replaced by FulfillmentKeysMismatch when a caller supplies success-outcome keys. The request is https://github.com/beyond10x/entity-runtime/issues/49; the relevant check is crates/entity-core/src/runtime.rs.

## Acceptance

Named conformance scenarios refusal_ignores_success_fulfillments and subject_refusal_ignores_success_fulfillments return the selected declared outcome and error unchanged when success fulfillment keys are supplied, while successful decisions still enforce their exact fulfillment contract and refusals mutate no caller state.

## Delivery

One issue-fix integration branch and one PR with issues #50 and #51, as requested by the operator. Regression tests precede implementation; preserve deterministic replay and record ESS evidence.

## Scope

Derived 2026-10-03 by aep:story-scoper.

- Primary surface: crates/entity-executor/src/lib.rs:492, Executor::decide — cited.
- Tests: crates/entity-executor/tests/service_3_fulfillment_retry.rs — cited.
- New scenarios: ess/scenarios/executor/refusal-ignores-success-fulfillments.yaml and subject-refusal-ignores-success-fulfillments.yaml — inferred.
- Integration: ess/ess-inputs.yaml, ess/generated/model.json, ess/generated/suite.json, ess/coverage.json — cited.
- Documents: docs/ess/executor-traceability.md, CHANGELOG.md — cited.
- Confidence: high; the executor duplicate check replaces Refused with an empty-outcome fulfillment error — cited.
- Collision: #50 edits the same Executor::decide function and shared ESS files — cited.
- Safety: runtime.rs:1242 already selects refusal before fulfillment validation; preserve this while retaining extra-key errors on successful no-fulfillment outcomes — cited, source-walk level 3, unproven by execution.

## Finding

The kernel already preserves declared refusals. The remaining defect is in the executor, including subject-dependent selectors. Input-only refusals already return early there; the regression must exercise a stored-field guard. Existing conformance Failure::debug carries the exact outcome/error in its detail, so no new error schema is needed.
