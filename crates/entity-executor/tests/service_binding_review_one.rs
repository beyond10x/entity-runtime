//! Adversarial retry case for service binding boundary source review one.

use entity_core::Registry;
use entity_executor::{test_support::block_on, CreateRequest, Executor};
use entity_store::{asynchronous::MemoryRecordedStore, Recording};
use serde_json::json;

fn request(fields: serde_json::Value) -> CreateRequest {
    CreateRequest {
        subject: entity_store::asynchronous::Subject::new("probe", "p-1").expect("subject"),
        definition_version: 1,
        fields,
        recording: Recording {
            record_id: "r-1".to_owned(),
            recorded_at: "2026-09-16T00:00:00Z".to_owned(),
            correlation: None,
            causation: None,
            actor: None,
        },
    }
}

#[test]
fn service_2_retry_replays_the_same_present_value_after_canonical_normalization() {
    let definition = serde_json::from_value(json!({
        "entity": "probe", "version": 1, "semantics": "service/2",
        "schema": { "fields": {
            "fixed": { "type": "string", "required": true },
            "metadata": { "type": "object", "properties": {},
                          "additional_properties": true }
        }},
        "lifecycle": { "initial": "Held", "states": ["Held"] },
        "create": {
            "arguments": { "fields": {
                "fixed": { "type": "string", "required": true },
                "bound": { "type": "object", "required": true, "properties": {
                    "metadata": { "type": "object", "properties": {},
                                  "additional_properties": true }
                }}
            }},
            "outcomes": [{
                "name": "accepted", "effect": "creates",
                "set": { "fixed": "$args.fixed" },
                "set_if_present": { "metadata": { "argument": "bound.metadata" } }
            }]
        }
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition registers");
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);

    block_on(executor.create(request(json!({
        "fixed": "fixed", "bound": {"metadata": {"b": 2, "a": 1}}
    }))))
    .expect("first request commits");
    let retry = block_on(executor.create(request(json!({
        "bound": {"metadata": {"a": 1, "b": 2}}, "fixed": "fixed"
    }))))
    .expect("the same normalized request replays");
    assert!(retry.replayed());
}
