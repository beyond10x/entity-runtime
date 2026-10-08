---
format: aep.planning-md/3
id: task:release-0-30-1
kind: task
status: implemented
title: Prepare and publish the 0.30.1 release
relations:
- serves: vision:O2
revision: 5
transitions:
- {from: "draft", to: "proposed", at: "2026-10-08T04:06:21Z", actor: "human:timo", revision: 3, decided_on: {"recorded":{"test_result":1,"verification":1}}}
- {from: "proposed", to: "active", at: "2026-10-08T04:06:21Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1,"verification":1}}}
- {from: "active", to: "implemented", at: "2026-10-08T04:06:21Z", actor: "human:timo", revision: 5, decided_on: {"recorded":{"test_result":1,"verification":1}}}
---
# Prepare and publish the 0.30.1 release

## Outcome

Release 0.30.1 carries `story:entity-runtime-builds-against-eventlog-0-8-1`: every Eventlog
dependency at tag `0.8.1`. A patch version: no Entity Runtime API changes.

## Scope

Workspace and path-dependency versions, both lockfiles, the dated changelog section, the README
status and install lines.

## Acceptance

The package gates for the changed dependency (`task eventlog-runtime-check`,
`task provider-ess-check`) pass on the wave branch. The wave's one pull request keeps every
required check green before it merges. Release publication is observable at
https://github.com/beyond10x/entity-runtime/releases/tag/0.30.1: the annotated tag is reachable
from `main`, its release workflow succeeds, and the bot publishes all five platform archives and
their verified checksums.

## Delivery boundary

A pushed tag alone is queued, not released. Public documentation delivery proceeds asynchronously.
