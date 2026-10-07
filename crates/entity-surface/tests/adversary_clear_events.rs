//! Adversarial cases against R-165 from the projection side: the event contracts this crate
//! publishes for a definition must admit the events that definition's decisions emit.
//!
//! A `{cleared: true}` assignment makes a `kernel/1`, `service/1` or `service/2` event carry a
//! `removed` key for the first time. Both projections close the event object
//! (`additionalProperties: false`) and declare no `removed`, so the clear's own event is refused
//! by the schema it is published under — the property `overlapping_emitters_accept_real_events_in_
//! the_projected_schema` holds for every other event.

use std::collections::BTreeMap;

use entity_core::{
    EntityDefinition, LoadedDecision, OperationFieldAction, PreloadDecision, Registry, Runtime,
};
use entity_surface::{asyncapi, openapi};
use serde_json::{json, Value};

/// A `kernel/1` reminder whose `wake` clears `snoozed_until` and emits `Woken`.
fn reminder() -> EntityDefinition {
    serde_json::from_value(json!({
        "entity": "reminder",
        "version": 1,
        "schema": { "fields": {
            "title": { "type": "string", "required": true },
            "snoozed_until": { "type": "string" }
        }},
        "lifecycle": { "initial": "active", "states": ["active"] },
        "operations": { "wake": {
            "transitions": [{ "from": "active", "to": "active" }],
            "set": { "snoozed_until": { "cleared": true } },
            "emits": [{ "type": "Woken", "payload": { "was": "$old_fields.snoozed_until" } }]
        }}
    }))
    .expect("the fixture is a definition document")
}

/// The `Woken` event a real `wake` emits.
fn woken_event(definition: &EntityDefinition) -> Value {
    let mut registry = Registry::new();
    registry.register(definition.clone()).expect("registers");
    let runtime = Runtime::new(&registry);
    let created = runtime
        .create(
            "reminder",
            1,
            "r-1",
            json!({ "title": "call back", "snoozed_until": "2026-10-08T09:00:00Z" }),
        )
        .expect("created");
    let woken = runtime
        .execute(&created.instance, "wake", json!({}))
        .expect("cleared");
    let event = serde_json::to_value(&woken.events[0]).expect("an event serializes");
    assert_eq!(event["removed"], json!(["snoozed_until"]), "{event}");
    event
}

#[test]
fn a_kernel_1_clear_event_is_admitted_by_the_asyncapi_message_it_is_published_under() {
    let definition = reminder();
    let event = woken_event(&definition);
    let spec = asyncapi(std::slice::from_ref(&definition));
    let schema = &spec["components"]["messages"]["reminder_Woken"]["payload"];
    assert!(schema.is_object(), "the message is published: {spec}");
    assert!(
        jsonschema::is_valid(schema, &event),
        "the published Woken message refuses the Woken event `wake` emits: event {event}, \
         schema {schema}"
    );
}

#[test]
fn a_kernel_1_clear_event_is_admitted_by_the_openapi_domain_event_schema() {
    let definition = reminder();
    let event = woken_event(&definition);
    let api = openapi(std::slice::from_ref(&definition));
    // `GET …/{id}/events` answers an array of this schema.
    let schema = &api["components"]["schemas"]["DomainEvent"];
    assert!(
        jsonschema::is_valid(schema, &event),
        "the published DomainEvent refuses the event `wake` emits: event {event}, \
         schema {schema}"
    );
}

/// Control for the origin of the same omission: a `service/3` fulfillment `Remove` already put
/// `removed` on an event before R-165, so if this is red at the base too, the schema gap is older
/// than the clear and the clear only widens it to `kernel/1`.
#[test]
fn a_service_3_remove_event_is_admitted_by_the_openapi_domain_event_schema() {
    let definition: EntityDefinition = serde_json::from_value(json!({
        "entity": "invoice", "version": 1, "semantics": "service/3",
        "schema": { "fields": {
            "issued_at": { "type": "string", "required": true },
            "note": { "type": "string" }
        }},
        "lifecycle": { "initial": "Draft", "states": ["Draft"] },
        "operations": { "Issue": { "outcomes": [{
            "name": "issued", "effect": "updates",
            "fulfills": {
                "issued_at": { "actions": "required" },
                "note": { "actions": "optional" }
            },
            "emits": [{ "type": "Issued", "payload": { "at": "$fields.issued_at" } }]
        }]}}
    }))
    .expect("the fixture is a definition document");
    let mut registry = Registry::new();
    registry.register(definition.clone()).expect("registers");
    let runtime = Runtime::new(&registry);
    let created = runtime
        .create(
            "invoice",
            1,
            "i-1",
            json!({ "issued_at": "pending", "note": "remove" }),
        )
        .expect("created");
    let PreloadDecision::Load(prepared) = runtime
        .decide_before_load("invoice", 1, "i-1", "Issue", json!({}))
        .expect("prepared")
    else {
        panic!("the subject is required")
    };
    let LoadedDecision::NeedsFulfillment(prepared) =
        prepared.select_with(&created.instance).expect("selected")
    else {
        panic!("the outcome needs fulfillment")
    };
    let decision = prepared
        .complete(BTreeMap::from([
            (
                "issued_at".to_owned(),
                OperationFieldAction::Set {
                    value: json!("2026-09-16T10:00:00Z"),
                },
            ),
            ("note".to_owned(), OperationFieldAction::Remove),
        ]))
        .expect("completed")
        .into_decision()
        .expect("accepted");
    let event = serde_json::to_value(&decision.events[0]).expect("an event serializes");
    assert_eq!(event["removed"], json!(["note"]), "{event}");
    let api = openapi(std::slice::from_ref(&definition));
    let schema = &api["components"]["schemas"]["DomainEvent"];
    assert!(
        jsonschema::is_valid(schema, &event),
        "the published DomainEvent refuses the event `Issue` emits: event {event}, \
         schema {schema}"
    );
}
