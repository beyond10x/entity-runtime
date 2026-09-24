---
format: aep.planning-md/2
id: story:recorded-store-imports-a-batch-with-one-capture
kind: story
status: implemented
title: The recorded Eventlog store imports a batch of histories with one capture and one append group
relations:
- serves: vision:O2
revision: 4
---
## Outcome

The recorded Eventlog store imports a batch of subject histories with one capture of the destination
and one atomic append group: `import_anchors(Vec<SubjectHistory>)` beside the singular
`import_anchor`, same anchor wire format, same identities, same receipts. A caller importing N
subjects pays one capture, not 2N.

## Why

Measured 2026-09-21 by unit 9 of wave-validate-v2-20260920 (AEP), on a copy of the real ESS
planning store, instrumentation reverted and kept as
`waves/0005-aep-migration/wave-validate-v2-20260920/unit-9-import-batching/instrumentation.patch`:

| quantity | value |
| --- | --- |
| imports in one ESS `apply` | **7,810**: 3,222 `LegacyEvidenceBlob` + 3,222 `LegacyRecordCoordinate` + 917 relations + 448 entities + 1 boundary |
| `capture_model()` calls per import | **2** |
| per-import cost | 46.6 ms at the start, 743 ms by import 736, slope ~1 ms per prior import |
| `sha2::compress256` share of samples | 62.9 % — every capture re-verifies every blob digest of the whole authority |
| projected ESS `apply` today | ≈ 8.5 h (the real M4d was cancelled at ~88 % after 4 h 53 m) |
| with one capture per batch (this story) | ≈ 6.5 min |
| plus one append group and batched blob durability (Eventlog story) | ≈ 60 s |

The AEP migration cannot do this on its own side: `import_anchor` is singular and the anchor wire
format (`anchor_from_history`, `encode_anchor`, `framed_key`, `subject_stream_id`, both blob domains)
is `pub(crate)` in `entity-eventlog`; reimplementing it in AEP would duplicate this repository's
verified bytes and break identity. AEP's only lever (fewer `complete_file_snapshot` calls) is worth
2 % of today's time.

Unit 7 (AEP) removed the same pattern from reads: one capture per command instead of one per
record, 76 s → 1.25 s. This is the write-side twin.

## Acceptance

- Red first, through a counting seam: importing N histories through `import_anchors` makes O(1)
  captures (two: before and after the group) and one append group, independent of N; fails at
  8569da2 (no such entry point; N imports make 2N captures).
- Byte identity: for the same input, `import_anchors` produces the same anchors, subject stream ids,
  blob keys and receipts as N calls of `import_anchor` (compared as values on a fixture and on the
  ESS copy's first 100 subjects).
- The singular `import_anchor` is unchanged and its callers still pass.
- Group semantics: a failure inside the batch leaves nothing of the batch committed (the
  provider's atomic append group), and the returned receipt says so.
- Measured here at n=150, end to end, `strace -f -c -w`: singular 3,785 fsyncs / 300 captures /
  59.0 s wall → batch 343 / 2 / 2.2 s (unit 10, eef4a0fa). The ESS-copy before/after belongs to
  story:migration-import-costs-one-capture-per-batch (AEP), where the caller lives.
- Decisions after pass 1 (2026-09-21): on a provider whose port refuses the guarded blob-bearing
  group (fail-closed default), `import_anchors` takes the singular sequence, and the docs say so;
  every member's global record keys are collected into one sorted set before the per-member loop,
  so a shared identity inside a batch reports the typed `RecordConflict` and the batch holds one
  global lock order; identical bytes mentioned twice in one batch replay as the singular path does,
  different bytes under one identity refuse; `import_source_anchors` leaves the public surface
  unless a caller exists; blob failures map through the blob operation's documented error surface.
- `task check` green; feature checks green; then Eventlog → ER → AEP re-pin.
