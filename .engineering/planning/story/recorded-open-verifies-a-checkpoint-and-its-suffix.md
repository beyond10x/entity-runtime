---
format: aep.planning-md/3
id: story:recorded-open-verifies-a-checkpoint-and-its-suffix
kind: story
status: draft
title: A recorded-store open verifies a persisted checkpoint and the suffix after it
summary: 'open cost proportional to the suffix since the last complete verification (GitHub #55)'
owner: entity-runtime
refs:
- provider: github
  reference: beyond10x/entity-runtime#55
relations:
- serves: vision:O2
- informed_by: story:seeded-open-under-one-second
- informed_by: story:bounded-batch-and-facade-reads
- depends_on: story:recorded-open-checkpoint-design
scope:
- confidence: cited
  path: AGENTS.md
- confidence: inferred
  path: checks/ess-conformance
- confidence: cited
  path: crates/entity-eventlog/src/adapter.rs
- confidence: inferred
  path: crates/entity-eventlog/src/adapter/scoped.rs
- confidence: cited
  path: crates/entity-eventlog/src/adapter/tracked.rs
- confidence: cited
  path: crates/entity-eventlog/src/facade.rs
- confidence: inferred
  path: crates/entity-eventlog/src/projection.rs
- confidence: cited
  path: crates/entity-eventlog/src/sync.rs
- confidence: cited
  path: crates/entity-eventlog/tests/shared_clock_cost.rs
- confidence: inferred
  path: docs/design/eventlog-recorded-adapter-v0.1.md
- confidence: inferred
  path: docs/design/eventlog-recorded-sync-bridge-v0.1.md
- confidence: cited
  path: ess/provider-tracking
revision: 18
---
# A recorded-store open verifies a persisted checkpoint and the suffix after it

## Outcome

Opening a recorded Eventlog store with `CapturePolicy::ProviderTracked` costs time proportional to
the history appended since the last completely verified observation, not to the whole history.
The open reads a persisted checkpoint of a complete verification (its position, the digest of the
model it verified, and the checkpoint's own integrity digest), checks it, verifies and folds only
the suffix after it, and produces the same model a complete verification would. A complete
verification stays available on demand, and `CapturePolicy::FullVerification` keeps verifying
everything on every open.

## Why

GitHub beyond10x/entity-runtime#55, measured by a consumer on Entity Runtime 0.26.0: every open
completely verifies the store; `start_with_read_policy` documents it
(`crates/entity-eventlog/src/sync.rs:1300`: "opening always verifies the whole authority";
`crates/entity-eventlog/src/adapter/tracked.rs:13`: "Initial open is always verified
completely"). The consumer's process is short-lived per command, so the in-memory
`CaptureCheckpoint` held by `tracked.rs:29-35` never outlives one command. Consumer's measurements
(beyond10x/connectors#101, #103, not re-measured here): open 75 / 492 / 963 ms at 55 / 601 / 1,203
events; 4.1 s for one list command at 2,623 events.

## Acceptance

1. **Gate:** this story implements option (a) of `docs/design/recorded-open-checkpoint-v0.1.md`,
   which `story:recorded-open-checkpoint-design` chose (wave 1, 2026-10-06). It starts only after
   `dependency-blocker:eventlog-durable-capture-continuity` (beyond10x/eventlog#39) is cleared.
2. **Spec-first.** The design's four new commands are written into
   `ess/provider-tracking/domains/operations.yaml` before any code:
   `entity-provider.tracking.EnableDurableCheckpoints`, `DiscardCheckpoint`, and the fixtures
   `KeepCopy` and `TruncateTail`, plus the new `SqlMutate` kind that tampers the checkpoint record
   (design § on the specification). Scenarios: a tampered checkpoint falls back to complete
   verification; a checkpoint ahead of the head refuses the open and `DiscardCheckpoint` recovers
   it; an open after appends verifies only the suffix; a foreign write between the observation and
   the durable checkpoint yields a complete open; every answer a bounded handle gives equals what a
   complete handle gives on the same store. `provider-ess-check` executes them.
3. **The five reviewed tamper scenarios**
   (`ess/provider-tracking/scenarios/unchanged-head-{blob,delete-blob,event,identity,projection}-tamper.yaml`)
   pass unchanged: their reopen follows a refusal, so no checkpoint exists (verified by the design
   reviews, `review-result:er-w1-u2-design-review-1`). Tamper-after-checkpoint variants are added
   per item 2. `provider-ess-check` exits 0.
4. **Full verification on demand:** a test opens a store with a valid checkpoint under
   `FullVerification` and shows the whole history is verified (the cold-open capture count
   `crates/entity-eventlog/tests/shared_clock_cost.rs:266-273` asserts it right after the open).
5. **Cost.** The release-mode probe the design story added to
   `crates/entity-eventlog/tests/shared_clock_cost.rs` measures the open with a checkpoint at the
   previous head: median of 5 runs at 1,203 events is at most 2× the median at 55 events (the bound
   `story:bounded-batch-and-facade-reads` used). The treatment output is compared with the
   baseline the design story committed at `docs/ess/evidence/provider-tracking/open-cost-baseline.txt`,
   and recorded as `verification` evidence naming both files.
6. **No silent unverified read.** Every verification removed from the open path names, in a test,
   where it now happens.
7. `CHANGELOG.md` line (including the one-way enable step and its consequence for older binaries on
   the same store); `docs/requirements.md` R-151 ("fully verifies on open", line 224) amended to the
   wording the design gives;
   `AGENTS.md` § Boundaries, the "IO stays at named edges" bullet (`AGENTS.md:245`), gains one
   sentence stating what a `ProviderTracked` open verifies and what it does not, in the same change.

## Out of scope

The consumer's own growth reduction (fewer events per command) and its owner-process memory
(beyond10x/connectors#103), except where the design shows the same change bounds both. The constant
factor of `build_model` on a first open is `story:seeded-open-under-one-second`.

## Scope

Derived 2026-10-06 by `aep:story-scoper` at `7926ec45` (= `origin/main`). Every line is **cited** (read
from the story or the tree) or **inferred** (a reading that could be wrong).

- **Primary surface:** `crates/entity-eventlog`, the open path — cited
- **Files:** `crates/entity-eventlog/src/adapter/tracked.rs` — `CapturePolicy` doc :7-16, `Cache`/`Held` :23-35, `tracked_model` :38-163 (complete open calls `build_model` at :82), suffix verifier `advance` :210-470 — cited
- **Files:** `crates/entity-eventlog/src/adapter.rs` — `open_with_policy` :959-992 (opening is `capture_model` at :987), `capture_model` :1042-1048, `model_of` :1050, `CapturedModel` :3657-3684, `build_model*` :3869-3921 — cited
- **Files:** `crates/entity-eventlog/src/sync.rs` — `RecordedEventlogBridge::start_with_read_policy` :1300-1316, `EventlogRecordedStoreOwner::open_with_policy` :254 — cited
- **Files:** `crates/entity-eventlog/src/facade.rs` — `RecordedProviderFacade::start_with_read_policy` :174-185 — cited
- **Files:** `crates/entity-eventlog/tests/shared_clock_cost.rs` — `seeded` builds 55/601/1,203 and asserts the cold open is one full capture (:287); the probe (:310-427) times `open_elapsed` once per size, not a median of 5 — cited
- **Also likely:** a canonical encoding and digest of `CapturedModel` in `adapter.rs`; the struct has no serde and the only model digest is test-only and `Debug`-based (`adapter.rs:5299`) — inferred
- **Spec:** `ess/provider-tracking/` (new commands in `domains/operations.yaml`, scenarios, `ess-inputs.yaml`, regenerated `generated/model.json`, `generated/suite.json`, `coverage.json`) — cited for the directory, inferred for the files
- **Also likely:** `checks/ess-conformance/src/provider.rs` (`reopen` :86-106) — inferred
- **Checkpoint location — needs an Eventlog provider change** for acceptance 1–2 as written — inferred, medium: pinned `CaptureCheckpoint` is "never a persisted or caller-made cursor" (eventlog-core `capture.rs:47-52`, rev `6983cc25`); the SQLite proof lives inside one process (`PRAGMA data_version` and `total_changes` per connection, `eventlog-sqlite` `tracked_capture.rs:20-41`; issuer `Arc<()>` :87-98; in-memory journal). Eventlog `origin/main` (`0.6.0-4-g1d089714`, read 2026-10-06) adds no durable proof.
- **ER-only option:** the pinned port has `read_feed(tenant, after_position)` (eventlog-core `lib.rs:879`, unused by ER), per-stream `save_snapshot_checked`/`load_snapshot` (`lib.rs:905-940`, all four providers), or a caller-selected path on `EventlogRecordedStoreOwner`. It cannot see SQL edits to the verified prefix made through another connection — inferred
- **Documents:** `docs/design/recorded-open-checkpoint-v0.1.md` (new), `CHANGELOG.md`, `AGENTS.md` — cited
- **Also likely:** `docs/requirements.md:224` (R-151 "fully verifies on open"), `docs/design/eventlog-recorded-adapter-v0.1.md:389,393,404`, `docs/design/eventlog-recorded-sync-bridge-v0.1.md:499`, `docs/ess/README.md:26`, `docs/ess/evidence/provider-tracking/` — inferred
- **Coordinator-owned, not unit scope:** `CHANGELOG.md`, `docs/requirements.md` (R-151) — cited
- **Confidence:** high for the ER surface; medium for the provider-change verdict (read from pinned source, not run)
- **Would collide with:** any unit touching `entity-eventlog`'s open/capture path (`adapter.rs`, `adapter/tracked.rs`), `sync.rs`/`facade.rs` constructors, `ess/provider-tracking/`, `checks/ess-conformance`, `shared_clock_cost.rs`, the R-151 row, `CHANGELOG.md` — in particular `story:seeded-open-under-one-second`, whose `build_model` (`adapter.rs:3869-3921` now) is on the same dispatch; run the two one after the other
- **Safety fact:** five scenarios (`ess/provider-tracking/scenarios/unchanged-head-{blob,delete-blob,event,identity,projection}-tamper.yaml`, steps :33-37) require a `ProviderTracked` reopen to refuse with `ProviderIntegrity` after a second connection edits old data while the event head stays the same. A suffix-only open keeps them only with a provider proof that survives a process restart; the pinned SQLite provider issues none. Proof level 2, unproven.

### Not established

- Resolved in revision: acceptance 7 names `AGENTS.md:245` (the IO-boundary bullet) as the place for the sentence.
- Whether the checkpoint's "own integrity digest" can resist forgery: an unkeyed digest catches corruption, not a rewritten-and-rehashed checkpoint; ER holds no key.
- How a bounded open gets the tenant totals `admit_growth` (`adapter.rs:1122`) needs; today only `TenantCaptureDelta.resulting_usage` carries them.
