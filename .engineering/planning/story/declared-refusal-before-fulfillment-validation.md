---
format: aep.planning-md/3
id: story:declared-refusal-before-fulfillment-validation
kind: story
status: implemented
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
  path: crates/entity-executor/tests/refusal_fulfillment_review.rs
- confidence: cited
  path: crates/entity-executor/tests/service_3_fulfillment_retry.rs
- confidence: cited
  path: docs/ess/executor-traceability.md
- confidence: cited
  path: docs/requirements.md
- confidence: cited
  path: ess/coverage.json
- confidence: cited
  path: ess/ess-inputs.yaml
- confidence: cited
  path: ess/generated/model.json
- confidence: cited
  path: ess/generated/suite.json
- confidence: cited
  path: ess/scenarios/executor/refusal-ignores-success-fulfillments.yaml
- confidence: cited
  path: ess/scenarios/executor/subject-refusal-ignores-success-fulfillments.yaml
revision: 13
transitions:
- {from: "draft", to: "proposed", at: "2026-10-03T14:50:47Z", actor: "human:timo", revision: 5}
- {from: "proposed", to: "active", at: "2026-10-03T14:50:47Z", actor: "human:timo", revision: 6}
- {from: "active", to: "implemented", at: "2026-10-03T15:28:40Z", actor: "human:timo", revision: 13, decided_on: {"recorded":{"test_result":2,"verification":1,"ess_conformance_coverage_v1":1}}}
---
## Context

GitHub issue #49 reports that service guard refusals are replaced by FulfillmentKeysMismatch when a caller supplies success-outcome keys. The request is https://github.com/beyond10x/entity-runtime/issues/49; the relevant check is crates/entity-core/src/runtime.rs.

## Acceptance

Named conformance scenarios `refusal-ignores-success-fulfillments` and `subject-refusal-ignores-success-fulfillments` return the selected declared outcome and error unchanged when success fulfillment keys are supplied, while successful decisions still enforce their exact fulfillment contract and refusals mutate no caller state. These are the repository-conventional hyphenated forms of the initially proposed scenario names; Rust regressions retain underscore names.

## Delivery

One issue-fix integration branch and one PR with issues #50 and #51, as requested by the operator. Regression tests precede implementation; preserve deterministic replay and record ESS evidence.

## Scope

Final scope confirmed by implementation commit c6164443, review-result:issue-49-adversary-public and the integrated conformance report.

- crates/entity-executor/src/lib.rs: restrict extra fulfillment-key validation to accepted completed evaluations; keep declared Refused intact — cited.
- crates/entity-executor/tests/service_3_fulfillment_retry.rs: three new input/stored-refusal and accepted-key regression cases — cited.
- crates/entity-executor/tests/refusal_fulfillment_review.rs: two adversarial rollback, malformed-action and retry cases — cited.
- ess/scenarios/executor/refusal-ignores-success-fulfillments.yaml and subject-refusal-ignores-success-fulfillments.yaml: originally inferred new paths, now authored and executed — cited.
- Coordinator ess/ess-inputs.yaml, ess/generated/suite.json, ess/coverage.json, docs/requirements.md (R-152), docs/ess/executor-traceability.md and CHANGELOG.md — cited.
- Correction: no kernel change was needed; the kernel already returned the declared refusal. Generated model.json required no byte change because existing request and observation types express the regression.
- Collision with #50 required serial executor edits and coordinator-owned common artifacts.

Measured evidence: original package red reproduced FulfillmentKeysMismatch; package count 17 to 20 after the fix and 20 to 22 after independent review. Conformance count 414 to 416 passed with no changed prior contracts, followed by 421/421 after #50 integration. Refusals preserve exact details and state/history/receipt absence; accepted outcomes retain exact keys.

## Finding

The kernel already preserves declared refusals. The remaining defect is in the executor, including subject-dependent selectors. Input-only refusals already return early there; the regression must exercise a stored-field guard. Existing conformance Failure::debug carries the exact outcome/error in its detail, so no new error schema is needed.
