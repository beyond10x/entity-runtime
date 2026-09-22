//! Pass-one regression cases for operation-field fulfillment.

use std::collections::BTreeMap;

use entity_core::{OperationFieldAction, Registry};
use entity_executor::{test_support::block_on, CreateRequest, ExecuteRequest, Executor};
use entity_store::{
    asynchronous::{AsyncStoreError, MemoryRecordedStore, Subject},
    Recording,
};
use serde_json::json;

fn registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "ticket", "version": 1, "semantics": "service/1",
        "schema": { "fields": {
            "title": { "type": "string", "required": true }
        }},
        "lifecycle": { "initial": "Open", "states": ["Open"] },
        "operations": { "Rename": {
            "arguments": { "fields": {
                "title": { "type": "string", "required": true }
            }},
            "outcomes": [{
                "name": "renamed", "effect": "updates",
                "set": { "title": "$args.title" }
            }]
        }}
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    registry
}

fn recording(record_id: &str) -> Recording {
    Recording {
        record_id: record_id.to_owned(),
        recorded_at: "2026-09-16T12:00:00Z".to_owned(),
        correlation: None,
        causation: None,
        actor: None,
    }
}

#[test]
fn an_older_service_retry_does_not_ignore_new_fulfillment_coordinates() {
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    block_on(executor.create(CreateRequest {
        subject: Subject::new("ticket", "t-1").unwrap(),
        definition_version: 1,
        fields: json!({"title": "old"}),
        recording: recording("r-1"),
    }))
    .expect("creation commits");

    let request = ExecuteRequest {
        subject: Subject::new("ticket", "t-1").unwrap(),
        expected_revision: 1,
        operation: "Rename".to_owned(),
        arguments: json!({"title": "new"}),
        fulfillments: BTreeMap::new(),
        recording: recording("r-2"),
    };
    block_on(executor.execute(request.clone())).expect("ordinary service/1 request commits");

    let mut changed = request;
    changed.fulfillments.insert(
        "undeclared".to_owned(),
        OperationFieldAction::Set {
            value: json!("ignored"),
        },
    );
    let error =
        block_on(executor.execute(changed)).expect_err("a changed request is not an exact retry");
    assert!(matches!(
        error.store_error(),
        Some(AsyncStoreError::RecordConflict { record_id }) if record_id == "r-2"
    ));
}
