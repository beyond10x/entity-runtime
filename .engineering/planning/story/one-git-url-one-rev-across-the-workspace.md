---
format: aep.planning-md/2
id: story:one-git-url-one-rev-across-the-workspace
kind: story
status: draft
title: One git dependency URL names one rev across the workspace, checked by the gate
summary: task check refuses a split rev for one git URL; Rust, not Python
owner: entity-runtime
relations:
- informed_by: task:eventlog-provider-pin-verify-once
- informed_by: review-result:er-eventlog-pin-review-1
revision: 2
---
## Outcome

`task check` refuses a workspace in which one git dependency URL is named at two different `rev`
values, in any manifest or in `Cargo.lock`. The check is a Rust executable, not a Python script:
`AGENTS.md:312-314` records that the existing Python checkers are legacy under atlas's touch rule
and that anything that runs is Rust unless the operator accepts the org exception.

## Why

2026-09-21, task:eventlog-provider-pin-verify-once. The Eventlog pin was moved in
`crates/entity-eventlog/Cargo.toml` and `task check` exited 0 — ten steps, 634 passed, 0 failed —
while three other manifests still named the old commit:

| manifest | line | dependency |
| --- | --- | --- |
| crates/entity-sqlite/Cargo.toml | 19 | eventlog-core |
| crates/entity-postgres/Cargo.toml | 30, 31 | eventlog-core, eventlog-postgres |
| crates/entity-cli/Cargo.toml | 31 | eventlog-core |

Cargo resolved two `eventlog-core` crates. `cargo check --locked -p entity-sqlite --features
eventlog-facade`, `-p entity-postgres --features eventlog-facade` and `-p entity-cli --features
eventlog-providers` each exited 101 with E0308, "there are multiple different versions of crate
`eventlog_core` in the dependency graph". The gate could not see it because `eventlog-facade` and
`eventlog-providers` are `default = []` and no gate step enables them. A consumer enabling either
feature would have been the first to find out.

The class is machine-checkable in a way a hand-maintained list of manifests is not: *within this
workspace, every `rev` naming one git URL names the same commit*.

## Acceptance

- A Rust check, run by a `task check` step, scans every workspace manifest and `Cargo.lock` and
  exits non-zero when one git dependency URL is named at more than one `rev`, listing each manifest
  and line.
- `task check` also compiles the optional feature configurations that a split pin breaks and that
  no gate step enables today: `entity-sqlite --features eventlog-facade`,
  `entity-postgres --features eventlog-facade`, `entity-cli --features eventlog-providers`.
  Independent pass 1 over 4ce78c11 confirmed this gap is pre-existing — it is red at 8b175736 too
  (`crates/entity-xtask/tests/gate_sees_pinned_git_features.rs:133`, exit 101) — and that the state
  it misses is demonstrably reached: it is what this unit's first attempt produced with a green gate.
- Red first, per AGENTS.md invariant 5: the check exits non-zero on a tree where one manifest's
  `rev` is moved and the others are not, and exits 0 on an unedited tree. Both states demonstrated
  by running, not by reading.
- Whether the step is added to `.github/workflows/gate.yml` as well as the Taskfile is decided
  explicitly and the decision is written down; `AGENTS.md:174-179` says the two are not assumed
  identical and `gate.yml:6` omits the local-only steps on purpose.
- No Python is added. `scripts/check-git-rev-uniformity.py`, written by the unit-4 implementor and
  left unapplied under `home-path:sha256:3125679774d7c786c751b0a42f8dcf8686d5ff750c04845940b3834446900330`, is a specification of the
  behaviour to port, not a file to adopt.
