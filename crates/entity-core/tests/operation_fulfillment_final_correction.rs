//! Whole-class correction cases for removal evidence in legacy event histories.

use std::collections::BTreeSet;

use entity_core::{rehydrate, CoreError, Registry, Runtime};
use serde_json::{json, Value};

fn registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "ticket",
        "version": 1,
        "schema": {
            "fields": {
                "title": { "type": "string", "required": true },
                "note": { "type": "string" }
            }
        },
        "lifecycle": {
            "initial": "open",
            "states": ["open", "finished", "closed"]
        },
        "create": {
            "emit": {
                "type": "TicketOpened",
                "payload": { "ticket": "$id" }
            }
        },
        "operations": {
            "finish": {
                "transitions": [{ "from": "open", "to": "finished" }],
                "emits": [{
                    "type": "TicketFinished",
                    "payload": { "ticket": "$id" }
                }]
            },
            "close": {
                "transitions": [{ "from": "open", "to": "closed" }],
                "emits": [
                    {
                        "type": "TicketClosed",
                        "payload": { "ticket": "$id" }
                    },
                    {
                        "type": "TicketArchived",
                        "payload": { "ticket": "$id" }
                    },
                    {
                        "type": "TicketNotified",
                        "payload": { "ticket": "$id" }
                    }
                ]
            }
        }
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    registry
}

fn fields(note: Option<&str>) -> Value {
    let mut fields = serde_json::Map::from_iter([("title".to_owned(), json!("review"))]);
    if let Some(note) = note {
        fields.insert("note".to_owned(), json!(note));
    }
    Value::Object(fields)
}

fn assert_removal_evidence_refused(error: CoreError) {
    let CoreError::Validation(errors) = error else {
        panic!("removal evidence must be a validation refusal: {error}");
    };
    assert!(
        errors
            .iter()
            .any(|error| error.to_string().contains("removal evidence")),
        "the refusal must name the unsupported evidence: {errors:?}"
    );
}

#[test]
fn kernel_creation_refuses_present_absent_and_unknown_removal_coordinates() {
    for (label, note, removed) in [
        ("present", Some("keep"), "note"),
        ("absent", None, "note"),
        ("unknown", None, "not_declared"),
    ] {
        let registry = registry();
        let runtime = Runtime::new(&registry);
        let mut event = runtime
            .create("ticket", 1, label, fields(note))
            .expect("creation succeeds")
            .events
            .into_iter()
            .next()
            .expect("creation event");
        event.removed = BTreeSet::from([removed.to_owned()]);

        let definition = registry.get("ticket", 1).expect("registered");
        let error = rehydrate(definition, &[event])
            .expect_err("kernel creation cannot author removal evidence");
        assert_removal_evidence_refused(error);
    }
}

#[test]
fn kernel_operation_refuses_present_absent_and_unknown_removal_coordinates() {
    for (label, note, removed) in [
        ("present", Some("keep"), "note"),
        ("absent", None, "note"),
        ("unknown", None, "not_declared"),
    ] {
        let registry = registry();
        let runtime = Runtime::new(&registry);
        let created = runtime
            .create("ticket", 1, label, fields(note))
            .expect("creation succeeds");
        let mut finished = runtime
            .execute(&created.instance, "finish", json!({}))
            .expect("operation succeeds");
        finished.events[0].removed = BTreeSet::from([removed.to_owned()]);
        let history = [created.events[0].clone(), finished.events[0].clone()];

        let definition = registry.get("ticket", 1).expect("registered");
        let error = rehydrate(definition, &history)
            .expect_err("kernel operation cannot author removal evidence");
        assert_removal_evidence_refused(error);
    }
}

#[test]
fn every_member_of_a_multi_event_kernel_revision_refuses_removal_evidence() {
    for forged_index in 0..3 {
        let registry = registry();
        let runtime = Runtime::new(&registry);
        let created = runtime
            .create(
                "ticket",
                1,
                format!("member-{forged_index}"),
                fields(Some("keep")),
            )
            .expect("creation succeeds");
        let mut closed = runtime
            .execute(&created.instance, "close", json!({}))
            .expect("operation succeeds");
        closed.events[forged_index].removed = BTreeSet::from(["note".to_owned()]);
        let mut history = created.events.clone();
        history.extend(closed.events);

        let definition = registry.get("ticket", 1).expect("registered");
        let error = rehydrate(definition, &history)
            .expect_err("each event is checked for impossible removal evidence");
        assert_removal_evidence_refused(error);
    }

    let registry = registry();
    let runtime = Runtime::new(&registry);
    let created = runtime
        .create("ticket", 1, "all-members", fields(Some("keep")))
        .expect("creation succeeds");
    let mut closed = runtime
        .execute(&created.instance, "close", json!({}))
        .expect("operation succeeds");
    for event in &mut closed.events {
        event.removed = BTreeSet::from(["note".to_owned()]);
    }
    let mut history = created.events.clone();
    history.extend(closed.events);

    let definition = registry.get("ticket", 1).expect("registered");
    let error =
        rehydrate(definition, &history).expect_err("matching forged members remain unsupported");
    assert_removal_evidence_refused(error);
}

#[test]
fn honest_empty_removal_kernel_creation_and_multi_event_history_still_fold() {
    let registry = registry();
    let runtime = Runtime::new(&registry);
    let created = runtime
        .create("ticket", 1, "honest", fields(Some("keep")))
        .expect("creation succeeds");
    let closed = runtime
        .execute(&created.instance, "close", json!({}))
        .expect("operation succeeds");
    assert!(created
        .events
        .iter()
        .chain(&closed.events)
        .all(|event| event.removed.is_empty()));
    let mut history = created.events.clone();
    history.extend(closed.events);

    let definition = registry.get("ticket", 1).expect("registered");
    assert_eq!(
        rehydrate(definition, &history).expect("honest legacy history folds"),
        closed.instance
    );
}
