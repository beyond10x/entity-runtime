---
format: aep.planning-md/3
id: task:release-0-30-0
kind: task
status: draft
title: Prepare and publish the 0.30.0 release
refs:
- provider: github
  reference: beyond10x/entity-runtime#54
relations:
- serves: vision:O2
- delivers: design:wave-filed-defects
- delivers: design:wave-issue-54-entity-core-features
revision: 1
---
# Prepare and publish the 0.30.0 release

## Outcome

Release 0.30.0 carries the waves recorded in `design:wave-issue-54-entity-core-features` (operation
`set_if_present`, `{increment: n}`, `{cleared: true}`, GitHub #54; the website katex override) and
`design:wave-filed-defects` (six filed defects). A minor version: `CoreError` and `DefinitionError`
gain variants, `ConditionalSetOnOperation` is removed, and `entity-core` gains
`recompute_create` and `recompute_before_load`.

## Scope

Workspace and path-dependency versions, both lockfiles, the dated changelog section, the README
status and install lines, the regenerated status page.

## Acceptance

The full `task check` passes on the wave branch at the release commit, with both ESS suites. The
wave's one pull request keeps every required check green before it merges. Release publication is
observable at https://github.com/beyond10x/entity-runtime/releases/tag/0.30.0: the annotated tag is
reachable from `main`, its release workflow succeeds, and the bot publishes all five platform
archives and their verified checksums.

## Delivery boundary

A pushed tag alone is queued, not released. Public documentation delivery proceeds asynchronously.
