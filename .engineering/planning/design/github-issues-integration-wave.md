---
format: aep.planning-md/3
id: design:github-issues-integration-wave
kind: design
status: implemented
title: 'GitHub issues 49–51: one integration branch and one PR'
relations:
- designs: story:declared-refusal-before-fulfillment-validation
- designs: story:executor-input-refusal-before-existence
- designs: story:bounded-batch-and-facade-reads
revision: 20
transitions:
- {from: "draft", to: "in_review", at: "2026-10-06T09:23:18Z", actor: "human:timo", revision: 18}
- {from: "in_review", to: "approved", at: "2026-10-06T09:23:18Z", actor: "human:timo", revision: 19}
- {from: "approved", to: "implemented", at: "2026-10-06T09:23:19Z", actor: "human:timo", revision: 20}
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

## Conformance checkpoint

Issue 49's complete conformance check exited 0: {"total":416,"passed":416,"failed":0,"error":0,"unsupported":0,"skipped":0}. The prior 414 scenario contracts were all unchanged; two authored scenarios were added. Commit b01d15f0 contains the exact regenerated suite and machine-readable evidence. The raw adversary record issue-49-adversary-pass-1 contains a local compiler path and remains private/untracked; the public record issue-49-adversary-public explicitly redacts only that path. Do not stage the private record. It is retained for local archival with raw logs.

## Integrated second unit

Issue #50 is merged: unit commit be0ce7e4, integration merge 201f8fc8. Independent review-result:issue-50-adversary found no defect and added three tests (35 to 38); production source was unchanged during review. Integration corrected two scenario identifiers to ESS hyphen syntax after generation rejected underscores. Conformance generation and execution now succeed: 421 total, 421 passed, zero failed/error/unsupported/skipped. Coverage diff: 416 to 421, removed [], changed_existing []. Exact report/2 and suite are retained in integration scratch issue50-conformance/; AEP imported the exact pair. The full repository gate is running. Issue #51 remains blocked pending the explicit integrity-boundary choice and a provider-owned change-proof contract.

## Third unit unblocked by operator choice

The operator explicitly accepted SQLite change tracking on 2026-10-03: full verification on open, warm invalidation for SQLite-mediated writes including external-connection tampering, raw database-file edits bypassing SQLite outside that warm guarantee. The integrity decision blocker is cleared. Continue #51 under the approved all-issues scope; do not create the integration PR prematurely. Scope/design agents are assessing a generic optional Eventlog provider capability and the runtime incremental verifier before dispatch. Upstream source support may require a bot-published dependency commit; retain exactly one Entity Runtime integration PR, without an unrelated release or main merge.

## First integrated gate

CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=2 task check exited 0 for integrated #49 and #50. All ordered steps completed: formatting, workspace Clippy and tests, documentation, examples, requirements, fixture pins, Eventlog all-feature Rust 1.91 tests/Clippy, notes and ESS. PostgreSQL's dedicated server lane explicitly skipped because ENTITY_POSTGRES_URL was unset; it is not claimed as executed. ESS final output: {"total":421,"passed":421,"failed":0,"error":0,"unsupported":0,"skipped":0}. Exact log and exit status remain in target/issue-wave-scratch/task-check.log and task-check.exit. The two executor stories are implemented; #51 continues before publication and requires a new integrated gate after its changes.

## Third unit implementation underway

Provider unit: Eventlog managed id er-51-capture-20261003, branch fix/er-51-capture-checkpoints, base06c1e99c86c7130169c7ed584ec7326f7e9a546e, scratch target/issue-51-scratch, build target. Runtime unit: Entity Runtime id issue-51-20261003, branch impl/issue-51-bounded-reads, basef57bf9753abec7aaa5ff9103bd9b66e29f3398cd, scratch target/issue-51-scratch, build target. Both source implementations were admitted after their transient ESS declarations validated and coordinator AEP stories moved active. Distinct runtime performance author owns only tests/shared_clock_cost.rs; baseline complete and frozen before source changes compiled.

Baseline release probe: 55/601/1203 exact events, warm shared-clock batch medians 92.517484ms/7.038153041s/12.449723852s, ratios76.07x/134.57x; intended bound assertion red, exit101. All real-provider seed/history/revision checks passed. Immutable source/test/binary hashes and samples are in runtime scratch perf/. This is baseline evidence only.

Coordinator's separate provider conformance draft validates 7 authored scenarios and uses a real SQLite/facade adapter; compilation/execution awaits unit integration. Existing 421-scenario five-library scope remains separate. Full task check will include both afterward. A dedicated disposable PostgreSQL TLS fixture has been prepared outside repositories for the next full gates; other sessions' containers are untouched.

The previously skipped PostgreSQL lane was run against the task-owned PostgreSQL17.6 fixture and exited0: 13 provider conformance tests passed; the feature-disabled facade target and doc-test target each selected zero tests. Initial fixture attempt failed to connect because Docker reassigned the ephemeral host port during TLS restart; reinspecting the container port corrected setup without a source change. Both raw logs and exit statuses remain in integration scratch. Future integrated gates will use the live fixture and include the server lane.

## Completed integration

All three issue stories are implemented. Runtime unit1516e748 was fast-forwarded after independent review; provider6983cc25 is bot-published with signed common checks and its545-test PostgreSQL/TLS proof. All dependency pins/locks use that exact revision. Both introduced review defects (receipt-key parity and same-table projection aliasing) retain original failing cases and separate fixed rechecks. Final integrated task check exits0 with all ordered steps completed; PostgreSQL executes, original ESS421 and new provider ESS17 all pass. Final explicit release performance probe passes with1.177x growth between55 and1203events. No existing scenario contract was removed or changed.

Per-step result: fmt-check0, clippy0, test0, doc-check0, example-check0, req-check0, pin-check0, postgres-check0, eventlog-runtime-check0, notes-check0, ess-check0, provider-ess-check0. The aggregate fail-fast task exits0; raw output and exit receipt are in assigned scratch. The package-level performance case is intentionally ignored and separately executed once after final gates, exit0. Example and documentation targets that select zero tests are not counted as backend proof. The public evidence README and source hashes distinguish the final working-tree observation from later evidence-only commits.

Agent counts: unit49 executor17→22, unit50 executor22→38, unit51 runtime library66→74/package186; Eventlog focused176→199 and final complete persistence545. Agent tokens, tool counts and per-agent wall duration are unknown because this harness did not expose them. Final performance medians11.532148/13.049370/13.568977ms at55/601/1203events. Detailed evidence stays in the immutable records and recovery archives.

The approved endpoint remains one PR from fix/github-issues-49-51 to main. Primary checkouts remain unchanged. Publication is followed by managed archive/finish/gc of only this wave's trees, preserving raw evidence privately. No main merge or release is authorized by this completion record.

## PR delivery and CI parity

PR52 is open at https://github.com/beyond10x/entity-runtime/pull/52 under b10x-bot[bot]. The reusable CI workflow carries its own step list; final delivery review added the same already-passing provider checker commands to its Rust1.91 job, with retained exact reports/generated suite/coverage. This closes local/CI gate drift. Source and Cargo hashes remain unchanged; no executable implementation changed. Follow-up common checks and GitHub checks judge the final workflow commit. Raw/private evidence remains outside published source.

## Release authorization on 2026-10-03

The operator subsequently requested that PR #52 be merged and a new version cut. This supersedes the earlier PR-only delivery boundary. Task release-0-26-0 prepares the minor release on the existing integration branch; merge and publication are authorized. The SQLite change-tracking boundary remains exactly as accepted. Completion requires the bot-authored published release, its successful exact-tag workflow, and verified platform artifacts; documentation publication remains asynchronous.
