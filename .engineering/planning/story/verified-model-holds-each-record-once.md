---
format: aep.planning-md/3
id: story:verified-model-holds-each-record-once
kind: story
status: draft
title: A verified model holds each committed record once, and each distinct definition once
owner: entity-runtime
refs:
- provider: github
  reference: beyond10x/entity-runtime#59
relations:
- serves: vision:O2
- informed_by: story:recorded-open-verifies-a-checkpoint-and-its-suffix
revision: 1
---
# A verified model holds each committed record once, and each distinct definition once

## Outcome

The verified model an `EventlogRecordedStore` builds keeps one stored copy per committed record:
`records`, `histories` and the record's batch refer to that one copy (an `Arc<StoredRecord>` or an
index into one vector). Records that name the same definition share one decoded
`EntityDefinition`. Every answer the model gives is unchanged.

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
