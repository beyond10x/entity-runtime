---
format: aep.planning-md/1
id: review-result:recorded-eventlog-adapter-source-pass-2
kind: review-result
status: active
title: 'Final complete adapter source examination: terminal admission coordination'
relations:
- reviews: story:eventlog-recorded-adapter-and-bridge
revision: 1
---
unit: final whole ER Eventlog adapter consumer source pass 2 of 2 at `3190afe02abd2685613c31512c89ba47dba706e0` (tree `d8aa5e9e6774efcb36b6380ff18eda9b2025294f`, accepted base `250f6993181822ab1e36c17d38dbc909d084d423`)
verdict: NEEDS-CHANGE
cases: executed 28, red 0; one source-proven panic/admission finding has no claimed executed reproducer
origin: introduced 1 / pre-existing 0 / undecided 0
wrote-outside-worktree: one evidence directory, including isolated target and TMPDIR
needs-coordinator: correct F3 before accepting the consumer bridge; retain B-ADMIN-REVIEW and the four provider-private native-stage gaps

---

## Candidate and source disposition

I reviewed the entire 21-path adapter delta from accepted Entity Runtime
`250f6993181822ab1e36c17d38dbc909d084d423` to composed candidate
`3190afe02abd2685613c31512c89ba47dba706e0`. The candidate has parents
`424a788a2b1e0f7cb5d9cff91ba90ce3d0c80b7c` and
`250f6993181822ab1e36c17d38dbc909d084d423`; both are ancestors. The checkout
started and ended clean. `git diff --check` exits 0.

`candidate-source.sha256` is the complete 21-path source/test/manifest/design/workflow manifest,
SHA-256 `4ee162534597143e094bd6cb12cfd1180f8b115ac0b70be439b78abf81d9f0f1`.
All 21 rows reverified against this checkout, exit 0. This includes the unchanged 442-line first
reviewer target at SHA-256
`43e8501a42c99f802c9a938bf71a867a43a38b4132f82bc477cd1f1a178aaf19` and the service/3
composition target at SHA-256
`c3a051010d9d3f7f251a8dfc66478b701da7933aba7c12bfac5d39d17ff38ec1`.

I made no candidate source, test, expected-value, manifest, lockfile, design, workflow, AEP,
Eventlog-provider, provider-administration or root-control change. No additive test was left in the
reviewer tree. `review-inputs.sha256`, SHA-256
`b60806225fb13ebb3610f240f931870efa292012bfcb3d8fef08795047b3e198`, records the brief,
AGENTS, complete accepted design set and all predecessor reports/matrices read for this pass.
`evidence.sha256`, SHA-256
`910ad9e6e01651358b2da3aa8b90450adf8483fe2ac3b008058d0a5afc4d1fec`, covers the
non-disposable evidence other than this immutable report.

## First-review findings

F1 and F2 are resolved without weakening their byte-exact reviewer assertions.

- The `UnknownCommit` binding path now preserves a complete
  `ProvisionBindingFailure::Conflict { requested, found }`; it keeps `Uncertain::UnknownCommit`
  only for absent or unavailable recovery. The unchanged F1 test passes.
- Binding recovery discovers the observed tuple, validates its tenant/generation, reconstructs the
  complete captured event/blob model and exact four projection sets against that tuple, and only
  then reports a foreign conflict. The unchanged F2 test passes.

The exact two-case target executed 2 passed, 0 failed. I also traced matching, foreign, corrupt,
ordinary-history and imported-history recovery through the correction's whole-class tests and
retained causal controls. I found no assertion loss or early foreign-conflict return in the final
source.

## F3 — a concurrent admission can be stranded when another request panics the worker

`RecordedEventlogBridge` is `Sync`; `bridge-sync-probe.rs` type-checks two safe concurrent public
`&RecordedEventlogBridge` callers and an explicit `RecordedEventlogBridge: Sync` bound under
Rust 1.91. This establishes the public caller shape. This finding does **not** rely on racing
`shutdown(&mut self)` or `Drop`: safe Rust's exclusive owner borrow prevents that interleave while
shared calls exist.

The reachable autonomous-panic schedule is:

1. Public caller A has a request in `DISPATCHED`. Public caller B enters another public read or
   write and observes `RUNNING` at `crates/entity-eventlog/src/sync.rs:1042`.
2. A's production driver future panics. `drive_request` catches it and returns `true`; the worker
   stores `CLOSING_CANCEL` at `:1349`, breaks, and performs its one nonblocking receiver drain at
   `:1359-1365`.
3. B resumes after that drain, increments `queued` at `:1045`, and calls `try_send` at `:1046`.
   During provider retirement at `:1366`, the `Receiver` is still alive, so `try_send` may return
   `Ok(())` even though the worker will never read the request.
4. The request is dropped when `worker_main` returns. Its separate caller-held result cell remains
   `QUEUED` and is never completed or notified. `CallWait::Forever` hangs; a finite wait eventually
   reports `DeadlineBeforeDispatch` instead of the required `WorkerStoppedBeforeDispatch`.

The accepted bridge design requires a panic to close admission, cancel every queued request, and
wake every waiter; it explicitly says a Forever caller must not hang after owner unwind
(`eventlog-recorded-sync-bridge-v0.1.md:242-253`). The current lifecycle check and queue insertion
are not coordinated with the worker's final drain. Existing panic tests enqueue their second cells
before releasing the panic barrier, so the one drain sees them; they do not cover an admission that
already read `RUNNING` and sends after the drain. The focused 25-case sync suite consequently
passes without disproving this schedule.

A deterministic public-only failure was not available: public code cannot inject a production
future panic or pause precisely after the lifecycle read, and the charter forbids adding a public
injection seam or editing production behavior. I therefore claim source reachability plus the
compiled public concurrency bound, not an executed assertion failure. A timing-loop result would
not have been honest evidence. The correction needs to make admission and terminal receiver drain
one coordinated protocol so every caller that passes the running check either reaches the worker
or receives a terminal rejection; I did not apply that repair.

The defect is introduced relative to accepted base `250f699`, where `crates/entity-eventlog` does
not exist. The same uncoordinated lifecycle-load/queue-increment shape was already present in the
original adapter candidate `a973af1`; pass 2 found it in the final composed candidate rather than
in service/3 composition or the F1/F2 correction.

**Verdict `NEEDS-CHANGE`, origin `introduced`, severity `blocker`.** The consequence is an
unbounded public caller hang on the exact in-process panic-containment path the bridge promises to
make terminal.

## Complete review coverage

I traced the full consumer source and its changed supporting seams, not only F1/F2 and F3:

- Canonical `er.record/*`, `er.request/*` and `er.batch/1` byte retention; closed binding, entry and
  anchor wrappers; domain-separated hashes; arbitrary-precision JSON handling; authority,
  `Subject` and `BatchKey` namespace preservation; and literal reference vectors.
- Immutable logical-scope/tenant/generation binding, validate-only attachment, complete native
  capture, unknown/redacted/foreign-event refusal, referenced blob validation, and exact equality
  of the four fixed projection row sets.
- Transaction-local binding/batch/global-record/subject locks, deterministic lock order, repeated
  subjects, mixed decisions and observations, zero-domain-event records, actual physical receipts,
  final batch membership and semantic retry/conflict/uncertainty precedence.
- Imported-anchor pure validation, available-evidence versus complete-subject assurance, envelope
  record identity, no fabricated request/batch/receipt chronology, exact replay, ordinary/import
  races and post-anchor suffix verification.
- Ordinary reads, lookup, mixed history and complete snapshot reconstruction, including exact
  reopen behavior and current-state validation through the accepted ER verifier.
- The synchronous facade's provider/runtime ownership, bounded queue, cancellation/dispatch CAS,
  read versus write uncertainty, error passthrough, reentrancy, current-thread operation, panic
  classification, drain/cancel escalation, retained join and provider retirement. F3 is the one
  uncovered terminal-admission gap.
- Composed service/3 `Set`/`Preserve`/`Remove`, exact `er.record/4` and `er.request/4` bytes,
  changed-action conflicts, complete verified history, named atomic batches, restart and bridge
  reopen. The exact SQLite bridge composition selector passes.
- Rust 1.91 adapter closure, pure-workspace Rust 1.85 exclusion, exact Eventlog Git pin
  `43ceaa09ceec610e25891815e33e03e8df92ee28`, optional provider features, dependency boundary,
  task/workflow selection and accepted requirements pin.

No other concrete source defect was found.

## Executed checks

Every Cargo command used the isolated review target and TMPDIR, Rust 1.91, `--locked --offline`,
one job, lld, empty compiler wrappers, disabled debug information and disabled incremental
compilation. Preflights recorded 25-26 GiB free disk and more than 42 GiB available memory, above
the 20/16 GiB stops.

| Check | Result | Evidence |
|---|---|---|
| Exact unchanged first-review binding target | 2 passed, exit 0 | `logs/01-reviewer-binding.log`, SHA-256 `2cef79e2d01889cadf9da0df9fd92022921541201a78be20c7dc72c326148db5` |
| Exact public SQLite service/3 bridge retry/history/reopen selector | 1 passed, exit 0 | `logs/02-service3-sqlite-bridge.log`, SHA-256 `b95a9d5e478d7e6b84a4948f569f5364975f419faa380cf1d615222bb6d3e8c9` |
| Focused production sync worker suite | 25 passed, exit 0 | `logs/03-sync-production-worker-suite.log`, SHA-256 `fecae693f033e5289ed1b37dc6d5cf3eeb39cafc6396e12ac6077c7dc0de43ac` |
| Rust 1.91 public bridge Sync/concurrent-caller compile probe | exit 0 | `bridge-sync-probe.rs`, SHA-256 `a0ffb58200c03368deefbc13b6a4f4f5df896845778aa72c016b2e61dd77a671`; empty-success log plus exit file |
| Complete 21-row candidate source manifest recheck | 21 OK, exit 0 | `logs/05-candidate-source-recheck.log`, SHA-256 `29524273d8cfee5f1d0746b0e96420dd28d84aba94aa03e75b7e55fdafebee2b` |
| Commit/tree/parents/reachability/origin, diff check and clean status | expected values; exits 0 except expected absent-base path 128 | `logs/06-origin-status.log`, SHA-256 `4cfef05f764c5ca8ac47ad2ec6fff03a147947849e1055d76a9b01ac373ceb12` |

The retained composed-candidate full `task check` with assigned PostgreSQL is SHA-256
`df31441a7fc6888fde29fd3a8b3fc856469902fd622d965991e2dddae9f488b0`, exit 0. It includes the
actual PostgreSQL stages and all five service/3 provider/bridge cases. The retained pure Rust 1.85
build is SHA-256 `392c51d66c466505a344211eacb6393e34886012464e0b6ce2e4a6e6e7253429`, exit 0. I did not
repeat either broad gate in this focused review lane and make no new PostgreSQL execution claim.

Exact commands for the three Cargo checks were the following common environment plus the shown
selector:

```console
CARGO_TARGET_DIR=<review>/target CARGO_NET_OFFLINE=true CARGO_INCREMENTAL=0 \
CARGO_PROFILE_TEST_DEBUG=0 RUSTC_WRAPPER= RUSTC_WORKSPACE_WRAPPER= \
RUSTFLAGS='-C link-arg=-fuse-ld=lld' TMPDIR=<review>/tmp \
cargo +1.91.0 test -p entity-eventlog --all-features --locked --offline --jobs 1 ...
```

Selectors were respectively `--test reviewer_binding_unknown_conflict -- --nocapture`,
`--test service_3_composition sqlite_bridge_crosses_service_3_retry_history_and_reopen -- --exact --nocapture`,
and `--lib sync::tests -- --nocapture`.

## Retained external limits

`B-ADMIN-REVIEW` remains unresolved. I did not enter the denied administration reviewer tree,
execute its unexecuted cases, inspect provider source or retry that review. The four provider-private
native stages remain unverified and are not relabelled as consumer evidence:

1. stopping after an individual event/group-range persistence step inside `append_group_guarded`;
2. stopping before or after private idempotency/group bookkeeping;
3. crashing immediately before native commit and distinguishing rollback; and
4. crashing immediately after native commit but before the provider constructs its reply.

These external limits are independent of F3, which is wholly in the submitted consumer worker and
public synchronous caller contract.

## Paths written outside the reviewer tree

All writes are under
`~/beyond10x/.ess-evolution/waves/0007-er-eventlog-adapter/source-review-2/`:

- `candidate-source.sha256`, `review-inputs.sha256`, `evidence.sha256`, `bridge-sync-probe.rs`, and
  this `report.md`;
- `logs/01-reviewer-binding.{log,exit}` through
  `logs/06-origin-status.{log,exit}` (with the numbered names shown in the check table); and
- isolated disposable `target/` and `tmp/`; `tmp/bridge-sync-probe` is the compiled diagnostic.

No process remains live. The focused build lane was released before report finalization.

## Findings block

```findings
- file: crates/entity-eventlog/src/sync.rs
  line: 1042
  category: concurrency
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a concurrent public submission can observe Running before another dispatched request panics, then enqueue after the worker's one-shot terminal drain while its Receiver remains alive during retirement; the result cell is never completed, so CallWait::Forever hangs instead of receiving WorkerStoppedBeforeDispatch
```
