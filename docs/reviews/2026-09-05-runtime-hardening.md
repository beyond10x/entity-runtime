# Runtime review corrections — 2026-09-05

The full review covered kernel evaluation, validation and replay; provider state, events and
recorded history; queries; shells; and generated contracts. It reproduced fourteen correctness
failures against 0.17.5 despite the existing gate passing. The corrections were applied on top of
0.17.6 for release 0.17.7, preserving its File Store record-index optimization.

## Findings and regression evidence

| Finding | Correction | Regression evidence |
|---|---|---|
| 1. File Store loses acknowledged concurrent updates | A process-safe root lock covers initialization, record-ID lookup, expected revision and publication; an epoch invalidates caches across writers | `separate_writers_preserve_exactly_one_winning_revision_and_all_its_records`, `separate_processes_serialize_revision_checks_and_survive_a_killed_lock_holder`, `an_existing_handle_invalidates_its_index_after_another_writer` |
| 2. Zero sorts above positive fractions | Exact decimal comparison handles zero explicitly and arbitrary exponent arithmetic without overflow | `integers_and_decimals_compare_without_losing_precision`, `zero_bounds_order_positive_and_negative_fractions_mathematically` |
| 3. Hybrid catch-up discards recorded provenance | Copy exact envelopes and interleave observations before advancing; retain unsafe repairs as divergences | `catch_up_preserves_exact_recorded_envelopes_and_observations_between_revisions`, `equal_state_does_not_hide_a_missing_observation`, `missing_evidence_behind_the_destination_keeps_its_divergence_for_repair` |
| 4. A caught PostgreSQL batch conflict commits its prefix | A nested transaction/savepoint rolls back the batch while preserving the outer session | `a_caught_session_batch_conflict_rolls_back_its_prefix_but_keeps_the_transaction_usable` |
| 5. Abandoned temporary files block File Store | Enumeration and index scans ignore recognizable incomplete subject temporaries | `abandoned_subject_temporary_files_do_not_hide_ids_or_block_recorded_writes` |
| 6. Parent and marker symlinks escape read confinement | Check every existing component on reads and writes | `parent_and_marker_symlinks_are_refused_on_reads` |
| 7. Mixed write APIs return events out of revision order | Stable revision ordering across plain and recorded events in each provider | `verify_recorded`, invoked by every shipped provider's conformance tests |
| 8. PostgreSQL sessions omit recorded events | Merge session-visible recorded history with plain events | `session_events_include_recorded_and_plain_writes_in_revision_order` |
| 9. Concurrent exact PostgreSQL retries conflict | Lock the record identity before the duplicate lookup and subject write | `concurrent_identical_decisions_and_observations_are_idempotent` |
| 10. Shared-shell retries fail as stale intent | Compare normalized command and complete recording against accepted history before checking current revision | `an_exact_execute_retry_returns_the_original_commit_after_state_has_advanced` |
| 11. Query numeric equality differs between Memory and PostgreSQL | Share exact mathematical number comparison with recursive containment | `containment_compares_numeric_values_across_json_representations`, `memory_and_postgres_match_equivalent_numeric_json_values` |
| 12. Overlapping generated schemas reject valid requests/events | Use inclusive unions; condition creation fields on selected version | `overlapping_versions_accept_valid_requests_and_creation_checks_the_selected_version`, `overlapping_emitters_accept_real_events_in_the_projected_schema` |
| 13. Generated identifiers collide | Injective encoding for ambiguous names and composed event identities | `punctuation_case_and_composed_names_never_alias_contract_components` |
| 14. File Store drops equal repeated emissions | Preserve every event occurrence | `verify_recorded` exercises repeated equal emissions through both write APIs |

Tests use real independent File Store processes, a killed lock holder, independent PostgreSQL
connections, a deliberately unavailable loopback replica, and an offline JSON Schema evaluator.
Restoring the pre-fix File Store, PostgreSQL and Hybrid implementations makes the new regression
cases fail with the original data-loss, atomicity and history defects. The repaired implementations
are restored before the release gate.

## Validation and boundaries

The required validation is `task check` with `ENTITY_POSTGRES_URL` pointing at PostgreSQL 17,
`cargo +1.85.0 check --workspace --all-targets --locked`, and `task site-build`. Release builds also
run native File Store persistence/concurrency tests on Linux, macOS and Windows before packaging.
The schema evaluator's transitive IDNA/ICU versions are locked to retain the declared Rust 1.85
minimum; no schema test performs network resolution.

File Store locking requires all concurrent writers to use 0.17.7 or later and a filesystem that
supports advisory locks and atomic rename. The epoch file grows by one byte per attempted write.
Subject contents are flushed on every platform; directory flushes provide the stronger Unix
power-loss boundary. Windows directory-entry persistence across power loss is not promised.
External processes must not replace store paths or bypass the provider's locking protocol.

Hybrid catch-up retains missing legacy prefixes, historical gaps and observations behind the
replica's current revision for explicit repair. It does not rewrite accepted history or invent
provenance. Custom store wrappers must forward `Store::history` to support recorded catch-up and
shared-shell retry lookup. Atomic recorded batches and a unified mixed-history cursor remain
separate API extensions; this patch changes neither the remote wire version nor the subject format.
