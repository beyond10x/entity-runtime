---
format: aep.planning-md/3
id: story:executor-input-refusal-before-existence
kind: story
status: active
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
- confidence: inferred
  path: crates/entity-executor/tests/input_refusal_precedence.rs
- confidence: cited
  path: docs/design/recorded-execution-v0.1.md
- confidence: cited
  path: docs/design/service-binding-boundary-v0.1.md
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
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-10-03T14:50:47Z", actor: "human:timo", revision: 5}
- {from: "proposed", to: "active", at: "2026-10-03T14:50:47Z", actor: "human:timo", revision: 6}
---
## Context

GitHub issue #50 reports RevisionConflict hides an input-guarded declared refusal for creation over an existing row and execution against a missing row: https://github.com/beyond10x/entity-runtime/issues/50. entity-core already exposes decide_before_load; entity-executor decides row existence in its execution path.

## Acceptance

Named conformance scenarios executor_input_refusal_existing_create and executor_input_refusal_missing_execute return the declared input refusal before row existence checks, append nothing, and preserve successful execution, revision-conflict, and batch atomicity behavior.

## Delivery

One issue-fix integration branch and one PR with issues #49 and #51. Regression tests precede implementation and ESS evidence accompanies the fix.

## Scope

Derived 2026-10-03 by aep:story-scoper.

- Primary surface: crates/entity-executor/src/lib.rs:325, :432, :460-474; Executor::decide_and_append and decide — cited.
- Tests: crates/entity-executor/tests/input_refusal_precedence.rs — inferred new regression file.
- ESS: ess/scenarios/executor/, checks/ess-conformance/src/executor.rs, ess/ess-inputs.yaml, ess/coverage.json, ess/generated/model.json, ess/generated/suite.json — cited.
- Documents: docs/ess/executor-traceability.md, docs/design/service-binding-boundary-v0.1.md, docs/design/recorded-execution-v0.1.md, docs/requirements.md, CHANGELOG.md — cited.
- Confidence: medium; exact version selection for missing subjects is absent from the existing request — inferred.
- Collision: #49 edits the same executor decision path; common ESS artifacts and changelog are coordinator-owned — cited.
- Safety: append follows all decisions, so an early refusal preserves atomicity; existing request recovery precedes new decisions and must retain authority — cited, source-walk level 3, unproven by execution.

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
