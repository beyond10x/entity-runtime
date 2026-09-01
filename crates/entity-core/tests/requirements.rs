//! One test per requirement in `docs/requirements.md` that the kernel's behaviour can pin.
//!
//! Each test is named after the behaviour it protects, and the requirements register cites it by
//! name; `scripts/check-requirements.py` fails the gate when a cited test does not exist.

use entity_core::{
    execute, CoreError, DefinitionError, DefinitionErrors, EntityDefinition, EntityInstance,
    Registry, Runtime, Truth,
};
use serde_json::{json, Value};

fn definition(value: Value) -> EntityDefinition {
    serde_json::from_value(value).expect("a well-formed definition document")
}

fn register(value: Value) -> Result<Registry, DefinitionErrors> {
    let mut registry = Registry::new();
    registry.register(definition(value))?;
    Ok(registry)
}

/// A ticket with one field of every kind, two rules, a creation event and three operations.
fn ticket() -> Value {
    json!({
        "entity": "ticket",
        "version": 1,
        "schema": { "fields": {
            "title":      { "type": "string", "required": true, "min_length": 1, "max_length": 80 },
            "priority":   { "type": "enum", "values": ["low", "high"], "default": "low" },
            "points":     { "type": "integer", "min": 0, "max": 100 },
            "tags":       { "type": "array", "default": [], "items": { "type": "string" } },
            "meta":       { "type": "object", "properties": { "source": { "type": "string", "required": true } } },
            "extra":      { "type": "json" },
            "resolution": { "type": "string" },
            "assignee":   { "type": "string" }
        }},
        "lifecycle": { "initial": "open", "states": ["open", "in_progress", "closed"] },
        "invariants": [{
            "name": "closed_requires_resolution",
            "assert": { "any": [ { "ne": ["$state", "closed"] }, { "exists": "$fields.resolution" } ] },
            "message": "closed tickets need a resolution"
        }],
        "create": { "emit": { "type": "TicketOpened", "payload": {
            "id": "$id", "entity": "$entity", "version": "$version", "state": "$state",
            "title": "$fields.title", "literal": "$$not_a_reference"
        }}},
        "operations": {
            "start": {
                "arguments": { "fields": {
                    "assignee": { "type": "string", "required": true },
                    "note":     { "type": "string", "default": "none" }
                }},
                "transitions": [ { "from": "open", "to": "in_progress" } ],
                "set": { "assignee": "$args.assignee" },
                "emits": [ { "type": "TicketStarted", "payload": {
                    "assignee": "$fields.assignee", "note": "$args.note",
                    "from": "$from_state", "to": "$to_state", "all_args": "$args"
                }}]
            },
            "close": {
                "arguments": { "fields": { "resolution": { "type": "string", "required": true } } },
                "transitions": [ { "from": ["open", "in_progress"], "to": "closed" } ],
                "preconditions": [{
                    "name": "estimated",
                    "assert": { "gt": ["$fields.points", 0] },
                    "message": "unestimated tickets cannot be closed"
                }],
                "set": { "resolution": "$args.resolution" },
                "emits": [ { "type": "TicketClosed", "payload": { "resolution": "$fields.resolution" } } ]
            },
            "touch": {
                "transitions": [ { "from": "open", "to": "open" } ]
            }
        }
    })
}

fn with(mut base: Value, path: &str, value: Value) -> Value {
    let mut cursor = &mut base;
    let segments: Vec<&str> = path.split('.').collect();
    for segment in &segments[..segments.len() - 1] {
        cursor = cursor
            .as_object_mut()
            .expect("object on path")
            .entry(*segment)
            .or_insert_with(|| json!({}));
    }
    cursor
        .as_object_mut()
        .expect("object at leaf")
        .insert(segments[segments.len() - 1].to_owned(), value);
    base
}

fn open_ticket(registry: &Registry, points: i64) -> EntityInstance {
    Runtime::new(registry)
        .create(
            "ticket",
            1,
            "t-1",
            json!({ "title": "Login fails", "points": points }),
        )
        .expect("a valid ticket")
        .instance
}

// --- Dynamic definitions -------------------------------------------------------------------------

#[test]
fn definitions_are_registered_from_data_and_keyed_by_entity_and_version() {
    let mut registry = Registry::new();
    registry.register(definition(ticket())).expect("v1");
    registry
        .register(definition(with(ticket(), "version", json!(2))))
        .expect("v2");

    assert_eq!(registry.len(), 2);
    assert_eq!(registry.get("ticket", 1).map(|d| d.version), Some(1));
    assert_eq!(registry.get("ticket", 2).map(|d| d.version), Some(2));
    assert!(registry.get("ticket", 3).is_none());
    assert!(registry.get("order", 1).is_none());
}

#[test]
fn a_definition_without_a_version_defaults_to_one() {
    let mut document = ticket();
    document.as_object_mut().unwrap().remove("version");
    assert_eq!(definition(document).version, 1);
}

#[test]
fn a_definition_with_an_unknown_initial_state_is_refused() {
    let error = register(with(ticket(), "lifecycle.initial", json!("limbo"))).expect_err("refused");
    assert_eq!(
        error,
        DefinitionError::UnknownInitialState {
            state: "limbo".into()
        }
    );
}

#[test]
fn a_definition_with_duplicate_states_is_refused() {
    let error =
        register(with(ticket(), "lifecycle.states", json!(["open", "open"]))).expect_err("refused");
    assert_eq!(
        error,
        DefinitionError::DuplicateLifecycleState {
            state: "open".into()
        }
    );
}

#[test]
fn a_definition_with_a_zero_version_or_an_empty_name_is_refused() {
    assert_eq!(
        register(with(ticket(), "version", json!(0))).expect_err("refused"),
        DefinitionError::ZeroVersion
    );
    assert_eq!(
        register(with(ticket(), "entity", json!("  "))).expect_err("refused"),
        DefinitionError::EmptyEntityName
    );
}

#[test]
fn an_operation_without_transitions_is_refused() {
    let error =
        register(with(ticket(), "operations.touch.transitions", json!([]))).expect_err("refused");
    assert_eq!(
        error,
        DefinitionError::NoTransitions {
            operation: "touch".into()
        }
    );
}

#[test]
fn a_transition_through_an_undeclared_state_is_refused() {
    let to = register(with(
        ticket(),
        "operations.touch.transitions",
        json!([{ "from": "open", "to": "gone" }]),
    ));
    assert_eq!(
        to.expect_err("refused"),
        DefinitionError::UnknownToState {
            operation: "touch".into(),
            state: "gone".into()
        }
    );

    let from = register(with(
        ticket(),
        "operations.touch.transitions",
        json!([{ "from": "gone", "to": "open" }]),
    ));
    assert_eq!(
        from.expect_err("refused"),
        DefinitionError::UnknownFromState {
            operation: "touch".into(),
            state: "gone".into()
        }
    );
}

#[test]
fn two_transitions_from_one_state_in_one_operation_are_ambiguous() {
    let error = register(with(
        ticket(),
        "operations.touch.transitions",
        json!([{ "from": "open", "to": "open" }, { "from": ["closed", "open"], "to": "closed" }]),
    ))
    .expect_err("refused");
    assert_eq!(
        error,
        DefinitionError::AmbiguousTransition {
            operation: "touch".into(),
            state: "open".into()
        }
    );
}

#[test]
fn set_may_only_write_declared_fields() {
    let error = register(with(
        ticket(),
        "operations.touch.set",
        json!({ "ghost": "x" }),
    ))
    .expect_err("refused");
    assert_eq!(
        error,
        DefinitionError::UnknownSetField {
            operation: "touch".into(),
            field: "ghost".into()
        }
    );
}

#[test]
fn an_enum_without_values_and_an_array_without_items_are_refused() {
    let no_values = register(with(
        ticket(),
        "schema.fields.priority",
        json!({ "type": "enum" }),
    ))
    .expect_err("refused");
    assert!(
        matches!(no_values.first(), DefinitionError::InvalidField { ref path, .. } if path == "schema.priority"),
        "{no_values}"
    );

    let no_items = register(with(
        ticket(),
        "schema.fields.tags",
        json!({ "type": "array" }),
    ))
    .expect_err("refused");
    assert!(
        matches!(no_items.first(), DefinitionError::InvalidField { ref path, .. } if path == "schema.tags"),
        "{no_items}"
    );
}

/// R-13, revised. Validation used to stop at the first defect, so a document with three faults
/// took three attempts to fix and each attempt told you nothing about the next. Value validation
/// already reported every failing field at once (R-23); this is the same courtesy for the
/// definition, and `aep` invariant 3 asks for it by name.
#[test]
fn definition_validation_reports_every_defect_not_the_first() {
    let document = with(
        with(
            with(ticket(), "lifecycle.initial", json!("limbo")),
            "operations.touch.transitions",
            json!([{ "from": "open", "to": "open" }, { "from": "open", "to": "closed" }]),
        ),
        "schema.fields.points",
        json!({ "type": "integer", "default": "many" }),
    );
    let errors = register(document).expect_err("three defects");

    assert_eq!(errors.len(), 3, "{errors}");
    assert_eq!(
        errors.as_slice()[0],
        DefinitionError::UnknownInitialState {
            state: "limbo".into()
        }
    );
    assert!(
        matches!(
            &errors.as_slice()[1],
            DefinitionError::InvalidField { path, message }
                if path == "schema.points" && message.contains("invalid default")
        ),
        "{errors}"
    );
    assert_eq!(
        errors.as_slice()[2],
        DefinitionError::AmbiguousTransition {
            operation: "touch".into(),
            state: "open".into()
        }
    );

    // The sentence a person reads names the count and then every one of them.
    let reported = errors.to_string();
    assert!(reported.starts_with("3 defects; "), "{reported}");
    for defect in &errors {
        assert!(reported.contains(&defect.to_string()), "{reported}");
    }
}

/// The failure mode accumulation invites: one fault reported as many. A ladder with a duplicate
/// rung would make every transition in the document a second finding — *state `open` is not
/// declared* — burying the one defect that caused them.
#[test]
fn a_broken_ladder_is_reported_once_and_does_not_cascade_through_every_transition() {
    let errors = register(with(
        ticket(),
        "lifecycle.states",
        json!(["open", "open", "in_progress", "closed"]),
    ))
    .expect_err("refused");

    assert_eq!(
        errors,
        DefinitionError::DuplicateLifecycleState {
            state: "open".into()
        },
        "the duplicate rung, and nothing it caused: {errors}"
    );
}

/// A defect list is compared against a single defect only when it carries exactly that one, so
/// every existing single-defect assertion in this file is also asserting there were no others.
#[test]
fn comparing_a_defect_list_to_one_defect_holds_only_when_it_is_the_only_one() {
    let one = register(with(ticket(), "version", json!(0))).expect_err("refused");
    assert_eq!(one, DefinitionError::ZeroVersion);
    assert_eq!(one.len(), 1);
    assert_eq!(one.first(), &DefinitionError::ZeroVersion);

    let two = register(with(
        with(ticket(), "version", json!(0)),
        "entity",
        json!("  "),
    ))
    .expect_err("refused");
    assert_eq!(two.len(), 2);
    assert_ne!(two, DefinitionError::ZeroVersion);
    assert_eq!(two.first(), &DefinitionError::EmptyEntityName);
}

// --- Time ----------------------------------------------------------------------------------------

/// The clock is read at the edge (R-62): a definition compares two instants it was handed, and
/// there is no `$now` for it to ask.
#[test]
fn before_and_after_order_two_instants_the_shell_supplied() {
    let cases = [
        // `title` is present and is not a timestamp: unreadable, so Unknown rather than false.
        (
            json!({ "before": ["$fields.title", "2026-08-25"] }),
            Truth::Unknown,
        ),
        // A reference that resolves to nothing is Unknown for the ordinary reason.
        (
            json!({ "before": ["$fields.resolution", "2026-08-25"] }),
            Truth::Unknown,
        ),
        (
            json!({ "before": ["2026-01-01", "2026-08-25"] }),
            Truth::True,
        ),
        (
            json!({ "after":  ["2026-01-01", "2026-08-25"] }),
            Truth::False,
        ),
        (
            json!({ "after":  ["2026-08-25T12:00:01Z", "2026-08-25T12:00:00Z"] }),
            Truth::True,
        ),
        // Not lexicographic luck: a single-digit month would sort wrong as text.
        (
            json!({ "before": ["2026-01-31", "2026-02-01"] }),
            Truth::True,
        ),
        // Equal instants are neither before nor after.
        (
            json!({ "before": ["2026-08-25", "2026-08-25T00:00:00"] }),
            Truth::False,
        ),
        (
            json!({ "after":  ["2026-08-25", "2026-08-25T00:00:00"] }),
            Truth::False,
        ),
    ];
    for (condition, expected) in cases {
        let registry = register(with(
            ticket(),
            "operations.touch.preconditions",
            json!([{ "assert": condition }]),
        ))
        .unwrap();
        assert_verdict(&registry, &condition, expected);
    }
}

/// An instant this kernel cannot read is `Unknown`, **not** `false` — and the refusal names the
/// operand it could not read.
///
/// Deliberately different from `gt` on two non-numbers, which is `false`. *These are not numbers*
/// is an observation anybody can make; *this is not a timestamp I can read* is a statement about
/// this kernel's reach. Reading it as `false` would let `after: [$args.now, $fields.due]` answer
/// "not yet due" for a value nobody understood, which is the collapse three-valued rules exist to
/// prevent.
#[test]
fn an_instant_this_kernel_cannot_read_is_unobservable_and_the_refusal_names_it() {
    let registry = register(with(
        with(
            ticket(),
            "operations.touch.arguments",
            json!({ "fields": { "now": { "type": "string" } } }),
        ),
        "operations.touch.preconditions",
        json!([{
            "name": "not_yet_due",
            "message": "the deadline has not passed",
            "assert": { "before": ["$args.now", "2026-12-31"] }
        }]),
    ))
    .unwrap();
    let instance = open_ticket(&registry, 3);

    // An offset is refused rather than normalised: comparing it with a naive instant has no
    // correct answer, and a shell that has offsets has a clock to normalise with.
    for unreadable in [
        "2026-08-25T12:00:00+02:00",
        "yesterday",
        "1756108800",
        "2026-8-25",
    ] {
        let error = Runtime::new(&registry)
            .execute(&instance, "touch", json!({ "now": unreadable }))
            .unwrap_err();
        assert!(
            matches!(
                error,
                CoreError::PreconditionUnobservable { ref unresolved, .. }
                    if unresolved == &["$args.now".to_owned()]
            ),
            "{unreadable}: {error}"
        );
    }

    // And one it can read decides.
    Runtime::new(&registry)
        .execute(&instance, "touch", json!({ "now": "2026-08-25" }))
        .expect("a readable instant before the deadline");
}

// --- Typed references ----------------------------------------------------------------------------

/// A `ref` that does not say what it points at is a string with extra ceremony.
#[test]
fn a_ref_declares_the_entity_it_points_at_or_it_is_not_a_ref() {
    let untargeted = register(with(
        ticket(),
        "schema.fields.epic",
        json!({ "type": "ref" }),
    ))
    .expect_err("refused");
    assert!(
        matches!(untargeted.first(), DefinitionError::InvalidField { path, message }
            if path == "schema.epic" && message.contains("must declare 'entity'")),
        "{untargeted}"
    );

    let blank = register(with(
        ticket(),
        "schema.fields.epic",
        json!({ "type": "ref", "entity": "  " }),
    ))
    .expect_err("refused");
    assert!(
        matches!(blank.first(), DefinitionError::InvalidField { .. }),
        "{blank}"
    );

    // And the constraint is refused where it does not govern, exactly as `values` on a string is.
    let misplaced = register(with(
        ticket(),
        "schema.fields.title",
        json!({ "type": "string", "entity": "epic" }),
    ))
    .expect_err("refused");
    assert!(
        matches!(misplaced.first(), DefinitionError::ConstraintNotApplicable { constraint, .. }
            if *constraint == "entity/inverse/acyclic"),
        "{misplaced}"
    );
}

/// R-26 through the machinery R-26 exists for. `acyclic` is an `Option<bool>` and not a `bool`
/// precisely so that **written** and **absent** are different things: with a plain `bool`,
/// `acyclic: false` on a string is indistinguishable from not writing it, so it would be accepted
/// in silence — a key nobody reads, on a field the author believes is governed.
#[test]
fn a_written_acyclic_is_refused_on_a_kind_it_does_not_govern() {
    for written in [json!(true), json!(false)] {
        for kind in ["string", "json", "integer"] {
            let refused = register(with(
                ticket(),
                "schema.fields.probe",
                json!({ "type": kind, "acyclic": written }),
            ))
            .unwrap_err();
            assert!(
                matches!(
                    refused.first(),
                    DefinitionError::ConstraintNotApplicable { constraint, .. }
                        if *constraint == "entity/inverse/acyclic"
                ),
                "{kind} with acyclic: {written} — {refused}"
            );
        }
    }

    // And on a `ref`, where it does govern, both are accepted and mean different things.
    for written in [json!(true), json!(false)] {
        register(with(
            ticket(),
            "schema.fields.probe",
            json!({ "type": "ref", "entity": "other", "acyclic": written }),
        ))
        .unwrap_or_else(|error| panic!("a ref may declare acyclic: {written} — {error}"));
    }
}

/// The kernel checks the shape of an identity and stops there. It is handed one instance (R-01),
/// so whether an epic called `epic:x` exists is not a question it can be asked — and answering it
/// by lookup would make the same inputs give different answers at different moments (R-02).
#[test]
fn a_reference_is_an_identity_and_the_kernel_checks_nothing_else_about_it() {
    let registry = register(with(
        ticket(),
        "schema.fields.epic",
        json!({ "type": "ref", "entity": "epic", "inverse": "stories", "acyclic": true }),
    ))
    .expect("a well-formed ref");
    let runtime = Runtime::new(&registry);

    // No `epic` is registered and no epic instance exists anywhere. It is still accepted.
    let created = runtime
        .create(
            "ticket",
            1,
            "t-1",
            json!({ "title": "t", "epic": "epic:nothing-has-this-id" }),
        )
        .expect("the kernel does not resolve a reference");
    assert_eq!(
        created.instance.fields["epic"],
        json!("epic:nothing-has-this-id")
    );

    for bad in [
        json!(""),
        json!("   "),
        json!(7),
        json!(null),
        json!(["epic:a"]),
    ] {
        runtime
            .create("ticket", 1, "t-2", json!({ "title": "t", "epic": bad }))
            .expect_err("an identity is a non-empty string");
    }
}

/// Two types that point at each other are ordinary. A check that ran at registration would make
/// them impossible to register in either order, which is why the set is validated as a set.
#[test]
fn mutually_referencing_types_register_in_either_order_and_validate_as_a_set() {
    let story = json!({
        "entity": "story",
        "schema": { "fields": { "epic": { "type": "ref", "entity": "epic" } } },
        "lifecycle": { "initial": "draft", "states": ["draft"] },
        "operations": { "touch": { "transitions": [{ "from": "draft", "to": "draft" }] } }
    });
    let epic = json!({
        "entity": "epic",
        "schema": { "fields": { "stories": {
            "type": "array", "items": { "type": "ref", "entity": "story" }
        }}},
        "lifecycle": { "initial": "draft", "states": ["draft"] },
        "operations": { "touch": { "transitions": [{ "from": "draft", "to": "draft" }] } }
    });

    for order in [[&story, &epic], [&epic, &story]] {
        let mut registry = Registry::new();
        for document in order {
            registry
                .register(definition(document.clone()))
                .expect("each is a valid definition on its own");
        }
        registry.validate_all().expect("and the set is consistent");
    }
}

/// Every missing target, not the first, and at any depth — a reference nested in a list is still a
/// reference.
#[test]
fn validate_all_names_every_reference_whose_type_nobody_registered() {
    let mut registry = Registry::new();
    registry
        .register(definition(json!({
            "entity": "order",
            "schema": { "fields": {
                "customer": { "type": "ref", "entity": "customer" },
                "lines": { "type": "array", "items": { "type": "object", "properties": {
                    "sku": { "type": "ref", "entity": "product" }
                }}}
            }},
            "lifecycle": { "initial": "draft", "states": ["draft"] },
            "operations": { "assign": {
                "arguments": { "fields": { "courier": { "type": "ref", "entity": "courier" } } },
                "transitions": [{ "from": "draft", "to": "draft" }]
            }}
        })))
        .expect("valid on its own — nothing here is about other types");

    let errors = registry
        .validate_all()
        .expect_err("three types are missing");
    assert_eq!(errors.len(), 3, "{errors}");

    let reported: Vec<String> = errors
        .iter()
        .map(|defect| match defect {
            DefinitionError::UnknownRelationTarget { path, target, .. } => {
                format!("{path} -> {target}")
            }
            other => panic!("unexpected defect: {other}"),
        })
        .collect();
    assert!(
        reported.contains(&"schema.customer -> customer".to_owned()),
        "{reported:?}"
    );
    assert!(
        reported.contains(&"schema.lines[].sku -> product".to_owned()),
        "{reported:?}"
    );
    assert!(
        reported.contains(&"operations.assign.arguments.courier -> courier".to_owned()),
        "{reported:?}"
    );
}

#[test]
fn an_invalid_default_is_refused_at_registration() {
    let error = register(with(
        ticket(),
        "schema.fields.points",
        json!({ "type": "integer", "default": "many" }),
    ))
    .expect_err("refused");
    assert!(
        matches!(error.first(), DefinitionError::InvalidField { ref path, ref message } if path == "schema.points" && message.contains("invalid default")),
        "{error}"
    );
}

#[test]
fn an_empty_event_type_is_refused() {
    let error = register(with(ticket(), "create.emit.type", json!(""))).expect_err("refused");
    assert_eq!(error, DefinitionError::EmptyEventType { operation: None });
}

#[test]
fn a_rule_referencing_an_undeclared_field_is_refused() {
    let error = register(with(
        ticket(),
        "operations.touch.preconditions",
        json!([{ "assert": { "exists": "$fields.nonexistent" } }]),
    ))
    .expect_err("refused");
    assert!(
        matches!(error.first(), DefinitionError::InvalidRule { ref path, .. } if path.starts_with("operations.touch.preconditions[0].assert.exists")),
        "{error}"
    );
}

#[test]
fn an_invariant_may_not_read_arguments_or_previous_state() {
    for reference in ["$args.x", "$old_fields.title", "$from_state", "$args"] {
        let error = register(with(
            ticket(),
            "invariants",
            json!([{ "assert": { "exists": reference } }]),
        ))
        .expect_err(reference);
        assert!(
            matches!(error.first(), DefinitionError::InvalidRule { ref path, .. } if path == "invariants[0].assert.exists"),
            "{reference}: {error}"
        );
    }
}

#[test]
fn an_empty_all_or_any_is_refused() {
    for operator in ["all", "any"] {
        let error = register(with(
            ticket(),
            "invariants",
            json!([{ "assert": { operator: [] } }]),
        ))
        .expect_err(operator);
        assert!(
            matches!(error.first(), DefinitionError::InvalidRule { ref message, .. } if message.contains(operator)),
            "{operator}: {error}"
        );
    }
}

// --- Schema --------------------------------------------------------------------------------------

#[test]
fn create_applies_defaults_then_validates() {
    let registry = register(ticket()).unwrap();
    let created = Runtime::new(&registry)
        .create("ticket", 1, "t-1", json!({ "title": "Login fails" }))
        .expect("defaults fill the gaps");
    assert_eq!(created.instance.fields["priority"], json!("low"));
    assert_eq!(created.instance.fields["tags"], json!([]));
    assert!(
        !created.instance.fields.contains_key("points"),
        "no default, no value"
    );
}

#[test]
fn validation_accumulates_every_field_error() {
    let registry = register(ticket()).unwrap();
    let error = Runtime::new(&registry)
        .create(
            "ticket",
            1,
            "t-1",
            json!({
                "title": "", "points": 500, "priority": "urgent", "tags": ["ok", 7],
                "meta": { "unknown": 1 }, "nope": true
            }),
        )
        .expect_err("six things are wrong");
    let CoreError::Validation(errors) = error else {
        panic!("{error}")
    };
    let mut paths: Vec<&str> = errors.iter().map(|error| error.path.as_str()).collect();
    paths.sort_unstable();
    assert_eq!(
        paths,
        [
            "fields.meta.source",
            "fields.meta.unknown",
            "fields.nope",
            "fields.points",
            "fields.priority",
            "fields.tags[1]",
            "fields.title",
        ]
    );
}

#[test]
fn fields_and_arguments_must_be_objects() {
    let registry = register(ticket()).unwrap();
    let runtime = Runtime::new(&registry);
    let error = runtime
        .create("ticket", 1, "t-1", json!([1, 2]))
        .expect_err("not an object");
    assert!(
        matches!(&error, CoreError::Validation(errors) if errors.len() == 1 && errors[0].path == "fields"),
        "{error}"
    );

    let instance = open_ticket(&registry, 3);
    let error = runtime
        .execute(&instance, "touch", json!("nope"))
        .expect_err("not an object");
    assert!(
        matches!(&error, CoreError::Validation(errors) if errors.len() == 1 && errors[0].path == "arguments"),
        "{error}"
    );
}

#[test]
fn unknown_fields_are_allowed_only_when_the_schema_says_so() {
    let strict = register(ticket()).unwrap();
    let error = Runtime::new(&strict)
        .create("ticket", 1, "t-1", json!({ "title": "x", "surprise": 1 }))
        .expect_err("undeclared field");
    assert!(
        matches!(&error, CoreError::Validation(errors) if errors[0].path == "fields.surprise"),
        "{error}"
    );

    let lenient = register(with(ticket(), "schema.additional_fields", json!(true))).unwrap();
    let created = Runtime::new(&lenient)
        .create("ticket", 1, "t-1", json!({ "title": "x", "surprise": 1 }))
        .expect("allowed");
    assert_eq!(created.instance.fields["surprise"], json!(1));
}

#[test]
fn every_field_kind_is_checked() {
    let registry = register(ticket()).unwrap();
    let runtime = Runtime::new(&registry);
    let wrong = json!({
        "title": 1, "priority": 2, "points": "three", "tags": "four", "meta": "five", "extra": [6]
    });
    let error = runtime
        .create("ticket", 1, "t-1", wrong)
        .expect_err("five wrong kinds; json accepts anything");
    let CoreError::Validation(errors) = error else {
        panic!("{error}")
    };
    let mut paths: Vec<&str> = errors.iter().map(|error| error.path.as_str()).collect();
    paths.sort_unstable();
    assert_eq!(
        paths,
        [
            "fields.meta",
            "fields.points",
            "fields.priority",
            "fields.tags",
            "fields.title"
        ]
    );
}

// --- Lifecycle and operations --------------------------------------------------------------------

#[test]
fn create_enters_the_initial_state_at_revision_one_and_emits_the_creation_event() {
    let registry = register(ticket()).unwrap();
    let created = Runtime::new(&registry)
        .create("ticket", 1, "t-1", json!({ "title": "Login fails" }))
        .expect("created");
    assert_eq!(created.instance.lifecycle_state, "open");
    assert_eq!(created.instance.revision, 1);
    assert_eq!(created.instance.id, "t-1");
    assert_eq!(created.events.len(), 1);
    let event = &created.events[0];
    assert_eq!(event.event_type, "TicketOpened");
    assert_eq!(event.revision, 1);
    assert_eq!(
        event.payload,
        json!({
            "id": "t-1", "entity": "ticket", "version": 1, "state": "open",
            "title": "Login fails", "literal": "$not_a_reference"
        })
    );
}

#[test]
fn an_operation_not_declared_from_the_current_state_is_refused_before_its_preconditions() {
    // `close` has a precondition that would fail on an unestimated ticket, and `start` moves the
    // ticket away from the only state `touch` accepts. The lifecycle answers first.
    let registry = register(with(
        ticket(),
        "operations.touch.preconditions",
        json!([{ "assert": false }]),
    ))
    .unwrap();
    let runtime = Runtime::new(&registry);
    let started = runtime
        .execute(
            &open_ticket(&registry, 0),
            "start",
            json!({ "assignee": "alice" }),
        )
        .expect("started")
        .instance;
    let error = runtime
        .execute(&started, "touch", json!({}))
        .expect_err("no transition from in_progress");
    assert_eq!(
        error,
        CoreError::InvalidTransition {
            operation: "touch".into(),
            state: "in_progress".into()
        }
    );
}

#[test]
fn a_transition_may_start_from_several_states() {
    let registry = register(ticket()).unwrap();
    let runtime = Runtime::new(&registry);
    let from_open = runtime
        .execute(
            &open_ticket(&registry, 3),
            "close",
            json!({ "resolution": "fixed" }),
        )
        .expect("open -> closed");
    assert_eq!(from_open.instance.lifecycle_state, "closed");

    let started = runtime
        .execute(
            &open_ticket(&registry, 3),
            "start",
            json!({ "assignee": "alice" }),
        )
        .expect("started")
        .instance;
    let from_started = runtime
        .execute(&started, "close", json!({ "resolution": "fixed" }))
        .expect("in_progress -> closed");
    assert_eq!(from_started.instance.lifecycle_state, "closed");
}

#[test]
fn operation_arguments_are_defaulted_and_validated() {
    let registry = register(ticket()).unwrap();
    let runtime = Runtime::new(&registry);
    let instance = open_ticket(&registry, 3);

    let started = runtime
        .execute(&instance, "start", json!({ "assignee": "alice" }))
        .expect("defaults apply");
    assert_eq!(started.events[0].payload["note"], json!("none"));
    assert_eq!(
        started.events[0].payload["all_args"],
        json!({ "assignee": "alice", "note": "none" })
    );

    let error = runtime
        .execute(&instance, "start", json!({}))
        .expect_err("assignee is required");
    assert!(
        matches!(&error, CoreError::Validation(errors) if errors.len() == 1 && errors[0].path == "arguments.assignee"),
        "{error}"
    );

    let error = runtime
        .execute(
            &instance,
            "start",
            json!({ "assignee": "alice", "extra": 1 }),
        )
        .expect_err("undeclared argument");
    assert!(
        matches!(&error, CoreError::Validation(errors) if errors[0].path == "arguments.extra"),
        "{error}"
    );
}

#[test]
fn set_assignments_read_the_pre_operation_fields_whatever_their_order() {
    // Two assignments that swap two fields. If `set` were applied in order, `b` would read the
    // already-overwritten `a`; because every assignment reads the pre-operation fields, the swap
    // is a swap.
    let document = with(
        with(
            with(ticket(), "schema.fields.a", json!({ "type": "string" })),
            "schema.fields.b",
            json!({ "type": "string" }),
        ),
        "operations.swap",
        json!({
            "transitions": [ { "from": "open", "to": "open" } ],
            "set": { "a": "$fields.b", "b": "$fields.a" },
            "emits": [ { "type": "Swapped", "payload": { "a": "$fields.a", "was_a": "$old_fields.a" } } ]
        }),
    );
    let registry = register(document).unwrap();
    let runtime = Runtime::new(&registry);
    let instance = runtime
        .create(
            "ticket",
            1,
            "t-1",
            json!({ "title": "x", "a": "left", "b": "right" }),
        )
        .expect("created")
        .instance;
    let swapped = runtime
        .execute(&instance, "swap", json!({}))
        .expect("swapped");
    assert_eq!(swapped.instance.fields["a"], json!("right"));
    assert_eq!(swapped.instance.fields["b"], json!("left"));
    assert_eq!(
        swapped.events[0].payload,
        json!({ "a": "right", "was_a": "left" })
    );
}

#[test]
fn fields_are_revalidated_after_set() {
    let document = with(
        ticket(),
        "operations.estimate",
        json!({
            "arguments": { "fields": { "points": { "type": "integer", "required": true } } },
            "transitions": [ { "from": "open", "to": "open" } ],
            "set": { "points": "$args.points" }
        }),
    );
    let registry = register(document).unwrap();
    let runtime = Runtime::new(&registry);
    let instance = open_ticket(&registry, 3);
    let error = runtime
        .execute(&instance, "estimate", json!({ "points": 500 }))
        .expect_err("the argument is fine; the field is not");
    assert!(
        matches!(&error, CoreError::Validation(errors) if errors.len() == 1 && errors[0].path == "fields.points"),
        "{error}"
    );
    assert_eq!(
        runtime
            .execute(&instance, "estimate", json!({ "points": 50 }))
            .expect("within range")
            .instance
            .fields["points"],
        json!(50)
    );
}

#[test]
fn every_successful_operation_increments_the_revision_by_one() {
    let registry = register(ticket()).unwrap();
    let runtime = Runtime::new(&registry);
    let opened = open_ticket(&registry, 3);
    assert_eq!(opened.revision, 1);
    let touched = runtime
        .execute(&opened, "touch", json!({}))
        .unwrap()
        .instance;
    assert_eq!(touched.revision, 2);
    let started = runtime
        .execute(&touched, "start", json!({ "assignee": "alice" }))
        .unwrap();
    assert_eq!(started.instance.revision, 3);
    assert_eq!(
        started.events[0].revision, 3,
        "an event carries the revision it produced"
    );
    // A refusal does not consume a revision.
    runtime
        .execute(&started.instance, "touch", json!({}))
        .expect_err("refused");
    let closed = runtime
        .execute(&started.instance, "close", json!({ "resolution": "fixed" }))
        .unwrap();
    assert_eq!(closed.instance.revision, 4);
}

#[test]
fn an_instance_of_another_definition_is_refused() {
    let registry = register(ticket()).unwrap();
    let runtime = Runtime::new(&registry);
    let mut instance = open_ticket(&registry, 3);

    let error = runtime
        .execute(&instance, "reopen", json!({}))
        .expect_err("no such operation");
    assert_eq!(
        error,
        CoreError::OperationNotFound {
            operation: "reopen".into()
        }
    );

    instance.version = 2;
    let error = runtime
        .execute(&instance, "touch", json!({}))
        .expect_err("v2 is not registered");
    assert_eq!(
        error,
        CoreError::EntityNotRegistered {
            entity: "ticket".into(),
            version: 2
        }
    );

    let error = execute(
        registry.get("ticket", 1).unwrap(),
        &instance,
        "touch",
        json!({}),
    )
    .expect_err("handed the wrong definition");
    assert_eq!(
        error,
        CoreError::EntityMismatch {
            expected_entity: "ticket".into(),
            expected_version: 1,
            actual_entity: "ticket".into(),
            actual_version: 2,
        }
    );

    let error = runtime
        .create("order", 1, "o-1", json!({}))
        .expect_err("no such entity");
    assert_eq!(
        error,
        CoreError::EntityNotRegistered {
            entity: "order".into(),
            version: 1
        }
    );
}

/// A rule that should hold produces a decision; one that should not produces a
/// `PreconditionFailed` and nothing else. `is_ok()` would accept a refusal for any other reason —
/// a validation error, a lifecycle refusal — and so could not tell "the reference read false" from
/// "the condition blew up".
/// Runs `touch` and reports what its single precondition evaluated to, as the three values a
/// condition now has. Which refusal came back *is* the verdict: `PreconditionFailed` means the
/// rule was answered and contradicted, `PreconditionUnobservable` means it could not be answered.
fn assert_verdict(registry: &Registry, condition: &Value, expected: Truth) {
    let instance = open_ticket(registry, 3);
    let verdict = match Runtime::new(registry).execute(&instance, "touch", json!({})) {
        Ok(decision) => {
            assert_eq!(decision.instance.revision, 2);
            Truth::True
        }
        Err(CoreError::PreconditionFailed { .. }) => Truth::False,
        Err(CoreError::PreconditionUnobservable { .. }) => Truth::Unknown,
        Err(other) => panic!("{condition} must be decided as a precondition, not as {other}"),
    };
    assert_eq!(verdict, expected, "{condition}");
}

// --- Rules ---------------------------------------------------------------------------------------

#[test]
fn a_failed_precondition_yields_no_decision_and_names_the_rule() {
    let registry = register(ticket()).unwrap();
    let instance = open_ticket(&registry, 0);
    let error = Runtime::new(&registry)
        .execute(&instance, "close", json!({ "resolution": "wontfix" }))
        .expect_err("unestimated");
    assert_eq!(
        error,
        CoreError::PreconditionFailed {
            operation: "close".into(),
            rule: Some("estimated".into()),
            message: "unestimated tickets cannot be closed".into(),
        }
    );
    assert_eq!(instance.lifecycle_state, "open");
    assert_eq!(instance.revision, 1);
}

#[test]
fn a_failed_invariant_after_an_operation_yields_no_decision_and_no_events() {
    // Remove the argument that would have satisfied the invariant, so `close` reaches `closed`
    // without a resolution. The invariant asks `exists`, which is a question about the store and
    // is answerable: there is no resolution, so this is a plain violation. Three-valued rules did
    // not change this test, which is the point of confining `Unknown` to value questions.
    let document = with(
        with(ticket(), "operations.close.arguments", json!({})),
        "operations.close.set",
        json!({}),
    );
    let registry = register(document).unwrap();
    let instance = open_ticket(&registry, 3);
    let error = Runtime::new(&registry)
        .execute(&instance, "close", json!({}))
        .expect_err("no resolution");
    assert_eq!(
        error,
        CoreError::InvariantViolation {
            rule: Some("closed_requires_resolution".into()),
            message: "closed tickets need a resolution".into(),
        }
    );
    assert_eq!(instance.lifecycle_state, "open");
}

/// The other half of R-51 under three-valued rules: an invariant that something *observed*
/// contradicts is still a violation after an operation, not an unobservable. Without this the
/// register's claim that a failure is `InvariantViolation` would be pinned only at creation.
#[test]
fn an_invariant_contradicted_after_an_operation_is_a_violation_not_an_unobservable() {
    // Also the guard idiom in situ: `not exists` short-circuits nothing, but Kleene's `False`
    // dominance means the comparison beside it cannot stall the rule once presence is decided.
    let document = with(
        ticket(),
        "invariants",
        json!([{
            "name": "resolution_says_something",
            // The presence test guards the comparison, so the invariant is satisfiable before a
            // resolution is written — an invariant is checked at creation too, and *not yet
            // resolved* must not read as *resolved badly*.
            "assert": { "any": [
                { "not": { "exists": "$fields.resolution" } },
                { "ne": ["$fields.resolution", "unknown"] }
            ]},
            "message": "'unknown' is not a resolution"
        }]),
    );
    let registry = register(document).unwrap();
    let instance = open_ticket(&registry, 3);
    let error = Runtime::new(&registry)
        .execute(&instance, "close", json!({ "resolution": "unknown" }))
        .expect_err("'unknown' is not a resolution");
    assert_eq!(
        error,
        CoreError::InvariantViolation {
            rule: Some("resolution_says_something".into()),
            message: "'unknown' is not a resolution".into(),
        }
    );
}

#[test]
fn a_failed_invariant_at_creation_yields_no_decision() {
    let registry = register(with(
        ticket(),
        "invariants",
        json!([{ "name": "titled", "assert": { "ne": ["$fields.title", "untitled"] } }]),
    ))
    .unwrap();
    let error = Runtime::new(&registry)
        .create("ticket", 1, "t-1", json!({ "title": "untitled" }))
        .expect_err("refused");
    assert!(
        matches!(error, CoreError::InvariantViolation { rule: Some(rule), .. } if rule == "titled")
    );
}

#[test]
fn a_rule_without_a_message_reports_a_default() {
    let registry = register(with(
        ticket(),
        "operations.touch.preconditions",
        json!([{ "assert": false }]),
    ))
    .unwrap();
    let error = Runtime::new(&registry)
        .execute(&open_ticket(&registry, 3), "touch", json!({}))
        .expect_err("refused");
    assert_eq!(
        error,
        CoreError::PreconditionFailed {
            operation: "touch".into(),
            rule: None,
            message: "condition evaluated to false".into()
        }
    );
}

/// R-54, revised. This test used to assert the collapse it now exists to prevent: a missing
/// reference read `false`, so *nobody recorded a resolution* and *the resolution is not "x"* came
/// back as one refusal. The old wording is kept in the register beside the new one.
///
/// The split is by **what is being asked**, not by which operator asks it. `exists` is a question
/// about the store and stays two-valued; every comparison is a question about a value, and has no
/// answer when there is no value to read.
#[test]
fn a_value_question_over_a_missing_reference_is_unobservable_and_exists_stays_two_valued() {
    let cases = [
        // Value questions. Nothing was recorded, so there is nothing to compare against.
        (json!({ "eq": ["$fields.resolution", "x"] }), Truth::Unknown),
        (json!({ "ne": ["$fields.resolution", "x"] }), Truth::Unknown),
        (json!({ "gt": ["$fields.resolution", 0] }), Truth::Unknown),
        (
            json!({ "in": ["$fields.resolution", ["x"]] }),
            Truth::Unknown,
        ),
        (
            json!({ "contains": ["$fields.resolution", "x"] }),
            Truth::Unknown,
        ),
        // A question about the store. Always answerable, so two-valued — and `not` is ordinary.
        (json!({ "exists": "$fields.resolution" }), Truth::False),
        (
            json!({ "not": { "exists": "$fields.resolution" } }),
            Truth::True,
        ),
        (json!({ "exists": "$fields.title" }), Truth::True),
        (
            json!({ "not": { "exists": "$fields.title" } }),
            Truth::False,
        ),
        (json!({ "exists": "$from_state" }), Truth::True),
        // A reference that resolves is unaffected: two-valued, exactly as before.
        (
            json!({ "eq": ["$fields.title", "Login fails"] }),
            Truth::True,
        ),
        (json!({ "eq": ["$fields.title", "other"] }), Truth::False),
        // Kleene: `False` dominates a conjunction and `True` dominates a disjunction, so a rule
        // that is already decided is not held up by what is missing beside it.
        (
            json!({ "all": [ false, { "eq": ["$fields.resolution", "x"] } ] }),
            Truth::False,
        ),
        (
            json!({ "any": [ true, { "eq": ["$fields.resolution", "x"] } ] }),
            Truth::True,
        ),
        (
            json!({ "all": [ true, { "eq": ["$fields.resolution", "x"] } ] }),
            Truth::Unknown,
        ),
        (
            json!({ "any": [ false, { "eq": ["$fields.resolution", "x"] } ] }),
            Truth::Unknown,
        ),
        // The idiom that makes the two groups compose: guard the value question with the store
        // question, and a rule about something nobody recorded refuses plainly instead of
        // stalling. Order does not matter — Kleene is commutative — so this is not a trick.
        (
            json!({ "all": [
                { "exists": "$fields.resolution" },
                { "eq": ["$fields.resolution", "x"] }
            ]}),
            Truth::False,
        ),
    ];
    for (condition, expected) in cases {
        let registry = register(with(
            ticket(),
            "operations.touch.preconditions",
            json!([{ "assert": condition }]),
        ))
        .unwrap();
        assert_verdict(&registry, &condition, expected);
    }
}

/// One refusal the operator can act on once, rather than three in sequence. This is what buys
/// out `all`/`any` short-circuiting: the truth value is the same either way, but which addresses
/// have been gathered when the answer comes back is not.
#[test]
fn an_unobservable_refusal_names_every_unresolved_reference_not_the_first() {
    let registry = register(with(
        ticket(),
        "operations.touch.preconditions",
        json!([{
            "name": "evidenced",
            "message": "every fact the gate reads must be recorded",
            "assert": { "all": [
                { "eq": ["$fields.resolution", "fixed"] },
                { "eq": ["$fields.assignee", "someone"] },
                { "gt": ["$fields.extra", 0] }
            ]}
        }]),
    ))
    .unwrap();
    let instance = open_ticket(&registry, 3);
    let error = Runtime::new(&registry)
        .execute(&instance, "touch", json!({}))
        .expect_err("nothing was recorded");
    assert_eq!(
        error,
        CoreError::PreconditionUnobservable {
            operation: "touch".into(),
            rule: Some("evidenced".into()),
            message: "every fact the gate reads must be recorded".into(),
            unresolved: vec![
                "$fields.assignee".into(),
                "$fields.extra".into(),
                "$fields.resolution".into(),
            ],
        }
    );
    assert!(
        error.to_string().contains(
            "nothing was observed at $fields.assignee, $fields.extra, $fields.resolution"
        ),
        "{error}"
    );
}

/// `key:` with nothing after it is how YAML front matter spells *nobody filled this in*, so a
/// present `null` is not a value: `exists` says so, and a comparison against it has no answer.
/// Schema validation cannot catch this for a `json` field, where `null` is legal.
#[test]
fn a_present_null_is_not_a_value_for_either_kind_of_question() {
    let base = with(
        ticket(),
        "operations.touch.preconditions",
        json!([{ "name": "recorded", "assert": { "exists": "$fields.extra" } }]),
    );
    let registry = register(base).unwrap();
    let runtime = Runtime::new(&registry);
    let blank = runtime
        .create("ticket", 1, "t-1", json!({ "title": "t", "extra": null }))
        .expect("a null json field is a legal value")
        .instance;
    assert_eq!(blank.fields["extra"], Value::Null, "the null was stored");

    // The store question is answered: there is nothing there.
    let error = runtime
        .execute(&blank, "touch", json!({}))
        .expect_err("a blank field is not a value");
    assert!(
        matches!(error, CoreError::PreconditionFailed { .. }),
        "{error}"
    );

    // The value question has no answer, and says which address it could not read.
    let registry = register(with(
        ticket(),
        "operations.touch.preconditions",
        json!([{ "name": "positive", "assert": { "gt": ["$fields.extra", 0] } }]),
    ))
    .unwrap();
    let error = Runtime::new(&registry)
        .execute(&blank, "touch", json!({}))
        .expect_err("nothing to compare");
    assert!(
        matches!(
            error,
            CoreError::PreconditionUnobservable { ref unresolved, .. }
                if unresolved == &["$fields.extra".to_owned()]
        ),
        "{error}"
    );
}

#[test]
fn contains_and_in_cover_arrays_strings_objects_and_membership() {
    let registry = register(with(
        ticket(),
        "operations.touch.preconditions",
        json!([{ "assert": { "all": [
            { "contains": ["$fields.tags", "urgent"] },
            { "contains": ["$fields.title", "bug"] },
            { "contains": ["$fields.meta", "source"] },
            { "in": ["$fields.priority", ["low", "high"]] },
            { "not": { "in": ["$fields.priority", ["medium"]] } },
            { "not": { "contains": ["$fields.tags", "trivial"] } }
        ]}}]),
    ))
    .unwrap();
    let runtime = Runtime::new(&registry);
    let instance = runtime
        .create(
            "ticket",
            1,
            "t-1",
            json!({ "title": "a bug", "tags": ["urgent"], "meta": { "source": "mail" } }),
        )
        .unwrap()
        .instance;
    runtime
        .execute(&instance, "touch", json!({}))
        .expect("every clause holds");

    let other = runtime
        .create(
            "ticket",
            1,
            "t-2",
            json!({ "title": "a bug", "tags": [], "meta": { "source": "mail" } }),
        )
        .unwrap()
        .instance;
    let error = runtime
        .execute(&other, "touch", json!({}))
        .expect_err("tags no longer contain urgent");
    assert!(matches!(error, CoreError::PreconditionFailed { .. }));
}

#[test]
fn numeric_comparisons_are_numeric_and_compare_false_otherwise() {
    let cases = [
        (json!({ "gt":  ["$fields.points", 2] }), true),
        (json!({ "gte": ["$fields.points", 3] }), true),
        (json!({ "lt":  ["$fields.points", 3] }), false),
        (json!({ "lte": ["$fields.points", 3] }), true),
        (json!({ "gt":  ["$fields.title", 0] }), false),
        (json!({ "eq":  ["$fields.points", 3] }), true),
        (json!({ "eq":  ["$fields.points", "3"] }), false),
        (json!({ "any": [ false, { "eq": [1, 1] } ] }), true),
        (json!({ "all": [ true, { "eq": [1, 2] } ] }), false),
        (json!(true), true),
    ];
    for (condition, holds) in cases {
        let registry = register(with(
            ticket(),
            "operations.touch.preconditions",
            json!([{ "assert": condition }]),
        ))
        .unwrap();
        assert_verdict(&registry, &condition, Truth::from_bool(holds));
    }
}

#[test]
fn a_precondition_may_read_the_arguments_and_the_transition() {
    let registry = register(with(
        ticket(),
        "operations.start.preconditions",
        json!([{ "name": "named_assignee", "assert": { "all": [
            { "ne": ["$args.assignee", "nobody"] },
            { "eq": ["$from_state", "open"] },
            { "eq": ["$to_state", "in_progress"] },
            { "eq": ["$old_fields.title", "$fields.title"] }
        ]}}]),
    ))
    .unwrap();
    let runtime = Runtime::new(&registry);
    runtime
        .execute(
            &open_ticket(&registry, 3),
            "start",
            json!({ "assignee": "alice" }),
        )
        .expect("holds");
    let error = runtime
        .execute(
            &open_ticket(&registry, 3),
            "start",
            json!({ "assignee": "nobody" }),
        )
        .expect_err("refused");
    assert!(
        matches!(error, CoreError::PreconditionFailed { rule: Some(rule), .. } if rule == "named_assignee")
    );
}

// --- Templates -----------------------------------------------------------------------------------

#[test]
fn a_template_the_scope_cannot_resolve_is_refused_at_registration() {
    // `touch` declares no arguments, so `$args.actor` could never resolve, and `$now` is not a
    // reference at all. Both used to register and then fail on every single execution.
    for (path, template) in [
        (
            "operations.touch.emits",
            json!([{ "type": "T", "payload": { "who": "$args.actor" } }]),
        ),
        (
            "operations.touch.emits",
            json!([{ "type": "T", "payload": { "when": "$now" } }]),
        ),
        ("operations.touch.set", json!({ "title": "$args.nope" })),
    ] {
        let error = register(with(ticket(), path, template)).expect_err(path);
        assert!(
            matches!(error.first(), DefinitionError::InvalidTemplate { .. }),
            "{path}: {error}"
        );
    }

    // A creation event has no previous state and no arguments, and says so at registration.
    let error = register(with(
        ticket(),
        "create.emit.payload",
        json!({ "from": "$from_state" }),
    ))
    .expect_err("create has no previous state");
    assert!(
        matches!(error.first(), DefinitionError::InvalidTemplate { ref path, .. }
            if path == "create.emit.payload.from"),
        "{error}"
    );

    // What an operation template may legitimately read still registers.
    register(with(
        ticket(),
        "operations.touch.emits",
        json!([{ "type": "T", "payload": { "state": "$state", "was": "$from_state" } }]),
    ))
    .expect("an operation template reads the transition");
}

#[test]
fn an_unresolvable_template_reference_is_an_error_not_a_null() {
    // What registration cannot decide is left to run time: a path into a `json` field, whose
    // shape no schema describes.
    let registry = register(with(
        ticket(),
        "operations.touch.emits",
        json!([{ "type": "Touched", "payload": { "deep": "$fields.extra.missing" } }]),
    ))
    .unwrap();

    let error = Runtime::new(&registry)
        .execute(&open_ticket(&registry, 3), "touch", json!({}))
        .expect_err("nothing is there");
    assert!(
        matches!(error, CoreError::Template { ref expression, .. }
            if expression == "$fields.extra.missing"),
        "{error}"
    );
}

#[test]
fn templates_resolve_recursively_and_escape_a_literal_dollar() {
    let registry = register(with(
        ticket(),
        "operations.touch.emits",
        json!([{ "type": "Touched", "payload": {
            "nested": { "ids": ["$id", "$$id"], "state": { "now": "$to_state", "was": "$from_state" } },
            "whole": "$fields",
            "number": 7
        }}]),
    ))
    .unwrap();
    let instance = open_ticket(&registry, 3);
    let touched = Runtime::new(&registry)
        .execute(&instance, "touch", json!({}))
        .expect("touched");
    let payload = &touched.events[0].payload;
    assert_eq!(payload["nested"]["ids"], json!(["t-1", "$id"]));
    assert_eq!(
        payload["nested"]["state"],
        json!({ "now": "open", "was": "open" })
    );
    assert_eq!(
        payload["whole"],
        serde_json::to_value(&instance.fields).unwrap()
    );
    assert_eq!(payload["number"], json!(7));
}

// --- Determinism ---------------------------------------------------------------------------------

#[test]
fn the_same_inputs_produce_the_same_decision_byte_for_byte() {
    let run = || {
        let registry = register(ticket()).unwrap();
        let runtime = Runtime::new(&registry);
        let opened = runtime
            .create("ticket", 1, "t-1", json!({ "title": "Login fails", "points": 3, "tags": ["b", "a"], "meta": { "source": "mail" } }))
            .unwrap();
        let started = runtime
            .execute(&opened.instance, "start", json!({ "assignee": "alice" }))
            .unwrap();
        let closed = runtime
            .execute(&started.instance, "close", json!({ "resolution": "fixed" }))
            .unwrap();
        (opened, started, closed)
    };
    let first = run();
    let second = run();
    assert_eq!(first, second);
    assert_eq!(
        serde_json::to_string(&first.2).unwrap(),
        serde_json::to_string(&second.2).unwrap()
    );
    assert_eq!(first.2.events.len(), 1);
    assert_eq!(first.2.events[0].payload, json!({ "resolution": "fixed" }));
}

#[test]
fn a_refusal_leaves_the_caller_owned_instance_untouched() {
    let registry = register(ticket()).unwrap();
    let runtime = Runtime::new(&registry);
    let instance = open_ticket(&registry, 0);
    let before = instance.clone();
    for (operation, arguments) in [
        ("close", json!({ "resolution": "x" })), // precondition
        ("start", json!({})),                    // argument validation
        ("reopen", json!({})),                   // unknown operation
    ] {
        runtime
            .execute(&instance, operation, arguments)
            .expect_err(operation);
    }
    assert_eq!(instance, before);
}

// --- Refusals the review of 0.1.0 added --------------------------------------------------------

/// Parsing without the panic, for the documents that must not even deserialise.
fn parse(value: Value) -> Result<EntityDefinition, serde_json::Error> {
    serde_json::from_value(value)
}

#[test]
fn an_integer_beyond_i64_is_compared_numerically_not_wrapped() {
    // 2^64-1 used to be coerced `as u64 as i64` to -1, which passed `max: 100` and made a `min`
    // message name a number nobody sent.
    let huge = json!({ "title": "big", "points": 18_446_744_073_709_551_615_u64 });

    let bounded = register(with(
        ticket(),
        "schema.fields.points",
        json!({ "type": "integer", "max": 100 }),
    ))
    .unwrap();
    let error = Runtime::new(&bounded)
        .create("ticket", 1, "t-1", huge.clone())
        .expect_err("2^64-1 is not below 100");
    let CoreError::Validation(errors) = error else {
        panic!("expected a validation refusal")
    };
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].path, "fields.points");
    assert!(
        errors[0].message.contains("exceeds maximum"),
        "{}",
        errors[0]
    );

    let floored = register(with(
        ticket(),
        "schema.fields.points",
        json!({ "type": "integer", "min": 0 }),
    ))
    .unwrap();
    let created = Runtime::new(&floored)
        .create("ticket", 1, "t-1", huge)
        .expect("2^64-1 is above 0");
    assert_eq!(
        created.instance.fields["points"],
        json!(18_446_744_073_709_551_615_u64)
    );
}

#[test]
fn a_default_declared_inside_an_object_is_applied() {
    let document = with(
        ticket(),
        "schema.fields.origin",
        json!({
            "type": "object",
            "properties": { "country": { "type": "string", "required": true, "default": "DE" } }
        }),
    );
    let registry = register(document).unwrap();
    let runtime = Runtime::new(&registry);

    let created = runtime
        .create("ticket", 1, "t-1", json!({ "title": "x", "origin": {} }))
        .expect("the nested default fills the gap");
    assert_eq!(
        created.instance.fields["origin"],
        json!({ "country": "DE" })
    );

    // A default does not invent the object that would hold it.
    let created = runtime
        .create("ticket", 1, "t-2", json!({ "title": "x" }))
        .expect("no object, no nested default");
    assert!(!created.instance.fields.contains_key("origin"));
}

#[test]
fn a_precondition_may_not_read_state_and_an_invariant_may_not_read_the_transition() {
    // `$state` resolves to the state the operation is heading *for*, so a precondition reading it
    // looks like a guard on the current state and is the opposite of one.
    let error = register(with(
        ticket(),
        "operations.touch.preconditions",
        json!([{ "assert": { "eq": ["$state", "open"] } }]),
    ))
    .expect_err("a precondition cannot read $state");
    assert!(
        matches!(error.first(), DefinitionError::InvalidRule { ref message, .. }
            if message.contains("$state") && message.contains("$from_state")),
        "{error}"
    );

    for reference in ["$to_state", "$from_state", "$args", "$old_fields.title"] {
        let error = register(with(
            ticket(),
            "invariants",
            json!([{ "assert": { "exists": reference } }]),
        ))
        .expect_err(reference);
        assert!(
            matches!(error.first(), DefinitionError::InvalidRule { ref path, .. } if path == "invariants[0].assert.exists"),
            "{reference}: {error}"
        );
    }

    // What each scope may read still registers.
    register(with(
        ticket(),
        "operations.touch.preconditions",
        json!([{ "assert": { "all": [ { "eq": ["$from_state", "open"] }, { "eq": ["$to_state", "open"] } ] } }]),
    ))
    .expect("a precondition reads the transition");
    register(with(
        ticket(),
        "invariants",
        json!([{ "assert": { "eq": ["$state", "open"] } }]),
    ))
    .expect("an invariant reads the state it is judging");
}

#[test]
fn a_condition_carrying_two_operators_or_an_unknown_one_is_refused() {
    // Untagged deserialisation used to take the first variant that matched and drop the rest, so
    // an indentation slip enforced half a rule.
    let two = parse(with(
        ticket(),
        "invariants",
        json!([{ "assert": { "eq": [1, 1], "ne": [1, 1] } }]),
    ))
    .expect_err("two operators");
    assert!(two.to_string().contains("exactly one operator"), "{two}");

    let unknown = parse(with(
        ticket(),
        "invariants",
        json!([{ "assert": { "gte_": [1, 1] } }]),
    ))
    .expect_err("misspelled operator");
    assert!(
        unknown.to_string().contains("gte_") && unknown.to_string().contains("expected one of"),
        "{unknown}"
    );

    let shape = parse(with(
        ticket(),
        "invariants",
        json!([{ "assert": { "eq": [1] } }]),
    ))
    .expect_err("eq takes two operands");
    assert!(
        shape.to_string().contains("exactly two operands"),
        "{shape}"
    );
}

#[test]
fn a_misspelled_definition_key_is_refused_rather_than_ignored() {
    for document in [
        with(
            ticket(),
            "schema.fields.points",
            json!({ "type": "integer", "requried": true }),
        ),
        with(ticket(), "operations.touch.precondition", json!([])),
        with(ticket(), "lifecycle.state", json!(["open"])),
    ] {
        let error = parse(document).expect_err("an unknown key is a defect, not a comment");
        assert!(error.to_string().contains("unknown field"), "{error}");
    }
}

#[test]
fn equality_is_numeric_so_an_integer_equals_the_same_number_written_with_a_decimal_point() {
    let document = with(
        with(ticket(), "schema.fields.total", json!({ "type": "number" })),
        "operations.touch.preconditions",
        json!([{ "assert": { "eq": ["$fields.total", 100] } }]),
    );
    let registry = register(document).unwrap();
    let runtime = Runtime::new(&registry);

    for written in [json!(100), json!(100.0)] {
        let instance = runtime
            .create(
                "ticket",
                1,
                "t-1",
                json!({ "title": "x", "total": written }),
            )
            .expect("created")
            .instance;
        runtime
            .execute(&instance, "touch", json!({}))
            .unwrap_or_else(|error| panic!("{written} should equal 100: {error}"));
    }

    // `in` and `contains` agree with it.
    let document = with(
        with(ticket(), "schema.fields.total", json!({ "type": "number" })),
        "operations.touch.preconditions",
        json!([{ "assert": { "in": ["$fields.total", [100, 200]] } }]),
    );
    let registry = register(document).unwrap();
    let runtime = Runtime::new(&registry);
    let instance = runtime
        .create("ticket", 1, "t-1", json!({ "title": "x", "total": 100.0 }))
        .unwrap()
        .instance;
    runtime
        .execute(&instance, "touch", json!({}))
        .expect("100.0 is in [100, 200]");
}

#[test]
fn a_nested_reference_path_is_checked_against_the_schema() {
    for (reference, refused) in [
        ("$fields.meta.source", false),
        ("$fields.meta.sourc", true),
        ("$fields.title.length", true),
        ("$fields.extra.anything.at.all", false),
        ("$fields.nonexistent", true),
    ] {
        let outcome = register(with(
            ticket(),
            "invariants",
            json!([{ "assert": { "exists": reference } }]),
        ));
        assert_eq!(
            outcome.is_err(),
            refused,
            "{reference}: {:?}",
            outcome.err()
        );
    }
}

#[test]
fn an_instance_claiming_a_state_the_definition_does_not_declare_is_refused() {
    let registry = register(ticket()).unwrap();
    let mut instance = open_ticket(&registry, 3);
    instance.lifecycle_state = "limbo".into();

    let error = Runtime::new(&registry)
        .execute(&instance, "touch", json!({}))
        .expect_err("no such state");
    assert_eq!(
        error,
        CoreError::UnknownState {
            entity: "ticket".into(),
            state: "limbo".into()
        }
    );
}

#[test]
fn registering_over_an_existing_definition_is_refused_and_replace_is_how_to_mean_it() {
    let mut registry = Registry::new();
    registry.register(definition(ticket())).expect("first");

    let error = registry
        .register(definition(ticket()))
        .expect_err("the second would silently win");
    assert_eq!(
        error,
        DefinitionError::DuplicateDefinition {
            entity: "ticket".into(),
            version: 1
        }
    );

    registry
        .replace(definition(with(
            ticket(),
            "schema.additional_fields",
            json!(true),
        )))
        .expect("replace says it out loud");
    assert_eq!(registry.len(), 1);
    assert!(
        registry
            .get("ticket", 1)
            .expect("registered")
            .schema
            .additional_fields
    );
}

#[test]
fn a_constraint_that_does_not_apply_to_its_kind_is_refused() {
    for (field, constraint) in [
        (json!({ "type": "string", "values": ["a"] }), "values"),
        (
            json!({ "type": "integer", "min_length": 3 }),
            "min_length/max_length",
        ),
        (
            json!({ "type": "string", "items": { "type": "string" } }),
            "items",
        ),
        (
            json!({ "type": "string", "properties": { "a": { "type": "string" } } }),
            "properties/additional_properties",
        ),
        (json!({ "type": "boolean", "min": 1 }), "min/max"),
    ] {
        let error = register(with(ticket(), "schema.fields.probe", field)).expect_err(constraint);
        assert!(
            matches!(error.first(), DefinitionError::ConstraintNotApplicable { ref path, constraint: found, .. }
                if path == "schema.probe" && found == &constraint),
            "{constraint}: {error}"
        );
    }
}

#[test]
fn an_empty_identity_is_refused() {
    let registry = register(ticket()).unwrap();
    for id in ["", "   "] {
        let error = Runtime::new(&registry)
            .create("ticket", 1, id, json!({ "title": "x" }))
            .expect_err("an opaque id is still not an empty one");
        assert!(
            matches!(&error, CoreError::Validation(errors) if errors.len() == 1 && errors[0].path == "id"),
            "{error}"
        );
    }
}

#[test]
fn fields_are_ordered_by_name_so_two_identical_decisions_serialise_alike() {
    let registry = register(with(ticket(), "schema.additional_fields", json!(true))).unwrap();
    let created = Runtime::new(&registry)
        .create(
            "ticket",
            1,
            "t-1",
            json!({ "zebra": 1, "title": "x", "alpha": 2 }),
        )
        .expect("created");

    let names: Vec<&str> = created.instance.fields.keys().map(String::as_str).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(
        names, sorted,
        "field order is not the map's insertion order"
    );

    let serialised = serde_json::to_string(&created.instance.fields).expect("json");
    assert!(
        serialised.starts_with("{\"alpha\""),
        "serialisation must follow the same order: {serialised}"
    );
}
