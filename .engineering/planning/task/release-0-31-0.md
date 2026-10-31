---
format: aep.planning-md/3
id: task:release-0-31-0
kind: task
status: active
title: Prepare and publish the 0.31.0 release
relations:
- serves: vision:O2
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-10-31T08:16:06Z", actor: "human:timo", revision: 3}
- {from: "proposed", to: "active", at: "2026-10-31T08:16:06Z", actor: "human:timo", revision: 4}
---
# Prepare and publish the 0.31.0 release

## Outcome

Release 0.31.0 carries `story:schema-fragments` (document-local `$defs` fragments and definition
inheritance in the YAML adapter) and `story:creation-set-if-present-ignores-parent-defaults`
(registration refuses a parent default on a conditional argument path). A minor version: a feature,
and a narrowing of the definitions registration admits.

## Scope

Workspace and path-dependency versions, both lockfiles, the dated changelog section, the README
status and install lines.

## Acceptance

Every required check of the release pull request is green before it merges. Release publication is
observable at https://github.com/beyond10x/entity-runtime/releases/tag/0.31.0: the annotated tag is
reachable from `main`, its release workflow succeeds, and the bot publishes all five platform
archives and their verified checksums.

## Delivery boundary

A pushed tag alone is queued, not released. Public documentation delivery proceeds asynchronously.
