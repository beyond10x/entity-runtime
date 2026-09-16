//! `service/3` retries bind the exact fulfillment actions under `er.request/4`.

use std::collections::BTreeMap;

use entity_core::{OperationFieldAction, Registry};
use entity_executor::{
    test_support::block_on, BatchAction, CreateRequest, ExecuteRequest, Executor,
};
use entity_store::{
    asynchronous::{
        request_domain, verify_subject_history, AsyncRecordedReader, AsyncStateReader,
        AsyncStoreError, BatchKey, CommitReceipt, MemoryRecordedStore, Subject,
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

fn legacy_registry() -> Registry {
    let mut registry = Registry::new();
    for (entity, semantics) in [
        ("kernel_ticket", None),
        ("service_1_ticket", Some("service/1")),
        ("service_2_ticket", Some("service/2")),
    ] {
        let mut definition = json!({
            "entity": entity, "version": 1,
            "schema": { "fields": {
                "title": { "type": "string", "required": true }
            }},
            "lifecycle": { "initial": "Open", "states": ["Open"] },
            "operations": { "Rename": {
                "arguments": { "fields": {
                    "title": { "type": "string", "required": true }
                }},
                "transitions": [{ "from": "Open", "to": "Open" }],
                "set": { "title": "$args.title" }
            }}
        });
        if let Some(semantics) = semantics {
            definition["semantics"] = json!(semantics);
        }
        registry
            .register(serde_json::from_value(definition).expect("definition parses"))
            .expect("definition validates");
    }
    registry
}

fn legacy_execute(entity: &str, record_id: &str) -> ExecuteRequest {
    ExecuteRequest {
        subject: Subject::new(entity, "t-1").expect("subject"),
        expected_revision: 1,
        operation: "Rename".to_owned(),
        arguments: json!({"title": "new"}),
        fulfillments: BTreeMap::new(),
        recording: recording(record_id),
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

#[test]
fn legacy_request_domains_compare_new_actions_in_single_and_batch_recovery() {
    let registry = legacy_registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    let cases = [
        ("kernel_ticket", "kernel-execute"),
        ("service_1_ticket", "service-1-execute"),
        ("service_2_ticket", "service-2-execute"),
    ];
    for (entity, _) in cases {
        block_on(executor.create(CreateRequest {
            subject: Subject::new(entity, "t-1").expect("subject"),
            definition_version: 1,
            fields: json!({"title": "old"}),
            recording: recording(&format!("{entity}-create")),
        }))
        .expect("creation commits");
    }

    let actions: Vec<_> = cases
        .iter()
        .map(|(entity, record_id)| BatchAction::Execute(legacy_execute(entity, record_id)))
        .collect();
    let batch_key = BatchKey::Named("legacy-domains".to_owned());
    let original = block_on(executor.batch(batch_key.clone(), actions.clone()))
        .expect("legacy requests commit as one named batch");
    let original_receipt = original.receipt().expect("batch receipt").clone();
    for ((entity, _), domain) in cases
        .iter()
        .zip(["er.request/1", "er.request/2", "er.request/3"])
    {
        let history = block_on(store.history(&Subject::new(*entity, "t-1").expect("subject")))
            .expect("history");
        assert_eq!(request_domain(&history.records[1].entry), domain);
    }

    let exact_batch = block_on(executor.batch(batch_key.clone(), actions.clone()))
        .expect("exact empty-map batch retry replays");
    assert!(exact_batch.replayed());
    assert_eq!(exact_batch.receipt(), Some(&original_receipt));

    let CommitReceipt::Batch(batch_receipt) = &original_receipt else {
        panic!("the original named batch has a batch receipt")
    };
    for (index, ((_, record_id), action)) in cases.iter().zip(&actions).enumerate() {
        let BatchAction::Execute(request) = action else {
            unreachable!("every legacy action is an execute")
        };
        let exact_single = block_on(executor.execute(request.clone()))
            .expect("exact empty-map single retry replays");
        assert!(exact_single.replayed());
        let member_receipt = CommitReceipt::Single(batch_receipt.members[index].clone());
        assert_eq!(exact_single.receipt(), Some(&member_receipt));

        let mut changed_request = request.clone();
        changed_request.fulfillments.insert(
            "undeclared".to_owned(),
            OperationFieldAction::Set {
                value: json!("must-not-disappear"),
            },
        );
        let single_error = block_on(executor.execute(changed_request.clone()))
            .expect_err("a legacy single retry cannot ignore new fulfillment actions");
        assert!(matches!(
            single_error.store_error(),
            Some(AsyncStoreError::RecordConflict { record_id: actual }) if actual == record_id
        ));

        let mut changed_batch = actions.clone();
        changed_batch[index] = BatchAction::Execute(changed_request);
        let batch_error = block_on(executor.batch(batch_key.clone(), changed_batch))
            .expect_err("a legacy batch retry cannot ignore new fulfillment actions");
        assert!(matches!(
            batch_error.store_error(),
            Some(AsyncStoreError::RecordConflict { record_id: actual }) if actual == record_id
        ));
    }
}
