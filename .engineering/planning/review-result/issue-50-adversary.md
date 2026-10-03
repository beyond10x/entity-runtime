---
format: aep.planning-md/3
id: review-result:issue-50-adversary
kind: review-result
status: active
title: Independent review of explicit executor version binding
relations:
- reviews: story:executor-input-refusal-before-existence
revision: 1
---
unit: story:executor-input-refusal-before-existence, issue-50-20261003 working tree over c6164443df9f303f4898f1de1abfc1c899e8c921
verdict: nothing found
cases: executed 35→38, red 0
origin: introduced 0 / pre-existing 0 / undecided 0
wrote-outside-worktree: none
needs-coordinator: execute adapter/ESS scenarios and integration gate

1. git --no-pager diff --stat

```text
 checks/ess-conformance/src/executor.rs |  91 ++++++++--
 crates/entity-executor/src/lib.rs      | 305 +++++++++++++++++++++++++++++----
 2 files changed, 350 insertions(+), 46 deletions(-)
```

These tracked changes were inherited from the implementor. The inherited.patch captured before review is byte-identical to the tracked diff after review (cmp exited 0). This reviewer changed no production file and no pre-existing test. Reviewer additions are untracked and shown by git diff --no-index --stat /dev/null crates/entity-executor/tests/version_binding_review.rs:

```text
 .../tests/version_binding_review.rs                | 184 +++++++++++++++++++++
 1 file changed, 184 insertions(+)
```

Path redaction: machine-specific absolute checkout prefixes in compiler output below are replaced with <worktree>; no other output or findings are transformed. Private raw logs remain in assigned scratch.

2. New cases, written before any execution

All three are in crates/entity-executor/tests/version_binding_review.rs. All green; no red output was produced. Each case ran alone before the package suite.

- divergent_registered_versions_select_only_the_requested_guard uses versions 1 and 2 with opposite refusal predicates and distinct outcome/error identifiers. Each exact version returns its own refusal without a load; unregistered version 3 returns EntityNotRegistered with version 3 and performs no load.
- explicit_and_row_bound_execution_persist_identical_bytes_and_replay_across_apis compares actual records written to two real stores by legacy and explicit APIs for both versions. Request bytes, complete entry and receipt are identical; retries cross API boundaries with an empty registry and preserve receipts; changed version is RecordConflict.
- accepted_wrong_version_in_local_batch_rolls_back_and_releases_all_identities creates version 2 locally then executes an accepted version-1 operation, requiring EntityMismatch. Neither record, batch receipt nor row is saved. Correcting version commits both actions and a registry-free retry returns the same receipt.

All commands below use CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=2.
Command: cargo test -p entity-executor --locked --test version_binding_review divergent_registered_versions_select_only_the_requested_guard -- --exact
Exit: 0
```text
   Compiling entity-executor v0.25.1 (<worktree>/crates/entity-executor)
    Finished `test` profile [unoptimized] target(s) in 0.50s
     Running tests/version_binding_review.rs (target/debug/deps/version_binding_review-42d95fd63a5cad46)

running 1 test
test divergent_registered_versions_select_only_the_requested_guard ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s

```
Command: cargo test -p entity-executor --locked --test version_binding_review explicit_and_row_bound_execution_persist_identical_bytes_and_replay_across_apis -- --exact
Exit: 0
```text
    Finished `test` profile [unoptimized] target(s) in 0.06s
     Running tests/version_binding_review.rs (target/debug/deps/version_binding_review-42d95fd63a5cad46)

running 1 test
test explicit_and_row_bound_execution_persist_identical_bytes_and_replay_across_apis ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.01s

```
Command: cargo test -p entity-executor --locked --test version_binding_review accepted_wrong_version_in_local_batch_rolls_back_and_releases_all_identities -- --exact
Exit: 0
```text
    Finished `test` profile [unoptimized] target(s) in 0.04s
     Running tests/version_binding_review.rs (target/debug/deps/version_binding_review-42d95fd63a5cad46)

running 1 test
test accepted_wrong_version_in_local_batch_rolls_back_and_releases_all_identities ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s

```

3. Package suite

Before count 35 comes from the implementor's report; this reviewer ran no suite before adding the probes. The resulting count is 38. All existing lanes retain their counts.
Command: CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=2 cargo test -p entity-executor --locked
Exit: 0
```text
    Finished `test` profile [unoptimized] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/entity_executor-8dfaba511738c0d0)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/async_recorded_contract.rs (target/debug/deps/async_recorded_contract-0ff004ea4fd67469)

running 9 tests
test exact_imported_retry_is_historical_and_missing_matching_facts_are_not_success ... ok
test stale_observation_in_a_mixed_batch_rolls_back_every_record_and_receipt ... ok
test complete_decisions_keep_duplicate_events_and_observations_keep_explicit_null_provenance ... ok
test global_record_ids_duplicate_requests_and_empty_batches_are_closed_before_writes ... ok
test changed_expectation_or_provenance_conflicts_with_the_original_retry ... ok
test commit_then_uncertain_and_dropped_response_recover_one_effect_and_receipt ... ok
test mixed_batches_use_ordered_local_state_and_observations_do_not_advance_revision ... ok
test retry_uses_saved_definition_and_verified_prefix_before_current_authority ... ok
test named_and_single_retries_preserve_original_receipt_coordinates ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running tests/input_refusal_precedence.rs (target/debug/deps/input_refusal_precedence-f78a1ca7f7ecaaf6)

running 13 tests
test executor_input_refusal_missing_execute ... ok
test recorded_refusal_keeps_explicit_definition_authority ... ok
test versioned_empty_and_invalid_shape_requests_perform_no_io ... ok
test versioned_merge_refusal_precedes_the_merge_history_read ... ok
test imported_execution_retry_checks_explicit_version_before_accepting_identity ... ok
test executor_input_refusal_existing_create ... ok
test versioned_refusal_precedes_existing_revision_and_definition_mismatch ... ok
test accepted_input_preserves_absence_revision_and_definition_checks ... ok
test versioned_batch_refusal_rolls_back_earlier_local_decisions ... ok
test legacy_committed_execution_can_be_retried_with_its_explicit_saved_version ... ok
test legacy_execute_keeps_row_authority_without_choosing_a_registered_version ... ok
test explicit_definition_does_not_require_registry_for_committed_retry ... ok
test named_mixed_batch_retry_uses_saved_versions_and_rejects_changed_binding ... ok

test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running tests/operation_fulfillment_review_1.rs (target/debug/deps/operation_fulfillment_review_1-591184104ce6323d)

running 1 test
test an_older_service_retry_does_not_ignore_new_fulfillment_coordinates ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/refusal_fulfillment_review.rs (target/debug/deps/refusal_fulfillment_review-30d16e3b6f2bfef3)

running 2 tests
test refusal_wins_over_required_field_removal_and_invalid_value_after_restart ... ok
test later_refusal_rolls_back_earlier_fulfillment_and_leaves_batch_identity_reusable ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running tests/service_2_retry.rs (target/debug/deps/service_2_retry-3136ec45f310d2f2)

running 1 test
test service_2_retry_distinguishes_absent_and_present_optional_arguments ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/service_3_fulfillment_retry.rs (target/debug/deps/service_3_fulfillment_retry-e82cdeb516e5a052)

running 5 tests
test refusal_ignores_success_fulfillments ... ok
test subject_refusal_ignores_success_fulfillments ... ok
test accepted_outcomes_still_require_exact_fulfillment_keys ... ok
test retry_restart_and_verified_history_preserve_set_and_removal_actions ... ok
test legacy_request_domains_compare_new_actions_in_single_and_batch_recovery ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s

     Running tests/service_binding_review_one.rs (target/debug/deps/service_binding_review_one-ad0275abebd220b6)

running 1 test
test service_2_retry_replays_the_same_present_value_after_canonical_normalization ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/service_branchless_retry.rs (target/debug/deps/service_branchless_retry-3559d39a28f19581)

running 3 tests
test a_branchless_service_1_retry_carrying_other_input_is_not_the_request_already_committed ... ok
test a_branchless_service_1_retry_reads_the_normalized_input_defaults_included ... ok
test two_branchless_service_1_creations_in_one_batch_keep_their_own_requests ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/version_binding_review.rs (target/debug/deps/version_binding_review-42d95fd63a5cad46)

running 3 tests
test divergent_registered_versions_select_only_the_requested_guard ... ok
test accepted_wrong_version_in_local_batch_rolls_back_and_releases_all_identities ... ok
test explicit_and_row_bound_execution_persist_identical_bytes_and_replay_across_apis ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

   Doc-tests entity_executor

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

```

Package clippy --all-targets --locked -- -D warnings exited 0, with the same bounded build environment. cargo fmt --all -- --check and git diff --check exited 0 with empty output.

4. Judgement findings

Nothing found. Findings cover the working tree identified above. Full production and adapter diff, all new tests, all five new ESS scenario documents, acceptance, and callers of the changed execution/recovery paths were read. Adapter and ESS execution were not run in this package-only review and remain with the coordinator.

5. Attacks that did not break

Divergent version definitions preserved exact requested authority, including lower registered and absent versions.
Direct legacy-versus-explicit execution comparison preserved canonical request bytes, complete decision entry and receipt for versions 1 and 2.
Accepted wrong-version batch execution preserved atomicity and did not consume record or batch identities.
The existing committed and imported retry cases ran and preserved exact version matching with an empty registry; changed versions conflicted and missing imported definitions remained unverifiable.
The existing input-refusal tests ran and pinned trace absence of state load or merge-history read, while creation-over-existing, malformed shape, accepted absence/revision conflicts and recorded refusal version behavior remained covered.
The adapter checks presence before typed deserialization: null/string versions cannot fall back silently; batch execute/merge members in versioned mode require explicit versions, and create/observe members reject an extraneous definition_version. This is source-level inspection only.
The new merge scenario asserts the selected base and both branch tips at a capturing port that refuses append. It makes no durable-merge claim. Separate durable merge execution was not exercised by this reviewer.

6. Outside-worktree writes

None. Scratch logs/report/inherited diff are under target/issue-50-scratch/review; compiler output is this checkout's target. The worktree CLI maintains its mandatory lease registry; no outside files were manually written.

```findings
[]
```
