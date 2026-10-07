---
format: aep.planning-md/3
id: dependency-blocker:eventlog-durable-capture-continuity
kind: dependency-blocker
status: cleared
title: Eventlog has no durable capture continuity a later process can continue from
refs:
- provider: github
  reference: beyond10x/entity-runtime#55
- provider: github
  reference: beyond10x/eventlog#39
relations:
- blocks: story:recorded-open-verifies-a-checkpoint-and-its-suffix
- informed_by: story:recorded-open-checkpoint-design
revision: 3
transitions:
- {from: "open", to: "cleared", at: "2026-10-07T02:37:30Z", actor: "human:timo", revision: 3}
---
# Eventlog has no durable capture continuity

`story:recorded-open-verifies-a-checkpoint-and-its-suffix` implements option (a) of
`docs/design/recorded-open-checkpoint-v0.1.md`: a bounded open continues from a persisted checkpoint
only when the provider proves, in a later process, that nothing but its own acknowledged appends
changed the tenant's captured material since that checkpoint. Eventlog offers no such proof:

- eventlog-core's `CaptureCheckpoint` is "never a persisted or caller-made cursor"
  (`crates/eventlog-core/src/capture.rs:47-52` at `6983cc25`);
- the SQLite change proof is per connection and per process (`crates/eventlog-sqlite/src/tracked_capture.rs:20-41`, `:87-97`);
- the commit Entity Runtime 0.26.0 pins, `6983cc25`, is only on the unmerged Eventlog branch
  `fix/er-51-capture-checkpoints`; Eventlog `main` (`1d089714`, 0.6.0-4) has no
  `capture_tenant_since` (checked 2026-10-06 with `git merge-base --is-ancestor`).

The ask is filed as beyond10x/eventlog#39 (the API, the one-way migration calls, the acceptance
cases). This blocker clears when an Eventlog release carries that API and the prerequisite commit,
and Entity Runtime can pin it.

## Cleared, 2026-10-07

Eventlog `0.8.0` carries the API (release published 2026-10-07T02:20:21Z by `b10x-bot[bot]`, tag
on `de30462b`; `gh release view 0.8.0 --repo beyond10x/eventlog`). beyond10x/eventlog#39 closed
2026-10-07T02:10:18Z. In that release: `ConsistentTenantCapture::durable_checkpoint`,
`restore_checkpoint` and `checkpoint_usage` (`crates/eventlog-core/src/capture.rs:422-442` at
`de30462b`), `DurableCaptureCheckpoint` (`:83`), and
`SqliteEventStore::enable_durable_continuity` / `disable_durable_continuity`
(`crates/eventlog-sqlite/src/durable_capture.rs:1105`, `:1116`). Its changelog answers the
design's open point on standalone writes: standalone `append`, `put_blob` and `delete_blob` are not
journaled, so they end continuity. `entity-eventlog --all-features --all-targets` compiles against
it on Rust 1.91.0 with no source change.
