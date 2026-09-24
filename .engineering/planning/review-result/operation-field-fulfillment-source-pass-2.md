---
format: aep.planning-md/2
id: review-result:operation-field-fulfillment-source-pass-2
kind: review-result
status: active
title: 'Final operation-field source examination: legacy event removal refusal'
owner: Independent source reviewer
relations:
- reviews: story:service-operation-field-fulfillment
revision: 1
---
unit: complete operation-field fulfillment at a66122d4039ef5f04328f5016e21af2b97b30d6e against da5d368756f5a63e4b2efd5589f7bc3441cd7aff
verdict: needsrevision
cases: executed 0→1, red 1
origin: introduced 1 / pre-existing 0 / undecided 0
wrote-outside-worktree: 6 paths
needs-coordinator: route the kernel/1 event-removal finding for bounded correction and retain the reviewer regression; this was final pass 2 of 2

```text
 .../tests/operation_fulfillment_review_2.rs        | 52 ++++++++++++++++++++++
 1 file changed, 52 insertions(+)
```

## 2. Reviewer case

`crates/entity-core/tests/operation_fulfillment_review_2.rs:9`
`a_kernel_event_cannot_invent_service_3_removal_evidence` creates and closes a real `kernel/1`
entity through `Runtime`, verifies that the honest close event has no removal evidence, changes only
that event's new `removed` field to `{"note"}`, and requires the public `rehydrate` API to refuse
evidence no legacy operation can produce. The case is compile-valid and red now. Its exact SHA-256
is `5805028cea10ee5a2886bc6dffaa51451d002ad77b225c056073c7472ccc33f5`.

Isolated first run, before the reviewer suite:

```text
COMMAND: env TMPDIR=<review-evidence>/tmp CARGO_TARGET_DIR=<worktree>/target/operation-field-fulfillment-review-2 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 RUSTC_WRAPPER= RUSTC_WORKSPACE_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER= RUSTFLAGS='-C linker=clang -C link-arg=-fuse-ld=lld -C debuginfo=0' RUST_TEST_THREADS=1 cargo +1.98.1 test --offline --locked -j 1 -p entity-core --test operation_fulfillment_review_2 a_kernel_event_cannot_invent_service_3_removal_evidence -- --exact --nocapture
   Compiling proc-macro2 v1.0.107
   Compiling unicode-ident v1.0.24
   Compiling quote v1.0.47
   Compiling serde_core v1.0.229
   Compiling zmij v1.0.23
   Compiling syn v3.0.4
   Compiling serde v1.0.229
   Compiling serde_json v1.0.151
   Compiling serde_derive v1.0.229
   Compiling itoa v1.0.18
   Compiling memchr v2.8.3
   Compiling entity-core v0.18.1 (<review-worktree>/crates/entity-core)
   Compiling scan-support v0.18.1 (<review-worktree>/crates/scan-support)
    Finished `test` profile [unoptimized] target(s) in 9.99s
     Running tests/operation_fulfillment_review_2.rs (target/operation-field-fulfillment-review-2/debug/deps/operation_fulfillment_review_2-228f2b05ea62a875)

running 1 test
test a_kernel_event_cannot_invent_service_3_removal_evidence ...
thread 'a_kernel_event_cannot_invent_service_3_removal_evidence' (1721884) panicked at crates/entity-core/tests/operation_fulfillment_review_2.rs:51:10:
kernel/1 has no action that can produce removal evidence: EntityInstance { entity: "ticket", version: 1, id: "t-1", lifecycle_state: "closed", revision: 2, fields: {"title": String("review")} }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
FAILED

failures:

failures:
    a_kernel_event_cannot_invent_service_3_removal_evidence

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p entity-core --test operation_fulfillment_review_2`
EXIT_STATUS: 101
```

The placeholders in the retained command line expand to
`<review-evidence>/tmp`
and
`<review-worktree>`,
respectively. The complete raw output is `focused-isolated-regression.log`, SHA-256
`69e11802ecba8acc7d762aae3cb8e28f3816fc078c3b53af726e3b9e5235dd93`.

## 3. Dedicated reviewer suite

The suite ran only after the case above existed:

```text
COMMAND: env TMPDIR=<review-evidence>/tmp CARGO_TARGET_DIR=<worktree>/target/operation-field-fulfillment-review-2 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 RUSTC_WRAPPER= RUSTC_WORKSPACE_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER= RUSTFLAGS='-C linker=clang -C link-arg=-fuse-ld=lld -C debuginfo=0' RUST_TEST_THREADS=1 cargo +1.98.1 test --offline --locked -j 1 -p entity-core --test operation_fulfillment_review_2 -- --nocapture
    Finished `test` profile [unoptimized] target(s) in 0.07s
     Running tests/operation_fulfillment_review_2.rs (target/operation-field-fulfillment-review-2/debug/deps/operation_fulfillment_review_2-228f2b05ea62a875)

running 1 test
test a_kernel_event_cannot_invent_service_3_removal_evidence ...
thread 'a_kernel_event_cannot_invent_service_3_removal_evidence' (1725920) panicked at crates/entity-core/tests/operation_fulfillment_review_2.rs:51:10:
kernel/1 has no action that can produce removal evidence: EntityInstance { entity: "ticket", version: 1, id: "t-1", lifecycle_state: "closed", revision: 2, fields: {"title": String("review")} }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
FAILED

failures:

failures:
    a_kernel_event_cannot_invent_service_3_removal_evidence

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p entity-core --test operation_fulfillment_review_2`
EXIT_STATUS: 101
```

The complete raw output is `focused-dedicated-suite.log`, SHA-256
`48a2414fc675bbe9a3b3a2202cb3acbfc3ef19ce1915c5895ed3c4991cb4ca29`.
No workspace, PostgreSQL, MSRV, Clippy, format, rustdoc, or further Cargo command was run by this
reviewer. The author and correction retained affected, Rust 1.85, strict, full actual-PostgreSQL,
causal-control and common-receipt evidence was examined rather than re-executed; exact excerpts and
exits are retained in `source-examination.log`.

## 4. Grouped finding

### Legacy event provenance and unchanged kernel behavior

| Coordinate | Verdict / origin | Actual caller and permitted input | Observed source/result | Requirement and bounded class |
|---|---|---|---|---|
| `crates/entity-core/src/replay.rs:484` | `NEEDS-CHANGE` / `introduced` | `entity_core::rehydrate` is publicly exported at `crates/entity-core/src/lib.rs:146` and accepts caller-supplied `DomainEvent` values. The amendment publicly adds serde-readable `DomainEvent::removed` at `crates/entity-core/src/runtime.rs:85-87`. The reviewer case reaches the fold with two honest kernel events and changes only the close event's `removed` set. | Legacy writer reconstruction accepts a candidate solely when its recomputed `changed` equals the event at `crates/entity-core/src/replay.rs:714-728`; it never proves `removed` is empty. The fold checks only changed/removed disjointness, then applies the unproven removal at `:474-484`. Exit 101 shows the call returned `EntityInstance { fields: {"title": ...} }`, deleting the previously present optional `note`. | Unit deliverable 3 requires event-only legacy rehydration to reconstruct exact new evidence or explicitly refuse an unsupported carrier, and deliverable 4 keeps `kernel/1` behavior unchanged. A kernel operation has no fulfillment action and every honest kernel event records an empty removal set. The complete affected class is any nonempty `removed` evidence on a supported kernel event-only history: creation or operation revision, including every member of a multi-event revision. Service semantics remain outside this class because `rehydrate` already refuses all of them at `:228-239`; recorded `service/3` replay remains valid. |

The defect is introduced by this amendment. The accepted base has no `DomainEvent::removed` field
and folds only kernel-authored `changed` values; the new diff adds both the input and its unchecked
application. A bounded correction can preserve the existing service-history refusal and reject
nonempty removal evidence throughout kernel event histories before applying it.

## 5. Attacked and not broken

- Exact HEAD `a66122d4039ef5f04328f5016e21af2b97b30d6e`, tree
  `003c0246eaf49474a38584f23b3d66485d6db41a`, and all 23 submitted paths matched frozen manifest
  SHA-256 `d3b9db632dcf1074358e24e131e7e87c53cf4adbbe6b7fdcf92864d8fe914e69`;
  the accepted base is an ancestor and submitted `git diff --check` exited 0.
- Registration retains a closed ordered fulfillment map behind `service/3` and accumulates old
  semantics, creation/refusal placement, unknown/identity target, ordinary-set conflict, and field
  presence defects.
- Post-load selection checks the exact subject, state, ordered outcome, refusal and preconditions
  before yielding an opaque outcome. Completion owns the selected definition/input/instance and
  one resulting map drives schema, identity, invariants, events, response and the decision.
- Exact keys, Set validation, Preserve presence/absence, required Remove refusal, optional Remove,
  zero/one/many events, and disjoint decision/event removal evidence are covered by the submitted
  cases and matching pure source paths.
- Recorded `service/3` replay supplies saved actions through the same continuation and compares the
  complete record. Complete-store verification, snapshots and restart paths retain removal state.
- Explicit `service/3`, `er.record/4` and `er.request/4` framing is isolated from prior literal
  domains and `er.batch/1`; the first-review `/1`-through-`/3` retry omission is corrected by the
  nonempty-map conflict guard without adding keys to old bytes.
- Event-only service histories are still refused before reading an event. The finding does not
  weaken that boundary or the valid recorded-service replay path.
- The retained final source gate reports affected suites, strict Clippy, format, rustdoc, pure Rust
  1.85, full actual PostgreSQL provider tests, requirement checks, original eight controls and the
  retry-guard red/restored control. Those records were inspected and are not claimed as commands
  executed by this reviewer.

## 6. Paths written outside the worktree

- `<review-evidence>/review-2.md`
- `<review-evidence>/focused-isolated-regression.log`
- `<review-evidence>/focused-dedicated-suite.log`
- `<review-evidence>/source-examination.log`
- `<review-evidence>/whole-source.diff`
- `<review-evidence>/tmp/` (empty reviewer scratch/TMPDIR)

The Cargo target was worktree-local at
`<review-worktree>/target/operation-field-fulfillment-review-2`.

```findings
- file: crates/entity-core/src/replay.rs
  line: 484
  category: contract-drift
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: kernel/1 event rehydration applies a nonempty removed set that no legacy operation can author, so forged removal evidence deletes an optional field instead of being refused
```
