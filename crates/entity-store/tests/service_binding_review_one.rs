//! Adversarial framing cases for service binding boundary source review one.

use entity_core::{EntityInstance, Registry, Runtime};
use entity_store::{
    asynchronous::{
        original_request_comparison_bytes, read_record_in_domain, request_domain, RecordedEntry,
    },
    RecordedCommit, Recording,
};
use serde_json::{json, Value};

fn recording(record_id: &str) -> Recording {
    Recording {
        record_id: record_id.to_owned(),
        recorded_at: "2026-09-16T00:00:00Z".to_owned(),
        correlation: None,
        causation: None,
        actor: None,
    }
}

fn branchless_registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "probe", "version": 1, "semantics": "service/2",
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "Held", "states": ["Held"] },
        "operations": { "Touch": {
            "arguments": { "fields": {
                "note": { "type": "string", "default": "default" }
            }},
            "transitions": [{ "from": "Held", "to": "Held" }]
        }}
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition registers");
    registry
}

fn recorded(decision: entity_core::Decision, record_id: &str) -> RecordedEntry {
    RecordedEntry::Decision(
        RecordedCommit::new(decision, &recording(record_id)).expect("record is valid"),
    )
}

#[test]
fn branchless_service_2_creation_reconstructs_the_exact_request_under_request_3() {
    let registry = branchless_registry();
    let runtime = Runtime::new(&registry);
    let one = recorded(
        runtime
            .create("probe", 1, "p-1", json!({"title": "one"}))
            .expect("creation succeeds"),
        "r-1",
    );
    let two = recorded(
        runtime
            .create("probe", 1, "p-1", json!({"title": "two"}))
            .expect("creation succeeds"),
        "r-1",
    );
    assert_eq!(request_domain(&one), "er.request/3");
    let first = original_request_comparison_bytes(&one).expect("request encodes");
    let second = original_request_comparison_bytes(&two).expect("request encodes");
    assert_ne!(first, second);
    let value = read_record_in_domain("er.request/3", &first).expect("current reader accepts /3");
    assert_eq!(value["arguments"], json!({"title": "one"}));
    assert!(value.get("fields").is_none());
}

#[test]
fn service_2_execute_reconstructs_request_3_with_normalized_arguments() {
    let registry = branchless_registry();
    let runtime = Runtime::new(&registry);
    let instance = EntityInstance {
        entity: "probe".to_owned(),
        version: 1,
        id: "p-1".to_owned(),
        lifecycle_state: "Held".to_owned(),
        revision: 1,
        fields: serde_json::from_value::<Value>(json!({"title": "one"}))
            .unwrap()
            .as_object()
            .unwrap()
            .clone(),
    };
    let decision = runtime
        .execute(&instance, "Touch", json!({}))
        .expect("operation succeeds");
    let entry = recorded(decision, "r-2");
    assert_eq!(request_domain(&entry), "er.request/3");
    let bytes = original_request_comparison_bytes(&entry).expect("request encodes");
    let value = read_record_in_domain("er.request/3", &bytes).expect("current reader accepts /3");
    assert_eq!(value["kind"], "execute");
    assert_eq!(value["expected_revision"], 1);
    assert_eq!(value["operation"], "Touch");
    assert_eq!(value["arguments"], json!({"note": "default"}));
}
