---
format: aep.planning-md/3
id: story:verified-model-holds-each-record-once
kind: story
status: active
title: A verified model holds each committed record once
owner: entity-runtime
refs:
- provider: github
  reference: beyond10x/entity-runtime#59
relations:
- serves: vision:O2
- informed_by: story:recorded-open-verifies-a-checkpoint-and-its-suffix
scope:
- confidence: cited
  path: crates/entity-eventlog/src/adapter.rs
- confidence: inferred
  path: crates/entity-eventlog/src/adapter/memory.rs
- confidence: cited
  path: crates/entity-eventlog/src/adapter/shared.rs
- confidence: inferred
  path: crates/entity-eventlog/src/adapter/tracked.rs
- confidence: inferred
  path: crates/entity-eventlog/src/sync.rs
- confidence: cited
  path: crates/entity-eventlog/tests
- confidence: cited
  path: crates/entity-store/src/asynchronous.rs
- confidence: inferred
  path: crates/entity-store/src/asynchronous/verify.rs
revision: 14
transitions:
- {from: "draft", to: "proposed", at: "2026-10-06T22:56:53Z", actor: "human:timo", revision: 9}
- {from: "proposed", to: "active", at: "2026-10-06T22:56:56Z", actor: "human:timo", revision: 10}
---
# A verified model holds each committed record once

## Outcome

The verified model an `EventlogRecordedStore` builds keeps one stored copy per committed record:
`records`, `histories` and the record's batch refer to that one copy (an `Arc<StoredRecord>` or an
index into one vector). Every answer the model gives is unchanged. Sharing one decoded
`EntityDefinition` between records moved to `story:recorded-decisions-share-one-decoded-definition`
(see Constraints).

## Why

GitHub beyond10x/entity-runtime#59, measured by a consumer on 0.26.0 (not re-measured here): a
verified handle over a 601-event, 21 MB store holds about 235 MB of live heap. At the 492 MB peak
of one open plus one read, 233 MB (47 %) sits under `entity_core::definition`, and 195 MB under
`StoredRecord::clone` in `insert_committed`. That function keeps each record three times:
`records` (`RecordLookup::Committed(saved.clone())`), `histories`
(`history.records.push(saved.clone())`) and the batch. Peak RSS is 645 MB at 601 events and
1,371 MB at 1,201 events. The consumer's owner-memory target (beyond10x/connectors#103) is below
two models' live heap, so it cannot reach it alone. This is separate from #55
(`story:recorded-open-verifies-a-checkpoint-and-its-suffix`): #55 is about how often the model is
built, this story about its size per event.

## Acceptance

- A release-mode probe in `crates/entity-eventlog/tests/` measures the live heap of one verified
  handle at the 55 / 601 / 1,203-event shapes `shared_clock_cost.rs` seeds, before and after. The
  bytes per event after the change are at most half the bytes per event before, at 601 and at
  1,203 events.
- The model-digest pins in `crates/entity-eventlog/src/adapter.rs` (`model_digest`) and every
  `seeded_open` test pass unchanged: the model answers what it answered.
- Optionally: a terminal-row enumeration that does not copy every history, so a caller does not
  need `complete_snapshot` for it.
- `CHANGELOG.md` line.

## Out of scope

How often the model is built (#55); the consumer's own event growth.

## Constraints

- `StoredRecord`, `SubjectHistory`, `StoredBatch` and `RecordLookup` are public `entity-store`
  types, and `DecisionRecord.definition` (`Option<EntityDefinition>`) is a public `entity-core`
  field (`crates/entity-core/src/runtime.rs:142`). Five consumer repositories pin those crates, so
  no public item of `entity-core`, `entity-store`, `entity-executor`, `entity-shell` or
  `entity-yaml` changes signature here. The single copy lives in `entity-eventlog`'s private
  model; an additive `#[doc(hidden)]` verifier entry in `entity-store` is allowed, following
  `verify_subject_history_with_checked_bytes`.
- A shared decoded definition needs `DecisionRecord.definition` to hold a shared pointer, which is
  a change to the kernel's public type, so it is its own story. The first implementation round
  measured 0.577x (601 events) and 0.547x (1,203) with one copy per record; the second round also
  stops the provider-tracked handle from keeping a second copy of each record and batch blob.

## Scope

- `crates/entity-eventlog/src/adapter.rs` — `CapturedModel`, `insert_committed`, read paths,
  `model_digest` pins (cited: issue profile and this body).
- `crates/entity-eventlog/src/adapter/memory.rs` — `VerifiedHistory.records` holds another copy
  of each remembered record (inferred from `memory.rs:113`).
- `crates/entity-eventlog/src/adapter/tracked.rs` — incremental advance clones model records
  (inferred from `tracked.rs:351-391`).
- `crates/entity-eventlog/src/sync.rs` — `complete_snapshot` callers (inferred).
- `crates/entity-store/src/asynchronous/verify.rs` — verifier internals over a history view
  (inferred).
- `crates/entity-eventlog/tests/` — new release-mode heap probe (cited: Acceptance).
- `CHANGELOG.md` — coordinator-owned.
