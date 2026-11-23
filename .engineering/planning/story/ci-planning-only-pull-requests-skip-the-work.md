---
format: aep.planning-md/3
id: story:ci-planning-only-pull-requests-skip-the-work
kind: story
status: implemented
title: A planning-only pull request runs no build or test job and its required checks still pass
summary: 'Rule 3 of 2026-11-21; PR #96 (only .engineering/) ran 6 workflows per round'
owner: entity-runtime
relations:
- serves: vision:O2
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-23T14:35:02Z", actor: "human:timo", revision: 2, decided_on: {"recorded":{"test_result":1}}}
- {from: "proposed", to: "active", at: "2026-11-23T14:35:03Z", actor: "human:timo", revision: 3, decided_on: {"recorded":{"test_result":1}}}
- {from: "active", to: "implemented", at: "2026-11-23T14:35:03Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
## Outcome

A pull request whose changed files are all planning or prose files (`.engineering/`, `*.md` outside the built docs site) runs no build, test or KPI work, and every required check (`gate / Gate`, `gate / MSRV 1.85`, `Build Docusaurus`, `common / Security and privacy`, `planning validate`) still reports success, so the pull request can merge. A change to the built docs site (`website/`) still runs the docs build and skips the code's build and test jobs. This is rule "Planning-only pull requests skip the tests" of `~/beyond10x/AGENTS.md` (Timo, 2026-11-21, proposal 3), raised by host-operator on 2026-11-23.

## Evidence

- Pull request #96 (only `.engineering/planning/story/*.md`) ran 6 workflows twice on 2026-11-23 (a second round after updating the branch): CI 3,442 s, KPI 3,230 s, Documentation validation 2,380 s, Shared source gates 1,775 s, Documentation source check 1,348 s, Planning store 994 s (run_started_at to updated_at summed over both rounds, from the Actions API). The CI run includes `gate / Gate`, `gate / MSRV 1.85` and `gate / Eventlog runtime (Rust 1.91)`.
- The beyond10x runner queue was full that morning; runbook's 0.8.0 release waited behind it.

## Design constraint

`paths-ignore` on a workflow that provides a required check leaves that check pending forever. Each workflow that provides a required check (`gate.yml`, `check.yml`, `shared-gates.yml`, the docs workflows) and the KPI workflow starts with a job that classifies the changed paths against one path list kept in the repository; the work jobs depend on it and are skipped, or their steps are, while the job that carries the required check's name still ends with success.

## Acceptance

A pull request that changes only `.engineering/` files merges with every required check green and no job that compiles or tests running longer than the classify job; a pull request that changes one `crates/` file runs the full set as today. The path list has a test case for both sides (a planning-only path set and a set with one code path), run by the planning or gate workflow.
