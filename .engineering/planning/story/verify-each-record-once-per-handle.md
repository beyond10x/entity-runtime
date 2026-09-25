---
format: aep.planning-md/2
id: story:verify-each-record-once-per-handle
kind: story
status: implemented
title: Entity Runtime verifies each record once per handle on the Eventlog path
relations:
- serves: vision:O2
scope:
- confidence: cited
  path: crates/entity-eventlog/src/adapter.rs
- confidence: cited
  path: crates/entity-store/src/asynchronous/verify.rs
revision: 6
---
# Entity Runtime verifies each record once per handle on the Eventlog path

## Outcome

A store handle on the Eventlog path builds its verified model once per head, and verifies each blob, record, request
and subject history once; later reads reuse what the handle already verified. No stored byte, digest or refusal
changes.

## Why

Connectors' metadata authority (ESS evolution M7) holds a 2 s lock across publish, revoke and dispatch, and a 250 ms
audit recovery window. On Entity Runtime 0.22.0 a two-action batch on a 12-subject store took 432 ms and a cold open
62 ms. The cost was CPU spent re-encoding every record with its full definition (`canonical_domain_bytes` 19.5 %,
`record_comparison_bytes` 10 %), so the lock-bound Connectors cases failed 0/3.

## Delivered

- 0.20.0 (R1): the verified model is built once per (handle, head); canonicalisation without deep clones.
- 0.23.0 (entity-runtime#37, `032ebcee5`): a per-handle verification memory (blobs by digest, domain and exact bytes;
  decodes by exact bytes; request checks; subject histories record for record), capped at 64 MiB by measured retained
  size; a batch fast path; one blob read per round per role; no reordering pass for already-ordered maps.
- Measured: a two-action batch 432 → 83 ms, a cold open 62 → 30 ms (120-field definitions); typed decodes per warm
  batch 176 → 4. An adversary pass found no warm/cold divergence over hundreds of consistent capture rewrites.

## Evidence

- Release 0.23.0 on `77aac6eac`: `task check` exit 0, 747 tests, `postgres-check` against a disposable PostgreSQL.
- Connectors on 0.23.0: the four lock-bound cases pass at the unchanged bounds.

## Not done

- Eventlog re-hashes every blob it reads and makes 8 fsyncs per batch (7 `put_blob` + 1 append); batching the blob
  writes needs an Eventlog API that does not exist.
- The inline projector decodes a whole batch once per member (quadratic in batch size).
