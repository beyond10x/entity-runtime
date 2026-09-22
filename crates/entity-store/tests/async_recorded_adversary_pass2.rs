//! Second-pass adversarial regressions for the asynchronous recorded writer boundary.

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use entity_core::{Registry, Runtime};
use entity_store::{
    asynchronous::{
        original_request_comparison_bytes, AppendMember, AppendRequest, AsyncRecordedWriter,
        AsyncStoreError, BatchKey, MemoryRecordedStore, RecordedEntry, WriteFailure,
    },
    Expect, RecordedCommit, Recording,
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

fn creation(id: &str, record_id: &str) -> RecordedEntry {
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
    let decision = Runtime::new(&registry)
        .create("ticket", 1, id, json!({"title": id}))
        .expect("creation succeeds");
    let recording = Recording {
        record_id: record_id.to_owned(),
        recorded_at: "2026-09-15T00:00:00Z".to_owned(),
        correlation: None,
        causation: None,
        actor: None,
    };
    RecordedEntry::Decision(RecordedCommit::new(decision, &recording).expect("recording is valid"))
}

fn member(entry: RecordedEntry) -> AppendMember {
    let request_bytes =
        original_request_comparison_bytes(&entry).expect("complete request encodes");
    AppendMember::new(Expect::Absent, entry, request_bytes)
}

#[test]
fn the_writer_rechecks_duplicate_global_ids_in_a_public_append_request() {
    let store = MemoryRecordedStore::new();
    let request = AppendRequest {
        key: Some(BatchKey::Named("duplicate-ids".to_owned())),
        members: vec![
            member(creation("one", "shared-record-id")),
            member(creation("two", "shared-record-id")),
        ],
    };

    let error = block_on(AsyncRecordedWriter::append(&store, request))
        .expect_err("the mandatory writer boundary must refuse duplicate global record ids");
    assert!(
        matches!(
            error,
            WriteFailure::NotCommitted(AsyncStoreError::DuplicateRecordId { ref record_id })
                if record_id == "shared-record-id"
        ),
        "the refusal must identify the duplicate global record id: {error:?}"
    );
}
