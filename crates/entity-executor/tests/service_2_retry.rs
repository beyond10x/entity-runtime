//! Retry comparison preserves `service/2` optional argument presence under `er.request/3`.

use entity_core::Registry;
use entity_executor::{test_support::block_on, CreateRequest, Executor};
use entity_store::{
    asynchronous::{AsyncStoreError, MemoryRecordedStore, Subject},
    Recording,
};
use serde_json::json;

fn registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "probe", "version": 1, "semantics": "service/2",
        "schema": { "fields": {
            "fixed": { "type": "string", "required": true },
            "note": { "type": "string" }
        }},
        "lifecycle": { "initial": "Held", "states": ["Held"] },
        "create": {
            "arguments": { "fields": {
                "fixed": { "type": "string", "required": true },
                "bound": { "type": "object", "required": true,
                    "properties": { "note": { "type": "string" } } }
            }},
            "outcomes": [{
                "name": "accepted", "effect": "creates",
                "set": { "fixed": "$args.fixed" },
                "set_if_present": { "note": { "argument": "bound.note" } }
            }]
        }
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    registry
}

fn request(input: serde_json::Value) -> CreateRequest {
    CreateRequest {
        subject: Subject::new("probe", "p-1").expect("subject"),
        definition_version: 1,
        fields: input,
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
fn service_2_retry_distinguishes_absent_and_present_optional_arguments() {
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    let absent = json!({"fixed": "fixed", "bound": {}});
    block_on(executor.create(request(absent.clone()))).expect("first request commits");
    assert!(block_on(executor.create(request(absent)))
        .unwrap()
        .replayed());

    let conflict = block_on(executor.create(request(json!({
        "fixed": "fixed", "bound": {"note": "present"}
    }))))
    .expect_err("presence changes the normalized original request");
    assert!(matches!(
        conflict.store_error(),
        Some(AsyncStoreError::RecordConflict { record_id }) if record_id == "r-1"
    ));
}
