---
format: aep.planning-md/3
id: review-result:issue-49-adversary-public
kind: review-result
status: active
title: Issue 49 adversarial verification with machine paths redacted
relations:
- reviews: story:declared-refusal-before-fulfillment-validation
revision: 1
---
unit: story:declared-refusal-before-fulfillment-validation, issue-49-20261003 working tree over 72a9c36fbd58baee10be844a170649692772e51a
verdict: nothing found
cases: executed 20→22, red 0
origin: introduced 0 / pre-existing 0 / undecided 0
wrote-outside-worktree: none
needs-coordinator: run admitted ESS scenarios and integration gate

Path redaction: machine-specific absolute checkout prefixes in compiler output are replaced with <worktree>; all other test output and findings are unchanged.

1. git --no-pager diff --stat

```text
 crates/entity-executor/src/lib.rs                  |  20 ++-
 .../tests/service_3_fulfillment_retry.rs           | 135 ++++++++++++++++++++-
 2 files changed, 141 insertions(+), 14 deletions(-)
```

These tracked edits were inherited from the implementor; the captured inherited.patch remains byte-identical to git diff --binary after review (cmp exit 0). No production source was edited by this review. Reviewer additions are untracked and shown by git diff --no-index --stat /dev/null crates/entity-executor/tests/refusal_fulfillment_review.rs:

```text
 .../tests/refusal_fulfillment_review.rs            | 144 +++++++++++++++++++++
 1 file changed, 144 insertions(+)
```

2. Added cases, written before any runner execution

File: crates/entity-executor/tests/refusal_fulfillment_review.rs. Both green; no red output was produced.

- later_refusal_rolls_back_earlier_fulfillment_and_leaves_batch_identity_reusable: a successful first batch action creates the state that selects a refusal on the next action. The second action supplies an invalid array value and unknown removal key. Exact refusal outcome/error/message survives, repeated batch attempts leave state/history and both record identities untouched, and a corrected batch reuses its identity successfully. Exact retry returns the same receipt and only three history records exist.
- refusal_wins_over_required_field_removal_and_invalid_value_after_restart: required-field removal, null and object values all preserve the stored-field refusal after constructing a fresh executor; no state change or record appears.

Individual runs occurred in this order before the package suite. Both commands used CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 and --locked.

Command: cargo test -p entity-executor --locked --test refusal_fulfillment_review later_refusal_rolls_back_earlier_fulfillment_and_leaves_batch_identity_reusable -- --exact
Exit: 0
```text
   Compiling entity-executor v0.25.1 (<worktree>/crates/entity-executor)
    Finished `test` profile [unoptimized] target(s) in 0.38s
     Running tests/refusal_fulfillment_review.rs (target/debug/deps/refusal_fulfillment_review-30d16e3b6f2bfef3)

running 1 test
test later_refusal_rolls_back_earlier_fulfillment_and_leaves_batch_identity_reusable ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.01s

```
Command: cargo test -p entity-executor --locked --test refusal_fulfillment_review refusal_wins_over_required_field_removal_and_invalid_value_after_restart -- --exact
Exit: 0
```text
    Finished `test` profile [unoptimized] target(s) in 0.03s
     Running tests/refusal_fulfillment_review.rs (target/debug/deps/refusal_fulfillment_review-30d16e3b6f2bfef3)

running 1 test
test refusal_wins_over_required_field_removal_and_invalid_value_after_restart ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.00s

```

3. Package suite

Before count 20 comes from the implementor report; no pre-addition suite was run by this reviewer.
Command: CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 cargo test -p entity-executor --locked
Exit: 0
```text
    Finished `test` profile [unoptimized] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/entity_executor-8dfaba511738c0d0)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/async_recorded_contract.rs (target/debug/deps/async_recorded_contract-0ff004ea4fd67469)

running 9 tests
test stale_observation_in_a_mixed_batch_rolls_back_every_record_and_receipt ... ok
test exact_imported_retry_is_historical_and_missing_matching_facts_are_not_success ... ok
test global_record_ids_duplicate_requests_and_empty_batches_are_closed_before_writes ... ok
test complete_decisions_keep_duplicate_events_and_observations_keep_explicit_null_provenance ... ok
test changed_expectation_or_provenance_conflicts_with_the_original_retry ... ok
test mixed_batches_use_ordered_local_state_and_observations_do_not_advance_revision ... ok
test retry_uses_saved_definition_and_verified_prefix_before_current_authority ... ok
test commit_then_uncertain_and_dropped_response_recover_one_effect_and_receipt ... ok
test named_and_single_retries_preserve_original_receipt_coordinates ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running tests/operation_fulfillment_review_1.rs (target/debug/deps/operation_fulfillment_review_1-591184104ce6323d)

running 1 test
test an_older_service_retry_does_not_ignore_new_fulfillment_coordinates ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running tests/refusal_fulfillment_review.rs (target/debug/deps/refusal_fulfillment_review-30d16e3b6f2bfef3)

running 2 tests
test refusal_wins_over_required_field_removal_and_invalid_value_after_restart ... ok
test later_refusal_rolls_back_earlier_fulfillment_and_leaves_batch_identity_reusable ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running tests/service_2_retry.rs (target/debug/deps/service_2_retry-3136ec45f310d2f2)

running 1 test
test service_2_retry_distinguishes_absent_and_present_optional_arguments ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running tests/service_3_fulfillment_retry.rs (target/debug/deps/service_3_fulfillment_retry-e82cdeb516e5a052)

running 5 tests
test subject_refusal_ignores_success_fulfillments ... ok
test refusal_ignores_success_fulfillments ... ok
test accepted_outcomes_still_require_exact_fulfillment_keys ... ok
test retry_restart_and_verified_history_preserve_set_and_removal_actions ... ok
test legacy_request_domains_compare_new_actions_in_single_and_batch_recovery ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s

     Running tests/service_binding_review_one.rs (target/debug/deps/service_binding_review_one-ad0275abebd220b6)

running 1 test
test service_2_retry_replays_the_same_present_value_after_canonical_normalization ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running tests/service_branchless_retry.rs (target/debug/deps/service_branchless_retry-3559d39a28f19581)

running 3 tests
test two_branchless_service_1_creations_in_one_batch_keep_their_own_requests ... ok
test a_branchless_service_1_retry_carrying_other_input_is_not_the_request_already_committed ... ok
test a_branchless_service_1_retry_reads_the_normalized_input_defaults_included ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

   Doc-tests entity_executor

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

```

cargo fmt --check also exited 0. Package clippy --all-targets --locked -- -D warnings exited 0 after the additions, with the same bounded build environment.

4. Judgement findings

Nothing found. Findings cover the working tree identified above. No conformance execution is claimed: new YAML documents were read in full and their expected refusal/state/receipt/history values compared with the executor implementation and tests; the integration coordinator owns their actual ESS admission and execution.

5. Attacks that did not break

Exact declared refusal precedence held against malformed and unknown fulfillment actions, including required-field removal.
Two-action batches with a successful first decision and later refusal left no partial write or consumed identity.
Corrected retry committed both actions and exact successful retry preserved its receipt and complete history length.
Existing accepted-outcome missing/extra-key tests and successful fulfillment/retry controls still executed and passed.
Caller review: decide_and_append delegates every Execute action to the changed decide branch; merge also delegates there. A separate fork/merge fixture was not exercised.
No kernel source, IO boundary, unsafe declaration, dependency set, public API, or ordering collection changed in this unit.

6. Outside-worktree writes

None. Scratch logs/report/inherited diff are under target/issue-49-scratch/review; compiler output is this checkout's target. The worktree CLI manages its mandatory lease registry outside the checkout; no outside files were manually written.

```findings
[]
```
