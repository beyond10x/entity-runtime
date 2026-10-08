---
format: aep.planning-md/3
id: story:pedantic-lints
kind: story
status: draft
title: clippy::pedantic in the gate
summary: Raise the workspace lint level to pedantic and fix or justify every hit.
relations:
- derived_from: epic:kernel
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: cited
  path: Cargo.toml
- confidence: inferred
  path: crates
- confidence: cited
  path: crates/entity-cli/src/main.rs
- confidence: cited
  path: crates/entity-core/src/observed.rs
- confidence: cited
  path: crates/entity-core/src/runtime.rs
- confidence: cited
  path: crates/entity-core/src/truth.rs
- confidence: cited
  path: crates/entity-eventlog/src/adapter.rs
revision: 4
---
# Story: clippy::pedantic in the gate

## Outcome

The workspace lint level is `clippy::pedantic`, warnings fatal, and every remaining `allow` states
its reason beside the item.

## Acceptance

`[workspace.lints.clippy] pedantic = "warn"`; `task check` green; no `allow` without a comment.
