---
format: aep.planning-md/3
id: story:file-store-v3-state-and-append-only-history
kind: story
status: implemented
title: File Store v3 keeps current state apart from append-only history
summary: 'P14: one recorded execute still rewrites the whole subject (207,834 B written at 32 prior records); v2->v3 migration, consumer re-pins'
owner: entity-runtime
relations:
- serves: vision:O2
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-23T17:41:26Z", actor: "human:timo", revision: 2, decided_on: {"recorded":{"test_result":1}}}
- {from: "proposed", to: "active", at: "2026-11-23T17:41:26Z", actor: "human:timo", revision: 3, decided_on: {"recorded":{"test_result":1}}}
- {from: "active", to: "implemented", at: "2026-11-23T17:41:26Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
## Outcome

File Store v3 keeps a subject's current state and its append-only decision history apart, so one
recorded command appends its record and rewrites only the state, instead of rewriting the whole
subject document. Stores on `entity.file-store/2` migrate to v3, and the consumers that pin
Entity Runtime re-pin to the release that ships it.

## Finding

Review P14 (Entity Runtime 0.32.0): the native FileStore rereads and rewrites the full history for
one command. Decision P14 = A, with this story to be drafted (Timo, 01:02-01:04 CET 2026-11-22,
relayed by `host-operator`; master log `assignment.md` lines 49-50).

## Evidence

- Review measurement (`review.txt` P14): with 8 prior records one `entity execute` read 304,776
  subject bytes and wrote 57,070; with 32 it read 1,209,312 and wrote 207,834 for a 201,552-byte
  subject file.
- `9c0fb4e6` (fix/review-er-b, merged in #93) cut the reads: 6 subject read opens to 2;
  32 prior records 1,209,312 to 403,104 bytes read. Bytes written did not move: 57,070 (8 prior)
  and 207,834 (32 prior), probe output `er-b/before-history.txt`, `er-b/after-history.txt`.
- The commit body of `9c0fb4e6` states the remainder: "Each recorded write still rewrites the whole
  subject document; separating append-only history from current state is a File Store format
  change and is reported as a decision, not made here."
- KPI counters `file.execute.read_bytes.prior8` / `.prior32` in `checks/kpi/kpis.yaml` measure
  the read side; no counter measures bytes written per execute yet.
- Format today: `crates/entity-store/src/file.rs:30` `FORMAT = "entity.file-store/2"`,
  `SUBJECT_FORMAT = "entity.file-subject/2"` (one document per subject).

## Acceptance

A new KPI counter for bytes written by one recorded `entity execute` shows the same value, within
the size of one record, at 8 and at 32 prior records on a v3 store (v2 today: 57,070 and 207,834),
and a v2 store with recorded history, migrated to v3, opens, verifies and returns the same
`history()` records and digests as before the migration.
