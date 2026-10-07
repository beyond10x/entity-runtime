//! R-169 through the Eventlog recorded adapter: a store holding decisions recorded before step 14
//! checked a `service/1` response against its declared schema, whose responses that check refuses,
//! still opens, verifies its history from genesis and replays to the instance it recorded.
//!
//! The entries are the base kernel's own records (the entity-core fixture), appended unchanged
//! through the adapter's writer, so the stored bytes are the bytes a store written before the
//! check holds. The adapter admits an entry and verifies a stored one through the same recorded
//! verifier, so the append and the cold open both exercise it.
#![cfg(feature = "file")]

use std::{path::Path, sync::Arc};

use entity_core::{Decision, DecisionRecord, replay};
use entity_eventlog::{
    AsyncBindingProvisioner, Authority, ErRecordedProjector, EventlogBackend,
    EventlogBindingProvisioner, EventlogOperationContext, EventlogRecordedStore, OpenVerification,
};
use entity_store::{
    Expect, RecordedCommit, Recording,
    asynchronous::{
        AppendMember, AppendRequest, AsyncRecordedReader, AsyncRecordedWriter, AsyncStateReader,
        BatchKey, RecordedEntry, Subject, original_request_comparison_bytes,
        verify_subject_history,
    },
};
use eventlog_core::{CaptureLimits, EventStore, InlineProjectionAdmin, TenantId};
use serde_json::json;
use time::OffsetDateTime;

const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 256,
    max_blobs: 1_024,
    max_projection_rows: 1_024,
    max_payload_bytes: 8 * 1024 * 1024,
};

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

/// A creation and an operation the base kernel answered with responses outside their declared
/// `max_length` and `alphabet`.
fn recorded_before_responses_were_checked() -> Vec<DecisionRecord> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../entity-core/tests/fixtures/responses-recorded-before-they-were-checked.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("the fixture is readable"))
        .expect("the fixture is a list of decision records")
}

fn context(label: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "recorded-responses".into(),
        actor: "entity-eventlog-test".into(),
        request_id: format!("request-{label}"),
        trace_id: format!("trace-{label}"),
        causation_id: None,
        causation_depth: 0,
        occurred_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn append_request(at: usize, record: DecisionRecord) -> AppendRequest {
    let record_id = format!("r-{at}");
    let decision = Decision {
        instance: record.result.clone(),
        events: record.events.clone(),
        record,
    };
    let recording = Recording {
        record_id: record_id.clone(),
        recorded_at: "2026-10-01T00:00:00Z".into(),
        correlation: None,
        causation: None,
        actor: None,
    };
    let entry =
        RecordedEntry::Decision(RecordedCommit::new(decision, &recording).expect("valid commit"));
    let request_bytes = original_request_comparison_bytes(&entry).expect("request bytes");
    let expect = if at == 0 {
        Expect::Absent
    } else {
        Expect::Revision(at as u64)
    };
    AppendRequest::new(
        BatchKey::SingleRecord(record_id),
        vec![AppendMember::new(expect, entry, request_bytes)],
    )
    .expect("append request")
}

#[test]
fn a_file_store_holding_decisions_recorded_before_responses_were_checked_reopens_and_verifies() {
    block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        let backend = Arc::new(
            eventlog_file::FileEventStore::open(directory.path())
                .await
                .expect("file provider"),
        );
        let tenant = TenantId::new("recorded-responses").expect("tenant");
        let stream_identity = backend
            .stream_identity(&tenant)
            .await
            .expect("stream identity");
        let projector = Arc::new(ErRecordedProjector::new());
        backend
            .create_projections(projector.clone())
            .await
            .expect("projection admission");
        backend
            .attach_inline_existing(projector)
            .await
            .expect("projection attachment");
        let authority = Authority {
            logical_scope: "recorded-responses-scope".into(),
            tenant: tenant.as_str().into(),
            stream_identity,
        };
        let backend: Arc<dyn EventlogBackend> = backend;
        EventlogBindingProvisioner::new(backend.clone(), LIMITS)
            .provision_binding(authority.clone(), context("binding"))
            .await
            .expect("binding provisioned");

        let records = recorded_before_responses_were_checked();
        let store = EventlogRecordedStore::open(backend.clone(), authority.clone(), LIMITS)
            .await
            .expect("adapter opens");
        let operation = store.operation(context("write"));
        for (at, record) in records.iter().cloned().enumerate() {
            operation
                .append(append_request(at, record))
                .await
                .unwrap_or_else(|error| panic!("record {at} is admitted: {error:?}"));
        }
        drop(operation);
        drop(store);

        let reopened = EventlogRecordedStore::open(backend, authority, LIMITS)
            .await
            .expect("a store holding responses recorded before the check reopens");
        assert_eq!(reopened.open_verification(), OpenVerification::Complete);
        let subject = Subject::new("keypad", "k-1").expect("subject");
        let terminal = reopened
            .load(&subject)
            .await
            .expect("state read")
            .expect("state exists");
        assert_eq!(terminal, records[1].result);
        let history = reopened.history(&subject).await.expect("history read");
        verify_subject_history(&history, &terminal).expect("history verifies from genesis");
        let stored = history
            .records
            .iter()
            .map(|stored| match &stored.entry {
                RecordedEntry::Decision(commit) => commit.envelope.record.clone(),
                RecordedEntry::Observation(_) => panic!("every entry is a decision"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            stored, records,
            "the store holds the recorded bytes unchanged"
        );
        assert_eq!(
            stored[1].response,
            json!({ "echo": "12x" }).as_object().cloned(),
            "the response its alphabet refuses is still the recorded one"
        );
        assert_eq!(
            replay(&stored).expect("the stored records replay"),
            terminal
        );
    });
}
