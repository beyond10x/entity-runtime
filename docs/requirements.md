# Requirements register

Every requirement the runtime must satisfy, with what pins it. This is the document the
implementation is measured against; `scripts/check-requirements.py` (step `req-check` of the gate)
fails when a requirement is not referenced by a design under `docs/design/`, or cites a test that
does not exist.

**Provenance.** The requirements were extracted from a proof of concept the operator designed on
2026-08-25: a schema-driven entity runtime with an IO-free Rust core and YAML-declared types,
delivered as a three-crate workspace with a README and seven tests. That workspace was never
compiled by its author; it was reconstructed here, compiled, and the seven tests pass unchanged
(`crates/entity-yaml/tests/runtime.rs`). The requirements below hold for this implementation whether
or not it resembles the proof of concept in code. R-90 to R-93 were added by the operator afterwards:
the system offers a Rust library crate **and** a CLI layer.

**How to read the `pinned by` column.** A backticked name is a test function under `crates/` and
must exist. `type` means the property is closed by the type system — there is no API that could
violate it — and the design names the type. `manifest` means a `Cargo.toml` is the evidence.
`design` alone means nothing mechanical pins it yet; that is a gap, and each one is a story.

## Kernel purity and determinism

| id | requirement | pinned by |
|---|---|---|
| R-01 | The kernel has no filesystem, network, clock, identifier generator, random source, async runtime or storage access. Nothing in `entity-core` can perform IO. | `the_kernel_reaches_no_clock_filesystem_network_or_random_source`, `the_kernel_depends_on_serialisation_and_nothing_else`, `the_scan_would_notice_an_offence_and_ignores_comments_and_lookalikes` |
| R-02 | Same definition, instance, operation and arguments produce the same `Decision`, every time. Timestamps and identifiers are inputs the shell supplies; the kernel never manufactures one. | `the_same_inputs_produce_the_same_decision_byte_for_byte`, `an_unresolvable_template_reference_is_an_error_not_a_null` |
| R-03 | Events are the mutation boundary. An operation yields a `Decision { instance, events }`; the kernel persists nothing and publishes nothing. | `create_enters_the_initial_state_at_revision_one_and_emits_the_creation_event`, `the_kernel_depends_on_serialisation_and_nothing_else` |
| R-04 | A refusal yields no `Decision`. The caller-owned instance is never mutated, and no partial state or partial event list escapes. | `a_refusal_leaves_the_caller_owned_instance_untouched`, `a_failed_precondition_yields_no_decision_and_names_the_rule`, `a_failed_invariant_after_an_operation_yields_no_decision_and_no_events` |
| R-05 | Output ordering is stable: the kernel iterates ordered maps only, so two runs serialise to the same bytes. | `the_kernel_reaches_no_clock_filesystem_network_or_random_source`, `the_same_inputs_produce_the_same_decision_byte_for_byte` |

## Dynamic definitions

| id | requirement | pinned by |
|---|---|---|
| R-10 | Entity types are registered at run time from data. The kernel has no compiled knowledge of any particular type. | `definitions_are_registered_from_data_and_keyed_by_entity_and_version` |
| R-11 | Types are authored in YAML. The YAML adapter converts text to a definition and performs no IO of its own. | `yaml_definition_drives_lifecycle_and_events`, manifest (`crates/entity-yaml/Cargo.toml`) |
| R-12 | A definition is identified by `(entity, version)`. `version` defaults to `1` and must be greater than zero; several versions of one entity may be registered together. | `a_definition_without_a_version_defaults_to_one`, `a_definition_with_a_zero_version_or_an_empty_name_is_refused`, `definitions_are_registered_from_data_and_keyed_by_entity_and_version` |
| R-13 | A definition is validated when registered, and a malformed one is refused with a typed `DefinitionError`: empty names, zero version, empty lifecycle, unknown initial state, duplicate state, operation without transitions, transition through an undeclared state, ambiguous transition, `set` writing an undeclared field, empty event type, inconsistent field definition, invalid default, inconsistent rule. Nothing is stored on refusal. | `a_definition_with_an_unknown_initial_state_is_refused`, `a_definition_with_duplicate_states_is_refused`, `an_operation_without_transitions_is_refused`, `a_transition_through_an_undeclared_state_is_refused`, `set_may_only_write_declared_fields`, `an_enum_without_values_and_an_array_without_items_are_refused`, `an_invalid_default_is_refused_at_registration`, `an_empty_event_type_is_refused`, `an_empty_all_or_any_is_refused` |
| R-14 | References inside rules are checked against the schema at registration: a rule that reads an undeclared field or argument is refused before it can ever evaluate. | `a_rule_referencing_an_undeclared_field_is_refused`, `an_invariant_may_not_read_arguments_or_previous_state` |

## Schema

| id | requirement | pinned by |
|---|---|---|
| R-20 | Field kinds: `string`, `integer`, `number`, `boolean`, `enum`, `array`, `object`, `json`. Each is type-checked except `json`, which accepts anything. | `every_field_kind_is_checked` |
| R-21 | Constraints: `required`, `default`, `min_length`, `max_length`, `min`, `max`, `values`, `items`, `properties`, `additional_properties`; `additional_fields` on a schema. | `validation_accumulates_every_field_error`, `an_invalid_default_is_refused_at_registration`, `an_enum_without_values_and_an_array_without_items_are_refused` |
| R-22 | Defaults are applied before validation — to fields at creation and to arguments at execution. | `create_applies_defaults_then_validates`, `operation_arguments_are_defaulted_and_validated` |
| R-23 | Validation accumulates: every failing value of an object is reported, each with its path (`fields.total_cents`, `arguments.items[2].sku`) and a message. | `validation_accumulates_every_field_error` |
| R-24 | Undeclared fields are refused unless the schema sets `additional_fields: true`; undeclared object properties unless `additional_properties: true`. | `unknown_fields_are_allowed_only_when_the_schema_says_so`, `validation_accumulates_every_field_error` |
| R-25 | Fields and arguments must be objects; anything else is a validation error at path `fields` or `arguments`. | `fields_and_arguments_must_be_objects` |

## Lifecycle

| id | requirement | pinned by |
|---|---|---|
| R-30 | A lifecycle declares an initial state and its states. Creation puts the instance in the initial state. | `create_enters_the_initial_state_at_revision_one_and_emits_the_creation_event` |
| R-31 | Operations declare transitions `from → to`; `from` is one state or a list of states. | `a_transition_may_start_from_several_states` |
| R-32 | An operation with no transition from the instance's current state is refused as `InvalidTransition { operation, state }` — before preconditions, mutation or events. | `an_operation_not_declared_from_the_current_state_is_refused_before_its_preconditions`, `invalid_transition_is_rejected_without_a_new_decision` |
| R-33 | Within one operation, at most one transition may start from a given state; ambiguity is refused at registration. | `two_transitions_from_one_state_in_one_operation_are_ambiguous` |
| R-34 | The lifecycle state is not a patchable field. Only `create` and `execute` write `lifecycle_state`; there is no generic status write. | type (the lifecycle_state field is assigned in create and execute only; the kernel exposes no setter and takes instances by shared reference — `a_refusal_leaves_the_caller_owned_instance_untouched`) |

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
| R-50 | Preconditions belong to an operation. They are evaluated after argument validation and transition selection and before `set` or events; a failure is `PreconditionFailed { operation, rule, message }`. | `a_failed_precondition_yields_no_decision_and_names_the_rule`, `operation_precondition_blocks_mutation_and_event_emission` |
| R-51 | Invariants belong to the entity. They are evaluated after creation and after every successful operation, against the next state, before any event escapes; a failure is `InvariantViolation { rule, message }`. | `a_failed_invariant_at_creation_yields_no_decision`, `a_failed_invariant_after_an_operation_yields_no_decision_and_no_events`, `entity_invariant_is_checked_after_create`, `entity_invariant_is_checked_after_operation_before_events_escape` |
| R-52 | Scopes differ. A precondition may read `$args.*`, `$fields.*`, `$old_fields.*`, `$from_state`, `$to_state`, `$id`, `$entity`, `$version`. An invariant may read only `$fields.*`, `$state`, `$id`, `$entity`, `$version` — it cannot depend on how the state was reached. Violations are refused at registration. | `an_invariant_may_not_read_arguments_or_previous_state`, `a_precondition_may_read_the_arguments_and_the_transition`, `definition_rejects_operation_only_reference_in_invariant` |
| R-53 | The condition language is a data AST, not an expression language: literal `true`/`false`, `all`, `any`, `not`, `exists`, `eq`, `ne`, `gt`, `gte`, `lt`, `lte`, `in`, `contains`. `all` and `any` must not be empty. | `numeric_comparisons_are_numeric_and_compare_false_otherwise`, `contains_and_in_cover_arrays_strings_objects_and_membership`, `an_empty_all_or_any_is_refused` |
| R-54 | A reference that does not resolve makes a comparison or membership test `false`; `exists` is the explicit presence test. `all`/`any` short-circuit deterministically. `gt`/`gte`/`lt`/`lte` are numeric and `false` for non-numbers. `contains` covers array∋element, string⊇substring and object∋key. | `a_missing_reference_makes_a_comparison_false_and_exists_is_the_presence_test`, `numeric_comparisons_are_numeric_and_compare_false_otherwise`, `contains_and_in_cover_arrays_strings_objects_and_membership` |
| R-55 | Conditions have no function call, loop, arithmetic, clock, random source or lookup. | type (the Condition enum has thirteen variants and none of those), `an_unresolvable_template_reference_is_an_error_not_a_null` |
| R-56 | A refusal names the rule when it has a name and reports its message, or a default when it has none. | `a_rule_without_a_message_reports_a_default`, `a_failed_precondition_yields_no_decision_and_names_the_rule` |

## Templates

| id | requirement | pinned by |
|---|---|---|
| R-60 | In a template, a string beginning with `$` is a reference and everything else is a literal; `$$` escapes a literal leading `$`; templates resolve recursively through arrays and objects. | `templates_resolve_recursively_and_escape_a_literal_dollar` |
| R-61 | References: `$id`, `$entity`, `$version`, `$state`/`$to_state`, `$from_state`, `$args`, `$args.<path>`, `$fields`, `$fields.<path>`, `$old_fields`, `$old_fields.<path>`. `set` sees the pre-operation fields; events see the post-operation fields. | `templates_resolve_recursively_and_escape_a_literal_dollar`, `set_assignments_read_the_pre_operation_fields_whatever_their_order`, `a_precondition_may_read_the_arguments_and_the_transition` |
| R-62 | There is no `$now`, no `uuid()`, no lookup. A value the outside world knows enters as an argument the shell supplies. | `an_unresolvable_template_reference_is_an_error_not_a_null` |
| R-63 | A template reference that does not resolve, or an unknown expression, is a `Template { expression, message }` error — never a silent `null`. | `an_unresolvable_template_reference_is_an_error_not_a_null` |

## Evaluation order and outputs

| id | requirement | pinned by |
|---|---|---|
| R-70 | An operation is evaluated in exactly this order: (1) instance matches definition, (2) operation exists, (3) arguments defaulted and validated, (4) transition selected, (5) preconditions, (6) `set` resolved against pre-operation fields, (7) fields validated, (8) next instance constructed, (9) invariants against the next state, (10) events materialised, (11) `Decision` returned. | `an_operation_not_declared_from_the_current_state_is_refused_before_its_preconditions`, `fields_are_revalidated_after_set`, `a_failed_invariant_after_an_operation_yields_no_decision_and_no_events` |
| R-71 | `EntityInstance { entity, version, id, lifecycle_state, revision, fields }`. | type (entity_core::EntityInstance, exactly these six fields) |
| R-72 | `DomainEvent { entity, version, id, revision, type, payload }` is the domain fact only. Envelope metadata — event id, recorded-at time, correlation, causation, actor — is the shell's to add. | type (entity_core::DomainEvent has no such fields), `the_kernel_reaches_no_clock_filesystem_network_or_random_source` |
| R-73 | `Decision { instance, events }` is the only thing the kernel produces. | type (create and execute return a Decision or a CoreError and nothing else) |
| R-74 | Refusals are typed (`DefinitionError`, `ValidationError`, `CoreError`) and callers match on variants, never on message text. | type; every test under `crates/` matches a variant, e.g. `an_instance_of_another_definition_is_refused` |

## The shell

| id | requirement | pinned by |
|---|---|---|
| R-80 | The shell owns IO: it loads the instance, calls the kernel, and persists the instance, appends the events, updates projections and publishes — together. | design (`kernel-v0.1.md` § 9), `create_then_execute_through_a_pipe_and_a_refusal_with_its_typed_reason` |
| R-81 | The model is compatible with both state persistence and event sourcing. A future replay (`apply`/rehydrate) must not open a way to patch the lifecycle state directly. | design (`kernel-v0.1.md` § 10) |
| R-82 | Provider interfaces — state, event, search, blob — live outside `entity-core`. | manifest (`crates/entity-core/Cargo.toml`), `the_kernel_depends_on_serialisation_and_nothing_else` |

## Library and CLI

| id | requirement | pinned by |
|---|---|---|
| R-90 | The runtime is offered as a Rust library crate, `entity-core`, with every public item documented, no `unsafe`, and a stable typed API. | manifest (Cargo.toml workspace lints: missing_docs and unsafe_code = forbid, raised to errors by the gate's clippy step with -D warnings) |
| R-91 | The runtime is offered as a CLI layer, the `entity` command (clap derive), with `validate`, `inspect`, `graph`, `create` and `execute`. | `validate_accepts_the_example_and_exits_zero`, `inspect_and_graph_describe_the_definition`, `create_then_execute_through_a_pipe_and_a_refusal_with_its_typed_reason` |
| R-92 | Exit codes distinguish the three outcomes: `0` the kernel decided, `1` the kernel refused (the typed refusal is printed as JSON on stdout), `2` the invocation was wrong. | `validate_names_the_defect_and_exits_one`, `an_unreadable_or_unparsable_file_is_a_usage_error_with_exit_two`, `a_validation_refusal_lists_every_error` |
| R-93 | The CLI is a shell in the sense of R-80: all IO is there, identifiers come from the caller, and a `Decision` it prints can be fed back as the next `--instance`. | `create_then_execute_through_a_pipe_and_a_refusal_with_its_typed_reason`, `create_refuses_to_guess_between_two_definitions` |

## Roadmap, not requirements

The proof of concept named what it would add next. None of these is a requirement of this version;
each is a story in the planning store (`protocol artifact list --kind story`).

| addition | why it is not here yet |
|---|---|
| relationships and typed references between entities (`type: ref`) | needs a decision on whether a reference is checked by the kernel (which would need the other instance as an input) or by the shell |
| projection definitions for search and indexing | shell-side; the kernel emits the events a projection folds |
| optimistic concurrency on `revision` | shell-side; the kernel already numbers revisions (R-44) |
| the event envelope (`event_id`, `recorded_at`, correlation, causation, actor) | shell-side by R-72; a reference envelope type would help adopters agree |
| definition inheritance and reusable schema fragments | authoring convenience; nothing in the kernel changes |
| definition migrations between versions | needs a decision on where an instance's version is advanced |
| a storage/search provider SPI in a non-core crate | R-82 places it outside; the crate does not exist yet |
| static validation of `set`/event template paths at registration | rules already get this (R-14); templates fail at run time (R-63) |
| named reusable predicates | authoring convenience |
| an `explain` verb: why an operation is or is not permitted from here, without executing | needs the kernel to expose per-rule verdicts |
| three-valued rule evaluation (`unknown` when a reference is missing) | required before `engineering-protocols` can be driven by this, see `docs/design/engineering-protocols-adoption-v0.1.md` § 4 |
| definition validation that accumulates every defect instead of stopping at the first | R-13 refuses correctly but reports one defect per attempt |
