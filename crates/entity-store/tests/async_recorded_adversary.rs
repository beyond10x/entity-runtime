//! Adversarial regressions for asynchronous recorded-store assurance boundaries.

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use entity_core::{Registry, Runtime};
use entity_store::{
    asynchronous::{
        original_request_comparison_bytes, verify_store_histories, AppendMember, AppendRequest,
        AsyncRecordedReader, AsyncRecordedWriter, AsyncStoreError, BatchKey, CompleteStoreSnapshot,
        HistoryOrigin, ImportedRecordEvidence, KnownLegacyOrder, LegacyAnchor, LegacyCompleteness,
        LegacyEvidence, LegacyOrderDeclaration, MemoryRecordedStore, RecordedEntry, StoreCoverage,
        Subject, SubjectHistory,
    },
    Expect, RecordedCommit, RecordedObservation, Recording,
};
use serde_json::json;

fn block_on<F: Future>(future: F) -> F::Output {
    let mut context = Context::from_waker(Waker::noop());
    let mut future = std::pin::pin!(future);
    match Future::poll(Pin::as_mut(&mut future), &mut context) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("reference-store future unexpectedly remained pending"),
    }
}

fn recording(record_id: &str) -> Recording {
    Recording {
        record_id: record_id.to_owned(),
        recorded_at: "2026-09-15T00:00:00Z".to_owned(),
        correlation: None,
        causation: None,
        actor: None,
    }
}

fn registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "ticket",
        "version": 1,
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "open", "states": ["open"] },
        "operations": {}
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    registry
}

fn creation(id: &str, record_id: &str) -> RecordedCommit {
    let registry = registry();
    let decision = Runtime::new(&registry)
        .create("ticket", 1, id, json!({"title": id}))
        .expect("creation succeeds");
    RecordedCommit::new(decision, &recording(record_id)).expect("recording is valid")
}

fn append_creation(store: &MemoryRecordedStore, id: &str, record_id: &str) {
    let entry = RecordedEntry::Decision(creation(id, record_id));
    let request_bytes =
        original_request_comparison_bytes(&entry).expect("complete request encodes");
    let request = AppendRequest::new(
        BatchKey::SingleRecord(record_id.to_owned()),
        vec![AppendMember::new(Expect::Absent, entry, request_bytes)],
    )
    .expect("append request is valid");
    block_on(AsyncRecordedWriter::append(store, request)).expect("creation commits");
}

#[test]
fn a_partial_transcript_cannot_be_relabelled_as_a_complete_provider_snapshot() {
    let store = MemoryRecordedStore::new();
    append_creation(&store, "one", "create-one");
    append_creation(&store, "two", "create-two");

    let complete = block_on(AsyncRecordedReader::complete_snapshot(
        &store,
        "reference-store",
    ))
    .expect("provider snapshot is readable");
    assert_eq!(
        complete.histories.len(),
        2,
        "provider returned both subjects"
    );

    let partial_claiming_complete = CompleteStoreSnapshot {
        scope: complete.scope,
        coverage: StoreCoverage::CompleteSnapshot,
        histories: vec![complete.histories[0].clone()],
    };

    let error = verify_store_histories(&partial_claiming_complete)
        .expect_err("caller-selected evidence must not claim whole-provider coverage");
    assert!(
        matches!(error, AsyncStoreError::InvalidInput(_)),
        "the refusal should identify invalid assurance provenance: {error:?}"
    );
}

#[test]
fn imported_envelope_cannot_be_later_than_its_anchor_revision() {
    let store = MemoryRecordedStore::new();
    let anchor_commit = creation("legacy", "legacy-create");
    let subject = Subject::new("ticket", "legacy").expect("subject is valid");
    let later_observation = RecordedObservation {
        entity: "ticket".to_owned(),
        id: "legacy".to_owned(),
        revision: 2,
        envelope: recording("legacy-observation")
            .seal(json!({"source": "legacy"}))
            .expect("recording is valid"),
    };
    let evidence = ImportedRecordEvidence::new(
        RecordedEntry::Observation(later_observation),
        "legacy-source",
        "observations/0",
        KnownLegacyOrder::PerKind(0),
    )
    .expect("envelope is structurally valid");

    let error = store
        .seed_imported(SubjectHistory {
            subject,
            origin: HistoryOrigin::Imported(LegacyAnchor {
                instance: anchor_commit.instance,
                completeness: LegacyCompleteness::AvailableEvidenceOnly,
                order: LegacyOrderDeclaration::PerKindOnly,
                evidence: vec![LegacyEvidence::Envelope(evidence)],
            }),
            records: Vec::new(),
        })
        .expect_err("pre-anchor evidence cannot name a post-anchor revision");
    assert!(
        matches!(error, AsyncStoreError::CorruptHistory { .. }),
        "the malformed boundary should be an integrity refusal: {error:?}"
    );
}
