# Full review and its corrections — 2026-09-08

A full read of the repository at `origin/main` `e5ee9d6c` (the 0.17.7 tree plus documentation
commits): every crate's sources, the gate scripts, the workflows and the website configuration,
with the full gate run against PostgreSQL 17. It found no gate failure. It found one kernel
guarantee that the tests did not hold up, one broken verb under an environment override, one gate
guard checking the wrong binary, and a set of documentation and comment drift. Two independent
reviewers then read the corrections; they found one blocker introduced by a correction, one
pre-existing hole in the same threat model, and nine smaller items. Everything below was corrected
in one change set and is pinned by a test where a test can pin it.

## First review — findings and corrections

| Finding | Correction | Regression evidence |
|---|---|---|
| 1. `rehydrate` skipped invariants: a history whose event had its `changed` blanked folded to a `closed` ticket without a resolution, a state `execute` refuses | Per-event invariant evaluation after each event is installed, the creation event included | `a_history_folding_to_an_instance_a_declared_invariant_refuses_is_refused` |
| 2. `entity generate rust-cli` read the binary from `<build-dir>/target/release`, so `CARGO_TARGET_DIR` (or `build.target-dir`) broke it; the gate test itself failed with the variable exported | See "Independent review" below — the first correction was itself wrong | `generated_rust_cli_compiles_and_executes_definition_specific_commands`, now run with a relative `--build-dir`, a misdirected `CARGO_TARGET_DIR` and a host `CARGO_BUILD_TARGET` |
| 3. `plan-check` version-checked a binary named `protocol` while the next line ran `aep` | `entity-xtask aep-version` checks `aep`; the Taskfile hands it `$(command -v aep)` so the guard and the validator resolve the same file | `an_aep_version_line_is_extracted_without_accepting_partial_numbers`; `task plan-check` prints the checked path |
| 4. `entity-cli` depended on `entity-shell` without using it | Removed, then re-added when the store-backed verbs were routed through the shared shell (below) | `cargo clippy --workspace --all-targets --locked -D warnings` |
| 5. `release.yml` said `v*` tags were accepted; the trigger glob cannot match one | Comment corrected; the `${TAG#v}` strip removed | `on.push.tags` is `[0-9]*.[0-9]*.[0-9]*` |
| 6. CI ran `postgres:16` while the 2026-09-05 review named PostgreSQL 17 as the required validation | `gate.yml` uses `postgres:17`; the provider suite was run against 17.11 locally | 12 provider tests green against a `postgres:17` container |
| 7. A literal `before`/`after` operand nothing could read (`2026-13-01`, an offset, a number) registered and then refused every evaluation as unobservable naming nothing | Refused at registration as an invalid rule naming the operand and the forms that are read; nested `$` references are still walked first so a typo is named at its path | `a_before_or_after_rule_with_a_literal_the_kernel_cannot_read_is_refused_at_registration`, `a_before_or_after_rule_reading_a_literal_instant_against_a_reference_registers_and_decides` |
| 8. A run of spaces in a hybrid divergence message; `WIRE_VERSION` documented `/2` and `/3` but not `/4`; `Ask` said "four things" for eight variants | Message fixed; `/4` attributed to 0.15.0 (`70c4167`) with the variants it added; count removed | `under_local_authority_and_refuse_a_replica_that_accepted_while_the_authority_refused_is_recorded_as_a_divergence`, `a_recorded_commit_the_replica_accepted_and_the_authority_refused_is_recorded_as_a_divergence` |

## Independent review of the corrections — findings and corrections

| Finding | Correction | Regression evidence |
|---|---|---|
| Blocker: the generator passed `--target-dir` relative while Cargo's cwd was the build directory, so the default `build/entity-runtime/<name>` resolved twice and the documented invocation failed | The build directory is made absolute once; the executable is taken from Cargo's own `compiler-artifact` message instead of a guessed path, which also follows a `CARGO_BUILD_TARGET` triple | the generator test above; the documented invocation run from a scratch directory |
| The fold accepted an event whose `changed` contradicted the `set:` its own `args` determine | One operation among those declaring the transition and emitting the type must accept the arguments (schema, then preconditions) and reproduce `changed` from its `set:`; a creation event's `changed` must equal its recorded fields | `a_forged_event_whose_changed_contradicts_what_its_own_arguments_would_have_set_is_refused`, `a_creation_event_whose_changed_differs_from_its_recorded_fields_is_refused`, `a_forged_event_carrying_an_argument_its_operation_does_not_declare_is_refused` |
| The per-event invariant check ran before the schema check, inverting § 6's order and misnaming a wrong-typed field as an invariant violation | Schema validation per event, ahead of the invariants; the end-of-fold check removed | `a_forged_field_of_the_wrong_type_is_refused_by_the_schema_not_the_invariant_that_reads_it`, `an_honest_history_through_set_defaults_and_invariants_folds_to_the_executed_instance` |
| The fold treated each event as a decision: an operation with two `emits` could not fold (its second event was "a revision gap") and no payload was ever compared, so in a definition without `set:` — the AEP model — any account of who acted folded | A revision is folded as one decision: its events must agree, be exactly the emitting operation's `emits` in order, and each payload must resolve from its template in the context `execute` used; compared last, as `execute` materialises events last | `an_honest_history_of_an_operation_emitting_two_events_folds_to_the_executed_instance`, `a_forged_payload_on_an_operation_event_is_refused`, `a_forged_payload_on_the_creation_event_is_refused`, `two_events_at_one_revision_that_describe_different_decisions_are_refused`, `a_history_missing_one_of_the_two_events_a_decision_emitted_is_refused` |
| A third review of that regrouping: the first candidate operation matching arguments and `set:` was held to its own payload template, so where two operations differ only in what they emit, an honest history of the second was refused; three added guards (payloads past the first event of a revision, two creation events, identity of a revision's later events) had no test that failed when they were weakened; the design said a silent decision leaves "nothing to refuse", which is true only when it is the last one | Every candidate is kept and the payloads decide between them; the reviewer's probes adopted as tests and each of the three mutations watched to fail; the design and module doc say a silent decision followed by an emitting one is refused as a gap | `an_honest_history_of_the_second_of_two_payload_distinguished_operations_folds`, `a_forged_payload_on_the_last_event_of_a_revision_is_refused`, `two_creation_events_at_revision_one_are_refused`, `an_event_of_another_instance_at_the_second_slot_of_a_revision_is_refused`, `a_silent_decision_followed_by_an_emitting_one_makes_the_history_unfoldable_from_there` |
| Guide drift after the CLI change: `storage.md` still said a rerun "decides again … `invalid_transition`" (it is now `record_conflict` before the kernel); `cli.md` documented the store refusal shape without `kind`; `execute --store` on a missing id moved from exit 2 to exit 1 without a changelog line; `--instance` beside `--store` was silently ignored; the `emit` alias is a list on operations and a mapping under `create` | Sentences corrected; changelog extended; `--instance` now conflicts with `--store`; the alias shape and the zero-`emits` blind spot stated beside `emits` | `cargo test -p entity-cli`; `task site-build` |
| Nits: `gate.yml` header named one local-only step of three; "sixteen variants" for fifteen (design and R-55); "two things" for four; a doc claim about `$$` literals; `MINIMUM_AEP` holding a protocol version; a bare `is_err()` in a test R-97 cites; a divergence branch no test reached; a planning story citing `postgres:16`; the release recipe's hyphen where provenance requires an em dash | Each corrected in place; the story through `aep artifact body` | the hybrid tests above; `python3 scripts/check-requirements.py` |

## Follow-ups tracked and closed in the same change

| Task | Correction | Regression evidence |
|---|---|---|
| `task:fold-refuses-unemitted-event-types` | An operation event whose type no operation emits on its transition, and a creation event whose type the definition does not emit on creation, are refused instead of being checked for the transition only | see the R-97 row |
| `task:plan-check-guards-the-shell-aep` | `aep-version --binary "$(command -v aep)"`: the shell resolves the binary the next line runs; an empty value is refused as "not on the shell's PATH" | `task plan-check` output |
| `task:generic-cli-store-verbs-share-the-shell` | `entity create --store` and `entity execute --store` go through `StoredRuntime`, gaining the exact-retry rule and an optional `--expected-revision` | see the CLI tests named in `crates/entity-cli/tests/cli.rs` |

## Validation

`task check` with `ENTITY_POSTGRES_URL` naming a PostgreSQL 17.11 container, and `task site-build`,
both exit 0 on the final tree. Every guard added here was verified by breaking it — the one-line
mutation it exists to catch was applied, a test watched to fail naming the defect, and the mutation
reverted. Three of the fold's guards passed that check only after the third review supplied the
tests that reach them; the mutations were re-run against the adopted tests before this record was
written.

## Boundaries

The fold is now as strict as `execute` for what events can carry: a legacy history is accepted only
if the current definition could have produced every event in it, payloads included. What events
cannot carry — an operation that emits nothing — the fold cannot see, and the guide says so. A history recorded under a definition whose `set:`,
emitters, argument schemas or invariants have since changed is refused, naming the first event that
no longer fits. That is the intended reading of R-97 — a fold reaches no state `execute` would not
have permitted — and it is a change for any consumer folding old histories against a newer
definition; the changelog says so. Verified decision replay (`replay`) is unchanged.
