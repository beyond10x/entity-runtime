---
format: aep.planning-md/1
id: review-result:er-async-adversary-pass-1
kind: review-result
status: active
title: Async recorded executor adversarial pass one
relations:
- reviews: story:async-recorded-contract-executor
revision: 1
---
unit: story:async-recorded-contract-executor revision 9 at submitted commit 374286ee95efca37082dab57e4e46eb4704b5351
verdict: NEEDS-CHANGE
cases: executed 46→48, red 2
origin: introduced 2 / pre-existing 0 / undecided 0
wrote-outside-worktree: 20 paths
needs-coordinator: route both introduced blockers to the implementor
 .../entity-store/tests/async_recorded_adversary.rs | 143 +++++++++++++++++++++
 1 file changed, 143 insertions(+)

# Adversarial pass one — public-safe rendering

## 1. Test-only diffstat

The stat above is from `git --no-pager diff --stat --no-index /dev/null crates/entity-store/tests/async_recorded_adversary.rs`; the test is intentionally untracked because this charter forbids staging. `git status --porcelain=v1 --untracked-files=all` reports only that test file. HEAD remained detached at `374286ee95efca37082dab57e4e46eb4704b5351`, tree `9379537bfd6cf06b398f821884c009bd079b9411`.

## 2. Cases added and first red outputs

### `crates/entity-store/tests/async_recorded_adversary.rs:75`

`a_partial_transcript_cannot_be_relabelled_as_a_complete_provider_snapshot` first obtains a real two-subject provider-owned snapshot, removes one history, preserves the publicly constructible `StoreCoverage::CompleteSnapshot` marker, and requires verification to refuse the caller-selected transcript. It is red now. Its first execution was exit 101; the format-only current rerun was also exit 101 at assertion line 99.

```text
$ CARGO_TERM_COLOR=never CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --locked -j 2 -p entity-store --test async_recorded_adversary a_partial_transcript_cannot_be_relabelled_as_a_complete_provider_snapshot -- --exact --nocapture
   Compiling proc-macro2 v1.0.107
   Compiling quote v1.0.47
   Compiling unicode-ident v1.0.24
   Compiling serde_core v1.0.229
   Compiling zmij v1.0.23
   Compiling syn v3.0.4
   Compiling libc v0.2.189
   Compiling serde_json v1.0.151
   Compiling serde v1.0.229
   Compiling serde_derive v1.0.229
   Compiling itoa v1.0.18
   Compiling memchr v2.8.3
   Compiling entity-core v0.18.1 (<repository>/crates/entity-core)
   Compiling fs2 v0.4.3
   Compiling entity-store v0.18.1 (<repository>/crates/entity-store)
    Finished `test` profile [unoptimized] target(s) in 4.94s
     Running tests/async_recorded_adversary.rs (target/debug/deps/async_recorded_adversary-ac0ddef722345d5b)

running 1 test

thread 'a_partial_transcript_cannot_be_relabelled_as_a_complete_provider_snapshot' (local-run-id) panicked at crates/entity-store/tests/async_recorded_adversary.rs:95:10:
caller-selected evidence must not claim whole-provider coverage: StoreAssurance { scope: "reference-store", coverage: CompleteSnapshot, subjects: [VerifiedFromGenesis { subject: Subject { entity: "ticket", id: "one" } }] }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test a_partial_transcript_cannot_be_relabelled_as_a_complete_provider_snapshot ... FAILED

failures:

failures:
    a_partial_transcript_cannot_be_relabelled_as_a_complete_provider_snapshot

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p entity-store --test async_recorded_adversary`
```

### `crates/entity-store/tests/async_recorded_adversary.rs:106`

`imported_envelope_cannot_be_later_than_its_anchor_revision` supplies revision-2 enveloped evidence inside a revision-1 anchor and requires the public reference-provider seeding boundary to reject it before reserving its global ID. It is red now. Its first execution was exit 101; the format-only current rerun was also exit 101 at assertion line 138.

```text
$ CARGO_TERM_COLOR=never CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --locked -j 2 -p entity-store --test async_recorded_adversary imported_envelope_cannot_be_later_than_its_anchor_revision -- --exact --nocapture
    Finished `test` profile [unoptimized] target(s) in 0.03s
     Running tests/async_recorded_adversary.rs (target/debug/deps/async_recorded_adversary-ac0ddef722345d5b)

running 1 test

thread 'imported_envelope_cannot_be_later_than_its_anchor_revision' (local-run-id) panicked at crates/entity-store/tests/async_recorded_adversary.rs:134:10:
pre-anchor evidence cannot name a post-anchor revision: ()
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test imported_envelope_cannot_be_later_than_its_anchor_revision ... FAILED

failures:

failures:
    imported_envelope_cannot_be_later_than_its_anchor_revision

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p entity-store --test async_recorded_adversary`
```

The retained current reruns are `case-1-complete-snapshot-current.log` and `case-2-imported-anchor-current.log`. The exact current test patch is `test.patch`.

## 3. Affected package suite

The ordinary affected-package command also exited 101 but Cargo stopped at the failing adversary binary before 27 unchanged store cases. The retained final command adds only `--no-fail-fast`, so all 48 cases execute. It produced 46 green cases, the same handed-over baseline, plus these two red cases; exit 101.

```text
$ CARGO_TERM_COLOR=never CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --locked -j 2 -p entity-store -p entity-executor --no-fail-fast
    Finished `test` profile [unoptimized] target(s) in 0.03s
     Running unittests src/lib.rs (target/debug/deps/entity_executor-2ad8660ab7a21cff)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/async_recorded_contract.rs (target/debug/deps/async_recorded_contract-2717f57aac764754)

running 9 tests
test exact_imported_retry_is_historical_and_missing_matching_facts_are_not_success ... ok
test stale_observation_in_a_mixed_batch_rolls_back_every_record_and_receipt ... ok
test global_record_ids_duplicate_requests_and_empty_batches_are_closed_before_writes ... ok
test complete_decisions_keep_duplicate_events_and_observations_keep_explicit_null_provenance ... ok
test changed_expectation_or_provenance_conflicts_with_the_original_retry ... ok
test mixed_batches_use_ordered_local_state_and_observations_do_not_advance_revision ... ok
test commit_then_uncertain_and_dropped_response_recover_one_effect_and_receipt ... ok
test retry_uses_saved_definition_and_verified_prefix_before_current_authority ... ok
test named_and_single_retries_preserve_original_receipt_coordinates ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running unittests src/lib.rs (target/debug/deps/entity_store-3a993a88840b45de)

running 10 tests
test asynchronous::tests::async_ports_are_object_safe_and_return_send_futures ... ok
test envelope::tests::an_envelope_missing_an_optional_key_is_refused_rather_than_defaulted ... ok
test envelope::tests::invalid_time_and_blank_identity_are_refused ... ok
test envelope::tests::an_envelope_round_trips_with_explicit_absence ... ok
test asynchronous::tests::canonical_comparison_bytes_pin_exact_numbers_nulls_and_coordinate_identities ... ok
test asynchronous::tests::imported_evidence_reserves_ids_without_inventing_receipts_or_global_order ... ok
test asynchronous::tests::whole_batch_position_overflow_refuses_without_publishing_a_prefix ... ok
test asynchronous::tests::explicit_set_assurance_names_its_scope_subjects_and_import_boundaries ... ok
test asynchronous::tests::record_and_receipt_component_mismatches_are_integrity_refusals ... ok
test asynchronous::tests::reference_store_commits_complete_records_atomically_and_verifies_them ... ok

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/async_recorded_adversary.rs (target/debug/deps/async_recorded_adversary-ac0ddef722345d5b)

running 2 tests
test imported_envelope_cannot_be_later_than_its_anchor_revision ... FAILED
test a_partial_transcript_cannot_be_relabelled_as_a_complete_provider_snapshot ... FAILED

failures:

---- imported_envelope_cannot_be_later_than_its_anchor_revision stdout ----

thread 'imported_envelope_cannot_be_later_than_its_anchor_revision' (local-run-id) panicked at crates/entity-store/tests/async_recorded_adversary.rs:138:10:
pre-anchor evidence cannot name a post-anchor revision: ()
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- a_partial_transcript_cannot_be_relabelled_as_a_complete_provider_snapshot stdout ----

thread 'a_partial_transcript_cannot_be_relabelled_as_a_complete_provider_snapshot' (local-run-id) panicked at crates/entity-store/tests/async_recorded_adversary.rs:99:10:
caller-selected evidence must not claim whole-provider coverage: StoreAssurance { scope: "reference-store", coverage: CompleteSnapshot, subjects: [VerifiedFromGenesis { subject: Subject { entity: "ticket", id: "one" } }] }


failures:
    a_partial_transcript_cannot_be_relabelled_as_a_complete_provider_snapshot
    imported_envelope_cannot_be_later_than_its_anchor_revision

test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p entity-store --test async_recorded_adversary`
     Running tests/both_providers.rs (target/debug/deps/both_providers-ff483b114bca171f)

running 7 tests
test every_provider_answers_absent_for_something_nobody_stored ... ok
test every_provider_leaves_a_refused_commit_with_no_trace ... ok
test a_retried_commit_appends_its_events_once ... ok
test every_provider_lists_what_it_holds_sorted ... ok
test a_committed_instance_reads_back_with_its_events ... ok
test the_file_store_survives_being_reopened ... ok
test every_provider_refuses_a_stale_write_the_same_way ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s

     Running tests/concurrency.rs (target/debug/deps/concurrency-bb5da5a9482369a0)

running 5 tests
test an_instance_nobody_stored_is_absent_rather_than_an_error ... ok
test a_creation_expects_nothing_and_a_second_creation_of_the_same_identity_is_refused ... ok
test a_refused_commit_changes_nothing ... ok
test state_and_events_arrive_together ... ok
test two_executions_from_one_revision_leave_exactly_one_accepted ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/conformance.rs (target/debug/deps/conformance-0c7604a43fddcfca)

running 3 tests
test a_broken_provider_is_caught ... ok
test the_memory_provider_conforms ... ok
test the_file_provider_conforms ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s

     Running tests/file_record_index.rs (target/debug/deps/file_record_index-ebc879cd9bbad914)

running 7 tests
test a_fresh_handle_finds_a_record_id_another_handle_wrote_and_refuses_its_reuse ... ok
test abandoned_subject_temporary_files_do_not_hide_ids_or_block_recorded_writes ... ok
test a_handle_remembers_its_own_writes_without_rereading_the_store ... ok
test parent_and_marker_symlinks_are_refused_on_reads ... ok
test an_existing_handle_invalidates_its_index_after_another_writer ... ok
test separate_processes_serialize_revision_checks_and_survive_a_killed_lock_holder ... ok
test separate_writers_preserve_exactly_one_winning_revision_and_all_its_records ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s

     Running tests/projections.rs (target/debug/deps/projections-4df3a5f8cd6b9ce0)

running 5 tests
test a_projection_naming_a_field_the_schema_does_not_have_is_refused_at_registration ... ok
test a_projection_naming_a_state_the_lifecycle_does_not_have_is_refused_at_registration ... ok
test an_instance_whose_key_resolves_to_nothing_is_left_out_rather_than_filed_under_an_empty_key ... ok
test a_sequence_of_decisions_produces_the_declared_read_model ... ok
test a_projection_is_the_same_bytes_every_run ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests entity_executor

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests entity_store

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

error: 1 target failed:
    `-p entity-store --test async_recorded_adversary`
```

Terminal exit: `101`.

## 4. Judgement findings

### ER-ASYNC-ADV-1 — caller-selected evidence can claim provider completeness

- Measured at `crates/entity-store/src/asynchronous/verify.rs:517`: the verifier copies the public input marker into `StoreAssurance`. The case at `crates/entity-store/tests/async_recorded_adversary.rs:99` omitted one of two histories returned by the reference provider and received `StoreAssurance { coverage: CompleteSnapshot, subjects: [one] }`; exit 101.
- Reachable caller: any caller of the public `verify_store_histories` API can construct `CompleteStoreSnapshot` because its `coverage` and `histories` fields are public at `crates/entity-store/src/asynchronous/types.rs:592-598`. The test starts from `AsyncRecordedReader::complete_snapshot`, then performs exactly the partial-transcript relabeling forbidden by the story acceptance and R-126.
- Verdict: `NEEDS-CHANGE`. Origin: `introduced`; these types and verifier do not exist at base `eaf43090636abce025649e565e5af271c4513eae`. The API needs a provenance boundary that prevents selected histories from minting provider-complete assurance.

### ER-ASYNC-ADV-2 — imported evidence may lie after its own anchor

- Measured at `crates/entity-store/src/asynchronous/verify.rs:136`: `validate_imported_boundary` validates the anchor revision and each envelope independently but never orders the envelope revision at or before the anchor. The case at `crates/entity-store/tests/async_recorded_adversary.rs:138` passed revision-2 evidence beneath a revision-1 anchor; `MemoryRecordedStore::seed_imported` returned `()`; exit 101.
- Reachable caller: the public reference-provider `MemoryRecordedStore::seed_imported` calls this validator, then inserts every accepted imported envelope into the global record index. `verify_imported_record` subsequently describes any exact match as `VerifiedAfterBoundary { anchor_revision: 1 }`, so the malformed evidence crosses both the provider and verifier boundary.
- Verdict: `NEEDS-CHANGE`. Origin: `introduced`; the imported boundary and validator are new in the submitted commit. Evidence declared to be retained at an anchor must not describe a later revision.

Both findings cover submitted commit `374286ee95efca37082dab57e4e46eb4704b5351`, tree `9379537bfd6cf06b398f821884c009bd079b9411`.

## 5. Attacks that did not break

- Exact canonical record/request/batch encoding, explicit nulls, numeric spellings and coordinate identities were inspected against the normative fixtures; no additional case was warranted.
- Saved-definition retry matching, retry-before-current-authority ordering and immutable prefix recomputation were inspected through both single and named paths; no additional reachable defect was established.
- Ordered transaction-local mixed actions, zero-event revision advance, observation non-advance, global record-ID conflicts, receipt matching, position-overflow rollback and same-ID uncertain recovery were inspected; the retained implementation cases and causal mutations cover the attacked branches.
- Existing synchronous APIs and later Eventlog, bridge, SQL facade/import and runtime units were kept outside this review boundary.

## 6. Paths written outside the worktree

- `local-evidence:ess-evolution/waves/0002-er-contract/review1-scratch/`
- `local-evidence:ess-evolution/waves/0002-er-contract/review1-evidence/`
- `local-evidence:ess-evolution/waves/0002-er-contract/review1-evidence/affected-suite.exit`
- `local-evidence:ess-evolution/waves/0002-er-contract/review1-evidence/affected-suite-final.exit`
- `local-evidence:ess-evolution/waves/0002-er-contract/review1-evidence/affected-suite-final.log`
- `local-evidence:ess-evolution/waves/0002-er-contract/review1-evidence/affected-suite.log`
- `local-evidence:ess-evolution/waves/0002-er-contract/review1-evidence/affected-suite-no-fail-fast.exit`
- `local-evidence:ess-evolution/waves/0002-er-contract/review1-evidence/affected-suite-no-fail-fast.log`
- `local-evidence:ess-evolution/waves/0002-er-contract/review1-evidence/case-1-complete-snapshot-current.exit`
- `local-evidence:ess-evolution/waves/0002-er-contract/review1-evidence/case-1-complete-snapshot-current.log`
- `local-evidence:ess-evolution/waves/0002-er-contract/review1-evidence/case-1-complete-snapshot-red.exit`
- `local-evidence:ess-evolution/waves/0002-er-contract/review1-evidence/case-1-complete-snapshot-red.log`
- `local-evidence:ess-evolution/waves/0002-er-contract/review1-evidence/case-2-imported-anchor-current.exit`
- `local-evidence:ess-evolution/waves/0002-er-contract/review1-evidence/case-2-imported-anchor-current.log`
- `local-evidence:ess-evolution/waves/0002-er-contract/review1-evidence/case-2-imported-anchor-red.exit`
- `local-evidence:ess-evolution/waves/0002-er-contract/review1-evidence/case-2-imported-anchor-red.log`
- `local-evidence:ess-evolution/waves/0002-er-contract/review1-evidence/test-diffstat.txt`
- `local-evidence:ess-evolution/waves/0002-er-contract/review1-evidence/test.patch`
- `local-evidence:ess-evolution/waves/0002-er-contract/review1-evidence/pass-1.md`
- `local-evidence:ess-evolution/waves/0002-er-contract/review1-evidence/pass-1-aep.md`

The tree-local `target/` directory was the only build output and is inside the assigned worktree. No file was written under `review1-scratch/`.

## 7. Parseable findings

```findings
- file: crates/entity-store/src/asynchronous/verify.rs
  line: 517
  category: acceptance
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: caller-selected histories can retain the public CompleteSnapshot marker and receive whole-provider assurance after omitting provider records
- file: crates/entity-store/src/asynchronous/verify.rs
  line: 136
  category: boundary
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: imported envelope evidence later than its anchor revision is admitted as pre-anchor evidence and reserved in the global identity index
```

