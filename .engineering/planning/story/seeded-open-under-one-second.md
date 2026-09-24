---
format: aep.planning-md/1
id: story:seeded-open-under-one-second
kind: story
status: draft
title: A seeded open of a committed authority costs under one second on the ESS shape
summary: build_model is 85-92% of an 8.4 s seeded open at 7,815 blobs / 174 MB; apply 90.3 s CPU vs a 10 s target
owner: entity-runtime
revision: 1
---
## Outcome

A seeded open of a committed authority (capture plus `build_model`) costs under one second on the
ESS planning shape: 7,811 events, 7,815 blobs, 174 MB, blob sizes skewed (median 2,268 B, mean
9,526 B, max 5.9 MB). The model the open produces is byte-identical to the one the current path
produces on the same authority.

## Why

2026-09-22, unit 9 of wave-validate-v2-20260920 timed each capture site of the AEP migration apply
directly on a copy of the ESS store. `capture_tenant` is 8–15% of a capture's AEP-visible cost;
`build_model` is the other 85–92%, about 7.1–7.7 s of an 8.4 s seeded open. The apply opens the
authority several times (stage open 8.4 s; `write_file_control` on commit 24.5 s, a bridge open
with its own capture and `build_model`; projection publish 34.4–35.1 s). Whole apply: 90.3 s CPU,
112–129 s wall, against a 10 s target set by the operator on 2026-09-21.

`build_model` (`crates/entity-eventlog/src/adapter.rs:2037-2106` at b652c6ca) decodes every
event, calls `verify_digest` on every blob and decodes every record on every capture. The committed
record already names every blob digest. Unit 12 (Eventlog) made blob bytes lazy in captures with
verification on read (`BoundBlobs::read`); that removes about 1 s of the 8.4 s unless
`build_model` stops re-hashing what the committed record names.

Unit 12's eager `capture_tenant` on the same blob count: 667–1274 ms; SHA-256 on this CPU:
263 MB/s. So the 7 s in `build_model` is not hashing alone. The unit profiles first.

## Acceptance

- A by-symbol profile of `build_model` on the fixture above, before and after, timed directly
  (wall), recorded with the change.
- Three timed runs of the seeded open after the change, each under 1 s on that fixture.
- The model digest after the change equals the digest before it on the same authority.
- Every verification removed from the open path names, in a test, where it now happens. A blob
  digest verified on read is acceptable. A blob returned to a caller without verification is not.
- If under one second needs a change to the Eventlog capture shape or to `CapturedBlob`, the exact
  call is reported and the unit stops there.
