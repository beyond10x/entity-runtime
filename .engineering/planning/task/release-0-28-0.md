---
format: aep.planning-md/3
id: task:release-0-28-0
kind: task
status: implemented
title: Prepare and publish the 0.28.0 release
owner: entity-runtime
refs:
- provider: github
  reference: beyond10x/entity-runtime#59
relations:
- serves: vision:O2
- delivers: design:wave-issue-59-model-holds-each-record-once
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-10-07T01:00:44Z", actor: "human:timo", revision: 2, decided_on: {"recorded":{"test_result":1,"verification":1}}}
- {from: "proposed", to: "active", at: "2026-10-07T01:00:44Z", actor: "human:timo", revision: 3, decided_on: {"recorded":{"test_result":1,"verification":1}}}
- {from: "active", to: "implemented", at: "2026-10-07T01:00:44Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1,"verification":1}}}
---
# Prepare and publish the 0.28.0 release

## Outcome

Release 0.28.0 carries the wave recorded in `design:wave-issue-59-model-holds-each-record-once`:
a verified Eventlog model that holds each committed record once (GitHub #59), the Eventlog
dependency on its released `0.7.0` tag, the AEP lifecycle fixture at AEP `5a2a0e5`, the website's
npm audit fix, and the documentation and `entity-cli` library changes already unreleased on `main`.
A minor version: the Eventlog crates move from 0.6 to 0.7, and their types appear in
`entity-eventlog`'s public API.

## Scope

Workspace and path-dependency versions, both lockfiles, the dated changelog section, the README
status line. Guides keep naming the release their commands were run with (0.27.0).

## Acceptance

The full `task check` passes on the wave branch with both ESS suites. The wave's one pull request
keeps every required check green before it merges. Release publication is observable at
https://github.com/beyond10x/entity-runtime/releases/tag/0.28.0: the annotated tag is reachable from
`main`, its release workflow succeeds, and the bot publishes all five platform archives and their
verified checksums.

## Delivery boundary

A pushed tag alone is queued, not released. Public documentation delivery proceeds asynchronously.
