---
format: aep.planning-md/3
id: review-result:er-55-u1-adversary-pass-1
kind: review-result
status: active
title: 'Issue 55 U1 (bounded recorded open): adversary pass 1'
relations:
- reviews: story:recorded-open-verifies-a-checkpoint-and-its-suffix
revision: 1
---
unit: story:recorded-open-verifies-a-checkpoint-and-its-suffix, worktree er-55-u1 at af413382 plus one untracked test file
verdict: red (worst finding: blocker, NEEDS-CHANGE)
cases: executed 221→226, red 3
origin: introduced 3 / pre-existing 0 / undecided 0
wrote-outside-worktree: none
needs-coordinator: none

**1. Tree diff.** `git --no-pager diff --stat` is empty, because nothing tracked changed. `git status --short` shows one untracked file: `?? crates/entity-eventlog/tests/adversary_bounded_open.rs` (910 lines). It is a test file. No implementation file was touched.

**2. Cases added** (all in `crates/entity-eventlog/tests/adversary_bounded_open.rs` of the unit's worktree; each was first run alone)

| case | asserts | now |
|---|---|---|
| `a_foreign_sql_write_between_the_continuity_answer_and_the_row_read_is_not_served` | if another SQLite connection writes between the provider's "unchanged" answer and the row read, the read answers the verified state or refuses | red |
| `a_raw_edit_of_a_record_row_is_refused_by_the_read_that_reads_it` | design :395: "a raw edit the provider cannot see is refused by the read that reads it" | red |
| `a_discard_landing_inside_a_live_handles_drain_is_not_overwritten` | the rustdoc promise that a drain "never does over a tombstone it did not load" | red |
| `a_checkpoint_from_before_a_disable_and_re_enable_is_never_continued` | an SQL edit made while continuity is disabled, followed by re-enable: the next open refuses | green |
| `a_bounded_handle_that_moved_never_persists_a_foreign_write_made_before_its_drain` | a drain record never absorbs a foreign write made before the drain | green |

Red output from the first runs, verbatim:
```
a bounded handle served a row a foreign connection wrote after the provider's continuity answer: Ok(Some(1)); the handle's own next read: Err(ProviderIntegrity { provider: "eventlog", detail: "projection er_subjects_v1 differs from authoritative events" })
the edited record row was served instead of refused: Ok(Some((RecordPosition { subject: 7, store: 3 }, RecordPosition { subject: 7, store: 3 })))
the live handle's drain wrote over the operator's tombstone (wrote: true); the next open was Checkpoint instead of complete
```
- The raw-edit case first checks that a `FullVerification` open refuses the edited file. That check passed, so the edit did take effect.
- Two cases run SQL through the `sqlite3` shell, as `review_open_checkpoint.rs` does. I did not mark them `#[ignore]`.
- After the first runs I applied `rustfmt` to my file only. Clippy `-D warnings` then exited 0.

**3. Suite run.** `cargo +1.91.0 test -p entity-eventlog --all-features --locked --no-fail-fast` exited 101: 223 passed, 3 failed, 7 ignored. Every other target is green. The log is at `build/scratch/adversary-suite-full.log`.

**4. Findings** (covering af413382)

| # | verdict / origin | file:line | measured | what reaches it |
|---|---|---|---|---|
| F1 | NEEDS-CHANGE / introduced | `crates/entity-eventlog/src/adapter/tracked.rs:506` (`rows_state` after `continue_tracked`) | revision 1 served where revision 2 was verified; the handle's next read refuses that same row | Any read of state, record or batch on a checkpoint-opened handle, store or facade path, while another SQLite connection writes. The design (:107) says such a live write is caught. |
| F2 | CONFIRMED / introduced | `docs/design/recorded-open-checkpoint-v0.1.md:395`; `website/docs/concepts/storage.md:126` | a raw edit to the record row is served, because only blob reads are digest-checked | A raw file edit or media damage, then a tracked open of an enabled store. R-151, AGENTS.md and CHANGELOG state the limit correctly. |
| F3 | INFEASIBLE / introduced | `crates/entity-eventlog/src/adapter.rs:1111-1120`, rustdoc :1146 and `sync.rs:260` | the drain overwrote the operator's tombstone | Only an operator who runs a discard during another process's drain, which the docs already forbid. Harm not shown: the overwriting record is valid for the same file. |

- **F1 fix:** after reading the row and its blob, ask the provider again and serve the answer only if it still says the store is unchanged. Otherwise go through `continue_tracked` again.
- **F2 fix:** change those two doc lines to say that only blob edits are refused at read time.
- **F3 fix:** document the limit. Eventlog's snapshot generation is created once and never changes (`eventlog-sqlite/src/lib.rs:2040-2067`), so `save_snapshot_checked` gives no compare-and-set, and the check-then-write cannot be made atomic through this port.

**5. Attacked and could not break**
- **Your requested check:** the `review_open_checkpoint` change from 2 captures to 1 matches Eventlog 0.8.0 (`carry_continuity`/`neutral`, lib.rs:2052-2063) and was not loosened beyond that.
- **Checkpoint record:**
  - A record replayed, from another authority, or with a wrong digest is refused, a fallback, or verified as a suffix.
  - A forged position above the head with genuine provider bytes opens without refusing. Only a writer with SQL access can build that, and the design limits that refusal to the complete-capture path.
- **Disable/re-enable:** red-tested with an SQL edit in between; the next open refuses.
- **Foreign write before the drain:** red-tested; the next open refuses.
- **Write paths:** an `er.refused_request` or a standalone `put_blob` in the suffix ends continuity. A write's own read-back is verified as a suffix. The usage feeding `admit_growth` matches what a whole-model handle counts.
- **Projection rebuilds** copy rows back into the triggered table, so the next open is complete.
- **Two handles draining at different positions:** the lower handle writes nothing in sequence. The race between the two drains is acknowledged in the design (`recorded-open-checkpoint-v0.1.md:242-246`).

**6. Paths written outside the worktree:** none. Logs are in the worktree's ignored `build/scratch/`. The lease is released.

```findings
- file: crates/entity-eventlog/src/adapter/tracked.rs
  line: 506
  category: concurrency
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a checkpoint-opened handle reads the index row in a separate transaction after the provider's unchanged answer, so a foreign SQL write between them is served as verified state (revision 1 served, next read refuses)
- file: docs/design/recorded-open-checkpoint-v0.1.md
  line: 395
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: the design and website/docs/concepts/storage.md:126 claim a raw edit is refused by the read that reads it, but a raw edit of a record row is served because only blobs are digest-checked
- file: crates/entity-eventlog/src/adapter.rs
  line: 1120
  category: concurrency
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: a discard landing between the drain's persisted-record check and its snapshot write is overwritten, contradicting the rustdoc promise; no harmful reachable state shown since Eventlog's snapshot generation gives no compare-and-set
```
