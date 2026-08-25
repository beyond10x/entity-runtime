//! One test per requirement in `docs/requirements.md` that the kernel's behaviour can pin.
//!
//! Each test is named after the behaviour it protects, and the requirements register cites it by
//! name; `scripts/check-requirements.py` fails the gate when a cited test does not exist.

use entity_core::{
    execute, CoreError, DefinitionError, EntityDefinition, EntityInstance, Registry, Runtime,
};
use serde_json::{json, Value};

fn definition(value: Value) -> EntityDefinition {
    serde_json::from_value(value).expect("a well-formed definition document")
}

fn register(value: Value) -> Result<Registry, DefinitionError> {
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
        matches!(no_values, DefinitionError::InvalidField { ref path, .. } if path == "schema.priority"),
        "{no_values}"
    );

    let no_items = register(with(
        ticket(),
        "schema.fields.tags",
        json!({ "type": "array" }),
    ))
    .expect_err("refused");
    assert!(
        matches!(no_items, DefinitionError::InvalidField { ref path, .. } if path == "schema.tags"),
        "{no_items}"
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
        matches!(error, DefinitionError::InvalidField { ref path, ref message } if path == "schema.points" && message.contains("invalid default")),
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
        matches!(error, DefinitionError::InvalidRule { ref path, .. } if path.starts_with("operations.touch.preconditions[0].assert.exists")),
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
            matches!(error, DefinitionError::InvalidRule { ref path, .. } if path == "invariants[0].assert.exists"),
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
            matches!(error, DefinitionError::InvalidRule { ref message, .. } if message.contains(operator)),
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
    // without a resolution.
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

#[test]
fn a_missing_reference_makes_a_comparison_false_and_exists_is_the_presence_test() {
    let cases = [
        (json!({ "eq": ["$fields.resolution", "x"] }), false),
        (json!({ "ne": ["$fields.resolution", "x"] }), false),
        (json!({ "gt": ["$fields.resolution", 0] }), false),
        (json!({ "in": ["$fields.resolution", ["x"]] }), false),
        (json!({ "exists": "$fields.resolution" }), false),
        (json!({ "not": { "exists": "$fields.resolution" } }), true),
        (json!({ "exists": "$fields.title" }), true),
        (json!({ "exists": "$from_state" }), true),
    ];
    for (condition, holds) in cases {
        let registry = register(with(
            ticket(),
            "operations.touch.preconditions",
            json!([{ "assert": condition }]),
        ))
        .unwrap();
        let result =
            Runtime::new(&registry).execute(&open_ticket(&registry, 3), "touch", json!({}));
        assert_eq!(result.is_ok(), holds, "{condition}");
    }
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
        let result =
            Runtime::new(&registry).execute(&open_ticket(&registry, 3), "touch", json!({}));
        assert_eq!(result.is_ok(), holds, "{condition}");
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
fn an_unresolvable_template_reference_is_an_error_not_a_null() {
    let registry = register(with(
        ticket(),
        "operations.touch.emits",
        json!([{ "type": "Touched", "payload": { "who": "$args.actor" } }]),
    ))
    .unwrap();
    let error = Runtime::new(&registry)
        .execute(&open_ticket(&registry, 3), "touch", json!({}))
        .expect_err("no such argument");
    assert!(
        matches!(error, CoreError::Template { ref expression, .. } if expression == "$args.actor"),
        "{error}"
    );

    let registry = register(with(
        ticket(),
        "operations.touch.emits",
        json!([{ "type": "Touched", "payload": { "when": "$now" } }]),
    ))
    .unwrap();
    let error = Runtime::new(&registry)
        .execute(&open_ticket(&registry, 3), "touch", json!({}))
        .expect_err("no clock");
    assert!(
        matches!(error, CoreError::Template { ref expression, ref message } if expression == "$now" && message.contains("unknown")),
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
