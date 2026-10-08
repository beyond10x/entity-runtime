---
format: aep.planning-md/3
id: story:batch-cost-grows-linearly-with-members
kind: story
status: draft
title: A recorded batch costs time linear in its members
relations:
- serves: vision:O2
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: crates/entity-eventlog/src/adapter.rs
- confidence: inferred
  path: crates/entity-eventlog/src/facade.rs
- confidence: inferred
  path: crates/entity-eventlog/tests
- confidence: inferred
  path: crates/entity-executor
revision: 4
---
# A recorded batch costs time linear in its members

## Outcome

`RecordedProviderFacade::execute_batch` on a SQLite store opened `ProviderTracked` takes time
roughly linear in the number of batch members, with the store size held fixed, and roughly
constant per member as the store grows with the member count held fixed.

## Why

A downstream consumer measured 0.29.0 (release build): 196 members on a 601-event store took
21.4-28.2 s, 396 members on 1,201 events 96.8-117.7 s. Twice the members cost four to five times
the time; 1-3 members on a warm handle cost 40-55 ms. Checkpointed and complete opens behave the
same. Member count and store size grew together, so the two factors are not yet separated.
A profile attributes about 72% to unresolved libc frames (probably memcpy) plus malloc/free, and
the rest to serde_json of `FieldDefinition`, `OperationDefinition` and `DecisionRecord` and to
sha256 in `framed_key`. The consumer bounds batches at 32 members until this ships.

Path: `RecordedProviderFacade::execute_batch` -> `EventlogRecordedStore::execute_batch`
(`crates/entity-eventlog/src/adapter.rs`), `BatchReadStore::for_batch`, `Executor::batch`.

## Acceptance

- A reproduction in `crates/entity-eventlog/tests/` separates the two factors: batch size at a
  fixed store size, and store size at a fixed batch size, each at two or more points.
- A regression test fails on 0.30.1 and passes after the fix: doubling the members at a fixed
  store size raises the batch time by at most about 2.5x (or the equivalent count of
  serialisations, copies or hashes, if a count is a stable proxy for time).
- Record bytes and digests do not change.
- `CHANGELOG.md` names the change.

## Out of scope

Eventlog itself; a change to the capture format is reported, not made.
