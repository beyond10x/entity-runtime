//! Adversarial batch and malformed-action probes for declared refusal precedence.

use std::collections::BTreeMap;

use entity_core::{CoreError, OperationFieldAction, Registry};
use entity_executor::{
    test_support::block_on, BatchAction, CreateRequest, ExecuteRequest, ExecutionError, Executor,
};
use entity_store::{
    asynchronous::{AsyncRecordedReader, AsyncStateReader, BatchKey, MemoryRecordedStore, Subject},
    Recording,
};
use serde_json::json;

fn registry() -> Registry {
    let mut registry = Registry::new();
    registry
        .register(
            serde_json::from_value(json!({
                "entity": "invoice", "version": 1, "semantics": "service/3",
                "schema": {"fields": {"issued_at": {"type": "string", "required": true}}},
                "lifecycle": {"initial": "Draft", "states": ["Draft"]},
                "operations": {"Issue": {"outcomes": [
                    {"name": "rejected", "when": {"eq": ["$fields.issued_at", "pending"]},
                     "refuses": {"error": "NotAccepted", "message": "Invoice cannot be issued"}},
                    {"name": "issued", "effect": "updates",
                     "fulfills": {"issued_at": {"actions": "required"}}}
                ]}}
            }))
            .unwrap(),
        )
        .unwrap();
    registry
}

fn recording(id: &str) -> Recording {
    Recording {
        record_id: id.to_owned(),
        recorded_at: "2026-10-03T00:00:00Z".to_owned(),
        correlation: None,
        causation: None,
        actor: None,
    }
}

fn request(id: &str, revision: u64, value: serde_json::Value) -> ExecuteRequest {
    ExecuteRequest {
        subject: Subject::new("invoice", "i-1").unwrap(),
        expected_revision: revision,
        operation: "Issue".to_owned(),
        arguments: json!({}),
        fulfillments: BTreeMap::from([(
            "issued_at".to_owned(),
            OperationFieldAction::Set { value },
        )]),
        recording: recording(id),
    }
}

fn seed(executor: &Executor<'_>, value: &str) {
    block_on(executor.create(CreateRequest {
        subject: Subject::new("invoice", "i-1").unwrap(),
        definition_version: 1,
        fields: json!({"issued_at": value}),
        recording: recording("create"),
    }))
    .unwrap();
}

fn assert_declared(error: ExecutionError) {
    assert!(
        matches!(error, ExecutionError::Core(CoreError::Refused {
        ref outcome, ref error, ref message
    }) if outcome == "rejected" && error == "NotAccepted"
        && message.as_deref() == Some("Invoice cannot be issued")),
        "{error:?}"
    );
}

#[test]
fn later_refusal_rolls_back_earlier_fulfillment_and_leaves_batch_identity_reusable() {
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    seed(&executor, "ready");
    let subject = Subject::new("invoice", "i-1").unwrap();
    let before = block_on(store.load(&subject)).unwrap();
    let history = block_on(store.history(&subject)).unwrap();
    let key = BatchKey::Named("two-steps".to_owned());
    let mut refused = request("second", 2, json!([1, 2]));
    refused
        .fulfillments
        .insert("unknown".to_owned(), OperationFieldAction::Remove);
    let bad = vec![
        BatchAction::Execute(request("first", 1, json!("pending"))),
        BatchAction::Execute(refused),
    ];
    for _ in 0..2 {
        assert_declared(block_on(executor.batch(key.clone(), bad.clone())).unwrap_err());
        assert_eq!(block_on(store.load(&subject)).unwrap(), before);
        assert_eq!(block_on(store.history(&subject)).unwrap(), history);
        assert_eq!(block_on(store.lookup_record("first")).unwrap(), None);
        assert_eq!(block_on(store.lookup_record("second")).unwrap(), None);
        assert_eq!(block_on(store.lookup_batch(&key)).unwrap(), None);
    }
    let good = vec![
        BatchAction::Execute(request("first", 1, json!("ready-again"))),
        BatchAction::Execute(request("second", 2, json!("issued"))),
    ];
    let committed = block_on(executor.batch(key.clone(), good.clone())).unwrap();
    assert!(!committed.replayed());
    let state = block_on(store.load(&subject)).unwrap().unwrap();
    assert_eq!(state.revision, 3);
    assert_eq!(state.fields["issued_at"], json!("issued"));
    let replay = block_on(executor.batch(key, good)).unwrap();
    assert!(replay.replayed());
    assert_eq!(replay.receipt(), committed.receipt());
    assert_eq!(block_on(store.history(&subject)).unwrap().records.len(), 3);
}

#[test]
fn refusal_wins_over_required_field_removal_and_invalid_value_after_restart() {
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    seed(&executor, "pending");
    let subject = Subject::new("invoice", "i-1").unwrap();
    let before = block_on(store.load(&subject)).unwrap();
    for action in [
        OperationFieldAction::Remove,
        OperationFieldAction::Set { value: json!(null) },
        OperationFieldAction::Set {
            value: json!({"nested": true}),
        },
    ] {
        let restarted = Executor::new(&registry, &store);
        let mut refused = request("refused", 1, json!("unused"));
        refused.fulfillments.insert("issued_at".to_owned(), action);
        assert_declared(block_on(restarted.execute(refused)).unwrap_err());
        assert_eq!(block_on(store.load(&subject)).unwrap(), before);
        assert_eq!(block_on(store.lookup_record("refused")).unwrap(), None);
    }
    assert_eq!(block_on(store.history(&subject)).unwrap().records.len(), 1);
}
