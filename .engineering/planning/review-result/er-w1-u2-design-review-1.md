---
format: aep.planning-md/3
id: review-result:er-w1-u2-design-review-1
kind: review-result
status: active
title: 'U2 bounded-open design: independent review 1 (machine paths redacted)'
relations:
- reviews: story:recorded-open-checkpoint-design
revision: 1
---
needs-revision

docs/design/recorded-open-checkpoint-v0.1.md:418 — The issue says the capture shape checks should "keep refusing every other trigger and foreign key as today" and names only `capture.rs:144-158` and `:639-647`. The check at :146-165 refuses nothing: it only stops the provider from issuing a checkpoint. A store with a trigger on its events table opens, and each read takes another complete capture (captures, model_builds) = (2,2). The checks that do refuse are not named. One is the blob-table check at open (`lib.rs:953-963`, reached through `require_existing_schema` :395 and `open_existing` :318). The same check is on Eventlog `main` (`lib.rs:952-959`). The others are the projection-table check at attach (`inline_admin.rs:106`) and the inspector (`inspection.rs:351-360`). The cost is not stated anywhere: installing the provider's triggers is a one-way change, and Entity Runtime 0.26.0 or older can never open that store again. The issue also gives no way to install triggers on stores that already exist, because `open_existing` "never creates tables" (`lib.rs:112-113`). The same text is in eventlog-issue.md:66-68. — I ran this: `review_open_checkpoint.rs`, three trigger probes, green. Blob trigger: the provider refuses with `Invalid("unsupported SQLite blob trigger or table behavior")` and the facade with `BridgeStartError::Open(Backend(..))`. Projection trigger: `Open(Backend("projection admission does not match durable structure"))`.

docs/design/recorded-open-checkpoint-v0.1.md:405 — "The durable form binds … a durable mutation epoch" does not say which epoch. The in-process `Checkpoint` (`tracked_capture.rs:87-97`) holds only values that mean nothing outside one connection (`data_version`, `total_changes`, :20-41). If `durable_checkpoint` reads the epoch at the time it is called, a foreign SQL write is hidden: the handle appends, a second connection edits old data, then the handle drains without reading (the drain rule at :190-192 still writes). The next open gets `Unchanged`, and no ProviderTracked open after that detects the edit. To fix: the epoch has to be read inside the observation's own transaction. The acceptance list (:431-438) also needs a case "`Complete` after a foreign write between the observation and `durable_checkpoint`". And "in its own transaction" (:410) should read "in the group's transaction". — Read from code only; this cannot be run because the API does not exist yet.

docs/design/recorded-open-checkpoint-v0.1.md:190 — The drain writes only when "the held position differs from the persisted one". This is wrong in two directions:
- It never replaces a checkpoint that was thrown away while the head is unchanged. That covers a new `verifier` after every ER upgrade, a change of limits, and a bad digest. Read-only processes then pay a complete verification on every open until some write moves the head.
- It lets a long-lived handle at an older position overwrite a newer checkpoint.

The rule should be: write when the loaded checkpoint was thrown away or any field it binds differs, and never write a lower position. — Read from code; `save_snapshot_checked` accepts any matching generation (`lib.rs:2062`).

docs/design/recorded-open-checkpoint-v0.1.md:219 — The design says "the worst a forged or damaged checkpoint can force is the cost of today's open". That contradicts :35 and :214. Anyone with SQL access can write a checkpoint with a recomputed digest, since the digest is unkeyed and its construction is public (`encoding.rs:333-343`). If that checkpoint names a position beyond the head, every ProviderTracked open is refused, and the design gives no way to clear it. — Read from the design and the code.

docs/design/recorded-open-checkpoint-v0.1.md:413 — The epoch and the position are counters. "An older copy of the file shows a lower epoch or position" is true only until the restored copy is written to. After as many acknowledged groups as were lost, both counters match a newer durable checkpoint, and the provider answers `Unchanged`/`AppendDelta` wrongly. Entity Runtime does not reach this, because its checkpoint lives in the same file. A random token per write closes it. — Read from code; not reachable for Entity Runtime.

docs/design/recorded-open-checkpoint-v0.1.md:179 — The read-bound totals are kept twice: in the ER checkpoint's `held`, and as the usage the provider binds (`tracked_capture.rs:96`). On `Unchanged` nothing compares the two, and `CaptureCheckpoint` is opaque (`capture.rs:47-52`). Totals taken from the `CaptureHeld` that `grow()` advances (`adapter.rs:1160-1167`) would go unnoticed into `admit_growth`. — Read from code; only a bug or an informed edit reaches it.

docs/design/recorded-open-checkpoint-v0.1.md:342 — The cost section leaves out a fallback. The delta verifier inherits `advance`'s refusal of any event other than `er.recorded_entry` (`tracked.rs:216-225`). `record_refusal` also binds its blob with a standalone `put_blob` (`adapter.rs:2794-2798`), which ends continuity; the existing test `tracked.rs:765-783` shows that. So with `Executor::recording_refusals`, any refusal recorded since the checkpoint sends the next open to a complete verification. — Read from code.

**Claims checked and held** (tree: worktree er-w1-u2, base b7362882 plus uncommitted changes):
- **Citations:** every ER and Eventlog citation in the design is correct, at most a line or two off.
- **Eventlog branches:** `6983cc25` is only on `fix/er-51-capture-checkpoints`. The merge base with `main` is `06c1e99c`. `main` is at `1d089714` (0.6.0-4) and has neither `capture_tenant_since` nor `tracked_capture.rs`. Checked with ls-remote.
- **The five tamper scenarios:** the facade created at Provision opens with `FullVerification` (`sync.rs:613`), so it writes no checkpoint. The first tracked handle refuses at the warm read. So no checkpoint exists at the final Reopen, and all five pass under either option.
- **Baseline:** every median and range recomputes from the raw samples, and the probe's sha256 matches.
- **Snapshot claims (run):** a snapshot is not captured material, and the provider's own snapshot write ends in-process continuity (`a_checkpoint_snapshot_is_…`, green).
- **Index rows:** `validate_projection_sets` compares the rows exactly in both directions (`adapter.rs:4478-4486`).
- **Forked subjects:** subject rows carry no fork marker, but SQLite events never carry digests (`lib.rs:3451`), so the bounded reads are not exposed to forks.

**What I changed:** only the new file `crates/entity-eventlog/tests/review_open_checkpoint.rs` (354 lines), which is a test file. Four cases, all green, exit 0. Three of them need the `sqlite3` shell and are marked `#[ignore]`. rustfmt and clippy `-D warnings` are clean. Nothing else in the tree changed: `shared_clock_cost.rs` still hashes `326e4850…`.

**Written outside the worktree:**
- `~/.cache/er-w/u2/review/` (8 logs)
- debug test binaries in `~/.cache/b10x-target/er-w1-u2/debug/`

```findings
- file: docs/design/recorded-open-checkpoint-v0.1.md
  line: 418
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: "The issue says shape checks 'keep refusing' triggers per capture.rs:144-158, which only stops checkpoint issuance (measured); it omits the blob check at open (lib.rs:953-963, measured refusal), the projection check at attach and the inspector, so it does not state that provider triggers lock out every older Eventlog and ER, or how existing stores get them."
- file: docs/design/recorded-open-checkpoint-v0.1.md
  line: 405
  category: concurrency
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: "The durable form does not have to bind the epoch read in its own observation, so a durable_checkpoint that reads the current epoch hides a foreign write made after the handle's last verification; the acceptance list has no case for it."
- file: docs/design/recorded-open-checkpoint-v0.1.md
  line: 190
  category: boundary
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: "Writing at drain only when the position differs never replaces a checkpoint discarded after an upgrade, a limits change or a bad digest while the head is unchanged, and it lets an older handle write a lower position."
- file: docs/design/recorded-open-checkpoint-v0.1.md
  line: 219
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: "A forged checkpoint with a recomputed unkeyed digest and a position beyond the head refuses every ProviderTracked open with no stated recovery, contrary to 'the worst ... is the cost of today's open'."
- file: docs/design/recorded-open-checkpoint-v0.1.md
  line: 413
  category: property
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: "Counter epochs and positions on a restored copy that is then written can equal a newer durable checkpoint and give a false Unchanged; ER keeps its checkpoint in the same file, so ER does not reach it."
- file: docs/design/recorded-open-checkpoint-v0.1.md
  line: 179
  category: judgement
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: "The read-bound totals live in both the ER checkpoint and the provider's bound usage, and nothing compares them on Unchanged before they feed admit_growth."
- file: docs/design/recorded-open-checkpoint-v0.1.md
  line: 342
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: "The cost claim leaves out that a delta with any non-entry event, or a recorded refusal's standalone put_blob, sends the next open to complete verification."
```
