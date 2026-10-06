---
format: aep.planning-md/3
id: task:release-0-27-0
kind: task
status: draft
title: Prepare and publish the 0.27.0 release for issue 54
owner: entity-runtime
refs:
- provider: github
  reference: beyond10x/entity-runtime#54
relations:
- serves: vision:O2
- delivers: design:wave-issues-54-55-unblock-dependents
revision: 1
---
## Outcome

Release 0.27.0 carrying waves 1 and 2 for GitHub #54 (PR #56, PR #57): a `service/1` rule reads the
length of a declared `string`, and a `service/N` string declares its alphabet. A minor version
reflects two additive definition keys and the new public `FieldDefinition.alphabet` field (a
consumer building `FieldDefinition` as a struct literal without `..Default::default()` stops
compiling). The operator's standing rule: ready fixes on `main` with green gates are released
without a separate request.

## Scope

Workspace and path-dependency versions, both lockfiles, the dated changelog section. No new runtime
behaviour.

## Acceptance

The full `task check` passes against a real PostgreSQL (`postgres:17`, as CI) with both ESS suites.
The release PR keeps every required check green before its exact candidate merges. Release
publication is observable at https://github.com/beyond10x/entity-runtime/releases/tag/0.27.0: the
annotated tag is reachable from `main`, its release workflow succeeds, and the bot publishes all
five platform archives and their verified checksums.

## Delivery boundary

A pushed tag alone is queued, not released. Public documentation delivery proceeds asynchronously.
