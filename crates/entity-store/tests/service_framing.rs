//! Which framing a decision's comparison bytes are written in, and what a reader that does not
//! know a framing does about it.
//!
//! The governing rule for these formats is that a change to their shape, framing or scalar spelling
//! needs a new version domain and explicit migration. A `service/1` decision's record carries keys
//! `er.record/1` has never carried, so it takes one; a `kernel/1` decision keeps every byte it had.

use entity_core::{Registry, Runtime};
use entity_store::{
    asynchronous::{
        batch_comparison_bytes, read_record_in_domain, record_comparison_bytes, record_domain,
        record_framing, request_domain, AppendMember, RecordedEntry,
    },
    Expect, RecordedCommit, Recording,
};
use serde_json::json;

fn recording(record_id: &str) -> Recording {
    Recording {
        record_id: record_id.to_owned(),
        recorded_at: "2026-09-15T00:00:00Z".to_owned(),
        correlation: None,
        causation: None,
        actor: None,
    }
}

fn registry_of(document: serde_json::Value) -> Registry {
    let definition = serde_json::from_value(document).expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    registry
}

fn kernel_entry() -> RecordedEntry {
    let registry = registry_of(json!({
        "entity": "ticket",
        "version": 1,
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "open", "states": ["open"] },
        "operations": {}
    }));
    let decision = Runtime::new(&registry)
        .create("ticket", 1, "t-1", json!({ "title": "t-1" }))
        .expect("creation succeeds");
    RecordedEntry::Decision(RecordedCommit::new(decision, &recording("r-1")).expect("valid"))
}

fn service_entry() -> RecordedEntry {
    let registry = registry_of(json!({
        "entity": "invoice",
        "version": 1,
        "semantics": "service/1",
        "identity": { "field": "invoice_id" },
        "schema": { "fields": {
            "invoice_id": { "type": "string", "required": true },
            "total": { "type": "integer", "required": true }
        }},
        "lifecycle": { "initial": "pending", "states": ["pending"] },
        "create": {
            "arguments": { "fields": { "input": { "type": "object", "required": true, "properties": {
                "invoice_id": { "type": "string", "required": true },
                "amount": { "type": "integer", "required": true }
            }}}},
            "outcomes": [{
                "name": "accepted",
                "effect": "creates",
                "set": {
                    "invoice_id": "$args.input.invoice_id",
                    "total": "$args.input.amount"
                }
            }]
        }
    }));
    let decision = Runtime::new(&registry)
        .create(
            "invoice",
            1,
            "s:INV-1",
            json!({ "input": { "invoice_id": "INV-1", "amount": 10 } }),
        )
        .expect("creation succeeds");
    RecordedEntry::Decision(RecordedCommit::new(decision, &recording("r-2")).expect("valid"))
}

#[test]
fn a_kernel_1_decision_still_frames_as_er_record_1_and_er_request_1() {
    let entry = kernel_entry();
    assert_eq!(record_domain(&entry), "er.record/1");
    assert_eq!(request_domain(&entry), "er.request/1");
    let bytes = record_comparison_bytes(&entry).expect("encodes");
    assert_eq!(
        record_framing(&bytes).expect("reads its framing"),
        "er.record/1"
    );
    let request =
        entity_store::asynchronous::original_request_comparison_bytes(&entry).expect("encodes");
    let text = String::from_utf8(request).expect("utf-8");
    assert!(
        text.starts_with(r#"["er.request/1",{"definition_version":1,"fields":"#),
        "a kernel/1 creation reconstructs its fields, in the shape it has always used: {text}"
    );
}

#[test]
fn a_service_1_decision_frames_as_er_record_2_and_its_request_carries_arguments_not_fields() {
    let entry = service_entry();
    assert_eq!(record_domain(&entry), "er.record/2");
    assert_eq!(request_domain(&entry), "er.request/2");
    let request =
        entity_store::asynchronous::original_request_comparison_bytes(&entry).expect("encodes");
    let text = String::from_utf8(request).expect("utf-8");
    assert!(
        text.starts_with(
            r#"["er.request/2",{"arguments":{"input":{"amount":10,"invoice_id":"INV-1"}}"#
        ),
        "a service/1 creation reconstructs the caller's arguments: {text}"
    );
    assert!(
        !text.contains("\"fields\""),
        "reconstructing it as the branch's fields would hand a retry a request nobody sent: {text}"
    );
}

#[test]
fn an_er_record_1_reader_refuses_an_er_record_2_document_by_naming_the_framing() {
    let service = record_comparison_bytes(&service_entry()).expect("encodes");
    let error = read_record_in_domain("er.record/1", &service)
        .expect_err("a reader that knows only /1 refuses");
    let message = error.to_string();
    assert!(
        message.contains("er.record/1") && message.contains("er.record/2"),
        "the refusal names both framings: {message}"
    );
    // And it refuses at the tag, without parsing the second element at all: the same refusal is
    // produced for a document whose payload is not readable JSON.
    let truncated = b"[\"er.record/2\",{\"kind\":";
    let error = read_record_in_domain("er.record/1", truncated).expect_err("refuses at the tag");
    assert!(error.to_string().contains("er.record/2"));

    // The reader that does know the framing reads it.
    assert!(read_record_in_domain("er.record/2", &service).is_ok());
    let kernel = record_comparison_bytes(&kernel_entry()).expect("encodes");
    assert!(read_record_in_domain("er.record/1", &kernel).is_ok());
}

#[test]
fn a_batch_of_kernel_1_records_keeps_its_er_batch_1_bytes_while_a_service_member_refuses_at_the_record_tag(
) {
    let kernel = AppendMember::new(
        Expect::Absent,
        kernel_entry(),
        entity_store::asynchronous::original_request_comparison_bytes(&kernel_entry())
            .expect("encodes"),
    );
    let service = AppendMember::new(
        Expect::Absent,
        service_entry(),
        entity_store::asynchronous::original_request_comparison_bytes(&service_entry())
            .expect("encodes"),
    );
    let key = entity_store::asynchronous::BatchKey::Named("b".to_owned());

    let all_kernel = batch_comparison_bytes(&key, std::slice::from_ref(&kernel)).expect("encodes");
    let text = String::from_utf8(all_kernel.clone()).expect("utf-8");
    assert!(
        text.starts_with(r#"["er.batch/1",["named","b"],"#),
        "the batch tag does not move: {text}"
    );
    assert!(text.contains("\"er.record/1\""));
    assert!(!text.contains("\"er.record/2\""));

    // A batch carrying a `service/1` member keeps the same batch tag and carries the member's own
    // framing, so a reader walking it meets `er.record/2` and refuses there — by the name of the
    // framing it does not know, and at one tag rather than two.
    let mixed = batch_comparison_bytes(&key, &[kernel, service]).expect("encodes");
    let text = String::from_utf8(mixed).expect("utf-8");
    assert_eq!(
        record_framing(text.as_bytes()).expect("reads"),
        "er.batch/1"
    );
    assert!(text.contains("\"er.record/1\"") && text.contains("\"er.record/2\""));
}
