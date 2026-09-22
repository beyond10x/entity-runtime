//! Which framing a decision's comparison bytes are written in, and what a reader that does not
//! know a framing does about it.
//!
//! The governing rule for these formats is that a change to their shape, framing or scalar spelling
//! needs a new version domain and explicit migration. A `service/1` decision's record carries keys
//! `er.record/1` has never carried, so it takes one; a `kernel/1` decision keeps every byte it had.

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use entity_core::{DecisionCommand, Registry, Runtime};
use entity_store::{
    asynchronous::{
        batch_comparison_bytes, original_request_comparison_bytes, read_record_in_domain,
        record_comparison_bytes, record_domain, record_framing, request_domain,
        verify_subject_prefix, AppendMember, AppendRequest, AsyncRecordedReader,
        AsyncRecordedWriter, BatchKey, MemoryRecordedStore, RecordedEntry,
    },
    Expect, RecordedCommit, Recording,
};
use serde_json::{json, Value};

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

fn service_registry() -> Registry {
    registry_of(json!({
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
    }))
}

fn service_entry() -> RecordedEntry {
    let registry = service_registry();
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

fn derived_service_entry() -> RecordedEntry {
    let registry = service_registry();
    let decision = Runtime::new(&registry)
        .create_derived(
            "invoice",
            1,
            json!({ "input": { "invoice_id": "INV-1", "amount": 10 } }),
        )
        .expect("creation derives its address");
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
fn a_derived_identity_keeps_the_existing_record_and_original_request_bytes() {
    let supplied = service_entry();
    let derived = derived_service_entry();
    assert_eq!(
        record_comparison_bytes(&derived).expect("derived record encodes"),
        record_comparison_bytes(&supplied).expect("supplied record encodes")
    );
    assert_eq!(
        original_request_comparison_bytes(&derived).expect("derived request encodes"),
        original_request_comparison_bytes(&supplied).expect("supplied request encodes")
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

// --- § 1.2: the branchless `service/1` creation, and the four readers of one test -----------------

/// A `service/1` definition whose `create` declares no `outcomes`, which registration admits, and
/// one of whose fields is defaulted so the normalization half is measurable.
fn branchless_registry() -> Registry {
    registry_of(json!({
        "entity": "probe",
        "version": 1,
        "semantics": "service/1",
        "schema": { "fields": {
            "title": { "type": "string", "required": true },
            "count": { "type": "integer", "default": 0 }
        }},
        "lifecycle": { "initial": "held", "states": ["held"] }
    }))
}

fn branchless_entry(registry: &Registry, id: &str, record_id: &str, input: Value) -> RecordedEntry {
    let decision = Runtime::new(registry)
        .create("probe", 1, id, input)
        .expect("creation succeeds");
    RecordedEntry::Decision(RecordedCommit::new(decision, &recording(record_id)).expect("valid"))
}

fn request_text(entry: &RecordedEntry) -> String {
    String::from_utf8(original_request_comparison_bytes(entry).expect("encodes")).expect("utf-8")
}

#[test]
fn a_branchless_service_1_creation_reconstructs_the_normalized_input_the_caller_sent() {
    let registry = branchless_registry();
    let one = branchless_entry(&registry, "p-1", "r-1", json!({ "title": "one" }));
    let two = branchless_entry(&registry, "p-1", "r-1", json!({ "title": "two" }));

    // The framing does not move: a `service/1` record is a `service/1` record whether or not its
    // creation declares branches, and narrowing it to `/1` would spell a `/2` shape in a domain
    // that does not have it.
    assert_eq!(record_domain(&one), "er.record/2");
    assert_eq!(request_domain(&one), "er.request/2");

    let first = request_text(&one);
    let second = request_text(&two);
    assert_ne!(
        first, second,
        "two creations that differ in what the caller sent reconstruct to different requests"
    );
    assert!(
        first.contains("\"one\""),
        "the reconstruction carries what the caller sent: {first}"
    );
    assert!(
        !first.contains("\"fields\""),
        "a service/1 creation reconstructs its arguments, in the shape er.request/2 declares: {first}"
    );

    // Repeated input is one request: the same caller input under the same recording is byte for
    // byte the same reconstruction, which is what makes an exact retry recoverable.
    let again = branchless_entry(&registry, "p-1", "r-1", json!({ "title": "one" }));
    assert_eq!(first, request_text(&again));

    // And the normalization is the declared one: a defaulted field spelled out loud is the same
    // request, and another value for it is not.
    let defaulted = branchless_entry(
        &registry,
        "p-1",
        "r-1",
        json!({ "title": "one", "count": 0 }),
    );
    assert_eq!(first, request_text(&defaulted));
    assert!(
        first.contains("\"count\":0"),
        "the reconstruction carries the normalized arguments: {first}"
    );
    let other = branchless_entry(
        &registry,
        "p-1",
        "r-1",
        json!({ "title": "one", "count": 1 }),
    );
    assert_ne!(first, request_text(&other));
}

#[test]
fn the_framing_the_creation_path_replay_and_the_verifier_read_one_service_1_test() {
    // Four readers decide whether a `service/1` creation's input is its arguments; this asserts
    // they answer alike for a creation that declares no branches and for one that does.
    for entry in [
        branchless_entry(
            &branchless_registry(),
            "p-1",
            "r-1",
            json!({ "title": "one" }),
        ),
        service_entry(),
    ] {
        let RecordedEntry::Decision(commit) = &entry else {
            panic!("a decision")
        };
        let record = commit.envelope.record.clone();

        // 1. The framing.
        assert_eq!(record_domain(&entry), "er.record/2");
        assert_eq!(request_domain(&entry), "er.request/2");

        // 2. The creation path: the record carries the caller's input under `arguments`.
        let DecisionCommand::Create { arguments, .. } = &record.command else {
            panic!("a creation")
        };
        assert!(
            !arguments.is_empty(),
            "a service/1 creation records what the caller sent"
        );

        // 3. Replay reruns the decision from those arguments and byte-compares the record.
        entity_core::replay(std::slice::from_ref(&record)).expect("the record replays");

        // 4. The anchored verifier reruns it on the way into a store.
        let store = MemoryRecordedStore::new();
        let subject = entry.subject();
        let request = AppendRequest::new(
            BatchKey::SingleRecord(entry.record_id().to_owned()),
            vec![AppendMember::new(
                Expect::Absent,
                entry.clone(),
                original_request_comparison_bytes(&entry).expect("encodes"),
            )],
        )
        .expect("append request is valid");
        block_on(AsyncRecordedWriter::append(&store, request)).expect("the verifier accepts it");
        let history = block_on(AsyncRecordedReader::history(&store, &subject)).expect("history");
        verify_subject_prefix(&history, entry.record_id()).expect("the stored history verifies");
    }
}

#[test]
fn a_service_1_creation_record_whose_arguments_do_not_produce_its_fields_is_refused() {
    // The two halves are redundant by construction, and that is what makes a forged one detectable:
    // a record claiming one input and carrying another instance's fields is refused by the same
    // recomputation on both paths, rather than being reconstructed into a request nobody sent.
    let registry = branchless_registry();
    let mut decision = Runtime::new(&registry)
        .create("probe", 1, "p-1", json!({ "title": "one" }))
        .expect("creation succeeds");
    let DecisionCommand::Create { arguments, .. } = &mut decision.record.command else {
        panic!("a creation")
    };
    arguments.insert("title".to_owned(), json!("two"));

    let record = decision.record.clone();
    entity_core::replay(std::slice::from_ref(&record))
        .expect_err("the fields are not what these arguments produce");

    let entry =
        RecordedEntry::Decision(RecordedCommit::new(decision, &recording("r-1")).expect("valid"));
    let store = MemoryRecordedStore::new();
    let request = AppendRequest::new(
        BatchKey::SingleRecord("r-1".to_owned()),
        vec![AppendMember::new(
            Expect::Absent,
            entry.clone(),
            original_request_comparison_bytes(&entry).expect("encodes"),
        )],
    )
    .expect("append request is valid");
    block_on(AsyncRecordedWriter::append(&store, request))
        .expect_err("the anchored verifier reruns the saved command and compares");
}
