---
format: aep.planning-md/3
id: review-result:issue-51-runtime-recheck
kind: review-result
status: active
title: Incremental runtime receipt correction recheck
relations:
- reviews: story:bounded-batch-and-facade-reads
revision: 1
---
unit: #51 runtime working tree based on f57bf9753abec7aaa5ff9103bd9b66e29f3398cd, after implementor receipt guard
verdict: nothing found
cases: executed 186→186, red 0
origin: introduced 0 / pre-existing 0 / undecided 0
wrote-outside-worktree: none
needs-coordinator: none

```text
git --no-pager diff --stat
 crates/entity-eventlog/src/adapter.rs        |  52 +++++++++++-
 crates/entity-eventlog/src/adapter/scoped.rs |  15 +++-
 crates/entity-eventlog/src/facade.rs         | 121 ++++++++++++++++++++++++++-
 crates/entity-eventlog/src/lib.rs            |   6 +-
 crates/entity-eventlog/src/sync.rs           | 121 +++++++++++++++++++++++++--
 5 files changed, 298 insertions(+), 17 deletions(-)
```

The diff includes inherited implementation changes. Reviewer writes remain only the new 419-line `adapter/tracked/review_tests.rs` and the coordinator-approved test-only module declaration in `tracked.rs`; the implementor owns the production fix. Original defect and isolated red outputs are preserved in `report.md`. No test assertion was weakened for this recheck.

Both receipt regressions now pass: real SQLite/default-projector native blank-key append is refused with the same invalid-key `CorruptHistory` as full verification, and fabricated SingleRecord mismatch is refused by incremental admission. The coalesced-batch ordering, observation head, latest state source and missing-member test also remains green.

The focused command ran `adapter::tracked::review_tests` and executed exactly three cases. Full package command: `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 cargo +1.91.0 --config target/issue-51-scratch/paths.toml test -p entity-eventlog --all-features --locked`. Exact output follows. Machine-specific checkout prefixes alone are redacted to `<worktree>` / `<provider-worktree>`.

```text
   Compiling entity-eventlog v0.25.1 (<worktree>/crates/entity-eventlog)
    Finished `test` profile [unoptimized] target(s) in 7.81s
     Running unittests src/lib.rs (target/debug/deps/entity_eventlog-e345c4868818aed2)

running 74 tests
test adapter::batch_read_tests::sequential_decisions_measurement ... ok
test adapter::forked_append::a_provider_fork_refusal_of_a_stream_the_append_does_not_write_is_integrity ... ok
test adapter::forked_append::a_provider_fork_refusal_of_a_written_subject_is_that_subjects_fork ... ok
test adapter::seeded_open::a_batch_whose_stored_bytes_are_not_their_own_canonical_rendering_is_refused ... ok
test adapter::seeded_open::a_batch_is_parsed_once_rather_than_once_for_every_member_as_well ... ok
test adapter::seeded_open::a_member_expectation_carrying_a_field_of_its_own_is_refused ... ok
test adapter::seeded_open::seeded_open_measurement ... ok
test adapter::seeded_open::a_batch_member_that_does_not_reproduce_its_own_record_is_refused ... ok
test adapter::seeded_open::the_batch_key_expectation_and_member_wires_admit_exactly_one_shape_each ... ok
test adapter::seeded_open::a_remembered_history_is_not_the_answer_for_one_that_differs_in_a_single_expectation ... ok
test adapter::seeded_open::a_memory_of_the_sound_store_admits_no_forgery_of_it ... ok
test adapter::seeded_open::a_remembered_digest_does_not_admit_other_bytes_under_it ... ok
test adapter::small_store_cost::small_store_open_and_batch_measurement ... ok
test adapter::tests::every_eventlog_append_error_has_an_explicit_er_classification ... ok
test adapter::tests::every_guard_refusal_has_a_stable_code_and_typed_match ... ok
test adapter::tests::the_blob_and_append_mappings_agree_on_the_only_variant_put_blob_documents ... ok
test adapter::seeded_open::an_advanced_subject_is_verified_from_its_verified_state_and_refused_where_it_breaks ... ok
test adapter::batch_read_tests::lost_append_reply_recovers_with_a_fresh_verified_capture ... ok
test adapter::batch_read_tests::stale_preflight_is_guarded_and_replayed_append_recovers_from_fresh_capture ... ok
test adapter::tracked::review_tests::acknowledged_native_append_with_blank_named_key_is_refused_before_tracked_reuse ... ok
test adapter::batch_read_tests::a_head_another_writer_moved_forces_a_whole_build_that_sees_it ... ok
test adapter::tracked::review_tests::incremental_receipts_refuse_a_single_record_key_that_names_another_record ... ok
test adapter::batch_read_tests::reads_of_an_unmoved_head_reuse_the_model_the_handle_already_verified ... ok
test adapter::tracked::tests::one_scoped_history_read_preserves_order_duplicates_and_absence ... ok
test encoding::tests::a_digest_is_bound_to_its_exact_domain ... ok
test encoding::tests::closed_binding_decode_refuses_unknown_fields_and_noncanonical_order ... ok
test encoding::tests::literal_reference_vectors_pin_framing_domains_lengths_and_escaping ... ok
test encoding::tests::source_bound_anchor_refuses_a_contradictory_envelope_source ... ok
test encoding::tests::source_bound_anchor_version_preserves_old_bytes_and_old_reader_refusal ... ok
test sync::tests::an_expired_dispatched_write_retains_its_original_key_as_uncertain ... ok
test sync::tests::an_expired_queued_call_is_cancelled_before_dispatch ... ok
test sync::tests::completion_after_a_dispatched_waiter_times_out_is_safe ... ok
test sync::tests::completion_wins_a_deadline_race_without_reclassification ... ok
test sync::tests::dispatch_deadline_cas_has_one_winner_and_cancelled_work_never_calls_the_driver ... ok
test sync::tests::drain_finishes_in_order_and_escalation_cancels_only_queued_work ... ok
test sync::tests::drop_after_shutdown_timeout_escalates_and_never_waits_for_the_worker ... ok
test sync::tests::drop_requests_closure_without_waiting_for_an_active_future ... ok
test adapter::tracked::tests::unjournaled_provider_mutation_invalidates_the_model_before_reuse ... ok
test sync::tests::outer_worker_panic_marks_finished_and_preserves_a_retirement_failure ... ok
test sync::tests::panicked_or_stopped_worker_keeps_dispatched_write_uncertainty ... ok
test adapter::seeded_open::a_blob_named_under_two_domains_is_refused_without_being_hashed_twice ... ok
test sync::tests::panics_after_dispatch_keep_read_and_write_classification ... ok
test sync::tests::production_worker_enforces_capacity_and_discards_cancelled_queue_cells ... ok
test sync::tests::production_worker_calls_are_runtime_independent_and_reentrant_safe ... ok
test adapter::seeded_open::a_store_of_named_batches_written_at_base_reads_into_the_base_model ... ok
test sync::tests::production_worker_startup_owns_and_drops_the_driver_on_its_thread ... ok
test sync::tests::provider_errors_pass_through_without_bridge_reclassification ... ok
test sync::tests::queued_shutdown_cancellation_returns_closed_for_reads_and_writes ... ok
test sync::tests::reentrant_admission_is_rejected_before_queue_accounting ... ok
test sync::tests::queue_capacity_and_closed_admission_are_deterministic ... ok
test sync::tests::retirement_timeout_later_joins_once_and_preserves_provider_failure ... ok
test sync::tests::shutdown_joins_once_and_caches_provider_retirement_failure ... ok
test sync::tests::timed_out_shutdown_reports_in_flight_identity_and_can_escalate ... ok
test sync::tests::production_worker_preserves_every_driver_error_classification ... ok
test adapter::tracked::review_tests::coalesced_reverse_named_batches_keep_observation_heads_and_latest_state_source ... ok
test sync::tests::worker_panics_cancel_real_queued_cells_without_stranding_waiters ... ok
test adapter::tracked::tests::held_reader_keeps_its_old_model_while_a_refresh_rebuilds_safely ... ok
test adapter::seeded_open::a_capture_is_hashed_once_over_rather_than_several_times_over ... ok
test adapter::tracked::tests::delta_verification_matches_full_models_and_refuses_missing_or_forged_coordinates ... ok
test adapter::batch_read_tests::one_per_entity_read_serves_each_preflight_and_is_never_reused_across_calls ... ok
test adapter::tracked::tests::tracked_unchanged_reads_and_appends_do_not_recapture_verified_prefixes ... ok
test adapter::seeded_open::every_carried_digest_is_the_digest_of_the_bytes_the_model_holds ... ok
test sync::tests::timed_out_caller_does_not_cancel_commit_and_same_key_retry_recovers_it ... ok
test adapter::batch_read_tests::a_decision_this_handle_commits_advances_its_model_by_its_own_records_only ... ok
test adapter::memory::tests::a_memory_forgets_before_what_it_actually_retains_passes_its_cap ... ok
test adapter::seeded_open::every_bound_domain_is_still_refused_when_its_blob_is_not_its_digest ... ok
test adapter::small_store_cost::adversary_every_bound_blob_is_sorted_key_json_under_either_map_backend ... ok
test sync::tests::worker_panic_closes_admission_before_terminal_drain_and_wakes_late_accepted_caller ... ok
test adapter::seeded_open::the_model_is_byte_identical_to_the_verifying_path ... ok
test sync::tests::every_executor_write_keeps_its_original_key_after_dispatch_deadline ... ok
test adapter::small_store_cost::a_cold_open_decodes_and_replays_each_record_once ... ok
test adapter::small_store_cost::a_warm_handle_decodes_and_replays_only_the_records_a_batch_adds ... ok
test adapter::small_store_cost::adversary_a_warm_handle_answers_every_rewritten_capture_as_a_cold_build ... ok
test adapter::seeded_open::a_model_advanced_by_appended_events_is_the_model_a_whole_build_produces ... ok

test result: ok. 74 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.34s

     Running tests/adversary_guarded_write_races.rs (target/debug/deps/adversary_guarded_write_races-df43ad6cb924d100)

running 4 tests
test adversary_a_raced_import_through_the_override_reports_what_the_unraced_import_reports ... ok
test adversary_a_fallback_refused_append_counts_its_orphaned_blobs_against_the_read_bound ... ok
test adversary_a_raced_import_reports_the_same_refusal_through_override_and_fallback ... ok
test adversary_a_batch_key_raced_by_another_command_is_refused_alike_through_override_and_fallback ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s

     Running tests/adversary_observation_heads.rs (target/debug/deps/adversary_observation_heads-f3d436da1036f4e7)

running 4 tests
test a_stale_observation_recorded_after_the_other_branch_decided_does_not_break_the_merge ... ok
test a_batch_observing_then_deciding_on_a_multi_tip_subject_commits ... ok
test a_batch_writing_a_multi_tip_subject_twice_commits ... ok
test an_observation_on_a_three_tip_unforked_subject_commits_and_serves_the_decision ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.27s

     Running tests/adversary_r2_per_entity_reads.rs (target/debug/deps/adversary_r2_per_entity_reads-23ac8bb40ae7e5fd)

running 4 tests
test a_read_through_a_forgotten_tenant_does_not_mint_it_a_new_identity ... ok
test r151_a_command_read_is_per_entity_and_a_handle_read_is_one_complete_capture ... ok
test a_handle_does_not_commit_commands_it_can_no_longer_read_back ... ok
test an_honest_concurrent_writer_is_a_revision_conflict_not_provider_integrity ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s

     Running tests/adversary_two_batch_capture_limit.rs (target/debug/deps/adversary_two_batch_capture_limit-4ac15ec8d2126679)

running 1 test
test a_batch_commits_no_more_than_its_own_handle_can_read_back ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s

     Running tests/adversary_two_partial_overlap.rs (target/debug/deps/adversary_two_partial_overlap-ce95e04d7f2f16d4)

running 2 tests
test a_partially_overlapping_group_reports_the_member_that_is_actually_occupied ... ok
test a_partially_overlapping_group_commits_no_part_of_itself ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s

     Running tests/adversary_two_port_default_document.rs (target/debug/deps/adversary_two_port_default_document-e3ad7570edf1f5c7)

running 2 tests
test the_port_default_fails_closed_and_binds_no_blob ... ok
test the_sqlite_override_commits_a_group_with_its_blob_and_a_refused_guard_binds_neither ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/fault_acceptance.rs (target/debug/deps/fault_acceptance-47ca297aefaf1cdf)

running 26 tests
test postgres_provider_recovers_commit_before_lost_response_when_assigned ... ok
test file_provider_recovers_commit_before_lost_response ... ok
test provision_unknown_rollback_and_unavailable_recovery_remain_uncertain ... ok
test provision_recovers_commit_before_lost_response_with_original_coordinates ... ok
test provision_recovery_is_absent_then_unavailable_while_the_append_is_in_flight ... ok
test provision_preserves_exact_tuple_conflict ... ok
test append_unknown_rollback_with_unavailable_recovery_retains_the_batch_key ... ok
test import_unknown_rollback_and_malformed_reply_keep_subject_uncertainty ... ok
test append_recovers_committed_lost_reply_and_preserves_the_original_receipt ... ok
test import_recovers_committed_lost_reply_and_restart_with_fresh_context ... ok
test unknown_commit_with_corrupt_recovery_keeps_the_original_uncertainty ... ok
test competing_provisioners_are_atomic_in_both_winner_orders ... ok
test append_retries_a_false_physical_conflict_but_refuses_a_malformed_commit_reply ... ok
test import_refuses_after_each_individual_blob_upload_boundary ... ok
test append_never_trusts_a_guard_code_without_the_typed_transaction_slot ... ok
test reads_reject_missing_tampered_and_colliding_record_indexes ... ok
test absent_global_record_race_is_atomic_in_both_winner_orders ... ok
test ordinary_and_import_record_identity_race_is_atomic_in_both_winner_orders ... ok
test append_refuses_after_each_individual_blob_upload_boundary ... ok
test returned_backend_or_unknown_commit_wins_after_a_real_guard_refusal_rollback ... ok
test rebuild_propagates_provider_failure_and_revalidates_the_fresh_capture ... ok
test a_verified_head_is_never_reused_for_a_capture_that_differs_from_it ... ok
test binding_recovery_validates_the_complete_foreign_model_before_conflict ... ok
test open_rejects_every_mutated_authoritative_capture_shape ... ok
test binding_recovery_rejects_missing_malformed_and_substituted_capture_authority ... ok
test real_projector_failures_before_and_after_apply_roll_back_append_and_import ... ok

test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s

     Running tests/guarded_write_blob_binding.rs (target/debug/deps/guarded_write_blob_binding-83b7826271819f9e)

running 10 tests
test a_binding_lost_to_a_rival_binds_no_blob_on_sqlite_and_file ... ok
test a_failed_fallback_upload_keeps_each_writes_blob_error ... ok
test a_guard_refused_import_binds_no_blob_on_sqlite_and_file ... ok
test a_guard_refused_append_binds_no_blob_on_sqlite_and_file ... ok
test every_guarded_write_binds_its_blobs_with_its_group_on_sqlite_and_file ... ok
test every_guarded_write_commits_through_the_fallback ... ok
test an_import_raced_by_a_batch_hears_the_guard_through_override_and_fallback ... ok
test a_refused_append_does_not_count_its_unbound_blobs_against_the_read_bound ... ok
test a_re_admitted_retry_of_each_guarded_write_is_answered_as_its_commit ... ok
test a_guard_refused_append_reports_the_same_refusal_through_the_fallback ... ok

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.15s

     Running tests/import_batch_blob_binding.rs (target/debug/deps/import_batch_blob_binding-7b07b35ef0cf8d92)

running 1 test
test a_guard_refused_batch_binds_no_blob_on_the_sqlite_provider ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s

     Running tests/import_batch_record_identity.rs (target/debug/deps/import_batch_record_identity-1bcf645b68bbf572)

running 2 tests
test a_batch_refuses_a_record_identity_two_of_its_members_share ... ok
test a_batch_naming_one_subject_twice_settles_as_two_singular_calls_settle ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s

     Running tests/import_batch_retry_admission.rs (target/debug/deps/import_batch_retry_admission-87c284df4623fafa)

running 5 tests
test one_anchor_naming_a_record_identity_twice_is_refused ... ok
test the_guard_refuses_a_subject_another_writer_answered_for_under_different_bytes ... ok
test the_guard_refuses_a_record_identity_another_writer_committed_after_the_pre_capture ... ok
test a_batch_bearing_retry_is_admitted_over_the_commit_it_already_made ... ok
test the_guard_refuses_a_subject_its_anchor_started_and_something_has_since_appended_to ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s

     Running tests/per_entity_reads.rs (target/debug/deps/per_entity_reads-a33b2f2e4eaedd4a)

running 7 tests
test a_command_refuses_a_substituted_stream_identity_on_the_sqlite_provider ... ok
test a_command_refuses_a_substituted_stream_identity_on_the_tree_provider ... ok
test a_command_refuses_a_forged_index_row_of_its_own_subject_on_the_sqlite_provider ... ok
test a_command_on_one_subject_reads_only_its_own_stream_on_the_sqlite_provider ... ok
test a_command_refuses_a_forged_index_row_of_its_own_subject_on_the_tree_provider ... ok
test a_command_on_one_subject_reads_only_its_own_stream_on_the_tree_provider ... ok
test a_command_on_one_subject_reads_only_its_own_stream_on_the_file_provider ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s

     Running tests/provider_facades.rs (target/debug/deps/provider_facades-2cdb2f9041ebd9a9)

running 11 tests
test opening_an_absent_file_authority_does_not_create_provider_bytes ... ok
test opening_an_absent_sqlite_authority_does_not_create_provider_bytes ... ok
test legacy_import_revalidates_the_complete_source_before_any_destination_write ... ok
test opening_native_stores_without_er_bindings_preserves_provider_authority ... ok
test legacy_import_refuses_a_different_source_identity_for_an_identical_bare_boundary ... ok
test sqlite_memory_facade_uses_the_same_complete_surface ... ok
test legacy_import_retains_settled_progress_and_refuses_a_changed_source_boundary ... ok
test sqlite_file_provision_returns_the_exact_reopen_authority ... ok
test legacy_file_import_is_exact_resumable_and_provider_verified ... ok
test file_facade_preserves_atomic_groups_queries_retries_and_restart ... ok
test file_facade_groups_are_process_atomic_and_survive_a_publication_crash ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 19.73s

     Running tests/providers.rs (target/debug/deps/providers-2ef332b306f08368)

running 15 tests
test postgres_bridge_awaits_worker_owned_provider_retirement_when_assigned ... ok
test postgres_provider_provisions_replays_opens_and_rebuilds_when_assigned ... ok
test an_empty_batch_reaches_no_provider ... ok
test a_batch_naming_one_subject_twice_with_different_anchors_is_refused_before_any_write ... ok
test synchronous_bridge_operates_without_a_caller_runtime ... ok
test synchronous_bridge_owns_reopened_provider_and_reports_retirement ... ok
test synchronous_bridge_operates_inside_a_multithread_runtime ... ok
test a_refused_member_leaves_no_part_of_the_batch_committed ... ok
test synchronous_bridge_startup_refuses_missing_drifted_and_dirty_projection_admission ... ok
test a_batch_import_commits_through_the_port_default_on_the_sqlite_provider ... ok
test a_batch_import_writes_the_same_bytes_as_singular_imports_from_one_capture ... ok
test sqlite_memory_provider_provisions_replays_opens_and_rebuilds ... ok
test sqlite_file_provider_provisions_replays_opens_and_rebuilds ... ok
test tree_provider_provisions_replays_opens_and_rebuilds ... ok
test file_provider_provisions_replays_opens_and_rebuilds ... ok

test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.38s

     Running tests/reviewer_binding_unknown_conflict.rs (target/debug/deps/reviewer_binding_unknown_conflict-06c98a5180325eb3)

running 2 tests
test a_foreign_binding_is_not_a_conflict_until_its_derived_row_is_validated ... ok
test unknown_commit_recovery_preserves_a_conclusive_foreign_binding_conflict ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running tests/service_3_composition.rs (target/debug/deps/service_3_composition-3bd3469bd382ceea)

running 5 tests
test postgres_provider_preserves_complete_service_3_composition_when_assigned ... ok
test sqlite_bridge_crosses_service_3_retry_history_and_reopen ... ok
test sqlite_memory_provider_preserves_complete_service_3_composition ... ok
test sqlite_file_provider_preserves_complete_service_3_composition ... ok
test file_provider_preserves_complete_service_3_composition ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.40s

     Running tests/shared_clock_cost.rs (target/debug/deps/shared_clock_cost-2fb1c8a8f0dbdd18)

running 1 test
test warm_shared_clock_batches_remain_within_twice_small_store_cost ... ignored, release performance probe; run explicitly on a quiet host

test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/tree_branches.rs (target/debug/deps/tree_branches-676fbb82d076e08a)

running 10 tests
test two_branches_that_recorded_different_subjects_merge_into_one_store ... ok
test an_observation_on_one_branch_and_a_decision_on_the_other_do_not_fork_the_subject ... ok
test two_branches_that_only_observed_one_revision_do_not_fork_the_subject ... ok
test a_decision_after_an_observation_merged_beside_a_decision_extends_the_subject ... ok
test a_subject_both_branches_changed_is_forked_and_the_rest_of_the_store_still_serves ... ok
test a_decision_on_each_branch_still_forks_and_its_heads_are_the_decisions ... ok
test a_merge_decision_joins_a_forked_subject_and_it_serves_again_after_reopening ... ok
test a_fork_whose_observation_replays_after_the_other_branch_still_opens_as_forked ... ok
test a_refused_command_is_recorded_once_and_changes_no_subject ... ok
test an_observation_after_a_merge_decision_attaches_to_the_merge ... ok

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.44s

     Running tests/tree_text_blobs.rs (target/debug/deps/tree_text_blobs-5ee29b9322322ddd)

running 1 test
test a_first_layout_store_reads_the_same_after_its_texts_are_kept_once_and_keeps_recording ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.19s

   Doc-tests entity_eventlog

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

```

Exit 0: 186 passed, zero failed, one ignored performance probe. Formatting check exited 0 with no output. Provider source fingerprints matched before and after the run, and Cargo.lock was restored. `reviewed-source.sha256` identifies runtime/test bytes covered by this recheck. No additional finding remained. Read-only concurrency/import/lineage inspection limits remain as stated in the initial report; this is not an exhaustive proof of all schedules or malformed providers.

Outside-worktree artifacts: none.

```findings
[]
```
