---
format: aep.planning-md/3
id: task:release-0-29-0
kind: task
status: implemented
title: Prepare and publish the 0.29.0 release
refs:
- provider: github
  reference: beyond10x/entity-runtime#55
relations:
- serves: vision:O2
- delivers: design:wave-issue-55-bounded-open
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-10-07T08:00:32Z", actor: "human:timo", revision: 2, decided_on: {"recorded":{"test_result":1,"verification":1}}}
- {from: "proposed", to: "active", at: "2026-10-07T08:00:32Z", actor: "human:timo", revision: 3, decided_on: {"recorded":{"test_result":1,"verification":1}}}
- {from: "active", to: "implemented", at: "2026-10-07T08:00:32Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1,"verification":1}}}
---
# Prepare and publish the 0.29.0 release

## Outcome

Release 0.29.0 carries the wave recorded in `design:wave-issue-55-bounded-open`: a
`CapturePolicy::ProviderTracked` open that verifies a persisted checkpoint and the appends after
it (GitHub #55), and the Eventlog dependency on its released `0.8.0` tag. A minor version: the
Eventlog crates move from 0.7 to 0.8, their types appear in `entity-eventlog`'s public API, and
enabling durable open checkpoints on a store is one-way for 0.28.0 and earlier.

## Scope

Workspace and path-dependency versions, both lockfiles, the dated changelog section, the README
status and install lines, the status page date. Guides keep naming the release their commands were
run with.

## Acceptance

The full `task check` passes on the wave branch at the release commit, with both ESS suites. The
wave's one pull request keeps every required check green before it merges. Release publication is
observable at https://github.com/beyond10x/entity-runtime/releases/tag/0.29.0: the annotated tag is
reachable from `main`, its release workflow succeeds, and the bot publishes all five platform
archives and their verified checksums.

## Delivery boundary

A pushed tag alone is queued, not released. Public documentation delivery proceeds asynchronously.
