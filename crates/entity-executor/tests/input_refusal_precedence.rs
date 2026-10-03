//! Input refusal ordering and explicit definition authority for recorded execution.
use std::collections::BTreeMap;

use entity_core::{CoreError, Registry};
use entity_executor::{
    test_support::block_on, CreateRequest, ExecuteRequest, ExecutionError, Executor,
};
use entity_store::{
    asynchronous::{AsyncRecordedReader, MemoryRecordedStore, Subject},
    Recording,
};
use serde_json::json;

fn registry() -> Registry {
    let mut registry = Registry::new();
    for version in [1, 2] {
        registry.register(serde_json::from_value(json!({
            "entity": "invoice", "version": version, "semantics": "service/3",
            "schema": {"fields": {}},
            "lifecycle": {"initial": "Open", "states": ["Open"]},
            "create": {"arguments": {"fields": {"accept": {"type":"boolean","required":true}}},
                "outcomes": [
                    {"name":"rejected", "when":{"eq":["$args.accept",false]},
                     "refuses":{"error":"InputRejected","message":"Input was refused"}},
                    {"name":"created","effect":"creates"}
                ]},
            "operations": {"Pay": {
                "arguments": {"fields": {"accept": {"type":"boolean","required":true}}},
                "outcomes": [
                    {"name":"rejected", "when":{"eq":["$args.accept",false]},
                     "refuses":{"error":"InputRejected","message":"Input was refused"}},
                    {"name":"paid","effect":"updates"}
                ]}}
        })).unwrap()).unwrap();
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
fn create(id: &str, record: &str, accept: bool) -> CreateRequest {
    CreateRequest {
        subject: Subject::new("invoice", id).unwrap(),
        definition_version: 1,
        fields: json!({"accept":accept}),
        recording: recording(record),
    }
}
fn execute(id: &str, record: &str, accept: bool) -> ExecuteRequest {
    ExecuteRequest {
        subject: Subject::new("invoice", id).unwrap(),
        expected_revision: 1,
        operation: "Pay".into(),
        arguments: json!({"accept":accept}),
        fulfillments: BTreeMap::new(),
        recording: recording(record),
    }
}
fn assert_refused(error: ExecutionError) {
    assert!(
        matches!(&error,ExecutionError::Core(CoreError::Refused {outcome,error,message})
        if outcome=="rejected" && error=="InputRejected" && message.as_deref()==Some("Input was refused")),
        "expected declared input refusal; got {error:?}"
    );
}
#[test]
fn executor_input_refusal_existing_create() {
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    block_on(executor.create(create("a", "created", true))).unwrap();
    let before = block_on(store.history(&Subject::new("invoice", "a").unwrap())).unwrap();
    store.clear_trace();
    assert_refused(block_on(executor.create(create("a", "refused", false))).unwrap_err());
    assert_eq!(store.trace(), vec!["lookup_batch", "lookup_record:refused"]);
    assert_eq!(
        block_on(store.history(&Subject::new("invoice", "a").unwrap())).unwrap(),
        before
    );
    assert_eq!(block_on(store.lookup_record("refused")).unwrap(), None);
}
#[test]
fn executor_input_refusal_missing_execute() {
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    assert_refused(
        block_on(executor.execute_versioned(execute("missing", "refused", false), 1)).unwrap_err(),
    );
    assert_eq!(store.trace(), vec!["lookup_batch", "lookup_record:refused"]);
    assert_eq!(block_on(store.lookup_record("refused")).unwrap(), None);
}

#[test]
fn versioned_refusal_precedes_existing_revision_and_definition_mismatch() {
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    block_on(executor.create(create("a", "created", true))).unwrap();
    let mut request = execute("a", "refused", false);
    request.expected_revision = 99;
    store.clear_trace();
    assert_refused(block_on(executor.execute_versioned(request, 2)).unwrap_err());
    assert_eq!(store.trace(), vec!["lookup_batch", "lookup_record:refused"]);
}

#[test]
fn accepted_input_preserves_absence_revision_and_definition_checks() {
    use entity_store::asynchronous::AsyncStoreError;
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    assert!(matches!(
        block_on(executor.execute_versioned(execute("a", "missing", true), 1)),
        Err(ExecutionError::Store(AsyncStoreError::RevisionConflict {
            found: None,
            ..
        }))
    ));
    block_on(executor.create(create("a", "created", true))).unwrap();
    let mut stale = execute("a", "stale", true);
    stale.expected_revision = 2;
    assert!(matches!(
        block_on(executor.execute_versioned(stale, 1)),
        Err(ExecutionError::Store(AsyncStoreError::RevisionConflict {
            found: Some(1),
            ..
        }))
    ));
    assert!(matches!(
        block_on(executor.execute_versioned(execute("a", "wrong-version", true), 2)),
        Err(ExecutionError::Core(CoreError::EntityMismatch {
            expected_version: 2,
            actual_version: 1,
            ..
        }))
    ));
    assert!(matches!(
        block_on(executor.create(create("a", "duplicate", true))),
        Err(ExecutionError::Store(AsyncStoreError::RevisionConflict {
            found: Some(1),
            ..
        }))
    ));
    let mut malformed = create("a", "malformed", true);
    malformed.fields = json!({"accept":"bad"});
    assert!(matches!(
        block_on(executor.create(malformed)),
        Err(ExecutionError::Store(AsyncStoreError::RevisionConflict {
            found: Some(1),
            ..
        }))
    ));
}

#[test]
fn legacy_execute_keeps_row_authority_without_choosing_a_registered_version() {
    use entity_store::asynchronous::AsyncStoreError;
    for registry in [registry(), {
        let mut r = Registry::new();
        let definition = registry()
            .get("invoice", 1)
            .unwrap()
            .clone()
            .into_definition();
        r.register(definition).unwrap();
        r
    }] {
        let store = MemoryRecordedStore::new();
        let executor = Executor::new(&registry, &store);
        assert!(matches!(
            block_on(executor.execute(execute("absent", "legacy", false))),
            Err(ExecutionError::Store(AsyncStoreError::RevisionConflict {
                found: None,
                ..
            }))
        ));
        block_on(executor.create(create("a", "created", true))).unwrap();
        block_on(executor.execute(execute("a", "legacy", true))).unwrap();
    }
}

#[test]
fn versioned_batch_refusal_rolls_back_earlier_local_decisions() {
    use entity_executor::VersionedBatchAction as Action;
    use entity_store::asynchronous::{AsyncStateReader, BatchKey};
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    let key = BatchKey::Named("atomic".into());
    let actions = vec![
        Action::Create(create("a", "created", true)),
        Action::Execute {
            definition_version: 1,
            request: execute("a", "refused", false),
        },
    ];
    assert_refused(block_on(executor.batch_versioned(key.clone(), actions)).unwrap_err());
    assert!(!store.trace().iter().any(|call| call == "append"));
    assert_eq!(
        block_on(store.load(&Subject::new("invoice", "a").unwrap())).unwrap(),
        None
    );
    assert_eq!(block_on(store.lookup_record("created")).unwrap(), None);
    assert_eq!(block_on(store.lookup_batch(&key)).unwrap(), None);
    let actions = vec![
        Action::Create(create("a", "created", true)),
        Action::Execute {
            definition_version: 1,
            request: execute("a", "refused", true),
        },
    ];
    assert!(!block_on(executor.batch_versioned(key, actions))
        .unwrap()
        .replayed());
    assert_eq!(
        block_on(store.load(&Subject::new("invoice", "a").unwrap()))
            .unwrap()
            .unwrap()
            .revision,
        2
    );
}

#[test]
fn explicit_definition_does_not_require_registry_for_committed_retry() {
    use entity_store::asynchronous::{
        original_request_comparison_bytes, AsyncStoreError, RecordLookup,
    };
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    block_on(executor.create(create("a", "created", true))).unwrap();
    let request = execute("a", "paid", true);
    let original = block_on(executor.execute_versioned(request.clone(), 1)).unwrap();
    let Some(RecordLookup::Committed(stored)) = block_on(store.lookup_record("paid")).unwrap()
    else {
        panic!("missing commit")
    };
    assert_eq!(
        stored.request_bytes,
        original_request_comparison_bytes(&stored.entry).unwrap()
    );
    assert!(!String::from_utf8(stored.request_bytes)
        .unwrap()
        .contains("definition_version"));
    let empty = Registry::new();
    let restarted = Executor::new(&empty, &store);
    let retry = block_on(restarted.execute_versioned(request.clone(), 1)).unwrap();
    assert!(retry.replayed());
    assert_eq!(retry.receipt(), original.receipt());
    assert!(block_on(restarted.execute(request.clone()))
        .unwrap()
        .replayed());
    assert!(matches!(block_on(restarted.execute_versioned(request,2)),
        Err(ExecutionError::Store(AsyncStoreError::RecordConflict {record_id})) if record_id=="paid"));
}

#[test]
fn legacy_committed_execution_can_be_retried_with_its_explicit_saved_version() {
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    block_on(executor.create(create("a", "created", true))).unwrap();
    let request = execute("a", "paid", true);
    let original = block_on(executor.execute(request.clone())).unwrap();
    let empty = Registry::new();
    let retry = block_on(Executor::new(&empty, &store).execute_versioned(request, 1)).unwrap();
    assert!(retry.replayed());
    assert_eq!(retry.receipt(), original.receipt());
}

#[test]
fn imported_execution_retry_checks_explicit_version_before_accepting_identity() {
    use entity_core::Runtime;
    use entity_store::{
        asynchronous::{
            AppendOutcome, AsyncStoreError, HistoryOrigin, ImportedRecordEvidence,
            KnownLegacyOrder, LegacyAnchor, LegacyCompleteness, LegacyEvidence,
            LegacyOrderDeclaration, RecordedEntry, SubjectHistory,
        },
        RecordedCommit,
    };
    let registry = registry();
    let runtime = Runtime::new(&registry);
    let created = runtime
        .create("invoice", 1, "a", json!({"accept":true}))
        .unwrap();
    let decision = runtime
        .execute(&created.instance, "Pay", json!({"accept":true}))
        .unwrap();
    let commit = RecordedCommit::new(decision, &recording("paid")).unwrap();
    let store = MemoryRecordedStore::new();
    store
        .seed_imported(SubjectHistory {
            subject: Subject::new("invoice", "a").unwrap(),
            origin: HistoryOrigin::Imported(LegacyAnchor {
                instance: commit.instance.clone(),
                completeness: LegacyCompleteness::AvailableEvidenceOnly,
                order: LegacyOrderDeclaration::PerKindOnly,
                evidence: vec![LegacyEvidence::Envelope(
                    ImportedRecordEvidence::new(
                        RecordedEntry::Decision(commit),
                        "source",
                        "record/1",
                        KnownLegacyOrder::PerKind(0),
                    )
                    .unwrap(),
                )],
            }),
            records: vec![],
        })
        .unwrap();
    let empty = Registry::new();
    let executor = Executor::new(&empty, &store);
    assert!(matches!(
        block_on(executor.execute_versioned(execute("a", "paid", true), 1)).unwrap(),
        AppendOutcome::Historical { .. }
    ));
    assert!(
        matches!(block_on(executor.execute_versioned(execute("a","paid",true),2)),
        Err(ExecutionError::Store(AsyncStoreError::RecordConflict {record_id})) if record_id=="paid")
    );
    store.remove_imported_definition_for_test("paid");
    assert!(matches!(
        block_on(executor.execute_versioned(execute("a", "paid", true), 1)),
        Err(ExecutionError::Store(
            AsyncStoreError::HistoricalRetryUnverifiable { .. }
        ))
    ));
}

#[test]
fn versioned_merge_refusal_precedes_the_merge_history_read() {
    use entity_executor::{MergeRequest, VersionedBatchAction};
    use entity_store::asynchronous::BatchKey;
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    let action = VersionedBatchAction::Merge {
        definition_version: 1,
        request: MergeRequest {
            execute: execute("missing", "refused", false),
            first: "unused".into(),
        },
    };
    assert_refused(
        block_on(executor.batch_versioned(BatchKey::Named("merge".into()), vec![action]))
            .unwrap_err(),
    );
    assert_eq!(store.trace(), vec!["lookup_batch", "lookup_record:refused"]);
}

#[test]
fn versioned_empty_and_invalid_shape_requests_perform_no_io() {
    use entity_executor::VersionedBatchAction;
    use entity_store::asynchronous::{AppendOutcome, AsyncStoreError, BatchKey};
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    assert!(matches!(
        block_on(executor.batch_versioned(BatchKey::Named(String::new()), vec![])).unwrap(),
        AppendOutcome::Empty
    ));
    assert!(
        matches!(block_on(executor.execute_versioned(execute("a","bad-version",false),0)),
        Err(ExecutionError::Store(AsyncStoreError::InvalidInput(message))) if message.contains("definition version"))
    );
    let mut bad = execute("a", "bad-shape", false);
    bad.expected_revision = 0;
    assert!(matches!(block_on(executor.execute_versioned(bad,1)),
        Err(ExecutionError::Store(AsyncStoreError::InvalidInput(message))) if message.contains("revision")));
    let action = VersionedBatchAction::Execute {
        definition_version: 1,
        request: execute("a", "duplicate", false),
    };
    assert!(matches!(
        block_on(executor.batch_versioned(
            BatchKey::Named("dupes".into()),
            vec![action.clone(), action]
        )),
        Err(ExecutionError::Store(
            AsyncStoreError::DuplicateRecordId { .. }
        ))
    ));
    assert_eq!(store.trace(), Vec::<String>::new());
}

#[test]
fn named_mixed_batch_retry_uses_saved_versions_and_rejects_changed_binding() {
    use entity_executor::VersionedBatchAction as Action;
    use entity_store::{
        asynchronous::{AsyncStoreError, BatchKey},
        RecordedObservation,
    };
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    let key = BatchKey::Named("mixed".into());
    let mut actions = vec![
        Action::Create(create("a", "created", true)),
        Action::Observe(RecordedObservation {
            entity: "invoice".into(),
            id: "a".into(),
            revision: 1,
            envelope: recording("observed")
                .seal(json!({"source":"test"}))
                .unwrap(),
        }),
        Action::Execute {
            definition_version: 1,
            request: execute("a", "paid", true),
        },
    ];
    let original = block_on(executor.batch_versioned(key.clone(), actions.clone())).unwrap();
    let empty = Registry::new();
    let restarted = Executor::new(&empty, &store);
    let retry = block_on(restarted.batch_versioned(key.clone(), actions.clone())).unwrap();
    assert!(retry.replayed());
    assert_eq!(retry.receipt(), original.receipt());
    actions[2] = Action::Execute {
        definition_version: 2,
        request: execute("a", "paid", true),
    };
    assert!(matches!(block_on(restarted.batch_versioned(key,actions)),
        Err(ExecutionError::Store(AsyncStoreError::RecordConflict {record_id})) if record_id=="paid"));
}

#[test]
fn recorded_refusal_keeps_explicit_definition_authority() {
    use entity_store::asynchronous::{
        AsyncRefusalRecorder, AsyncStoreError, BoxFuture, RecordedRefusal,
    };
    use std::sync::Mutex;
    #[derive(Default)]
    struct Recorder(Mutex<Vec<RecordedRefusal>>);
    impl AsyncRefusalRecorder for Recorder {
        fn record_refusal<'a>(
            &'a self,
            refusal: &'a RecordedRefusal,
        ) -> BoxFuture<'a, Result<bool, AsyncStoreError>> {
            Box::pin(async move {
                self.0.lock().unwrap().push(refusal.clone());
                Ok(false)
            })
        }
        fn refusals<'a>(&'a self) -> BoxFuture<'a, Result<Vec<RecordedRefusal>, AsyncStoreError>> {
            Box::pin(async move { Ok(self.0.lock().unwrap().clone()) })
        }
    }
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let recorder = Recorder::default();
    let executor = Executor::recording_refusals(&registry, &store, &recorder);
    assert_refused(
        block_on(executor.execute_versioned(execute("a", "refused", false), 2)).unwrap_err(),
    );
    let refusals = block_on(recorder.refusals()).unwrap();
    assert_eq!(refusals.len(), 1);
    assert_eq!(refusals[0].request[0]["definition_version"], json!(2));
    assert_eq!(refusals[0].request[0]["operation"], json!("Pay"));
    assert_eq!(store.trace(), vec!["lookup_batch", "lookup_record:refused"]);
}
