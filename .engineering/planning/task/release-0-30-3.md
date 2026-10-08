---
format: aep.planning-md/3
id: task:release-0-30-3
kind: task
status: implemented
title: Prepare and publish the 0.30.3 release
relations:
- serves: vision:O2
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-10-08T15:38:11Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-10-08T15:38:12Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-10-08T15:59:16Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1,"verification":1}}}
---
# Prepare and publish the 0.30.3 release

## Outcome

Release 0.30.3 carries `story:entity-runtime-builds-against-eventlog-0-8-3`: every Eventlog
dependency names the `0.8.3` tag, and a recorded batch on a File store costs time linear in its
members. A patch version: no Entity Runtime API changes.

## Scope

Workspace and path-dependency versions, both lockfiles, the dated changelog section, the README
status and install lines.

## Acceptance

The package gates of the touched crates passed on the wave commit `5169621c` before its pull
request (76) was pushed, and every required check of that pull request was green before it merged.
The release pull request keeps every required check green before it merges. Release publication is
observable at https://github.com/beyond10x/entity-runtime/releases/tag/0.30.3: the annotated tag
is reachable from `main`, its release workflow succeeds, and the bot publishes all five platform
archives and their verified checksums.

## Delivery boundary

A pushed tag alone is queued, not released. Public documentation delivery proceeds asynchronously.
