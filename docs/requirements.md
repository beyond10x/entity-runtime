# Requirements register

Every requirement the runtime must satisfy, with what pins it. This is the document the
implementation is measured against; `scripts/check-requirements.py` (step `req-check` of the gate)
fails when a requirement is not referenced by a design under `docs/design/`, or cites something
that is not a live `#[test]` function.

**Provenance.** The requirements were extracted from a proof of concept the operator designed on
2026-08-25: a schema-driven entity runtime with an IO-free Rust core and YAML-declared types,
delivered as a three-crate workspace with a README and a test suite. That workspace was never
compiled by its author; it was reconstructed here, compiled, and its tests pass unchanged
(`crates/entity-yaml/tests/runtime.rs`). The requirements below hold for this implementation
whether or not it resembles the proof of concept in code. R-90 to R-95 were added by the operator
afterwards: the system offers a Rust library crate **and** a CLI layer.

**Rows whose requirement begins ✎ were rewritten after the 0.1.0 adversarial review**
([`docs/reviews/2026-08-25-adversarial-review.md`](reviews/2026-08-25-adversarial-review.md)),
either because the code grew to meet them or because the row claimed more than the code enforced.
Rows beginning ✚ are new refusals that review produced.

**Rows whose requirement begins ⟳ were revised when three-valued rule evaluation shipped**
(`story:three-valued-conditions`). The wording each one replaced is quoted beneath its table, so a
claim this register once made is not silently gone: a requirement that changes has to be readable
as a change.

**How to read the `pinned by` column.** A backticked name is a `#[test]` function under `crates/`
and must exist and not be `#[ignore]`d. `type` means the property is closed by the type system —
there is no API that could violate it — and the design names the type. `manifest` means a
`Cargo.toml` is the evidence. `design` alone means nothing mechanical pins it yet; that is a gap,
and each one is a story.

## Kernel purity and determinism

| id | requirement | pinned by |
|---|---|---|
| R-01 | ✎ The kernel has no filesystem, network, clock, identifier generator, random source, async runtime or storage access. Nothing in `entity-core` can perform IO — including through a grouped import, an alias, a macro or an unordered collection. | `the_kernel_reaches_no_clock_filesystem_network_or_random_source`, `the_kernel_depends_on_serialisation_and_nothing_else`, `the_scan_sees_every_evasion_it_is_meant_to_see`, `the_scan_does_not_fire_on_prose_or_lookalikes` |
| R-02 | Same definition, instance, operation and arguments produce the same `Decision`, every time. Timestamps and identifiers are inputs the shell supplies; the kernel never manufactures one. | `the_same_inputs_produce_the_same_decision_byte_for_byte`, `an_unresolvable_template_reference_is_an_error_not_a_null` |
| R-03 | Events are the mutation boundary. An operation yields a `Decision { instance, events }`; the kernel persists nothing and publishes nothing. | `create_enters_the_initial_state_at_revision_one_and_emits_the_creation_event`, `the_kernel_depends_on_serialisation_and_nothing_else` |
| R-04 | A refusal yields no `Decision`. The caller-owned instance is never mutated, and no partial state or partial event list escapes. | `a_refusal_leaves_the_caller_owned_instance_untouched`, `a_failed_precondition_yields_no_decision_and_names_the_rule`, `a_failed_invariant_after_an_operation_yields_no_decision_and_no_events` |
| R-05 | ✎ Output ordering is stable: fields are held in a name-ordered map, so two runs serialise to the same bytes. | `fields_are_ordered_by_name_so_two_identical_decisions_serialise_alike`, `the_same_inputs_produce_the_same_decision_byte_for_byte` |

## Dynamic definitions

| id | requirement | pinned by |
|---|---|---|
| R-10 | Entity types are registered at run time from data. The kernel has no compiled knowledge of any particular type. | `definitions_are_registered_from_data_and_keyed_by_entity_and_version` |
| R-11 | Types are authored in YAML. The YAML adapter converts text to a definition and performs no IO of its own. | `yaml_definition_drives_lifecycle_and_events`, manifest (`crates/entity-yaml/Cargo.toml`) |
| R-12 | A definition is identified by `(entity, version)`. `version` defaults to `1` and must be greater than zero; several versions of one entity may be registered together. | `a_definition_without_a_version_defaults_to_one`, `a_definition_with_a_zero_version_or_an_empty_name_is_refused`, `definitions_are_registered_from_data_and_keyed_by_entity_and_version` |
| R-13 | ⟳ A definition is validated when registered, and a malformed one is refused with **every** defect it has, not the first, as a non-empty `DefinitionErrors` of typed `DefinitionError`s. One fault is reported once: a check whose prerequisite already failed is skipped rather than reporting the same fault under a second name. The defects are: empty names, zero version, empty lifecycle, unknown initial state, duplicate state, operation without transitions, transition through an undeclared state, ambiguous transition, `set` writing an undeclared field, empty event type, inconsistent field definition, invalid default, inconsistent rule. Nothing is stored on refusal. | `a_definition_with_an_unknown_initial_state_is_refused`, `a_definition_with_duplicate_states_is_refused`, `an_operation_without_transitions_is_refused`, `a_transition_through_an_undeclared_state_is_refused`, `set_may_only_write_declared_fields`, `an_enum_without_values_and_an_array_without_items_are_refused`, `an_invalid_default_is_refused_at_registration`, `an_empty_event_type_is_refused`, `an_empty_all_or_any_is_refused`, `definition_validation_reports_every_defect_not_the_first`, `a_broken_ladder_is_reported_once_and_does_not_cascade_through_every_transition`, `comparing_a_defect_list_to_one_defect_holds_only_when_it_is_the_only_one`, `validate_prints_every_defect_of_a_file_not_only_the_first` |
| R-14 | ✎ References inside rules are checked against the schema at registration, **following the whole path**: a rule reading an undeclared field, an undeclared nested property, or a path descending into a scalar is refused before it can ever evaluate. | `a_rule_referencing_an_undeclared_field_is_refused`, `a_nested_reference_path_is_checked_against_the_schema`, `an_invariant_may_not_read_arguments_or_previous_state` |
| R-15 | ✚ Registering a definition over an existing `(entity, version)` is refused. Replacing one is `Registry::replace`, which says so. | `registering_over_an_existing_definition_is_refused_and_replace_is_how_to_mean_it`, `two_definitions_of_the_same_type_and_version_are_refused` |
| R-16 | ✚ A key a definition document does not declare is refused, not ignored: a misspelled `requried`, a `precondition:` that should be `preconditions:`, a condition carrying two operators or an unknown one. | `a_misspelled_definition_key_is_refused_rather_than_ignored`, `a_condition_carrying_two_operators_or_an_unknown_one_is_refused` |

## Schema

| id | requirement | pinned by |
|---|---|---|
| R-20 | ✎ Field kinds: `string`, `integer`, `number`, `boolean`, `enum`, `array`, `object`, `json`, `ref`. Each is type-checked except `json`, which accepts anything. An integer outside `i64` is compared numerically, never wrapped. | `every_field_kind_is_checked`, `a_reference_is_an_identity_and_the_kernel_checks_nothing_else_about_it`, `an_integer_beyond_i64_is_compared_numerically_not_wrapped` |
| R-21 | Constraints: `required`, `default`, `min_length`, `max_length`, `min`, `max`, `values`, `items`, `properties`, `additional_properties`; `additional_fields` on a schema. | `validation_accumulates_every_field_error`, `an_invalid_default_is_refused_at_registration`, `an_enum_without_values_and_an_array_without_items_are_refused` |
| R-22 | ✎ Defaults are applied before validation — to fields at creation and to arguments at execution — **at every depth an object or array element reaches**. A default is never used to invent an object that was not supplied. | `create_applies_defaults_then_validates`, `operation_arguments_are_defaulted_and_validated`, `a_default_declared_inside_an_object_is_applied` |
| R-23 | Validation accumulates: every failing value of an object is reported, each with its path (`fields.total_cents`, `arguments.items[2].sku`) and a message. | `validation_accumulates_every_field_error` |
| R-24 | Undeclared fields are refused unless the schema sets `additional_fields: true`; undeclared object properties unless `additional_properties: true`. | `unknown_fields_are_allowed_only_when_the_schema_says_so`, `validation_accumulates_every_field_error` |
| R-25 | Fields and arguments must be objects; anything else is a validation error at path `fields` or `arguments`. | `fields_and_arguments_must_be_objects` |
| R-26 | ✚ A constraint declared on a kind it does not apply to — `values` on a `string`, `items` on an `object`, `min_length` on an `integer`, `entity`/`inverse`/`acyclic` on anything but a `ref` — is refused at registration rather than silently ignored. | `a_constraint_that_does_not_apply_to_its_kind_is_refused`, `a_ref_declares_the_entity_it_points_at_or_it_is_not_a_ref`, `a_written_acyclic_is_refused_on_a_kind_it_does_not_govern` |
| R-27 | ✚ A `ref` field names the entity type it points at, in `entity`, and must: a `ref` without one is refused. Its value is an identity — a non-empty, non-whitespace string — and the kernel checks nothing else about it. Cardinality is the array machinery that already exists: one reference is `type: ref`, several is `type: array` with `items` of kind `ref`. `inverse` is an optional label naming how the other side reads the edge, and `acyclic` an optional declaration that it may not form a cycle; the kernel stores both and enforces neither, because a graph is not something one instance can see. | `a_ref_declares_the_entity_it_points_at_or_it_is_not_a_ref`, `a_reference_is_an_identity_and_the_kernel_checks_nothing_else_about_it` |
| R-28 | ✚ `Registry::validate_all` checks the registry as a **set**: every `ref`, at any depth in a schema or an operation's arguments, points at an entity type the registry holds. It reports every missing target, not the first. `register` deliberately does not do this — two types that reference each other are ordinary, and a registration-time check would make them impossible to register in either order. Whether an *instance* with that identity exists is never checked anywhere in the kernel: that is another instance, and R-01 means the kernel is never handed one. | `mutually_referencing_types_register_in_either_order_and_validate_as_a_set`, `validate_all_names_every_reference_whose_type_nobody_registered` |

## Lifecycle

| id | requirement | pinned by |
|---|---|---|
| R-30 | A lifecycle declares an initial state and its states. Creation puts the instance in the initial state. | `create_enters_the_initial_state_at_revision_one_and_emits_the_creation_event` |
| R-31 | Operations declare transitions `from → to`; `from` is one state or a list of states. | `a_transition_may_start_from_several_states` |
| R-32 | An operation with no transition from the instance's current state is refused as `InvalidTransition { operation, state }` — before preconditions, mutation or events. | `an_operation_not_declared_from_the_current_state_is_refused_before_its_preconditions`, `invalid_transition_is_rejected_without_a_new_decision` |
| R-33 | Within one operation, at most one transition may start from a given state; ambiguity is refused at registration. | `two_transitions_from_one_state_in_one_operation_are_ambiguous` |
| R-34 | ✎ **The kernel never writes a lifecycle state except through an operation.** Only `create` and `execute` assign one, there is no setter, and no generic status write exists. An `EntityInstance` is data a store round-trips, so the kernel cannot know whether the one it is handed is the one it produced — that is the shell's to know (R-80) — but an instance claiming a state the definition does not declare is refused (R-35). | `an_instance_claiming_a_state_the_definition_does_not_declare_is_refused`, `a_refusal_leaves_the_caller_owned_instance_untouched`, design (`kernel-v0.1.md` § 3.2) |
| R-35 | ✚ An instance whose `lifecycle_state` is not declared by the definition is refused as `UnknownState { entity, state }`, before the operation is looked at. | `an_instance_claiming_a_state_the_definition_does_not_declare_is_refused`, `an_instance_carrying_a_state_the_definition_does_not_declare_is_refused_by_name` |

## Operations

| id | requirement | pinned by |
|---|---|---|
| R-40 | Every operation has its own argument schema, defaulted and validated like fields. | `operation_arguments_are_defaulted_and_validated`, `operation_arguments_are_schema_validated` |
| R-41 | `set` assignments are deterministic templates resolved against the pre-operation fields, so the map has no ordering semantics. | `set_assignments_read_the_pre_operation_fields_whatever_their_order` |
| R-42 | After `set`, the resulting fields are validated against the entity schema again. | `fields_are_revalidated_after_set` |
| R-43 | An operation emits zero or more events, each with a type and a templated payload; creation may emit one. | `create_enters_the_initial_state_at_revision_one_and_emits_the_creation_event`, `yaml_definition_drives_lifecycle_and_events` |
| R-44 | `revision` is `1` after creation and increments by exactly one per successful operation; a refusal consumes none. Each event carries the revision it produced. | `every_successful_operation_increments_the_revision_by_one` |
| R-45 | Executing an instance against another definition is refused (`EntityMismatch`); an unknown operation (`OperationNotFound`) and an unregistered type (`EntityNotRegistered`) likewise. | `an_instance_of_another_definition_is_refused` |

## Rules

| id | requirement | pinned by |
|---|---|---|
| R-50 | ⟳ Preconditions belong to an operation. They are evaluated after argument validation and transition selection and before `set` or events. A rule answered and contradicted is `PreconditionFailed { operation, rule, message }`; a rule that could not be answered is `PreconditionUnobservable { operation, rule, message, unresolved }` (R-57). | `a_failed_precondition_yields_no_decision_and_names_the_rule`, `operation_precondition_blocks_mutation_and_event_emission` |
| R-51 | ⟳ Invariants belong to the entity. They are evaluated after creation and after every successful operation, against the next state, before any event escapes. A violation is `InvariantViolation { rule, message }`; an invariant that could not be answered is `InvariantUnobservable { rule, message, unresolved }` (R-57), and the next state is discarded either way. | `a_failed_invariant_at_creation_yields_no_decision`, `a_failed_invariant_after_an_operation_yields_no_decision_and_no_events`, `an_invariant_contradicted_after_an_operation_is_a_violation_not_an_unobservable`, `entity_invariant_is_checked_after_create`, `entity_invariant_is_checked_after_operation_before_events_escape` |
| R-52 | ✎ Scopes differ, and the difference is enforced at registration in both directions. A precondition may read `$args.*`, `$fields.*`, `$old_fields.*`, `$from_state`, `$to_state`, `$id`, `$entity`, `$version` — **not `$state`**, which would silently mean the state the operation is heading for. An invariant may read only `$fields.*`, `$state`, `$id`, `$entity`, `$version` — not the arguments, the previous state or `$to_state`. | `a_precondition_may_not_read_state_and_an_invariant_may_not_read_the_transition`, `an_invariant_may_not_read_arguments_or_previous_state`, `definition_rejects_operation_only_reference_in_invariant`, `a_precondition_may_read_the_arguments_and_the_transition` |
| R-53 | ⟳ The condition language is a data AST, not an expression language: literal `true`/`false`, `all`, `any`, `not`, `exists`, `eq`, `ne`, `gt`, `gte`, `lt`, `lte`, `in`, `contains`, `before`, `after`. `all` and `any` must not be empty, and a condition carries exactly one operator. | `numeric_comparisons_are_numeric_and_compare_false_otherwise`, `contains_and_in_cover_arrays_strings_objects_and_membership`, `an_empty_all_or_any_is_refused`, `a_condition_carrying_two_operators_or_an_unknown_one_is_refused` |
| R-54 | ⟳ A reference that does not resolve makes a **value** question — a comparison or membership test — `Unknown` (R-57), not `false`. `all`/`any` evaluate **every** operand — the Kleene answer is order-independent, so nothing is bought by stopping early and a complete list of unresolved addresses is. `gt`/`gte`/`lt`/`lte` are numeric and `false` when both operands resolve and either is not a number. **`eq`/`ne`/`in`/`contains` compare numbers numerically too**, so `100` and `100.0` are equal and the operator families agree. `contains` covers array∋element, string⊇substring and object∋key. | `a_value_question_over_a_missing_reference_is_unobservable_and_exists_stays_two_valued`, `numeric_comparisons_are_numeric_and_compare_false_otherwise`, `contains_and_in_cover_arrays_strings_objects_and_membership`, `equality_is_numeric_so_an_integer_equals_the_same_number_written_with_a_decimal_point` |
| R-59 | ✚ `before`/`after` order two ISO-8601 instants — `YYYY-MM-DD`, or `YYYY-MM-DDTHH:MM:SS[.fff][Z]`, with a space accepted for the `T`. An operand this kernel cannot read is **`Unknown`, not `false`**, and the refusal names it: *these are not numbers* is an observation, while *this is not a timestamp I can read* is a statement about the reader, and answering `false` would let a gate permit a move against a value nobody understood. An explicit offset is refused rather than normalised — comparing it with a naive instant has no correct answer, and a shell that has offsets has a clock to normalise with. There is still no `$now` (R-62): the clock is read at the edge. | `before_and_after_order_two_instants_the_shell_supplied`, `an_instant_this_kernel_cannot_read_is_unobservable_and_the_refusal_names_it`, `a_date_and_a_datetime_both_read_and_order`, `what_it_cannot_read_it_says_nothing_about`, `nothing_panics_however_wrong_the_input` |
| R-57 | ✚ A condition evaluates to `Truth { True, False, Unknown }` and a rule holds **only** when `True`. The connectives are Kleene's, so on any rule that never reads a missing value they are ordinary boolean logic. An `Unknown` rule is refused as `PreconditionUnobservable`/`InvariantUnobservable`, carrying **every** unresolved address, sorted and without repeats — never the first one only. | `a_value_question_over_a_missing_reference_is_unobservable_and_exists_stays_two_valued`, `an_unobservable_refusal_names_every_unresolved_reference_not_the_first`, `kleene_agrees_with_boolean_logic_wherever_nothing_is_unknown`, `the_connectives_are_commutative_and_associative`, `an_unobservable_refusal_names_every_address_nobody_observed` |
| R-58 | ✚ `Unknown` belongs to the question, not to the operator. A question about the **store** — `exists`, *is there a value at this address* — is always answerable, because the kernel holds the instance; it is two-valued and `not: { exists: x }` negates it in the ordinary way. A question about a **value** — every comparison and membership test — is `Unknown` when there is no value to read. A key **present and null** is not a value: `exists` reports `false` for it and a comparison against it reports `Unknown`. A `null` written as a literal in the definition is a value. | `a_value_question_over_a_missing_reference_is_unobservable_and_exists_stays_two_valued`, `a_present_null_is_not_a_value_for_either_kind_of_question`, `negation_cannot_turn_nobody_looked_into_it_is_wrong`, `only_true_satisfies_a_rule` |
| R-55 | Conditions have no function call, loop, arithmetic, clock, random source or lookup — `before`/`after` order two instants they are **handed** and cannot ask what time it is. | type (the `Condition` enum has sixteen variants and none of those), `an_unresolvable_template_reference_is_an_error_not_a_null` |
| R-56 | A refusal names the rule when it has a name and reports its message, or a default when it has none. | `a_rule_without_a_message_reports_a_default`, `a_failed_precondition_yields_no_decision_and_names_the_rule` |

**R-54, as it read before three-valued evaluation:** *"A reference that does not resolve makes a
comparison or membership test `false`; `exists` is the explicit presence test. `all`/`any`
short-circuit deterministically."* The first clause is now untrue and the second is gone; the
middle clause survived intact, which is the whole shape of the change. A missing value makes a
*comparison* `Unknown`, because a gate that reports *nobody recorded this* and *this is wrong* with
one word sends whoever reads the refusal to fix the wrong thing — but `exists` is still exactly the
presence test it always was, because presence is a question the kernel can always answer. `all`/
`any` evaluate every operand, because the address of each missing fact is worth more than the
evaluations saved. Numeric comparison and `contains` coverage are unchanged, and every definition
that never compares against a missing value evaluates exactly as it did.

**R-50 and R-51, before:** a precondition failure was `PreconditionFailed` and an invariant failure
was `InvariantViolation`, with no third case. Those variants still exist and still mean what they
meant; what changed is that a rule nobody can answer no longer borrows one of them.

## Templates

| id | requirement | pinned by |
|---|---|---|
| R-60 | In a template, a string beginning with `$` is a reference and everything else is a literal; `$$` escapes a literal leading `$`; templates resolve recursively through arrays and objects. | `templates_resolve_recursively_and_escape_a_literal_dollar` |
| R-61 | References: `$id`, `$entity`, `$version`, `$state`/`$to_state`, `$from_state`, `$args`, `$args.<path>`, `$fields`, `$fields.<path>`, `$old_fields`, `$old_fields.<path>`. `set` sees the pre-operation fields; events see the post-operation fields. | `templates_resolve_recursively_and_escape_a_literal_dollar`, `set_assignments_read_the_pre_operation_fields_whatever_their_order`, `a_precondition_may_read_the_arguments_and_the_transition` |
| R-62 | There is no `$now`, no `uuid()`, no lookup. A value the outside world knows enters as an argument the shell supplies. | `a_template_the_scope_cannot_resolve_is_refused_at_registration` |
| R-63 | ✎ A template reference that does not resolve, or an unknown expression, is a `Template { expression, message }` error — never a silent `null`. | `an_unresolvable_template_reference_is_an_error_not_a_null` |
| R-64 | ✚ A `set` value or event payload whose reference its scope cannot see — `$args.*` in a creation event, `$from_state` at creation, an argument the operation does not declare, `$now` — is refused at **registration**, as `InvalidTemplate { path, message }`. What only run time can decide (a path into a `json` field) stays R-63. | `a_template_the_scope_cannot_resolve_is_refused_at_registration` |

## Evaluation order and outputs

| id | requirement | pinned by |
|---|---|---|
| R-70 | ✎ An operation is evaluated in exactly this order: (1) instance matches definition and carries a declared state, (2) operation exists, (3) arguments defaulted and validated, (4) transition selected, (5) preconditions, (6) `set` resolved against pre-operation fields, (7) fields validated, (8) next instance constructed, (9) invariants against the next state, (10) events materialised, (11) `Decision` returned. | `an_operation_not_declared_from_the_current_state_is_refused_before_its_preconditions`, `fields_are_revalidated_after_set`, `a_failed_invariant_after_an_operation_yields_no_decision_and_no_events`, `an_instance_claiming_a_state_the_definition_does_not_declare_is_refused` |
| R-71 | ✎ `EntityInstance { entity, version, id, lifecycle_state, revision, fields }`, where `fields` is a name-ordered map and `id` is never empty. | type (`entity_core::EntityInstance`, exactly these six fields), `an_empty_identity_is_refused`, `fields_are_ordered_by_name_so_two_identical_decisions_serialise_alike` |
| R-72 | `DomainEvent { entity, version, id, revision, type, payload }` is the domain fact only. Envelope metadata — event id, recorded-at time, correlation, causation, actor — is the shell's to add. | type (`entity_core::DomainEvent` has no such fields), `the_kernel_reaches_no_clock_filesystem_network_or_random_source` |
| R-73 | `Decision { instance, events }` is the only thing the kernel produces. | type (the two kernel entry points return a Decision or a CoreError and nothing else) |
| R-74 | Refusals are typed (`DefinitionError`, `ValidationError`, `CoreError`) and callers match on variants, never on message text. | type; every test under `crates/` matches a variant, e.g. `an_instance_of_another_definition_is_refused` |
| R-75 | ✚ The identity a caller supplies is opaque to the kernel, and must not be empty or whitespace. | `an_empty_identity_is_refused` |

## The shell

| id | requirement | pinned by |
|---|---|---|
| R-80 | The shell owns IO: it loads the instance, calls the kernel, and persists the instance, appends the events, updates projections and publishes — together. Whether the instance it loads is the one the kernel produced is the shell's to know. | design (`kernel-v0.1.md` § 9), `create_then_execute_through_a_pipe_and_a_refusal_with_its_typed_reason` |
| R-81 | The model is compatible with both state persistence and event sourcing. A future replay (`apply`/rehydrate) must not open a way to patch the lifecycle state directly. | design (`kernel-v0.1.md` § 10) |
| R-82 | Provider interfaces — state, event, search, blob — live outside `entity-core`. | manifest (`crates/entity-core/Cargo.toml`), `the_kernel_depends_on_serialisation_and_nothing_else` |
| R-83 | ✚ A store writes an instance and its events **together**: `Store::commit` takes a whole `Decision`, so a state cannot move without the event that explains it. | design (`store-v0.1.md` § 2), `state_and_events_arrive_together`, `a_committed_instance_reads_back_with_its_events` |
| R-84 | ✚ Every write states what it expected to find, and a store holding anything else refuses rather than overwriting. The expectation is checked before anything is written, so a refusal changes nothing. | design (`store-v0.1.md` § 3), `two_executions_from_one_revision_leave_exactly_one_accepted`, `a_refused_commit_changes_nothing`, `every_provider_leaves_a_refused_commit_with_no_trace`, `a_retried_commit_appends_its_events_once` |
| R-85 | ✚ Every provider answers alike: one suite runs against each, and an instance nobody stored is absent rather than an error. | design (`store-v0.1.md` § 4), `every_provider_refuses_a_stale_write_the_same_way`, `every_provider_answers_absent_for_something_nobody_stored`, `the_file_store_survives_being_reopened` |
| R-86 | ✚ The shape around an event — `event_id`, `recorded_at`, `correlation`, `causation`, `actor` — lives outside `entity-core` and is supplied by the shell. Correlation and causation are separate fields answering separate questions. | design (`store-v0.1.md` § 6), `correlation_and_causation_are_separate_fields_and_a_flow_start_says_so`, `every_event_of_one_decision_shares_a_correlation_and_gets_its_own_id` |
| R-87 | ✚ Every envelope field is written, never defaulted: an envelope missing one is refused rather than read as a claim, and an absent actor serialises as an explicit null. | design (`store-v0.1.md` § 6), `an_envelope_missing_a_field_is_refused_rather_than_defaulted`, `an_absent_actor_serialises_as_null_rather_than_disappearing`, `an_envelope_round_trips_and_refuses_a_field_it_does_not_know` |
| R-88 | ✚ A derived event identity needs no clock and no random source: sealing one decision twice produces the same identities. | design (`store-v0.1.md` § 6), `sealing_the_same_decision_twice_produces_the_same_ids` |
| R-89 | ✚ An event records what it did: the state before and after, and the fields the operation wrote. An event that cannot rebuild what it describes is a notification, not a record. | design (`kernel-v0.1.md` § 10.1), `a_create_and_two_operations_fold_back_into_the_instance_they_produced` |
| R-97 | ✚ A fold refuses any event whose transition the definition does not declare, whose `from_state` is not where the fold reached, whose revision does not follow, or which belongs to another instance — so replay reaches no state `execute` would not have permitted (R-34). | design (`kernel-v0.1.md` § 10.1), `an_event_whose_transition_the_lifecycle_does_not_permit_is_refused`, `an_event_naming_a_transition_no_operation_declares_is_refused`, `a_history_belonging_to_another_instance_is_refused`, `a_history_with_a_gap_is_refused_rather_than_folded_over`, `a_history_whose_revisions_skip_a_number_is_refused`, `a_second_creation_event_partway_through_a_history_is_refused`, `a_creation_event_into_a_state_that_is_not_the_initial_one_is_refused`, `an_event_carrying_a_field_the_schema_does_not_declare_is_refused`, `an_event_carrying_a_field_of_the_wrong_type_is_refused` |
| R-98 | ✚ A definition declares its read models and performs none of them: `projections:` is data the shell evaluates, because a projection reads across instances and the kernel is handed one. | design (`store-v0.1.md` § 7), `a_sequence_of_decisions_produces_the_declared_read_model` |
| R-99 | ✚ A projection naming a field the schema does not declare, or a state the lifecycle does not declare, is refused at registration rather than producing a read model that is silently always empty. | design (`store-v0.1.md` § 7), `a_projection_naming_a_field_the_schema_does_not_have_is_refused_at_registration`, `a_projection_naming_a_state_the_lifecycle_does_not_have_is_refused_at_registration` |
| R-100 | ✚ A read model is the same bytes every run, and an instance whose key resolves to nothing is left out rather than filed under an empty key. | design (`store-v0.1.md` § 7), `a_projection_is_the_same_bytes_every_run`, `an_instance_whose_key_resolves_to_nothing_is_left_out_rather_than_filed_under_an_empty_key` |
| R-101 | ✚ The provider suite lives in the crate that owns the traits and travels to each provider, so every implementation answers the same cases. | design (`store-v0.1.md` § 8), `the_memory_provider_conforms`, `the_file_provider_conforms`, `the_sqlite_provider_conforms` |
| R-102 | ✚ The suite is run against a provider that is deliberately wrong, and must catch it — and localise it, rather than condemning the whole provider. | design (`store-v0.1.md` § 8), `a_broken_provider_is_caught` |
| R-103 | ✚ At least one provider writes state and events in a single transaction, so a refusal cannot leave half a commit behind. | design (`store-v0.1.md` § 8), `a_refused_commit_rolls_back_both_halves`, `the_rollback_case_is_one_a_non_transactional_store_actually_fails`, `it_survives_being_closed_and_reopened` |
| R-104 | ✚ A store that could not be reached answers `Unreachable`, never absent: silence says nothing about whether an instance exists. | design (`store-v0.1.md` § 9), `a_remote_that_did_not_answer_is_unreachable_and_never_absent`, `a_silent_remote_refuses_rather_than_answering_absent`, `a_stale_read_that_found_nothing_is_unreachable_rather_than_absent` |
| R-105 | ✚ The remote protocol is versioned and transport-agnostic: a request at an unknown wire version is refused by name, and no network client lives in this repository. | design (`store-v0.1.md` § 9), `a_request_at_a_wire_version_this_build_does_not_know_is_refused_by_name`, `a_version_refusal_is_not_reported_as_unreachable`, `an_unreachable_store_on_the_far_side_stays_unreachable_on_this_one`, `a_remote_store_conforms_like_a_local_one`, `a_conflict_crosses_the_wire_as_a_conflict_and_not_as_a_failure` |
| R-106 | ✚ A hybrid store declares authority, read path, unreachable behaviour and divergence behaviour as required words. There is no default policy. | design (`store-v0.1.md` § 10), `a_hybrid_with_the_remote_as_authority_conforms_like_any_other_store`, `with_the_remote_as_authority_a_refused_remote_write_never_reaches_the_local_copy`, `refusing_on_divergence_lets_no_write_stand_unreplicated`, `refusing_on_divergence_leaves_the_authority_untouched`, `a_hybrid_with_the_local_store_as_authority_conforms_like_any_other_store` |
| R-107 | ✚ A stale answer is a choice somebody typed, and says it was stale at the point of use; a losing write is recorded as a divergence, never swallowed. | design (`store-v0.1.md` § 10), `serving_a_stale_copy_is_a_choice_and_the_answer_says_it_was_stale`, `with_the_local_store_as_authority_a_losing_replica_write_is_recorded_and_not_swallowed` |
| R-108 | ✚ Catch-up replays what the authority holds now, keeps what it could not replay, and merges nothing. | design (`store-v0.1.md` § 10), `a_laptop_that_wrote_while_the_replica_was_down_catches_up_when_it_returns`, `catch_up_keeps_what_it_could_not_replay_rather_than_reporting_success`, `catch_up_replays_what_the_local_store_holds_now_and_not_the_write_that_diverged`, `catch_up_keeps_a_divergence_whose_local_side_cannot_be_read`, `catch_up_refuses_to_overwrite_a_replica_that_moved_on_its_own`, `catch_up_appends_only_what_the_replica_has_not_seen` |

## Library and CLI

| id | requirement | pinned by |
|---|---|---|
| R-90 | The runtime is offered as a Rust library crate, `entity-core`, with every public item documented, no `unsafe`, and a stable typed API. | manifest (Cargo.toml workspace lints: missing_docs and unsafe_code = forbid, raised to errors by the gate's clippy step with -D warnings) |
| R-91 | ⟳ The runtime is offered as a CLI layer, the `entity` command (clap derive), with `validate`, `inspect`, `graph`, `create` and `execute`. `graph` draws one definition's lifecycle, or — with `--references`, over several definitions — the `ref` edges between entity types, in `text`, `dot`, `svg` or `html`. ✚ `create` and `execute` take `--store`; with one, `execute` takes `--id` and `--entity` instead of `--instance`, and the envelope fields `--correlation`, `--recorded-at`, `--causation` and `--actor` are supplied by the caller as R-86 requires. | `validate_accepts_the_example_and_exits_zero`, `inspect_and_graph_describe_the_definition`, `create_then_execute_through_a_pipe_and_a_refusal_with_its_typed_reason`, `a_store_carries_the_instance_from_create_to_execute` |
| R-92 | ⟳ Exit codes distinguish the three outcomes: `0` the kernel decided, `1` the kernel refused or a definition was invalid (the typed refusal is printed as JSON on stdout; `validate` prints its per-file report instead, and never both), `2` the invocation was wrong. ✚ A refusal by the **store** rather than the kernel exits `1` carrying `{"refused": true, "by": "store", …}`, so a caller can tell which of the two said no without parsing prose. | `validate_names_the_defect_and_exits_one`, `an_unparsable_definition_is_a_usage_error_with_exit_two_where_one_is_expected`, `a_validation_refusal_lists_every_error`, `two_flags_cannot_both_read_standard_input`, `a_second_creation_of_one_identity_is_refused_by_the_store` |
| R-93 | ⟳ The CLI is a shell in the sense of R-80: all IO is there, identifiers come from the caller, and a `Decision` it prints can be fed back as the next `--instance`. ✚ With `--store` the shell does the whole of R-80 — it loads the instance, calls the kernel, then commits state and events together — so the instance need not be piped at all. | `create_then_execute_through_a_pipe_and_a_refusal_with_its_typed_reason`, `create_refuses_to_guess_between_two_definitions_and_says_how_to_choose`, `a_store_carries_the_instance_from_create_to_execute` |
| R-94 | ✚ Input a caller passes is read faithfully: JSON is parsed as JSON before YAML is tried, so escapes only JSON defines are accepted; and at most one flag per invocation may read standard input. | `json_escapes_that_yaml_rejects_are_read_as_json`, `two_flags_cannot_both_read_standard_input` |
| R-95 | ⟳ Generated output is escaped for the format it is generated in: a state or operation name carrying a quote or a backslash produces valid DOT, valid SVG and a valid HTML page — never a graph with attributes nobody wrote, and never markup a name injected. | `graph_dot_quotes_a_name_that_would_otherwise_close_the_string`, `a_name_that_would_close_the_string_it_is_written_into_is_escaped_in_every_format`, `a_control_character_cannot_reach_the_document` |
| R-96 | ✚ `validate` reports every file it was given, whatever went wrong with the one before it, and summarises how many were invalid. | `validate_reports_every_file_and_a_broken_one_is_a_finding_not_a_usage_error` |

## Roadmap, not requirements

The proof of concept named what it would add next, and the 0.1.0 review added to the list. None of
these is a requirement of this version; each is a story in the planning store
(`protocol artifact list --kind story`).

| addition | why it is not here yet |
|---|---|
| a sealed `EntityInstance` — private fields, parsed through a validated `Raw` type | would make R-34 a property of the type rather than a property of the kernel's own writes; a breaking API change, and the shell still owns which instance it loads |
| projection definitions for search and indexing | shell-side; the kernel emits the events a projection folds |
| optimistic concurrency on `revision` | shell-side; the kernel already numbers revisions (R-44) |
| the event envelope (`event_id`, `recorded_at`, correlation, causation, actor) | shell-side by R-72; a reference envelope type would help adopters agree |
| definition inheritance and reusable schema fragments | authoring convenience; nothing in the kernel changes |
| definition migrations between versions | needs a decision on where an instance's version is advanced |
| named reusable predicates | authoring convenience |
| an `explain` verb: why an operation is or is not permitted from here, without executing | needs the kernel to expose per-rule verdicts |
| lazily built validation paths | every declared field allocates its error path even when valid; measured cost is small, so it waits behind a benchmark |
