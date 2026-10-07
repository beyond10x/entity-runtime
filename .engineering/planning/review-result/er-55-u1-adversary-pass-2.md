---
format: aep.planning-md/3
id: review-result:er-55-u1-adversary-pass-2
kind: review-result
status: active
title: 'Issue 55 U1 (bounded recorded open): adversary pass 2'
relations:
- reviews: story:recorded-open-verifies-a-checkpoint-and-its-suffix
revision: 1
---
unit: story:recorded-open-verifies-a-checkpoint-and-its-suffix, worktree er-55-u1 at d68ce28a plus my uncommitted test additions
verdict: red, 1 case (INFEASIBLE, note); the F1 correction holds against every attack I ran
cases: executed 228→231, red 1
origin: introduced 1 / pre-existing 0 / undecided 0
wrote-outside-worktree: none
needs-coordinator: whether one SQLite file and prefix holding several tenants is a supported configuration; that decides whether the one finding matters

**1. Tree diff.** `git diff --stat`: `crates/entity-eventlog/tests/adversary_bounded_open.rs | 257 +++` (1 file, insertions only). It is a test file; no existing case changed. The additions are a repeating "rival writer" hook on the wrapper, a helper that provisions a second tenant, and 3 cases.

**2. Cases added** (each first run alone)

| case | asserts | now |
|---|---|---|
| `a_co_tenants_writes_during_a_bounded_read_cost_no_complete_verification` | another tenant in the same file and prefix writes during each read attempt; the read pays no complete verification | red |
| `a_bounded_read_overlapped_by_a_write_on_every_attempt_ends_in_one_complete_verification` | a writer of the same tenant commits during every attempt of a state read; the read ends with 1 capture and 1 model build, and answers what a `FullVerification` handle reads afterwards | green |
| `a_bounded_history_overlapped_by_a_write_on_every_attempt_ends_in_one_complete_verification` | the same test on the history path (`tracked_scoped`) | green |

Red output, verbatim:
```
assertion `left == right` failed: 3 writes to another tenant overlapped the read, which changed nothing it read, and it paid a complete verification of its own tenant: StoreCalls { captures: 1, model_builds: 1, model_advances: 3, records_decoded: 2, scoped_reads: 0 }
  left: (1, 1)
 right: (0, 0)
```
The answer itself was correct: the revision-2 assertion before it passed.

**3. Suite run.** `cargo +1.91.0 test -p entity-eventlog --all-features --locked --no-fail-fast` exited 101: 230 passed, 1 failed, 7 ignored. The only failure is the co-tenant case. Your 7 committed adversary cases, the re-pinned F2 and F3 included, are green. `rustfmt --check` passes on the file, and clippy with `-D warnings` printed no warnings. The log is at `build/scratch/adversary-pass2-suite.log`.

**4. Finding** (covering d68ce28a plus my additions)

| # | verdict / origin | file:line | measured | what reaches it |
|---|---|---|---|---|
| F4 | INFEASIBLE / introduced (d68ce28a) | `crates/entity-eventlog/src/adapter/tracked.rs:200` | 3 zero-event deltas (`model_advances: 3`), then a complete verification of this tenant | Two authorities provisioned into one SQLite path and prefix. The public provisioner accepts this; the case does it. I found no consumer that does. |

- **Why it happens:** Eventlog keeps one continuity mark per prefix, not per tenant. When only another tenant wrote, it answers with a zero-event `AppendDelta` (`eventlog-sqlite/src/durable_capture.rs:905-947`). The correction moves the serial on every verified delta, so the second question treats an empty one as "changed".
- **Fix:** leave the serial where it is when a verified delta carries no events, blobs or row changes for this tenant. An empty delta is the provider stating that nothing of this tenant changed.

**5. Attacked and could not break**
- **F1 (live write between the first continuity question and the row read):** resolved; the attacks below found no gap.
  - **A write between the second question and the serve:** the answer is fully read before the second question, so such a write simply comes later.
  - **A write the second question cannot see:**
    - Within Eventlog's contract there is none. The in-process answer needs equal `data_version`, `total_changes` and schema versions. The durable answer needs an equal mark, which carries a random token, so a write cannot restore it.
    - Outside the contract: raw edits, connections with triggers disabled, and rewrites of the continuity row or journal. All are documented as out of scope. For example, one `UPDATE` that puts the mark back hides any SQL tamper.
  - **A delta or the handle's own write passing as "unchanged":** not possible. The serial moves on every delta, verified or not (`tracked.rs:179`, `:200`, `:208`).
  - **The three-try loop:** it ends, and its fallback really verifies, on both the point path and the per-entity path (the two green cases).
  - **Read paths the re-check misses:** none. Every `rows_*` and `scoped_once` call sits inside `tracked_point` or `tracked_scoped`. Snapshot and recorded refusals are complete verifications. The bridge's Load, LookupRecord, LookupBatch and Histories route through those paths (`sync.rs:810-861`).
- **F2:** the corrected documents now match the code. The re-pinned case still proves the edit took effect: the `FullVerification` open refuses before the bounded lookup serves 7, and a complete read refuses too.
- **F3:** documented. The re-pinned case's hook asserts the discard returned true before the drain overwrote it.
- **Cost, noted only:** a bounded read now opens at least two `BEGIN IMMEDIATE` transactions (`eventlog-sqlite/src/capture.rs:134`). The design states "two continuity questions".

**6. Paths written outside the worktree:** none. Logs are in the worktree's ignored `build/scratch/`. The lease is released.

```findings
- file: crates/entity-eventlog/src/adapter/tracked.rs
  line: 200
  category: concurrency
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: the confirmation counts a verified zero-event delta caused by another tenant of the same file and prefix as a change, so a co-tenant writing during each attempt sends a read whose rows nothing changed into a complete verification of this tenant
```
