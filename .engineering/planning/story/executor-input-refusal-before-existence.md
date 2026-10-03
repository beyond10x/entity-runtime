---
format: aep.planning-md/3
id: story:executor-input-refusal-before-existence
kind: story
status: implemented
title: Evaluate executor input refusals before row existence
refs:
- provider: github
  reference: beyond10x/entity-runtime#50
relations:
- serves: vision:O2
- informed_by: story:service-binding-boundary
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: checks/ess-conformance/src/executor.rs
- confidence: cited
  path: crates/entity-executor/src/lib.rs
- confidence: cited
  path: crates/entity-executor/tests/input_refusal_precedence.rs
- confidence: cited
  path: crates/entity-executor/tests/version_binding_review.rs
- confidence: cited
  path: docs/design/recorded-execution-v0.1.md
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
  path: ess/scenarios/executor/
revision: 13
transitions:
- {from: "draft", to: "proposed", at: "2026-10-03T14:50:47Z", actor: "human:timo", revision: 5}
- {from: "proposed", to: "active", at: "2026-10-03T14:50:47Z", actor: "human:timo", revision: 6}
- {from: "active", to: "implemented", at: "2026-10-03T15:28:40Z", actor: "human:timo", revision: 13, decided_on: {"recorded":{"test_result":1,"review_outcome":1,"verification":1,"ess_conformance_coverage_v1":1}}}
---
## Context

GitHub issue #50 reports RevisionConflict hides an input-guarded declared refusal for creation over an existing row and execution against a missing row: https://github.com/beyond10x/entity-runtime/issues/50. entity-core already exposes decide_before_load; entity-executor decides row existence in its execution path.

## Acceptance

Named conformance scenarios executor-input-refusal-existing-create and executor-input-refusal-missing-execute return the declared input refusal before row existence checks, append nothing, and preserve successful execution, revision-conflict, and batch atomicity behavior. Explicit versions are required for pre-load execution; legacy APIs preserve row-derived authority. The scenario identifiers use ESS hyphens; the Rust regression function identifiers retain underscores.

## Delivery

One issue-fix integration branch and one PR with issues #49 and #51. Regression tests precede implementation and ESS evidence accompanies the fix.

## Scope

Final scope confirmed by the implementor report and independent review of commit be0ce7e4; coordinator integration changes are named below.

- crates/entity-executor/src/lib.rs: explicit version-bound APIs, prepared action reuse, creation refusal precedence and exact historical version checks — cited.
- crates/entity-executor/tests/input_refusal_precedence.rs: previously inferred new file, confirmed absent before implementation and added with 13 passing regressions, including two original red reproductions — cited.
- crates/entity-executor/tests/version_binding_review.rs: independent review added three passing authority, byte-identity and atomicity probes — cited.
- checks/ess-conformance/src/executor.rs: optional explicit version parsing; malformed versions refuse before polling — cited.
- Five new files in ess/scenarios/executor/issue-50-*.yaml plus coordinator ess/ess-inputs.yaml, ess/generated/model.json, ess/generated/suite.json and ess/coverage.json — cited.
- CHANGELOG.md, docs/design/recorded-execution-v0.1.md, docs/ess/executor-traceability.md and docs/requirements.md — cited coordinator changes.
- Correction: docs/design/service-binding-boundary-v0.1.md was scoped conservatively but required no change; the accepted executor boundary is documented in recorded-execution-v0.1.md.
- Collision with #49 was resolved by serial unit implementation and coordinator-owned common ESS artifacts.

Evidence: review-result:issue-50-adversary; package 22 to 35 implementor cases, then 35 to 38 adversarial cases, all green. Integrated ESS report executes 421/421 with no skipped or unsupported scenarios; all 416 earlier scenario contracts are unchanged. First generation refused two underscore scenario names; correcting identifiers to ESS hyphen syntax did not change assertions.

## Proposed design decision

Creation already has an explicit definition_version: call the kernel's decide_create and preserve Refused before checking existing state. Missing-subject execution requires a caller-selected definition version: ExecuteRequest currently has no version, and Registry::versions admits several. Recommend an additive version-bound execution entry point, preserving the existing API's row-derived version semantics rather than silently selecting latest. Reassess callers, request recovery identity and ESS declarations before implementation. This recommendation is not yet an accepted contract.

## Accepted execution design

## Explicit definition versions before loading

For fresh creates, evaluate the declared creation refusal before loading the subject. Preserve
existing conflict precedence for other creation errors and retain request recovery before fresh
decision. Creation already names its definition version.

For execution against subjects that may be absent, the caller selects a positive definition
version through additive `Executor::execute_versioned` and `Executor::batch_versioned` methods.
`VersionedBatchAction` carries a version with Execute and Merge while retaining Create and Observe.
The existing ExecuteRequest and BatchAction retain their source-compatible shapes and row-derived
version semantics. Neither API implicitly selects the latest or sole registered version.

The versioned path validates request shape, recovers existing record/batch identities, then runs
`decide_before_load` for each fresh versioned action before that action's state/history read.
A declared input refusal wins over existence, revision and merge-history checks; a request that
needs loaded state retains ordinary conflict checks. The prepared definition must match a loaded
instance. Preserve per-member batch decision order and append only after all members succeed.

Retries compare an explicit requested version with the saved record's definition/version before
reusing the existing canonical request comparison. Exact retries still work after clearing the
registry; a different explicit version conflicts. No `er.request/*` framing changes. Refusal
recording retains explicit caller version. Conformance Execute/Batch request documents support
optional definition_version at the adapter boundary, with real calls to the versioned APIs.

Required scenarios cover refusal for existing create and absent execution, no load on input
refusal, accepted missing/stale conflicts, loaded version mismatch, mixed-batch rollback and
committed/imported retry version identity. These are behavior over the existing
entity.executor.RequestDocument contract, not a new persisted entity.
