---
format: aep.planning-md/3
id: design:github-issues-integration-wave
kind: design
status: draft
title: 'GitHub issues 49–51: one integration branch and one PR'
relations:
- designs: story:declared-refusal-before-fulfillment-validation
- designs: story:executor-input-refusal-before-existence
- designs: story:bounded-batch-and-facade-reads
revision: 6
---
## Stage and authority

Skill version 0.19.1 — aep:implementing, wave mode. The operator approved the sequence on 2026-10-03, including the explicit-version API design for #50 and investigation of Eventlog changes for #51. Stage 2 starts now. Approval covers necessary scoped commits, unit integration, publication and one Entity Runtime PR; no release or main merge is requested. The #51 blocker remains until actual integrity-preserving evidence clears it.

## Proposed sequence

1. Wave of one: story:declared-refusal-before-fulfillment-validation (#49), serving vision:O2. Preserve selected executor refusals; add stored-field-guard regressions and actual executor ESS cases. Kernel behavior already has the required order. Scope is cited except proposed new scenario filenames.
2. After accepting the explicit version contract, story:executor-input-refusal-before-existence (#50), serving vision:O2. Evaluate creation refusals before existence. Add a caller-version-bound execution entry point for missing subjects while preserving existing row-derived APIs and retry identity. The design is a recommendation, not accepted authority. Scope is cited with inferred new test path.
3. story:bounded-batch-and-facade-reads (#51), serving vision:O2, remains blocked for complete acceptance by dependency-blocker:verified-provider-change-authority. Explicit scoped facade APIs are a feasible local contribution but do not close the issue. Resolve bounded verification/checkpoint or verified-change authority and provider ESS coverage first. Cross-repository Eventlog work is a separate scope decision; do not silently start it or claim this ER-only PR resolves the complete ask.

All issue work is retained on fix/github-issues-49-51 and ultimately proposed in one ER PR. #49 and #50 overlap Executor::decide and ESS files, so they are not parallel implementation units. Shared changelog, requirements, ESS regeneration and planning writes belong to the coordinator. The computed waves below are candidates, not readiness: they do not remove #51's blocker.

## Managed checkout and recovery

Integration id: github-issues-20261003; branch: fix/github-issues-49-51; base/head: 7462b3812d0f8ca16a867ca70cf81a0955c55faa. Managed path relative to HOME: .local/state/worktree/trees/b10x/entity-runtime/github-issues-20261003. Session owner: er-issues-20261003. Stage: scoped proposal, uncommitted planning records only. Unit trees and branches have not been created. The worktree is intentionally retained for the operator's wave decision; next owner is the coordinator in this session.

Build directory: target/issue-wave-preflight within the integration tree. Scratch root: target/issue-wave-scratch (reserved, not yet created). Future units receive their own managed tree, target and scratch root; no shared build directory. The repository primary checkout remains clean on main. Four earlier linked trees exist, ownership/completion not established; none has been removed or treated as abandoned.

## Pre-flight and validation

Initial free space was 14 GiB; it later rose to 18,088,456,192 bytes while other sessions were active. Build concurrency is one unit, two Cargo jobs; recheck before launch and stop new builds below a 5 GiB floor. sccache is configured machine-wide. Model concurrency limit was asked; no answer has yet arrived, so at most three read-only workers and one building implementor are proposed. The baseline executor build completed in 8.60 seconds with CARGO_PROFILE_DEV_DEBUG=0; cargo test -p entity-executor --locked exited 0, executing 17 integration tests and no unit/doc tests. This is baseline evidence only, not a regression verification or full gate.

No previous-wave cleanup or source edits are authorized by inference from tree age. Audit any old wave identified as unfinished before implementation. Full Eventlog/ESS build size is not yet measured, so do not extrapolate the executor measurement to a concurrent provider build.

## Roles and commit boundary

Read-only scopers used the aep:story-scoper procedure through the host's generic collaboration agents; plugin-specific subagent_type dispatch is unavailable. The same host adaptation will use aep:implementor, aep:adversary and aep:security-reviewer procedures independently. No implementation workers have run. This is issue triage into standalone stories, not decomposition of a new parent artifact; no four-critic decomposition panel was run.

Approval of the first wave authorizes its unit commit, merge into the integration branch, and coordinator planning/evidence commit. The original one-PR request already authorizes publication of the finished integration branch and one bot-authored PR. Later units require their stated contract/readiness conditions; no tag, release or main merge is included. Before opening the finished PR run task check to exit 0 and required site validation if website files change. Record baseline/treatment evidence and independent adversarial review; never mark #51 closed on partial facade changes.

## Computed candidate waves

The following is the exact output of aep plan artifact waves --kind story --status draft --format json. Its unrelated unassessed backlog entries are outside the three GitHub issue candidates and deliberately excluded from this task; all three issue candidates have been scoped. No unassessed issue is proposed for dispatch.

{
  "waves": [
    {
      "wave": 1,
      "artifacts": [
        {
          "id": "story:bounded-batch-and-facade-reads",
          "inferred": false,
          "scope": [
            {
              "confidence": "cited",
              "path": "CHANGELOG.md"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-eventlog"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-eventlog/src/adapter.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-eventlog/src/adapter/scoped.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-eventlog/src/adapter/small_store_cost.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-eventlog/src/facade.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-eventlog/src/sync.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-eventlog/tests/adversary_r2_per_entity_reads.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-eventlog/tests/per_entity_reads.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-eventlog/tests/provider_facades.rs"
            },
            {
              "confidence": "cited",
              "path": "docs/design/eventlog-recorded-adapter-v0.1.md"
            },
            {
              "confidence": "cited",
              "path": "docs/design/eventlog-recorded-sync-bridge-v0.1.md"
            },
            {
              "confidence": "cited",
              "path": "docs/requirements.md"
            }
          ]
        }
      ]
    },
    {
      "wave": 2,
      "artifacts": [
        {
          "id": "story:declared-refusal-before-fulfillment-validation",
          "inferred": true,
          "scope": [
            {
              "confidence": "cited",
              "path": "CHANGELOG.md"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-executor/src/lib.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-executor/tests/service_3_fulfillment_retry.rs"
            },
            {
              "confidence": "cited",
              "path": "docs/ess/executor-traceability.md"
            },
            {
              "confidence": "cited",
              "path": "ess/coverage.json"
            },
            {
              "confidence": "cited",
              "path": "ess/ess-inputs.yaml"
            },
            {
              "confidence": "cited",
              "path": "ess/generated/model.json"
            },
            {
              "confidence": "cited",
              "path": "ess/generated/suite.json"
            },
            {
              "confidence": "inferred",
              "path": "ess/scenarios/executor/refusal-ignores-success-fulfillments.yaml"
            },
            {
              "confidence": "inferred",
              "path": "ess/scenarios/executor/subject-refusal-ignores-success-fulfillments.yaml"
            }
          ]
        }
      ]
    },
    {
      "wave": 3,
      "artifacts": [
        {
          "id": "story:executor-input-refusal-before-existence",
          "inferred": true,
          "scope": [
            {
              "confidence": "cited",
              "path": "CHANGELOG.md"
            },
            {
              "confidence": "cited",
              "path": "checks/ess-conformance/src/executor.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-executor/src/lib.rs"
            },
            {
              "confidence": "inferred",
              "path": "crates/entity-executor/tests/input_refusal_precedence.rs"
            },
            {
              "confidence": "cited",
              "path": "docs/design/recorded-execution-v0.1.md"
            },
            {
              "confidence": "cited",
              "path": "docs/design/service-binding-boundary-v0.1.md"
            },
            {
              "confidence": "cited",
              "path": "docs/ess/executor-traceability.md"
            },
            {
              "confidence": "cited",
              "path": "docs/requirements.md"
            },
            {
              "confidence": "cited",
              "path": "ess/coverage.json"
            },
            {
              "confidence": "cited",
              "path": "ess/ess-inputs.yaml"
            },
            {
              "confidence": "cited",
              "path": "ess/generated/model.json"
            },
            {
              "confidence": "cited",
              "path": "ess/generated/suite.json"
            },
            {
              "confidence": "cited",
              "path": "ess/scenarios/executor/"
            }
          ]
        }
      ]
    }
  ],
  "collisions": [
    {
      "a": "story:bounded-batch-and-facade-reads",
      "b": "story:declared-refusal-before-fulfillment-validation",
      "path": "CHANGELOG.md",
      "confidence": "cited"
    },
    {
      "a": "story:bounded-batch-and-facade-reads",
      "b": "story:executor-input-refusal-before-existence",
      "path": "CHANGELOG.md",
      "confidence": "cited"
    },
    {
      "a": "story:bounded-batch-and-facade-reads",
      "b": "story:executor-input-refusal-before-existence",
      "path": "docs/requirements.md",
      "confidence": "cited"
    },
    {
      "a": "story:declared-refusal-before-fulfillment-validation",
      "b": "story:executor-input-refusal-before-existence",
      "path": "CHANGELOG.md",
      "confidence": "cited"
    },
    {
      "a": "story:declared-refusal-before-fulfillment-validation",
      "b": "story:executor-input-refusal-before-existence",
      "path": "crates/entity-executor/src/lib.rs",
      "confidence": "cited"
    },
    {
      "a": "story:declared-refusal-before-fulfillment-validation",
      "b": "story:executor-input-refusal-before-existence",
      "path": "docs/ess/executor-traceability.md",
      "confidence": "cited"
    },
    {
      "a": "story:declared-refusal-before-fulfillment-validation",
      "b": "story:executor-input-refusal-before-existence",
      "path": "ess/coverage.json",
      "confidence": "cited"
    },
    {
      "a": "story:declared-refusal-before-fulfillment-validation",
      "b": "story:executor-input-refusal-before-existence",
      "path": "ess/ess-inputs.yaml",
      "confidence": "cited"
    },
    {
      "a": "story:declared-refusal-before-fulfillment-validation",
      "b": "story:executor-input-refusal-before-existence",
      "path": "ess/generated/model.json",
      "confidence": "cited"
    },
    {
      "a": "story:declared-refusal-before-fulfillment-validation",
      "b": "story:executor-input-refusal-before-existence",
      "path": "ess/generated/suite.json",
      "confidence": "cited"
    }
  ],
  "unassessed": [
    "story:aep-markdown-materialized-view",
    "story:definition-json-schema",
    "story:definition-migrations",
    "story:explain-verb",
    "story:gate-and-release-hardening",
    "story:guarded-blob-writes-commit-with-their-group",
    "story:named-predicates",
    "story:one-git-url-one-rev-across-the-workspace",
    "story:pedantic-lints",
    "story:provider-feature-combinations-compile-and-are-gated",
    "story:recorded-stores-read-the-same-on-a-tree-that-keeps-text-once",
    "story:schema-fragments",
    "story:seeded-open-under-one-second"
  ],
  "cycles": []
}

## Handoff measurement

After the baseline build, target/issue-wave-preflight occupies 116,848,903 bytes; free disk is 17,951,981,568 bytes. The build process exited 0 and is no longer running. Its reproducible output is retained for the next wave, not archived as evidence. No branch was published and no PR was created at stage 1. All five new planning files are retained uncommitted in the integration worktree; primary source is unchanged. The coordinator releases only its own session lease at handoff and will reacquire it before resuming.

## Unit 49 launched

Managed id issue-49-20261003, branch impl/issue-49-refusal, base 72a9c36fbd58baee10be844a170649692772e51a. Path relative to HOME: .local/state/worktree/trees/b10x/entity-runtime/issue-49-20261003. Build target/, scratch target/issue-49-scratch/, brief target/issue-49-scratch/brief.md. Stage: implementation. Coordinator owns shared ESS artifacts and documents. Opening commit cheap gates fmt-check, req-check, pin-check and notes-check all exited 0; author and committer verified as b10x-bot[bot].

## Unit 49 integration and unit 50 launch

Unit 49 commit c6164443df9f303f4898f1de1abfc1c899e8c921 was fast-forwarded into the integration branch after red/green regression and adversarial checks. Executor package count 17→20→22; all final tests, formatter and Clippy exited 0. The review found no defect. New ESS cases await coordinator regeneration and full conformance gate. Bot author and committer verified.

Unit 50 managed id issue-50-20261003, branch impl/issue-50-input-refusal, base c6164443df9f303f4898f1de1abfc1c899e8c921. Path relative to HOME: .local/state/worktree/trees/b10x/entity-runtime/issue-50-20261003. Build target/, scratch target/issue-50-scratch/, brief target/issue-50-scratch/brief.md. Stage: implementation of the approved additive version API. Coordinator retains shared manifest/generated/coverage/doc ownership.
