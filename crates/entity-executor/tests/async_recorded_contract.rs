//! End-to-end conformance tests for the runtime-neutral async recorded executor.

use std::task::Poll;

use entity_core::{Registry, Runtime};
use entity_executor::{
    test_support::{block_on, poll_once},
    BatchAction, CreateRequest, ExecuteRequest, Executor,
};
use entity_store::{
    asynchronous::{
        AppendOutcome, AppendScript, AsyncRecordedReader, AsyncStateReader, AsyncStoreError,
        BatchKey, CommitReceipt, HistoryOrigin, ImportedRecordEvidence, KnownLegacyOrder,
        LegacyAnchor, LegacyCompleteness, LegacyEvidence, LegacyOrderDeclaration,
        MemoryRecordedStore, RecordKind, RecordedEntry, Subject, SubjectHistory,
    },
    RecordedObservation, Recording,
};
use serde_json::json;

fn recording(record_id: &str) -> Recording {
    Recording {
        record_id: record_id.to_owned(),
        recorded_at: "2026-09-15T00:00:00Z".to_owned(),
        correlation: Some("flow".to_owned()),
        causation: None,
        actor: None,
    }
}

fn registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "ticket",
        "version": 1,
        "schema": {
            "fields": {
                "title": { "type": "string", "required": true },
                "count": { "type": "integer", "default": 0 }
            }
        },
        "lifecycle": { "initial": "open", "states": ["open", "closed"] },
        "operations": {
            "touch": {
                "transitions": [{ "from": "open", "to": "open" }],
                "arguments": { "fields": {} },
                "emits": []
            },
            "close": {
                "transitions": [{ "from": "open", "to": "closed" }],
                "arguments": { "fields": {} },
                "emits": [
                    { "type": "TicketClosed", "payload": { "id": "$id" } },
                    { "type": "TicketClosed", "payload": { "id": "$id" } }
                ]
            }
        }
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    registry
}

fn create(id: &str, record_id: &str) -> CreateRequest {
    CreateRequest {
        subject: Subject::new("ticket", id).expect("subject"),
        definition_version: 1,
        fields: json!({"title": id}),
        recording: recording(record_id),
    }
}

fn execute(id: &str, revision: u64, operation: &str, record_id: &str) -> ExecuteRequest {
    ExecuteRequest {
        subject: Subject::new("ticket", id).expect("subject"),
        expected_revision: revision,
        operation: operation.to_owned(),
        arguments: json!({}),
        recording: recording(record_id),
    }
}

fn observe(id: &str, revision: u64, record_id: &str) -> RecordedObservation {
    RecordedObservation {
        entity: "ticket".to_owned(),
        id: id.to_owned(),
        revision,
        envelope: recording(record_id)
            .seal(json!({"source": "test", "value": null}))
            .expect("recording is valid"),
    }
}

#[test]
fn mixed_batches_use_ordered_local_state_and_observations_do_not_advance_revision() {
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    let outcome = block_on(executor.batch(
        BatchKey::Named("mixed".to_owned()),
        vec![
            BatchAction::Create(create("a", "a-create")),
            BatchAction::Observe(observe("a", 1, "a-observe-1")),
            BatchAction::Execute(execute("a", 1, "touch", "a-touch")),
            BatchAction::Observe(observe("a", 2, "a-observe-2")),
            BatchAction::Create(create("b", "b-create")),
        ],
    ))
    .expect("mixed batch commits");
    let receipt = match outcome {
        AppendOutcome::Committed {
            receipt: CommitReceipt::Batch(receipt),
            replayed: false,
        } => receipt,
        other => panic!("unexpected outcome: {other:?}"),
    };
    assert_eq!(receipt.members.len(), 5);
    assert_eq!(receipt.members[0].revision, 1);
    assert_eq!(receipt.members[1].revision, 1);
    assert_eq!(receipt.members[2].revision, 2);
    assert_eq!(receipt.members[3].revision, 2);
    assert_eq!(receipt.members[4].revision, 1);
    assert_eq!(receipt.members[0].position.store, 0);
    assert_eq!(receipt.members[4].position.store, 4);
    let state = block_on(AsyncStateReader::load(
        &store,
        &Subject::new("ticket", "a").expect("subject"),
    ))
    .expect("state read")
    .expect("state exists");
    assert_eq!(state.revision, 2, "zero-event decisions still advance once");
}

#[test]
fn stale_observation_in_a_mixed_batch_rolls_back_every_record_and_receipt() {
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    let error = block_on(executor.batch(
        BatchKey::Named("rollback".to_owned()),
        vec![
            BatchAction::Create(create("a", "a-create")),
            BatchAction::Execute(execute("a", 1, "touch", "a-touch")),
            BatchAction::Observe(observe("a", 1, "stale-observation")),
        ],
    ))
    .expect_err("the observation targets the pre-execute revision");
    assert!(error.is_revision_conflict(), "{error:?}");
    assert!(block_on(AsyncStateReader::load(
        &store,
        &Subject::new("ticket", "a").expect("subject")
    ))
    .expect("state read")
    .is_none());
    assert!(block_on(AsyncRecordedReader::lookup_batch(
        &store,
        &BatchKey::Named("rollback".to_owned())
    ))
    .expect("batch lookup")
    .is_none());
}

#[test]
fn retry_uses_saved_definition_and_verified_prefix_before_current_authority() {
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let first = block_on(Executor::new(&registry, &store).create(create("a", "create-a")))
        .expect("creation commits");
    store.clear_trace();

    let empty_registry = Registry::new();
    let retried = block_on(Executor::new(&empty_registry, &store).create(create("a", "create-a")))
        .expect("saved definition makes retry independent of current registry");
    assert_eq!(first.receipt(), retried.receipt());
    assert!(retried.replayed());
    assert_eq!(
        store.trace(),
        vec![
            "lookup_batch",
            "lookup_record:create-a",
            "history:[\"ticket\",\"a\"]"
        ]
    );

    store.tamper_record_event_for_test("create-a");
    let error = block_on(Executor::new(&empty_registry, &store).create(create("a", "create-a")))
        .expect_err("a matching id cannot bypass corrupt history");
    assert!(matches!(
        error.store_error(),
        Some(AsyncStoreError::CorruptHistory { .. })
    ));
}

#[test]
fn named_and_single_retries_preserve_original_receipt_coordinates() {
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    let original = block_on(executor.batch(
        BatchKey::Named("pair".to_owned()),
        vec![
            BatchAction::Create(create("a", "create-a")),
            BatchAction::Create(create("b", "create-b")),
        ],
    ))
    .expect("batch commits");
    let original_receipt = original.receipt().expect("receipt").clone();

    let retried = block_on(executor.batch(
        BatchKey::Named("pair".to_owned()),
        vec![
            BatchAction::Create(create("a", "create-a")),
            BatchAction::Create(create("b", "create-b")),
        ],
    ))
    .expect("exact batch retry recovers");
    assert_eq!(retried.receipt(), Some(&original_receipt));
    assert!(retried.replayed());

    let single =
        block_on(executor.create(create("b", "create-b"))).expect("named member retries singly");
    let member = match single {
        AppendOutcome::Committed {
            receipt: CommitReceipt::Single(member),
            replayed: true,
        } => member,
        other => panic!("unexpected outcome: {other:?}"),
    };
    assert_eq!(member.batch_key, BatchKey::Named("pair".to_owned()));
    assert_eq!(member.member_index, 1);

    let error = block_on(executor.batch(
        BatchKey::Named("new-pair".to_owned()),
        vec![BatchAction::Create(create("b", "create-b"))],
    ))
    .expect_err("prior entries cannot be claimed by a fresh named batch");
    assert!(matches!(
        error.store_error(),
        Some(AsyncStoreError::PreviouslyRecordedBatchEntries { indices }) if indices == &[0]
    ));
}

#[test]
fn global_record_ids_duplicate_requests_and_empty_batches_are_closed_before_writes() {
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    let empty = block_on(executor.batch(BatchKey::Named("".to_owned()), Vec::new()))
        .expect("empty batch is inert even with an otherwise invalid key");
    assert_eq!(empty, AppendOutcome::Empty);
    assert!(store.trace().is_empty());

    let duplicate = block_on(executor.batch(
        BatchKey::Named("duplicates".to_owned()),
        vec![
            BatchAction::Create(create("a", "same")),
            BatchAction::Create(create("b", "same")),
        ],
    ))
    .expect_err("duplicate request ids refuse");
    assert!(matches!(
        duplicate.store_error(),
        Some(AsyncStoreError::DuplicateRecordId { record_id }) if record_id == "same"
    ));
    assert!(block_on(AsyncStateReader::load(
        &store,
        &Subject::new("ticket", "a").expect("subject")
    ))
    .expect("state read")
    .is_none());

    block_on(executor.create(create("a", "global"))).expect("first id commits");
    block_on(executor.observe(observe("a", 1, "equal-content-1")))
        .expect("first equal-content record commits");
    block_on(executor.observe(observe("a", 1, "equal-content-2")))
        .expect("a different id remains a different record");
    let conflict = block_on(executor.observe(observe("a", 1, "global")))
        .expect_err("the id namespace spans record kinds");
    assert!(matches!(
        conflict.store_error(),
        Some(AsyncStoreError::RecordConflict { record_id }) if record_id == "global"
    ));
}

#[test]
fn commit_then_uncertain_and_dropped_response_recover_one_effect_and_receipt() {
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    store.script_next_append(AppendScript::CommitThenUncertain);
    let recovered = block_on(executor.create(create("a", "create-a")))
        .expect("uncertain append is recovered through verified identity");
    assert!(recovered.replayed());
    let receipt = recovered.receipt().expect("receipt").clone();

    store.script_next_append(AppendScript::CommitThenPending);
    let mut future = Box::pin(executor.create(create("b", "create-b")));
    assert!(matches!(poll_once(future.as_mut()), Poll::Pending));
    drop(future);
    let retried = block_on(executor.create(create("b", "create-b")))
        .expect("same identity recovers after caller drops the response future");
    assert!(retried.replayed());
    assert_ne!(retried.receipt(), Some(&receipt));

    let history = block_on(AsyncRecordedReader::history(
        &store,
        &Subject::new("ticket", "b").expect("subject"),
    ))
    .expect("history read");
    assert_eq!(
        history.records.len(),
        1,
        "the durable effect was not duplicated"
    );
}

#[test]
fn complete_decisions_keep_duplicate_events_and_observations_keep_explicit_null_provenance() {
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    block_on(executor.create(create("a", "create-a"))).expect("create");
    block_on(executor.execute(execute("a", 1, "close", "close-a"))).expect("close");
    block_on(executor.observe(observe("a", 2, "observe-a"))).expect("observe");

    let history = block_on(AsyncRecordedReader::history(
        &store,
        &Subject::new("ticket", "a").expect("subject"),
    ))
    .expect("history read");
    assert_eq!(history.records.len(), 3);
    assert_eq!(history.records[1].kind(), RecordKind::Decision);
    assert_eq!(history.records[1].entry.events().len(), 2);
    assert_eq!(
        history.records[1].entry.events()[0],
        history.records[1].entry.events()[1]
    );
    let encoded = serde_json::to_value(&history.records[2].entry).expect("serializes");
    assert!(encoded
        .pointer("/observation/envelope/actor")
        .is_some_and(serde_json::Value::is_null));
}

#[test]
fn changed_expectation_or_provenance_conflicts_with_the_original_retry() {
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);
    block_on(executor.create(create("a", "create-a"))).expect("create");
    block_on(executor.execute(execute("a", 1, "touch", "touch-a"))).expect("touch");

    let changed_expectation = block_on(executor.execute(execute("a", 2, "touch", "touch-a")))
        .expect_err("same id with another predecessor conflicts");
    assert!(matches!(
        changed_expectation.store_error(),
        Some(AsyncStoreError::RecordConflict { record_id }) if record_id == "touch-a"
    ));

    let mut changed = execute("a", 1, "touch", "touch-a");
    changed.recording.actor = Some("someone-else".to_owned());
    let changed_provenance =
        block_on(executor.execute(changed)).expect_err("same id with other provenance conflicts");
    assert!(matches!(
        changed_provenance.store_error(),
        Some(AsyncStoreError::RecordConflict { record_id }) if record_id == "touch-a"
    ));
}

#[test]
fn exact_imported_retry_is_historical_and_missing_matching_facts_are_not_success() {
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let decision = Runtime::new(&registry)
        .create("ticket", 1, "legacy", json!({"title": "legacy"}))
        .expect("creation");
    let commit = entity_store::RecordedCommit::new(decision, &recording("legacy-create"))
        .expect("recording");
    let subject = Subject::new("ticket", "legacy").expect("subject");
    store
        .seed_imported(SubjectHistory {
            subject: subject.clone(),
            origin: HistoryOrigin::Imported(LegacyAnchor {
                instance: commit.instance.clone(),
                completeness: LegacyCompleteness::AvailableEvidenceOnly,
                order: LegacyOrderDeclaration::PerKindOnly,
                evidence: vec![LegacyEvidence::Envelope(
                    ImportedRecordEvidence::new(
                        RecordedEntry::Decision(commit),
                        "legacy",
                        "records/0",
                        KnownLegacyOrder::PerKind(0),
                    )
                    .expect("evidence"),
                )],
            }),
            records: Vec::new(),
        })
        .expect("boundary seeds");

    let historical =
        block_on(Executor::new(&Registry::new(), &store).create(create("legacy", "legacy-create")))
            .expect("exact historical request is recognized");
    assert!(matches!(&historical, AppendOutcome::Historical { .. }));
    assert!(historical.receipt().is_none());

    store.remove_imported_definition_for_test("legacy-create");
    let error =
        block_on(Executor::new(&Registry::new(), &store).create(create("legacy", "legacy-create")))
            .expect_err("missing facts cannot fabricate a historical match");
    assert!(matches!(
        error.store_error(),
        Some(AsyncStoreError::HistoricalRetryUnverifiable { .. })
    ));
}
