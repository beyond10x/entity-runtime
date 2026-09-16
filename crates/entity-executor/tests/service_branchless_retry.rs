//! Retry identity for a `service/1` creation that declares no branches.
//!
//! The executor decides whether a retry is the same request by comparing the request it would
//! reconstruct with the one the stored record reconstructs, and both halves of that comparison read
//! a creation's recorded `arguments` under the `er.request/2` framing. A branchless `service/1`
//! creation has no branch to produce its fields, so its input *is* its fields — and those same
//! normalized values are what it records as its arguments. Recording none would reconstruct every
//! such creation as the same empty request, and a retry carrying different input would be accepted
//! as the one already committed.

use entity_core::Registry;
use entity_executor::{test_support::block_on, BatchAction, CreateRequest, Executor};
use entity_store::{
    asynchronous::{AsyncStoreError, BatchKey, MemoryRecordedStore, Subject},
    Recording,
};
use serde_json::json;

fn recording(record_id: &str) -> Recording {
    Recording {
        record_id: record_id.to_owned(),
        recorded_at: "2026-09-16T00:00:00Z".to_owned(),
        correlation: None,
        causation: None,
        actor: None,
    }
}

/// A `service/1` definition whose `create` declares no `outcomes`, which registration admits.
fn registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "probe",
        "version": 1,
        "semantics": "service/1",
        "schema": { "fields": {
            "title": { "type": "string", "required": true },
            "count": { "type": "integer", "default": 0 }
        }},
        "lifecycle": { "initial": "held", "states": ["held"] }
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    registry
}

fn create(title: &str, record_id: &str) -> CreateRequest {
    CreateRequest {
        subject: Subject::new("probe", "p-1").expect("subject"),
        definition_version: 1,
        fields: json!({ "title": title }),
        recording: recording(record_id),
    }
}

#[test]
fn a_branchless_service_1_retry_carrying_other_input_is_not_the_request_already_committed() {
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);

    block_on(executor.create(create("one", "c-1"))).expect("the first creation commits");

    // The exact retry is the same request and recovers the original receipt rather than writing
    // twice.
    let replayed =
        block_on(executor.create(create("one", "c-1"))).expect("an exact retry recovers");
    assert!(replayed.replayed(), "unexpected outcome: {replayed:?}");
    assert!(replayed.receipt().is_some());

    // The same record id carrying other input is a different request and says so.
    let conflict = block_on(executor.create(create("two", "c-1")))
        .expect_err("a retry that changes what the caller sent is not the same request");
    assert!(
        matches!(
            conflict.store_error(),
            Some(AsyncStoreError::RecordConflict { record_id }) if record_id == "c-1"
        ),
        "unexpected error: {conflict:?}"
    );
}

#[test]
fn a_branchless_service_1_retry_reads_the_normalized_input_defaults_included() {
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);

    // `count` is defaulted, so the request the record reconstructs carries the *normalized* input.
    // A retry that spells the default out loud is therefore the same request, and one that supplies
    // another value is not.
    block_on(executor.create(CreateRequest {
        subject: Subject::new("probe", "p-2").expect("subject"),
        definition_version: 1,
        fields: json!({ "title": "one" }),
        recording: recording("c-2"),
    }))
    .expect("the first creation commits");

    let spelled_out = block_on(executor.create(CreateRequest {
        subject: Subject::new("probe", "p-2").expect("subject"),
        definition_version: 1,
        fields: json!({ "title": "one", "count": 0 }),
        recording: recording("c-2"),
    }))
    .expect("the default spelled out is the same normalized request");
    assert!(spelled_out.replayed());

    let other = block_on(executor.create(CreateRequest {
        subject: Subject::new("probe", "p-2").expect("subject"),
        definition_version: 1,
        fields: json!({ "title": "one", "count": 1 }),
        recording: recording("c-2"),
    }))
    .expect_err("another value for the defaulted field is another request");
    assert!(
        matches!(
            other.store_error(),
            Some(AsyncStoreError::RecordConflict { record_id }) if record_id == "c-2"
        ),
        "unexpected error: {other:?}"
    );
}

#[test]
fn two_branchless_service_1_creations_in_one_batch_keep_their_own_requests() {
    let registry = registry();
    let store = MemoryRecordedStore::new();
    let executor = Executor::new(&registry, &store);

    let outcome = block_on(executor.batch(
        BatchKey::Named("pair".to_owned()),
        vec![
            BatchAction::Create(CreateRequest {
                subject: Subject::new("probe", "p-a").expect("subject"),
                definition_version: 1,
                fields: json!({ "title": "one" }),
                recording: recording("b-a"),
            }),
            BatchAction::Create(CreateRequest {
                subject: Subject::new("probe", "p-b").expect("subject"),
                definition_version: 1,
                fields: json!({ "title": "two" }),
                recording: recording("b-b"),
            }),
        ],
    ))
    .expect("the batch commits");
    assert!(!outcome.replayed());

    // Swapping what the two callers sent is not the batch that was committed.
    let swapped = block_on(executor.batch(
        BatchKey::Named("pair".to_owned()),
        vec![
            BatchAction::Create(CreateRequest {
                subject: Subject::new("probe", "p-a").expect("subject"),
                definition_version: 1,
                fields: json!({ "title": "two" }),
                recording: recording("b-a"),
            }),
            BatchAction::Create(CreateRequest {
                subject: Subject::new("probe", "p-b").expect("subject"),
                definition_version: 1,
                fields: json!({ "title": "one" }),
                recording: recording("b-b"),
            }),
        ],
    ))
    .expect_err("two creations that differ in what the caller sent are two requests");
    assert!(
        matches!(
            swapped.store_error(),
            Some(AsyncStoreError::RecordConflict { record_id }) if record_id == "b-a"
        ),
        "unexpected error: {swapped:?}"
    );
}
