---
format: aep.planning-md/2
id: review-result:er-async-adversary-pass-2
kind: review-result
status: active
title: Async recorded executor adversarial pass two
relations:
- reviews: story:async-recorded-contract-executor
revision: 1
---
unit: story:async-recorded-contract-executor revision 10 at submitted commit f181aacac3241689fca961cd21c34665e33d7a5d
verdict: NEEDS-CHANGE
cases: executed 52→53, red 1
origin: introduced 1 / pre-existing 0 / undecided 0
wrote-outside-worktree: 10 paths
needs-coordinator: route the introduced blocker to the implementor
 .../tests/async_recorded_adversary_pass2.rs        | 81 ++++++++++++++++++++++
 1 file changed, 81 insertions(+)

# Adversarial pass two — public-safe rendering

## 1. Test-only diffstat

The stat above is from `git --no-pager diff --stat --no-index /dev/null crates/entity-store/tests/async_recorded_adversary_pass2.rs`. `git status --porcelain=v1 --untracked-files=all` reports only that test. HEAD remained at `f181aacac3241689fca961cd21c34665e33d7a5d`; the test SHA-256 is `67e2702fbd6524328ed101ea874c81f26ac999bc6a64a0bec44e15e7f57a0b6a`.

## 2. Case added and first red output

### `crates/entity-store/tests/async_recorded_adversary_pass2.rs:59`

`the_writer_rechecks_duplicate_global_ids_in_a_public_append_request` constructs the public append value directly with two valid new decisions sharing one global record ID and requires the mandatory writer boundary to refuse the duplicate. It is red now. The first and only case execution selected one test and exited 101 at assertion line 72.

```text
$ CARGO_TARGET_DIR=target CARGO_TERM_COLOR=never CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --locked -j 2 -p entity-store --test async_recorded_adversary_pass2 the_writer_rechecks_duplicate_global_ids_in_a_public_append_request -- --exact --nocapture
   Compiling proc-macro2 v1.0.107
   Compiling unicode-ident v1.0.24
   Compiling quote v1.0.47
   Compiling serde_core v1.0.229
   Compiling zmij v1.0.23
   Compiling syn v3.0.4
   Compiling serde v1.0.229
   Compiling libc v0.2.189
   Compiling serde_json v1.0.151
   Compiling serde_derive v1.0.229
   Compiling memchr v2.8.3
   Compiling itoa v1.0.18
   Compiling entity-core v0.18.1 (<repository>/crates/entity-core)
   Compiling fs2 v0.4.3
   Compiling entity-store v0.18.1 (<repository>/crates/entity-store)
    Finished `test` profile [unoptimized] target(s) in 5.07s
     Running tests/async_recorded_adversary_pass2.rs (target/debug/deps/async_recorded_adversary_pass2-9fcf39531167bfbc)

running 1 test

thread 'the_writer_rechecks_duplicate_global_ids_in_a_public_append_request' (local-run-id) panicked at crates/entity-store/tests/async_recorded_adversary_pass2.rs:72:10:
the mandatory writer boundary must refuse duplicate global record ids: Committed { receipt: Batch(BatchReceipt { key: Named("duplicate-ids"), members: [RecordReceipt { record_id: "shared-record-id", subject: Subject { entity: "ticket", id: "one" }, kind: Decision, revision: 1, position: RecordPosition { subject: 0, store: 0 }, batch_key: Named("duplicate-ids"), member_index: 0 }, RecordReceipt { record_id: "shared-record-id", subject: Subject { entity: "ticket", id: "two" }, kind: Decision, revision: 1, position: RecordPosition { subject: 0, store: 1 }, batch_key: Named("duplicate-ids"), member_index: 1 }] }), replayed: false }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test the_writer_rechecks_duplicate_global_ids_in_a_public_append_request ... FAILED

failures:

failures:
    the_writer_rechecks_duplicate_global_ids_in_a_public_append_request

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p entity-store --test async_recorded_adversary_pass2`
```

Terminal exit: `101`.

## 3. Complete affected suite

```text
$ CARGO_TARGET_DIR=target CARGO_TERM_COLOR=never CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --locked -j 2 -p entity-store -p entity-executor --no-fail-fast
   Compiling entity-store v0.18.1 (<repository>/crates/entity-store)
   Compiling entity-executor v0.18.1 (<repository>/crates/entity-executor)
    Finished `test` profile [unoptimized] target(s) in 1.05s
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

running 14 tests
test asynchronous::tests::async_ports_are_object_safe_and_return_send_futures ... ok
test envelope::tests::an_envelope_missing_an_optional_key_is_refused_rather_than_defaulted ... ok
test envelope::tests::invalid_time_and_blank_identity_are_refused ... ok
test envelope::tests::an_envelope_round_trips_with_explicit_absence ... ok
test asynchronous::tests::canonical_comparison_bytes_pin_exact_numbers_nulls_and_coordinate_identities ... ok
test asynchronous::tests::bare_imported_decisions_and_events_must_name_the_anchor_subject ... ok
test asynchronous::tests::all_revision_bearing_imported_evidence_is_bounded_before_publication ... ok
test asynchronous::tests::partial_prior_imported_evidence_remains_allowed_across_all_typed_variants ... ok
test asynchronous::tests::imported_evidence_reserves_ids_without_inventing_receipts_or_global_order ... ok
test asynchronous::tests::editable_complete_markers_never_replace_a_direct_provider_capture ... ok
test asynchronous::tests::explicit_set_assurance_names_its_scope_subjects_and_import_boundaries ... ok
test asynchronous::tests::whole_batch_position_overflow_refuses_without_publishing_a_prefix ... ok
test asynchronous::tests::record_and_receipt_component_mismatches_are_integrity_refusals ... ok
test asynchronous::tests::reference_store_commits_complete_records_atomically_and_verifies_them ... ok

test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/async_recorded_adversary.rs (target/debug/deps/async_recorded_adversary-ac0ddef722345d5b)

running 2 tests
test imported_envelope_cannot_be_later_than_its_anchor_revision ... ok
test a_partial_transcript_cannot_be_relabelled_as_a_complete_provider_snapshot ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/async_recorded_adversary_pass2.rs (target/debug/deps/async_recorded_adversary_pass2-9fcf39531167bfbc)

running 1 test
test the_writer_rechecks_duplicate_global_ids_in_a_public_append_request ... FAILED

failures:

---- the_writer_rechecks_duplicate_global_ids_in_a_public_append_request stdout ----

thread 'the_writer_rechecks_duplicate_global_ids_in_a_public_append_request' (local-run-id) panicked at crates/entity-store/tests/async_recorded_adversary_pass2.rs:72:10:
the mandatory writer boundary must refuse duplicate global record ids: Committed { receipt: Batch(BatchReceipt { key: Named("duplicate-ids"), members: [RecordReceipt { record_id: "shared-record-id", subject: Subject { entity: "ticket", id: "one" }, kind: Decision, revision: 1, position: RecordPosition { subject: 0, store: 0 }, batch_key: Named("duplicate-ids"), member_index: 0 }, RecordReceipt { record_id: "shared-record-id", subject: Subject { entity: "ticket", id: "two" }, kind: Decision, revision: 1, position: RecordPosition { subject: 0, store: 1 }, batch_key: Named("duplicate-ids"), member_index: 1 }] }), replayed: false }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    the_writer_rechecks_duplicate_global_ids_in_a_public_append_request

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p entity-store --test async_recorded_adversary_pass2`
     Running tests/both_providers.rs (target/debug/deps/both_providers-ff483b114bca171f)

running 7 tests
test every_provider_answers_absent_for_something_nobody_stored ... ok
test every_provider_leaves_a_refused_commit_with_no_trace ... ok
test a_retried_commit_appends_its_events_once ... ok
test a_committed_instance_reads_back_with_its_events ... ok
test the_file_store_survives_being_reopened ... ok
test every_provider_lists_what_it_holds_sorted ... ok
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
test an_existing_handle_invalidates_its_index_after_another_writer ... ok
test parent_and_marker_symlinks_are_refused_on_reads ... ok
test a_handle_remembers_its_own_writes_without_rereading_the_store ... ok
test separate_processes_serialize_revision_checks_and_survive_a_killed_lock_holder ... ok
test separate_writers_preserve_exactly_one_winning_revision_and_all_its_records ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s

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
    `-p entity-store --test async_recorded_adversary_pass2`
```

Terminal exit: `101`. All 52 submitted cases passed; the added case is the only failure. No case was ignored or filtered in the affected run.

## 4. Judgement finding

### ER-ASYNC-ADV2-1 — the mandatory writer accepts duplicate IDs when the public request constructor is bypassed

- Measured at `crates/entity-store/src/asynchronous/memory.rs:198`: `append_transaction` checks each member only against the pre-transaction global map. It keeps no request-local ID set, so two new members with `shared-record-id` both reach publication. The assertion at `crates/entity-store/tests/async_recorded_adversary_pass2.rs:72` received a committed two-member receipt; exit 101. Publication then stores both subject histories while the `BTreeMap` global index insertion at `memory.rs:346` overwrites the first lookup with the second.
- Reachable caller: `AppendRequest.key` and `.members` are public at `crates/entity-store/src/asynchronous/types.rs:334`, and the public object-safe `AsyncRecordedWriter::append` port accepts that value at `crates/entity-store/src/asynchronous/ports.rs:55`. Any direct store caller can construct the value without `AppendRequest::new`; this is the mandatory lower-level append surface used by future adapters, not a test-only state.
- Verdict: `NEEDS-CHANGE`. Origin: `introduced`; the asynchronous request, writer port and reference provider do not exist at base `eaf43090636abce025649e565e5af271c4513eae`. The writer must revalidate the complete public request shape, including request-local duplicate identities, before staging or publishing any member.

This finding covers submitted commit `f181aacac3241689fca961cd21c34665e33d7a5d`.

## 5. Attacks that did not break

- Exact `er.request/1`, `er.record/1`, `er.batch/1` and coordinate encodings were inspected against the normative literal fixtures; no additional case survived inspection.
- Saved-definition retry matching, identity recovery before current authority, immutable prefix recomputation, named/single receipt integrity and same-ID uncertain recovery remained covered by reachable assertions.
- Transaction-local mixed batches, zero-event decisions, observation revision, physical allocation rollback, imported partial/legacy boundaries and the two corrected pass-one guards remained covered; all submitted cases passed.
- Existing synchronous APIs and the later Eventlog, bridge, SQL facade/import and runtime units remained outside this review boundary.

## 6. Paths written outside the worktree

- `local-evidence:ess-evolution/waves/0002-er-contract/review2-scratch/`
- `local-evidence:ess-evolution/waves/0002-er-contract/review2-evidence/`
- `local-evidence:ess-evolution/waves/0002-er-contract/review2-evidence/case-1-duplicate-global-id-red.log`
- `local-evidence:ess-evolution/waves/0002-er-contract/review2-evidence/case-1-duplicate-global-id-red.exit`
- `local-evidence:ess-evolution/waves/0002-er-contract/review2-evidence/affected-suite-no-fail-fast.log`
- `local-evidence:ess-evolution/waves/0002-er-contract/review2-evidence/affected-suite-no-fail-fast.exit`
- `local-evidence:ess-evolution/waves/0002-er-contract/review2-evidence/test-diffstat.txt`
- `local-evidence:ess-evolution/waves/0002-er-contract/review2-evidence/test.patch`
- `local-evidence:ess-evolution/waves/0002-er-contract/review2-evidence/pass-2.md`
- `local-evidence:ess-evolution/waves/0002-er-contract/review2-evidence/pass-2-aep.md`

The tree-local `target/` directory is the only build output. No file was written under `review2-scratch/`.

## 7. Parseable findings

```findings
- file: crates/entity-store/src/asynchronous/memory.rs
  line: 198
  category: acceptance
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: the mandatory writer accepts a public append request containing duplicate new global record IDs and publishes both histories while overwriting one global lookup
```
