//! Adversarial exact-version selection and byte-compatibility checks.

use std::collections::BTreeMap;

use entity_core::{CoreError, Registry};
use entity_executor::{
    test_support::block_on, CreateRequest, ExecuteRequest, ExecutionError, Executor,
    VersionedBatchAction,
};
use entity_store::{
    asynchronous::{
        AsyncRecordedReader, AsyncStateReader, AsyncStoreError, BatchKey, MemoryRecordedStore,
        RecordLookup, Subject,
    },
    Recording,
};
use serde_json::json;

fn registry() -> Registry {
    let mut registry = Registry::new();
    for version in [1, 2] {
        registry
            .register(
                serde_json::from_value(json!({
                    "entity": "invoice", "version": version, "semantics": "service/3",
                    "schema": {"fields": {}},
                    "lifecycle": {"initial": "Open", "states": ["Open"]},
                    "operations": {"Pay": {
                        "arguments": {"fields": {"accept": {"type": "boolean", "required": true}}},
                        "outcomes": [
                            {"name": format!("rejected-v{version}"),
                             "when": {"eq": ["$args.accept", version == 2]},
                             "refuses": {"error": format!("RejectedV{version}")}},
                            {"name": "paid", "effect": "updates"}
                        ]
                    }}
                }))
                .unwrap(),
            )
            .unwrap();
    }
    registry
}

fn recording(id: &str) -> Recording {
    Recording {
        record_id: id.into(),
        recorded_at: "2026-10-03T00:00:00Z".into(),
        correlation: None,
        causation: None,
        actor: None,
    }
}

fn create(version: u32) -> CreateRequest {
    CreateRequest {
        subject: Subject::new("invoice", "a").unwrap(),
        definition_version: version,
        fields: json!({}),
        recording: recording("created"),
    }
}

fn execute(accept: bool) -> ExecuteRequest {
    ExecuteRequest {
        subject: Subject::new("invoice", "a").unwrap(),
        expected_revision: 1,
        operation: "Pay".into(),
        arguments: json!({"accept": accept}),
        fulfillments: BTreeMap::new(),
        recording: recording("paid"),
    }
}

#[test]
fn divergent_registered_versions_select_only_the_requested_guard() {
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    for (version, accept) in [(1, false), (2, true)] {
        store.clear_trace();
        let result = block_on(executor.execute_versioned(execute(accept), version));
        assert!(matches!(result,
            Err(ExecutionError::Core(CoreError::Refused {outcome, error, message}))
            if outcome == format!("rejected-v{version}")
                && error == format!("RejectedV{version}") && message.is_none()));
        assert_eq!(store.trace(), vec!["lookup_batch", "lookup_record:paid"]);
    }
    store.clear_trace();
    assert!(matches!(
        block_on(executor.execute_versioned(execute(false), 3)),
        Err(ExecutionError::Core(CoreError::EntityNotRegistered {entity, version: 3}))
        if entity == "invoice"
    ));
    assert_eq!(store.trace(), vec!["lookup_batch", "lookup_record:paid"]);
}

#[test]
fn explicit_and_row_bound_execution_persist_identical_bytes_and_replay_across_apis() {
    for version in [1, 2] {
        let registry = registry();
        let legacy = MemoryRecordedStore::new();
        let explicit = MemoryRecordedStore::new();
        for (store, versioned) in [(&legacy, false), (&explicit, true)] {
            let executor = Executor::new(&registry, store);
            block_on(executor.create(create(version))).unwrap();
            let request = execute(version == 1);
            let committed = if versioned {
                block_on(executor.execute_versioned(request.clone(), version)).unwrap()
            } else {
                block_on(executor.execute(request.clone())).unwrap()
            };
            let empty = Registry::new();
            let restarted = Executor::new(&empty, store);
            let retry = if versioned {
                block_on(restarted.execute(request.clone())).unwrap()
            } else {
                block_on(restarted.execute_versioned(request.clone(), version)).unwrap()
            };
            assert!(retry.replayed());
            assert_eq!(retry.receipt(), committed.receipt());
            assert!(matches!(
                block_on(restarted.execute_versioned(request, 3 - version)),
                Err(ExecutionError::Store(AsyncStoreError::RecordConflict {record_id}))
                if record_id == "paid"
            ));
        }
        let Some(RecordLookup::Committed(legacy_record)) =
            block_on(legacy.lookup_record("paid")).unwrap()
        else {
            panic!("legacy execution did not commit");
        };
        let Some(RecordLookup::Committed(explicit_record)) =
            block_on(explicit.lookup_record("paid")).unwrap()
        else {
            panic!("explicit execution did not commit");
        };
        assert_eq!(legacy_record.request_bytes, explicit_record.request_bytes);
        assert_eq!(legacy_record.entry, explicit_record.entry);
        assert_eq!(legacy_record.receipt, explicit_record.receipt);
    }
}

#[test]
fn accepted_wrong_version_in_local_batch_rolls_back_and_releases_all_identities() {
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    let key = BatchKey::Named("mixed-versions".into());
    let actions = |version| {
        vec![
            VersionedBatchAction::Create(create(2)),
            VersionedBatchAction::Execute {
                definition_version: version,
                request: execute(version == 1),
            },
        ]
    };
    assert!(matches!(
        block_on(executor.batch_versioned(key.clone(), actions(1))),
        Err(ExecutionError::Core(CoreError::EntityMismatch {
            expected_version: 1,
            actual_version: 2,
            ..
        }))
    ));
    assert_eq!(block_on(store.lookup_record("created")).unwrap(), None);
    assert_eq!(block_on(store.lookup_record("paid")).unwrap(), None);
    assert_eq!(block_on(store.lookup_batch(&key)).unwrap(), None);
    assert_eq!(
        block_on(store.load(&Subject::new("invoice", "a").unwrap())).unwrap(),
        None
    );
    let committed = block_on(executor.batch_versioned(key.clone(), actions(2))).unwrap();
    assert!(!committed.replayed());
    let state = block_on(store.load(&Subject::new("invoice", "a").unwrap()))
        .unwrap()
        .unwrap();
    assert_eq!((state.version, state.revision), (2, 2));
    let empty = Registry::new();
    let replay = block_on(Executor::new(&empty, &store).batch_versioned(key, actions(2))).unwrap();
    assert!(replay.replayed());
    assert_eq!(replay.receipt(), committed.receipt());
}
