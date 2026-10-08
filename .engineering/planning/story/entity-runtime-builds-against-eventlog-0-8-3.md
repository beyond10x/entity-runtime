---
format: aep.planning-md/3
id: story:entity-runtime-builds-against-eventlog-0-8-3
kind: story
status: active
title: Entity Runtime builds against Eventlog 0.8.3
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
  path: crates/entity-eventlog/tests/batch_cost_file.rs
- confidence: cited
  path: crates/entity-postgres/Cargo.toml
- confidence: cited
  path: crates/entity-sqlite/Cargo.toml
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-10-08T15:14:29Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-10-08T15:14:29Z", actor: "human:timo", revision: 3}
---
# Entity Runtime builds against Eventlog 0.8.3

## Outcome

Every manifest that names an Eventlog crate depends on the `0.8.3` tag of
`github.com/beyond10x/eventlog` instead of `0.8.1`. Both lockfiles resolve the tag to
`2a7e92528893a30a80a16760d8bd4cc9b638727c`. The File-provider batch cost test
`crates/entity-eventlog/tests/batch_cost_file.rs::a_file_provider_batch_costs_time_linear_in_its_members`
runs without `#[ignore]` and passes.

## Why

Eventlog 0.8.2 makes File and PostgreSQL handles hash each distinct blob content once instead of
on every `get_blob`, so a guard reading a batch blob once per member pays one hash instead of M.
That was the remaining reason a File-provider batch cost grew quadratically in its members.
Eventlog 0.8.3 adds `PostgresEventStore::pool_churn` and capacity-laboratory fixes. No API used
here changes and nothing persisted changes.

## Acceptance

- Every Eventlog dependency line in `entity-cli`, `entity-eventlog`, `entity-sqlite`,
  `entity-postgres` and `checks/ess-conformance` names `tag = "0.8.3"`; `Cargo.lock` and
  `checks/ess-conformance/Cargo.lock` change only Eventlog packages and what 0.8.3 newly requires.
- `a_file_provider_batch_costs_time_linear_in_its_members` is no longer ignored and passes in
  `cargo test -p entity-eventlog --all-features`.
- `cargo clippy -p <crate> --all-targets -- -D warnings` and `cargo test -p <crate>` pass for each
  touched crate on the pushed commit; the pull request's CI gate passes.
- `CHANGELOG.md` names the pin move under `## [Unreleased]`.

## Out of scope

Any other Entity Runtime code change; Eventlog itself.
