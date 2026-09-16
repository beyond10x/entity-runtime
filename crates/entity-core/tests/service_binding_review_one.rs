//! Adversarial cases for service binding boundary source review one.

use entity_core::{
    decide, decide_before_load, CoreError, EntityDefinition, EntityInstance, Evaluation,
    PreloadDecision, Registry, Runtime, ValidatedDefinition,
};
use serde_json::{json, Value};

fn validated(value: Value) -> ValidatedDefinition {
    let definition: EntityDefinition = serde_json::from_value(value).expect("definition parses");
    ValidatedDefinition::new(definition).expect("definition registers")
}

fn held(entity: &str, id: &str) -> EntityInstance {
    EntityInstance {
        entity: entity.to_owned(),
        version: 1,
        id: id.to_owned(),
        lifecycle_state: "Held".to_owned(),
        revision: 1,
        fields: serde_json::Map::new(),
    }
}

#[test]
fn service_2_operation_conditional_outputs_preserve_absent_present_and_null() {
    let definition = validated(json!({
        "entity": "probe", "version": 1, "semantics": "service/2",
        "schema": { "fields": {} },
        "lifecycle": { "initial": "Held", "states": ["Held"] },
        "operations": { "Echo": {
            "arguments": { "fields": {
                "bound": { "type": "object", "required": true, "properties": {
                    "value": { "type": "json" }
                }}
            }},
            "response": { "fields": { "value": { "type": "json" } } },
            "outcomes": [{
                "name": "accepted",
                "emits": [{
                    "type": "Echoed", "payload": {},
                    "payload_if_present": { "value": { "argument": "bound.value" } }
                }],
                "responds_if_present": { "value": { "argument": "bound.value" } }
            }]
        }}
    }));

    for (input, expected) in [
        (json!({"bound": {}}), None),
        (json!({"bound": {"value": null}}), Some(Value::Null)),
        (
            json!({"bound": {"value": {"b": 2, "a": 1}}}),
            Some(json!({"a": 1, "b": 2})),
        ),
    ] {
        let direct = decide(&definition, &held("probe", "p-1"), "Echo", input.clone())
            .expect("operation decides");
        let PreloadDecision::Load(prepared) =
            decide_before_load(&definition, "p-1", "Echo", input).expect("prepares")
        else {
            panic!("an accepting operation requires its subject")
        };
        let continued = prepared
            .continue_with(&held("probe", "p-1"))
            .expect("continuation decides");
        assert_eq!(continued, direct);
        let Evaluation::Accepted(decision) = continued else {
            panic!("the branch accepts")
        };
        let payload = decision.events[0]
            .payload
            .as_object()
            .expect("object payload");
        let response = decision
            .record
            .response
            .as_ref()
            .expect("declared response");
        match expected {
            None => {
                assert!(!payload.contains_key("value"));
                assert!(!response.contains_key("value"));
            }
            Some(value) => {
                assert_eq!(payload.get("value"), Some(&value));
                assert_eq!(response.get("value"), Some(&value));
            }
        }
    }
}

#[test]
fn preparation_keeps_registry_identity_operation_and_argument_error_precedence() {
    let document = json!({
        "entity": "probe", "version": 1, "semantics": "service/1",
        "schema": { "fields": {} },
        "lifecycle": { "initial": "Held", "states": ["Held"] },
        "operations": { "Check": {
            "arguments": { "fields": { "required": { "type": "string", "required": true } } },
            "outcomes": [{ "name": "refused", "refuses": { "error": "No" } }]
        }}
    });
    let definition = validated(document.clone());

    assert!(matches!(
        decide_before_load(&definition, " ", "Missing", json!({})),
        Err(CoreError::Validation(errors)) if errors[0].path == "id"
    ));
    assert!(matches!(
        decide_before_load(&definition, "p-1", "Missing", json!({})),
        Err(CoreError::OperationNotFound { operation }) if operation == "Missing"
    ));
    assert!(matches!(
        decide_before_load(&definition, "p-1", "Check", json!({})),
        Err(CoreError::Validation(errors)) if errors.iter().any(|error| error.path == "arguments.required")
    ));

    let mut registry = Registry::new();
    registry
        .register(serde_json::from_value(document).expect("definition parses"))
        .expect("definition registers");
    let runtime = Runtime::new(&registry);
    assert!(matches!(
        runtime.decide_before_load("missing", 1, " ", "Missing", json!({})),
        Err(CoreError::EntityNotRegistered { entity, version }) if entity == "missing" && version == 1
    ));
}

#[test]
fn outer_kleene_domination_settles_a_quantifier_whose_collection_needs_subject() {
    let definition = |guard: Value| {
        validated(json!({
            "entity": "probe", "version": 1, "semantics": "service/1",
            "schema": { "fields": {
                "items": { "type": "array", "items": { "type": "integer" } }
            }},
            "lifecycle": { "initial": "Held", "states": ["Held"] },
            "operations": { "Check": { "outcomes": [
                { "name": "first", "when": guard, "refuses": { "error": "First" } },
                { "name": "later", "refuses": { "error": "Later" } }
            ]}}
        }))
    };
    let subject_quantifier = json!({
        "for_all": { "in": "$fields.items", "as": "item", "that": { "gt": ["$item", 0] } }
    });

    for guard in [
        json!({"any": [true, subject_quantifier.clone()]}),
        json!({"any": [subject_quantifier.clone(), true]}),
    ] {
        let PreloadDecision::Refused(refusal) =
            decide_before_load(&definition(guard), "p-1", "Check", json!({})).expect("decides")
        else {
            panic!("known true dominates a subject-dependent quantifier")
        };
        assert_eq!(refusal.outcome, "first");
    }
    for guard in [
        json!({"all": [false, subject_quantifier.clone()]}),
        json!({"all": [subject_quantifier.clone(), false]}),
    ] {
        let PreloadDecision::Refused(refusal) =
            decide_before_load(&definition(guard), "p-1", "Check", json!({})).expect("decides")
        else {
            panic!("known false skips the first branch without a subject load")
        };
        assert_eq!(refusal.outcome, "later");
    }
}
