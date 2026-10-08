---
format: aep.planning-md/3
id: story:recorded-decisions-share-one-decoded-definition
kind: story
status: draft
title: Recorded decisions that name one definition share one decoded copy of it
owner: entity-runtime
refs:
- provider: github
  reference: beyond10x/entity-runtime#59
relations:
- serves: vision:O2
- informed_by: story:verified-model-holds-each-record-once
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: Cargo.toml
- confidence: cited
  path: crates/entity-core/Cargo.toml
- confidence: inferred
  path: crates/entity-core/src/replay.rs
- confidence: cited
  path: crates/entity-core/src/runtime.rs
- confidence: cited
  path: crates/entity-core/tests/purity.rs
- confidence: inferred
  path: crates/entity-eventlog/src/encoding.rs
- confidence: cited
  path: crates/entity-eventlog/tests/verified_model_heap.rs
- confidence: inferred
  path: crates/entity-shell/src/lib.rs
- confidence: inferred
  path: crates/entity-store/src/asynchronous/encoding.rs
- confidence: inferred
  path: crates/entity-store/src/asynchronous/memory.rs
- confidence: inferred
  path: crates/entity-store/src/asynchronous/verify.rs
- confidence: inferred
  path: crates/entity-store/src/lib.rs
- confidence: inferred
  path: docs/ess/evidence/provider-tracking/model-heap-issue-59.txt
revision: 3
---
# Recorded decisions that name one definition share one decoded copy of it

## Outcome

Every complete decision record carries its definition snapshot (`DecisionRecord.definition`,
`crates/entity-core/src/runtime.rs:142`). Records that carry equal definitions share one decoded
`EntityDefinition` in memory instead of one decoded copy each. Recorded bytes, digests and every
answer stay as they are.

## Why

GitHub beyond10x/entity-runtime#59 asked for this alongside one stored copy per record. The
one-copy change (`story:verified-model-holds-each-record-once`) measured, on the 96-field
`metadata` fixture at 1,203 events, about 119 MB still held by the one stored record per committed
record, of which about 88 MB is the decoded entry, mostly the definition (inferred by the
implementor from the before/after difference, not separately measured). Sharing it needs
`DecisionRecord.definition` to hold a shared pointer (`Option<Arc<EntityDefinition>>`, serialised
as the definition itself), which changes a public `entity-core` field.

## Constraints

- `entity-core` is pinned by aep, aep-service, atlas, bench and ess (`AGENTS.md` § Boundaries).
  A field type change breaks a consumer that constructs or pattern-matches the field when it
  re-pins, so this is a coordinated migration, decided before it is implemented.
- The kernel's dependency list is pinned to `serde` and `serde_json`
  (`crates/entity-core/tests/purity.rs`); serde's `rc` feature is a change to that pin.
- Record bytes must not change: serialising the shared pointer writes the definition as today.

## Acceptance

- The `verified_model_heap` probe at 601 and 1,203 events shows the bytes per event fall by the
  share the decoded definitions held, measured before and after on the same command.
- Every model digest pin, every replay and every ESS scenario passes unchanged.
- A `CHANGELOG.md` line naming the changed field for consumers.

## Out of scope

The tracked handle's blob copies and the stored-record copies (`story:verified-model-holds-each-record-once`).
