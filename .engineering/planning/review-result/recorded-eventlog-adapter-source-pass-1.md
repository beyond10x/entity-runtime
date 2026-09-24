---
format: aep.planning-md/2
id: review-result:recorded-eventlog-adapter-source-pass-1
kind: review-result
status: active
title: 'Complete recorded adapter source review: binding recovery classification'
owner: Independent source reviewer
relations:
- reviews: story:eventlog-recorded-adapter-and-bridge
revision: 1
---
unit: complete ER Eventlog adapter consumer source at `a973af1cfcbaf7c06420ca5c7cf503aa98144d04` (tree `889cc465d4ddad55fb7f9f4a7482d401b784d706`, base `da5d368756f5a63e4b2efd5589f7bc3441cd7aff`) plus my additive reviewer test
verdict: NEEDS-CHANGE
cases: executed 48→50, red 2
origin: introduced 2 / pre-existing 0 / undecided 0
wrote-outside-worktree: 12 paths
needs-coordinator: PostgreSQL was unassigned for my package-suite run; retain the author's actual PostgreSQL evidence and the four explicitly excluded provider-private stage gaps

---

## 1. `git --no-pager diff --stat`

After marking the one untracked reviewer file intent-to-add so the required command includes it:

```text
 .../tests/reviewer_binding_unknown_conflict.rs     | 442 +++++++++++++++++++++
 1 file changed, 442 insertions(+)
```

`git status --short`:

```text
 A crates/entity-eventlog/tests/reviewer_binding_unknown_conflict.rs
```

Every changed path is a test file. I made no implementation, manifest, design, workflow, AEP,
root-control, provider-administration, or existing-test edit. `git diff --check` and standalone
Rust 1.91 rustfmt check both exit 0.

The test file is
`crates/entity-eventlog/tests/reviewer_binding_unknown_conflict.rs`, 442 lines, SHA-256
`43e8501a42c99f802c9a938bf71a867a43a38b4132f82bc477cd1f1a178aaf19`.

The submitted 19-file manifest remains exact after my additive test:
`submitted-source.sha256` SHA-256
`8a3b201b273a1a4977843b958ef7ff12f9f7ae5b173840608b0f23a28cbf328d`;
all 19 rows rechecked OK, exit 0. The submitted commit and tree match the brief and the checkout
started clean. Both findings are introduced: `crates/entity-eventlog/src/adapter.rs` does not
exist at base `da5d368756f5a63e4b2efd5589f7bc3441cd7aff`.

## 2. Cases added and run alone before the suite

Both cases use the public consumer traits over a real in-memory SQLite Eventlog provider. The
wrapper only controls the returned consumer boundary: it first lets a different complete binding
win, or removes the captured derived row. It does not inspect or change denied Eventlog
administration source.

### 2.1 Ambiguous response followed by a conclusive foreign winner

`crates/entity-eventlog/tests/reviewer_binding_unknown_conflict.rs:327`,
`unknown_commit_recovery_preserves_a_conclusive_foreign_binding_conflict`, asserts that a
post-append `UnknownCommit` followed by complete recovery of a different immutable authority
returns the exact typed `ProvisionBindingFailure::Conflict { requested, found }`.

Command:

```console
env -u CARGO_TARGET_DIR CARGO_NET_OFFLINE=true CARGO_INCREMENTAL=0 +  CARGO_PROFILE_TEST_DEBUG=0 RUSTC_WRAPPER= RUSTC_WORKSPACE_WRAPPER= +  RUSTFLAGS='-C link-arg=-fuse-ld=lld' +  TMPDIR=<review-evidence>/tmp +  cargo +1.91.0 test -p entity-eventlog --all-features +  --test reviewer_binding_unknown_conflict --locked --offline --jobs 1 +  unknown_commit_recovery_preserves_a_conclusive_foreign_binding_conflict +  -- --exact --nocapture
```

First isolated run, exit **101**, verbatim failure:

```text
running 1 test

thread 'unknown_commit_recovery_preserves_a_conclusive_foreign_binding_conflict' (1746207) panicked at crates/entity-eventlog/tests/reviewer_binding_unknown_conflict.rs:370:13:
assertion `left == right` failed
  left: Err(Uncertain { authority: Authority { logical_scope: "requested-scope", tenant: "reviewer-binding-conflict", stream_identity: "01a0aa12-a0d4-71ed-82af-de416bd891d6" }, cause: UnknownCommit })
 right: Err(Conflict { requested: Authority { logical_scope: "requested-scope", tenant: "reviewer-binding-conflict", stream_identity: "01a0aa12-a0d4-71ed-82af-de416bd891d6" }, found: Authority { logical_scope: "foreign-winner-scope", tenant: "reviewer-binding-conflict", stream_identity: "01a0aa12-a0d4-71ed-82af-de416bd891d6" } })
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test unknown_commit_recovery_preserves_a_conclusive_foreign_binding_conflict ... FAILED

failures:
    unknown_commit_recovery_preserves_a_conclusive_foreign_binding_conflict

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.01s
```

### 2.2 A foreign binding is not valid until its required row validates

`crates/entity-eventlog/tests/reviewer_binding_unknown_conflict.rs:381`,
`a_foreign_binding_is_not_a_conflict_until_its_derived_row_is_validated`, provisions a foreign
binding with the real provider, removes only its captured binding row, then asks public
`recover_binding` for another authority. It asserts provider integrity rather than accepting the
incomplete authority as a typed conflict.

Command uses the same environment and target with this exact selector:

```console
cargo +1.91.0 test -p entity-eventlog --all-features +  --test reviewer_binding_unknown_conflict --locked --offline --jobs 1 +  a_foreign_binding_is_not_a_conflict_until_its_derived_row_is_validated +  -- --exact --nocapture
```

First isolated run, exit **101**, verbatim failure:

```text
running 1 test

thread 'a_foreign_binding_is_not_a_conflict_until_its_derived_row_is_validated' (1747030) panicked at crates/entity-eventlog/tests/reviewer_binding_unknown_conflict.rs:432:13:
a foreign event without its derived binding row is corrupt, not a valid conflict: Err(Conflict { requested: Authority { logical_scope: "requested-scope", tenant: "reviewer-binding-row", stream_identity: "01a0aa12-d06f-70a0-a5c2-01f19a49ae66" }, found: Authority { logical_scope: "foreign-scope", tenant: "reviewer-binding-row", stream_identity: "01a0aa12-d06f-70a0-a5c2-01f19a49ae66" } })
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test a_foreign_binding_is_not_a_conflict_until_its_derived_row_is_validated ... FAILED

failures:
    a_foreign_binding_is_not_a_conflict_until_its_derived_row_is_validated

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.01s
```

The dedicated two-case target was then run once, after both isolated runs. Both cases failed for
the same asserted results, exit 101.

## 3. Adapter package suite after both cases existed

Command:

```console
env -u CARGO_TARGET_DIR CARGO_NET_OFFLINE=true CARGO_INCREMENTAL=0 +  CARGO_PROFILE_TEST_DEBUG=0 RUSTC_WRAPPER= RUSTC_WORKSPACE_WRAPPER= +  RUSTFLAGS='-C link-arg=-fuse-ld=lld' +  TMPDIR=<review-evidence>/tmp +  cargo +1.91.0 test -p entity-eventlog --all-features +  --locked --offline --jobs 1
```

Exit **101**. The suite results, verbatim:

```text
running 18 tests
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 23 tests
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.48s

running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.61s

running 2 tests
test a_foreign_binding_is_not_a_conflict_until_its_derived_row_is_validated ... FAILED
test unknown_commit_recovery_preserves_a_conclusive_foreign_binding_conflict ... FAILED

failures:
    a_foreign_binding_is_not_a_conflict_until_its_derived_row_is_validated
    unknown_commit_recovery_preserves_a_conclusive_foreign_binding_conflict

test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

error: test failed, to rerun pass `-p entity-eventlog --test reviewer_binding_unknown_conflict`
```

The implementing completion's actual pre-review adapter count was 18 unit/bridge + 23 fault + 7
provider/runtime = 48. All 48 existing functions remained green and both new functions were
selected, giving 50 executed and 2 red. `ENTITY_POSTGRES_URL` was unassigned in my environment:
the two `when_assigned` PostgreSQL functions count as executed Rust tests but returned at their
explicit environment guard. I make no new PostgreSQL execution claim; the author's retained final
gate is the provider evidence.

## 4. Findings

### F1 — UnknownCommit recovery discards a conclusive foreign binding conflict

| | |
|---|---|
| **what was measured** | `crates/entity-eventlog/tests/reviewer_binding_unknown_conflict.rs:370`, isolated exit 101: recovery returned `Uncertain { cause: UnknownCommit }` after the real provider durably exposed the complete foreign winner, where the contract requires `Conflict { requested, found }` |
| **what reaches it** | public `EventlogBindingProvisioner::provision_binding`; Eventlog's public append contract permits `UnknownCommit`, and a competing provisioner can win the immutable singleton before semantic recovery captures the tenant |

At `crates/entity-eventlog/src/adapter.rs:1047-1057`, the `UnknownCommit` branch accepts only
`Ok(Some(exact requested authority))`; its wildcard converts `recover_inner`'s already typed,
complete `ProvisionBindingFailure::Conflict` into uncertainty. This contradicts the accepted
adapter design: after `UnknownCommit`, equal originals replay, a valid different binding returns
`Conflict`, and only unresolved authority stays uncertain. It also discards the exact found tuple
the caller needs to diagnose the immutable winner.

Named, not applied: preserve a conclusive `recover_inner` conflict in the `UnknownCommit`
recovery match while retaining uncertainty for absent or unavailable recovery.

**Verdict `NEEDS-CHANGE`, origin `introduced`, severity `blocker`.**

### F2 — binding recovery returns Conflict before validating the foreign authority's derived row

| | |
|---|---|
| **what was measured** | `crates/entity-eventlog/tests/reviewer_binding_unknown_conflict.rs:432`, isolated exit 101: a complete capture containing the foreign binding event/blob but no required binding row returned `Conflict` instead of `NotCommitted(ProviderIntegrity)` |
| **what reaches it** | public `recover_binding` over the public `EventlogBackend` capability; any custom provider or corrupted native projection can return this semantically incomplete capture. No ordinary built-in provider writer producing the missing row was found |

At `crates/entity-eventlog/src/adapter.rs:1142-1146`, recovery returns `Conflict` as soon as the
event blob decodes to another authority. The exact projection-set validation at
`:1154-1157` is therefore unreachable for every foreign tuple. The accepted contract permits a
typed conflict only for a *valid* different binding; missing or malformed binding rows are provider
integrity failures. Existing mutation coverage checks missing rows only when the requested and
observed authorities match, so it does not exercise this early return.

Named, not applied: retain the observed tuple, validate its complete binding model and exact
projection set, then return the typed foreign conflict only after that validation succeeds.

**Verdict `NEEDS-CHANGE`, origin `introduced`, severity `blocker`.**

Both findings cover submitted source
`a973af1cfcbaf7c06420ca5c7cf503aa98144d04` and its exact submitted tree. They are two distinct
branches in the same bounded binding-recovery component.

## 5. What I attacked and could not break

- Canonical binding, record, request, batch, entry and anchor framing; closed decoders; exact
  byte-domain digests; opaque authority, Subject and BatchKey coordinates; the submitted literal
  vectors.
- Complete capture reconstruction and exact equality of the four fixed projection sets, including
  duplicate/missing/drifted rows, missing/domain-substituted blobs, redacted/unknown/foreign events,
  tenant/generation substitution and explicit rebuild revalidation.
- Transaction-local binding, batch, global-record and subject locks, deterministic lock order,
  repeated subjects, mixed decision/observation batches, zero-domain-event decisions, original
  request bytes, physical positions and exact retry receipts.
- Imported anchor validation, envelope evidence blobs, global imported record lookup, suffix
  verification, exact-anchor replay, upload boundaries and ordinary/import record-ID races.
- Guard refusal typed slots and returned-error precedence; ordinary append committed-lost-reply,
  false physical conflict, malformed result and unavailable-recovery handling. The four named
  provider-private persistence/bookkeeping/crash gaps remain excluded and unverified.
- The synchronous bridge's bounded queue, phase CAS, deadline classification, reentrancy,
  current-thread owner, panic containment, shutdown escalation, retained join and cached retirement
  logic. I found no additional source defect in the bridge. Its existing unit tests mostly exercise
  cells and inert bridge state directly; the three real SQLite bridge cases cover basic ownership
  and runtime isolation, not every barrier scenario listed by the design.
- Rust 1.91 closure and optional feature wiring for the adapter, the pure-workspace 1.85 exclusion,
  strict clippy lane, rusqlite alignment and CI/task gate selection. I did not rerun MSRV, clippy,
  docs, or the full workspace because the assigned build slot was the focused adapter package.
- M5 service/3/record4 is deliberately absent from this frozen source and was not treated as a
  defect. The denied Eventlog administration diff/tree/tests were not entered or retried.

## 6. Every path written outside the worktree

| path | what |
|---|---|
| `<review-evidence>/reviewer-binding-unknown-conflict-red.log` | first isolated F1 run, SHA-256 `ce479bf9002615999badee8cec6748b645ba06db75c8f2d44469c8b0dbee1ea3` |
| `<review-evidence>/reviewer-binding-unknown-conflict-red.exit` | F1 exit 101 |
| `<review-evidence>/reviewer-binding-row-validation-red.log` | first isolated F2 run, SHA-256 `d25008aaf84d06105e039e6a47cba395e9c14843e75b1a707cd04e2d8c6ea90e` |
| `<review-evidence>/reviewer-binding-row-validation-red.exit` | F2 exit 101 |
| `<review-evidence>/reviewer-binding-dedicated-red.log` | two-case dedicated target, SHA-256 `cb4a4ba525bf5dc5210cff06b590cc361099d4b4064078ee4d9461376bae404b` |
| `<review-evidence>/reviewer-binding-dedicated-red.exit` | dedicated exit 101 |
| `<review-evidence>/reviewer-adapter-suite-red.log` | complete adapter package suite, SHA-256 `b452b86e0b7c17303617ded2dae199d04ed70e86d59ff2b469aaafff7b55e4f1` |
| `<review-evidence>/reviewer-adapter-suite-red.exit` | package exit 101 |
| `<review-evidence>/submitted-source-recheck.log` | 19-row submitted-manifest recheck, SHA-256 `ccdee12831536f1bcd13715ebc7671d1e6ea763617ba97a1e65cba45097da3fa` |
| `<review-evidence>/submitted-source-recheck.exit` | manifest recheck exit 0 |
| `<review-evidence>/tmp` | empty owned Cargo temporary directory |
| `<review-evidence>/report.md` | this report |

The compiler used the reviewer tree's default `target/`; it is inside the assigned worktree and
was not cleaned. No other external path was written.

## 7. Findings block

```findings
- file: crates/entity-eventlog/src/adapter.rs
  line: 1053
  category: contract-drift
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: after UnknownCommit, binding recovery converts a complete typed foreign-authority conflict into Uncertain and discards the exact immutable winner
- file: crates/entity-eventlog/src/adapter.rs
  line: 1142
  category: acceptance
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: binding recovery returns Conflict for a foreign event before validating its required derived row, so an incomplete authority capture is accepted as a valid conflict instead of refused as provider corruption
```
