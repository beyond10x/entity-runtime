---
format: aep.planning-md/3
id: story:entity-runtime-builds-against-eventlog-0-8-1
kind: story
status: implemented
title: Entity Runtime builds against Eventlog 0.8.1
relations:
- serves: vision:O2
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: Cargo.lock
- confidence: cited
  path: checks/ess-conformance/Cargo.lock
- confidence: cited
  path: checks/ess-conformance/Cargo.toml
- confidence: cited
  path: crates/entity-cli/Cargo.toml
- confidence: cited
  path: crates/entity-eventlog/Cargo.toml
- confidence: cited
  path: crates/entity-postgres/Cargo.toml
- confidence: cited
  path: crates/entity-sqlite/Cargo.toml
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-10-08T03:12:15Z", actor: "human:timo", revision: 4}
- {from: "proposed", to: "active", at: "2026-10-08T03:12:15Z", actor: "human:timo", revision: 5}
- {from: "active", to: "implemented", at: "2026-10-08T04:06:21Z", actor: "human:timo", revision: 7, decided_on: {"recorded":{"test_result":1,"verification":1}}}
---
# Entity Runtime builds against Eventlog 0.8.1

## Outcome

Every manifest that names an Eventlog crate depends on the `0.8.1` tag of
`github.com/beyond10x/eventlog` instead of `0.8.0`. Both lockfiles resolve the tag to
`d5db40da9c0bee874eba3938a259c13c86f53277`.

## Why

Eventlog 0.8.1 fixes eventlog issue 42 (https://github.com/beyond10x/eventlog/issues/42):
`eventlog-file` appends and in-window reads stop growing with store size. A handle keeps its
committed journal in memory, at the committed size after open and up to about twice that. The
release makes no API or format change. Downstream consumers move their Entity Runtime pin after
the release that carries this story.

## Acceptance

- Ten dependency lines in five manifests (`entity-cli`, `entity-eventlog`, `entity-sqlite`,
  `entity-postgres`, `checks/ess-conformance`) name `tag = "0.8.1"`; `Cargo.lock` and `checks/ess-conformance/Cargo.lock`
  change only the Eventlog packages' version and source.
- `task eventlog-runtime-check` and `task provider-ess-check` pass on the wave's integration branch.
- `CHANGELOG.md` names the pin move under `## [Unreleased]`.

## Out of scope

Any Entity Runtime code change; Eventlog itself.
