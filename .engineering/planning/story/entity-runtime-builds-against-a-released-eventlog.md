---
format: aep.planning-md/3
id: story:entity-runtime-builds-against-a-released-eventlog
kind: story
status: active
title: Entity Runtime builds against a released Eventlog
owner: entity-runtime
relations:
- serves: vision:O2
scope:
- confidence: cited
  path: Cargo.lock
- confidence: cited
  path: checks/ess-conformance
- confidence: cited
  path: crates/entity-cli/Cargo.toml
- confidence: cited
  path: crates/entity-eventlog/Cargo.toml
- confidence: cited
  path: crates/entity-postgres/Cargo.toml
- confidence: cited
  path: crates/entity-sqlite/Cargo.toml
revision: 9
transitions:
- {from: "draft", to: "proposed", at: "2026-10-07T00:17:29Z", actor: "human:timo", revision: 8}
- {from: "proposed", to: "active", at: "2026-10-07T00:17:29Z", actor: "human:timo", revision: 9}
---
# Entity Runtime builds against a released Eventlog

## Outcome

Every manifest that names an Eventlog crate depends on the `0.7.0` tag of
`github.com/beyond10x/eventlog` instead of revision `6983cc25`, a commit that until now was only on
the unmerged Eventlog branch `fix/er-51-capture-checkpoints`. Both lockfiles resolve the tag to
`cf7f61e1439488e21e09a4852ec661bfed949b4c`.

## Why

Releases 0.26.0 and 0.27.0 built from an unmerged branch commit. Eventlog 0.7.0 contains
`6983cc25` unchanged: `cf7f61e1` is 5 commits ahead of it and changes only the workspace version,
the changelog and a documentation workflow line (GitHub compare `6983cc25...cf7f61e1`).

## Acceptance

- Ten dependency lines in five manifests name `tag = "0.7.0"`; `Cargo.lock` and
  `checks/ess-conformance/Cargo.lock` change only the five Eventlog packages' version and source.
- `task eventlog-runtime-check` and `task provider-ess-check` pass on the wave's gate.

## Out of scope

The durable capture continuity issue 55 waits on (a later Eventlog release).
