//! Creation whose selected branch determines the logical identity and therefore its address.

use entity_core::{
    create_derived, decide_create, decide_create_derived, replay, CoreError, EntityDefinition,
    Evaluation, Runtime, ValidatedDefinition,
};
use serde_json::{json, Value};

fn validated(value: Value) -> ValidatedDefinition {
    let definition: EntityDefinition =
        serde_json::from_value(value).expect("fixture is a definition document");
    ValidatedDefinition::new(definition).expect("fixture registers")
}

fn branch_identity() -> ValidatedDefinition {
    validated(json!({
        "entity": "branch_keyed",
        "version": 1,
        "semantics": "service/1",
        "identity": { "field": "key" },
        "schema": { "fields": {
            "key": { "type": "string", "required": true },
            "note": { "type": "string", "required": true }
        }},
        "lifecycle": { "initial": "held", "states": ["held"] },
        "create": {
            "arguments": { "fields": {
                "choice": { "type": "string", "required": true },
                "first_key": { "type": "string", "required": true },
                "second_key": { "type": "string", "required": true },
                "note": { "type": "string", "required": true }
            }},
            "outcomes": [
                {
                    "name": "first",
                    "when": { "eq": ["$args.choice", "first"] },
                    "effect": "creates",
                    "set": { "key": "$args.first_key", "note": "$args.note" },
                    "emits": [
                        { "type": "Marker", "payload": {} },
                        { "type": "Created", "payload": { "key": "$fields.key" } }
                    ]
                },
                {
                    "name": "second",
                    "effect": "creates",
                    "set": { "key": "$args.second_key", "note": "$args.note" },
                    "emits": [
                        { "type": "Created", "payload": { "key": "$fields.key" } }
                    ]
                }
            ]
        }
    }))
}

fn input(choice: &str) -> Value {
    json!({
        "choice": choice,
        "first_key": "key-a",
        "second_key": "key-b",
        "note": "preserve"
    })
}

#[test]
fn each_selected_creation_branch_derives_and_publishes_its_own_identity() {
    let definition = branch_identity();
    let first = create_derived(&definition, input("first")).expect("first branch creates");
    assert_eq!(first.instance.id, "s:key-a");
    assert_eq!(first.instance.fields["key"], json!("key-a"));
    assert_eq!(first.events[1].payload["key"], json!("key-a"));

    let second = create_derived(&definition, input("second")).expect("second branch creates");
    assert_eq!(second.instance.id, "s:key-b");
    assert_eq!(second.instance.fields["key"], json!("key-b"));
    assert_eq!(second.events[0].payload["key"], json!("key-b"));
}

#[test]
fn derived_and_supplied_creation_are_one_decision_and_replay_contract() {
    let definition = branch_identity();
    let derived = create_derived(&definition, input("second")).expect("derives");
    let supplied = decide_create(&definition, derived.instance.id.clone(), input("second"))
        .expect("supplied path decides")
        .into_decision()
        .expect("accepts");
    assert_eq!(derived, supplied);
    assert_eq!(
        serde_json::to_vec(&derived.record).expect("record serializes"),
        serde_json::to_vec(&supplied.record).expect("record serializes")
    );
    assert_eq!(
        replay(std::slice::from_ref(&derived.record)).expect("record replays"),
        derived.instance
    );
}

#[test]
fn runtime_exposes_the_same_derived_capability_without_a_candidate_address() {
    let definition = branch_identity();
    let mut registry = entity_core::Registry::new();
    registry
        .register(definition.as_definition().clone())
        .expect("registers");
    let runtime = Runtime::new(&registry);
    let evaluation = runtime
        .decide_create_derived("branch_keyed", 1, input("second"))
        .expect("decides");
    let Evaluation::Accepted(decision) = evaluation else {
        panic!("second branch accepts");
    };
    assert_eq!(decision.instance.id, "s:key-b");
}

#[test]
fn pre_address_id_in_a_selector_or_selected_assignment_is_explicitly_circular() {
    let selector = validated(json!({
        "entity": "selector_id",
        "version": 1,
        "semantics": "service/1",
        "identity": { "field": "key" },
        "schema": { "fields": { "key": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "held", "states": ["held"] },
        "create": {
            "arguments": { "fields": { "key": { "type": "string", "required": true } } },
            "outcomes": [
                {
                    "name": "matched",
                    "when": { "eq": ["$id", "s:chosen"] },
                    "effect": "creates",
                    "set": { "key": "$args.key" }
                },
                {
                    "name": "fallback",
                    "effect": "creates",
                    "set": { "key": "$args.key" }
                }
            ]
        }
    }));
    let circular = decide_create_derived(&selector, json!({ "key": "chosen" }))
        .expect_err("derived selection has no pre-address id");
    assert_eq!(circular.kind(), "template");
    assert!(circular.to_string().contains("circular"));
    assert!(
        decide_create(&selector, "s:chosen".to_owned(), json!({ "key": "chosen" })).is_ok(),
        "the supplied-address API retains its existing selector scope"
    );

    let assignment = validated(json!({
        "entity": "assignment_id",
        "version": 1,
        "semantics": "service/1",
        "identity": { "field": "key" },
        "schema": { "fields": { "key": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "held", "states": ["held"] },
        "create": {
            "outcomes": [{
                "name": "created",
                "effect": "creates",
                "set": { "key": "$id" }
            }]
        }
    }));
    let circular = decide_create_derived(&assignment, json!({}))
        .expect_err("derived assignment has no pre-address id");
    assert_eq!(circular.kind(), "template");
    assert!(circular.to_string().contains("circular"));
    assert!(matches!(
        decide_create(&assignment, "s:key".to_owned(), json!({})),
        Err(CoreError::IdentityMismatch { .. })
    ));
}

#[test]
fn derivation_requires_a_declared_logical_identity() {
    let definition = validated(json!({
        "entity": "unkeyed",
        "version": 1,
        "schema": { "fields": { "value": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "held", "states": ["held"] }
    }));
    assert_eq!(
        decide_create_derived(&definition, json!({ "value": "x" }))
            .expect_err("there is no address source"),
        CoreError::CreationIdentityUnavailable {
            entity: "unkeyed".to_owned(),
            detail: "the validated definition declares no logical identity field".to_owned()
        }
    );
}
