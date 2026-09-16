//! `service/3` retries bind the exact fulfillment actions under `er.request/4`.

use std::collections::BTreeMap;

use entity_core::{OperationFieldAction, Registry};
use entity_executor::{test_support::block_on, CreateRequest, ExecuteRequest, Executor};
use entity_store::{
    asynchronous::{
        verify_subject_history, AsyncRecordedReader, AsyncStateReader, AsyncStoreError,
        MemoryRecordedStore, Subject,
    },
    Recording,
};
use serde_json::json;

fn registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "invoice", "version": 1, "semantics": "service/3",
        "schema": { "fields": {
            "issued_at": { "type": "string", "required": true },
            "note": { "type": "string" }
        }},
        "lifecycle": { "initial": "Draft", "states": ["Draft"] },
        "operations": { "Issue": {
            "outcomes": [{
                "name": "issued", "effect": "updates",
                "fulfills": {
                    "issued_at": { "actions": "required" },
                    "note": { "actions": "optional" }
                }
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
        recorded_at: "2026-09-16T10:00:00Z".to_owned(),
        correlation: None,
        causation: None,
        actor: None,
    }
}

fn fulfill(at: &str) -> BTreeMap<String, OperationFieldAction> {
    BTreeMap::from([
        (
            "issued_at".to_owned(),
            OperationFieldAction::Set { value: json!(at) },
        ),
        ("note".to_owned(), OperationFieldAction::Remove),
    ])
}

fn execute(actions: BTreeMap<String, OperationFieldAction>) -> ExecuteRequest {
    ExecuteRequest {
        subject: Subject::new("invoice", "i-1").unwrap(),
        expected_revision: 1,
        operation: "Issue".to_owned(),
        arguments: json!({}),
        fulfillments: actions,
        recording: recording("r-2"),
    }
}

#[test]
fn retry_restart_and_verified_history_preserve_set_and_removal_actions() {
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    block_on(executor.create(CreateRequest {
        subject: Subject::new("invoice", "i-1").unwrap(),
        definition_version: 1,
        fields: json!({"issued_at": "pending", "note": "remove"}),
        recording: recording("r-1"),
    }))
    .expect("creation commits");

    let actions = fulfill("2026-09-16T10:00:00Z");
    block_on(executor.execute(execute(actions.clone()))).expect("fulfillment commits");
    let restarted = Executor::new(&registry, &store);
    assert!(block_on(restarted.execute(execute(actions)))
        .expect("identical retry replays")
        .replayed());

    let subject = Subject::new("invoice", "i-1").unwrap();
    let terminal = block_on(store.load(&subject)).unwrap().unwrap();
    assert_eq!(terminal.fields["issued_at"], json!("2026-09-16T10:00:00Z"));
    assert!(!terminal.fields.contains_key("note"));
    let history = block_on(store.history(&subject)).unwrap();
    verify_subject_history(&history, &terminal).expect("complete history replays and verifies");

    let conflict = block_on(restarted.execute(execute(fulfill("different"))))
        .expect_err("changed action conflicts with the canonical request");
    assert!(matches!(
        conflict.store_error(),
        Some(AsyncStoreError::RecordConflict { record_id }) if record_id == "r-2"
    ));
}
