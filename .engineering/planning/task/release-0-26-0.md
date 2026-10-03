---
format: aep.planning-md/3
id: task:release-0-26-0
kind: task
status: implemented
title: Prepare the 0.26.0 release candidate for issues 49–51
relations:
- delivers: design:github-issues-integration-wave
- serves: vision:O2
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-10-03T18:32:49Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-10-03T18:32:49Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-10-03T18:39:22Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
## Outcome

The operator requested merge and a new version after the issue-fix wave passed. Prepare 0.26.0 in the same PR (#52), then deliver it through the repository bot release process. A minor version reflects the additive explicit-version executor and provider-tracked capture APIs.

## Scope

Update workspace and path-dependency versions, both lockfiles, and the dated changelog section. No new runtime behavior or specification entities are introduced.

## Acceptance

The full `task check` passes against a real PostgreSQL fixture with both ESS suites. PR #52 retains all required green checks before its exact candidate is merged. Release publication is separately observable at https://github.com/beyond10x/entity-runtime/releases/tag/0.26.0: the annotated tag must be reachable from main, its release workflow must succeed, and the bot must publish all five platform archives and their verified checksums.

## Delivery boundary

This task records release preparation; GitHub records the subsequent merge, tagged build, and publication. A pushed tag alone is queued, not released. Public documentation delivery proceeds asynchronously.
