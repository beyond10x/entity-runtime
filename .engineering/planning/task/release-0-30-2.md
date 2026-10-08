---
format: aep.planning-md/3
id: task:release-0-30-2
kind: task
status: draft
title: Prepare and publish the 0.30.2 release
relations:
- serves: vision:O2
revision: 1
---
# Prepare and publish the 0.30.2 release

## Outcome

Release 0.30.2 carries `story:batch-cost-grows-linearly-with-members`: a recorded batch on a
SQLite store costs time linear in its members. A patch version: no Entity Runtime API changes.

## Scope

Workspace and path-dependency versions, both lockfiles, the dated changelog section, the README
status and install lines.

## Acceptance

`task check` passed on the wave commit `42108e1f` before its pull request (73) was pushed, and
every required check of that pull request was green before it merged. The release pull request
keeps every required check green before it merges. Release publication is observable at
https://github.com/beyond10x/entity-runtime/releases/tag/0.30.2: the annotated tag is reachable
from `main`, its release workflow succeeds, and the bot publishes all five platform archives and
their verified checksums.

## Delivery boundary

A pushed tag alone is queued, not released. Public documentation delivery proceeds asynchronously.
