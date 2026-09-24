---
format: aep.planning-md/2
id: review-result:service-binding-boundary-source-pass-1
kind: review-result
status: active
title: Service binding boundary source examination
relations:
- reviews: task:service-binding-boundary
revision: 1
---
unit: service binding boundary at commit 5da72ebd0cb343912cfdcc4adbd54a8c9e16b562
verdict: nothing found
cases: executed 21→27, red 0
origin: introduced 0 / pre-existing 0 / undecided 0
wrote-outside-worktree: 1 path
needs-coordinator: none

1. `git --no-pager diff --stat`

```text
 .../tests/service_binding_review_one.rs            | 171 +++++++++++++++++++++
 .../tests/service_binding_review_one.rs            |  63 ++++++++
 .../tests/service_binding_review_one.rs            |  98 ++++++++++++
 3 files changed, 332 insertions(+)
```

All three paths are within the brief's test-only write scope. `git diff --check` exited 0. The
worktree remains at submitted commit `5da72ebd0cb343912cfdcc4adbd54a8c9e16b562`, tree
`7c30534a25b7e2afa665cee3725af4c8922bcf10`. The accepted design SHA-256 is
`8861b04214e0d27d38fa555bf5930bc98612f82d2e5f2f4f2a435853067b2e76`.

2. Cases added

- `crates/entity-core/tests/service_binding_review_one.rs`
  - `service_2_operation_conditional_outputs_preserve_absent_present_and_null`: drives a
    `service/2` operation, not creation, through absent, present-null and present-object event and
    response members; compares the continued result with direct `decide`. Green now; first valid
    focused execution: 1 passed, 0 failed, exit 0.
  - `preparation_keeps_registry_identity_operation_and_argument_error_precedence`: pins free-API
    identity-before-operation-before-arguments precedence and the runtime registry lookup before
    identity validation. Green now; first focused execution: 1 passed, 0 failed, exit 0.
  - `outer_kleene_domination_settles_a_quantifier_whose_collection_needs_subject`: covers both
    child orders for outer `any(true, subject-quantifier)` and `all(false, subject-quantifier)`.
    Green now; first focused execution: 1 passed, 0 failed, exit 0.
- `crates/entity-store/tests/service_binding_review_one.rs`
  - `branchless_service_2_creation_reconstructs_the_exact_request_under_request_3`: proves two
    different branchless creation inputs remain different `/3` request bytes and reconstruct as
    `arguments`, with no `fields` key. Green now; first focused execution: 1 passed, 0 failed,
    exit 0.
  - `service_2_execute_reconstructs_request_3_with_normalized_arguments`: proves an execute request
    uses `/3`, predecessor revision 1 and its defaulted normalized arguments. Green now; first
    focused execution: 1 passed, 0 failed, exit 0.
- `crates/entity-executor/tests/service_binding_review_one.rs`
  - `service_2_retry_replays_the_same_present_value_after_canonical_normalization`: proves the same
    present optional object replays when caller object order differs. Green now; first valid focused
    execution: 1 passed, 0 failed, exit 0.

There was no behavioral red output. Test file SHA-256 values:

```text
9a8cfb095d4696dd629d33d355a3b49f5fefbc8fd2d0c676c882400213142c6e  crates/entity-core/tests/service_binding_review_one.rs
c73970753c464c8582f4f006313273181baaaf5373e671885f73614e0eb9390c  crates/entity-store/tests/service_binding_review_one.rs
af93f7c8bcec88a92b8d794155b71f7d73cd230b783ffdb95f8783980cb20a98  crates/entity-executor/tests/service_binding_review_one.rs
```

3. Suite runs after the cases existed

Environment for every Cargo command: Rust/Cargo 1.98.1, `--locked --offline`, two jobs, lld,
wrappers empty, debug and incremental disabled, `CARGO_TARGET_DIR` and encoded flags unset, and
`TMPDIR=$PWD/target/binding-review-tmp`.

Core focused command:

```console
env -u CARGO_TARGET_DIR -u CARGO_ENCODED_RUSTFLAGS RUSTC_WRAPPER= RUSTC_WORKSPACE_WRAPPER= SCCACHE_WRAPPER= CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS='-C linker=clang -C link-arg=-fuse-ld=lld' TMPDIR=$PWD/target/binding-review-tmp cargo test --locked --offline -p entity-core --test service_binding_boundary --test service_binding_review_one
```

```text
running 15 tests
test a_state_guard_is_not_evaluated_before_the_subject_is_loaded ... ok
test a_preload_scan_stops_at_the_first_subject_dependent_branch ... ok
test a_nonpositive_payment_refuses_before_any_subject_load ... ok
test an_unknown_input_guard_refuses_without_trying_a_later_default ... ok
test a_positive_payment_requires_the_exact_subject_and_continues_in_er ... ok
test kernel_1_and_service_1_keep_their_definitions_decisions_and_refusal_order ... ok
test unloaded_subject_existence_is_not_confused_with_an_absent_field ... ok
test a_prepared_operation_binds_definition_input_operation_and_identity ... ok
test kleene_dominating_guards_refuse_without_loading_an_unknown_subject ... ok
test one_optional_argument_reuses_one_presence_and_value_across_state_event_and_response ... ok
test preload_quantifiers_preserve_empty_collections_and_available_binders ... ok
test implicit_kernel_and_branchless_service_operations_preload_then_match_direct_decide ... ok
test conditional_presence_distinguishes_absent_present_and_null_in_all_output_positions ... ok
test service_2_replay_recomputes_presence_and_refuses_every_tampered_position ... ok
test conditional_presence_refuses_wrong_types_and_missing_required_neighbors_before_selection ... ok

test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

running 3 tests
test preparation_keeps_registry_identity_operation_and_argument_error_precedence ... ok
test outer_kleene_domination_settles_a_quantifier_whose_collection_needs_subject ... ok
test service_2_operation_conditional_outputs_preserve_absent_present_and_null ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Store focused command:

```console
env -u CARGO_TARGET_DIR -u CARGO_ENCODED_RUSTFLAGS RUSTC_WRAPPER= RUSTC_WORKSPACE_WRAPPER= SCCACHE_WRAPPER= CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS='-C linker=clang -C link-arg=-fuse-ld=lld' TMPDIR=$PWD/target/binding-review-tmp cargo test --locked --offline -p entity-store --test service_2_framing --test service_binding_review_one
```

```text
running 5 tests
test an_er_record_2_reader_refuses_er_record_3_before_reading_its_payload ... ok
test an_er_request_2_reader_refuses_er_request_3_before_reading_its_payload ... ok
test existing_record_and_request_literal_tags_are_unchanged ... ok
test service_2_uses_literal_record_3_and_request_3_framing ... ok
test mixed_record_domains_remain_members_of_er_batch_1 ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

running 2 tests
test service_2_execute_reconstructs_request_3_with_normalized_arguments ... ok
test branchless_service_2_creation_reconstructs_the_exact_request_under_request_3 ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Executor focused command:

```console
env -u CARGO_TARGET_DIR -u CARGO_ENCODED_RUSTFLAGS RUSTC_WRAPPER= RUSTC_WORKSPACE_WRAPPER= SCCACHE_WRAPPER= CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS='-C linker=clang -C link-arg=-fuse-ld=lld' TMPDIR=$PWD/target/binding-review-tmp cargo test --locked --offline -p entity-executor --test service_2_retry --test service_binding_review_one
```

```text
running 1 test
test service_2_retry_distinguishes_absent_and_present_optional_arguments ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

running 1 test
test service_2_retry_replays_the_same_present_value_after_canonical_normalization ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

The combined focused result is the header's executed 21→27, red 0. The affected package command
then ran every `entity-core`, `entity-store`, and `entity-executor` unit, integration and doc test:

```console
env -u CARGO_TARGET_DIR -u CARGO_ENCODED_RUSTFLAGS RUSTC_WRAPPER= RUSTC_WORKSPACE_WRAPPER= SCCACHE_WRAPPER= CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS='-C linker=clang -C link-arg=-fuse-ld=lld' TMPDIR=$PWD/target/binding-review-tmp cargo test --locked --offline -p entity-core -p entity-store -p entity-executor
```

It exited 0 with 367 passed, 0 failed across its emitted test binaries. No PostgreSQL or full
repository gate was run. At close, no Cargo/rustc/linker process from this review remained; disk
had 68 GiB available and `MemAvailable` was 49,453,212 KiB.

4. Judgement findings

None. The finding set covers commit `5da72ebd0cb343912cfdcc4adbd54a8c9e16b562` after two and no
more than two whole-source passes. No base-origin classification was needed because no finding
holds.

5. Attacked and not broken

- The exact 20-file base-to-submission diff and every changed runtime, validation, replay, store,
  executor and CLI caller were read; the diff stream SHA-256 was
  `a6c75783d2dfdead5d9fbb89d141565a2d219075010e8406196ec8267224429a`.
- Pre-load lookup and normalization order, implicit kernel/service paths, ordered state guards,
  subject-dependent branch stopping, partial all/any/not/quantifier logic, binder availability,
  opaque continuation contents and entity/id/state precedence were attacked.
- `service/2` closed registration, nested optional leaf rules, complete target equality, conflicts,
  creation state presence, operation event/response presence, absent versus null and canonical
  object values were attacked.
- Complete replay, branchless creation reconstruction, execute reconstruction, request `/3`, record
  `/3`, unchanged batch `/1`, older-reader tag refusal, absent/present retry identity and canonical
  same-request replay were attacked.
- The accepted design, original `service/1` semantics, recorded-execution contract, complete literal
  fixture and producer acceptance/control evidence were compared with source; no contradiction was
  found.

6. Paths written outside the worktree

- `home-path:sha256:26f38cb13e9357703f017ca8fe263a042625cd49bf8994405ec4d2114fa2d0c5`

```findings
[]
```
