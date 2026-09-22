//! Independent source examination, pass 1: the original-request reconstruction of a `service/1`
//! definition whose creation declares no branches.
//!
//! § 1.2: *"A `service/1` creation's *original request* is the caller's **arguments**, not the
//! fields the branch produced. Reconstructing it as `"fields"` would hand a retry a request the
//! caller never sent, which is what `original_request_comparison_bytes` exists to prevent."*

use entity_core::{Registry, Runtime};
use entity_store::{
    asynchronous::{original_request_comparison_bytes, request_domain, RecordedEntry},
    RecordedCommit, Recording,
};
use serde_json::{json, Value};

fn recording(record_id: &str) -> Recording {
    Recording {
        record_id: record_id.to_owned(),
        recorded_at: "2026-09-16T00:00:00Z".to_owned(),
        correlation: None,
        causation: None,
        actor: None,
    }
}

/// A `service/1` definition whose `create` declares no `outcomes` — the shape registration admits
/// and the shape every probe in `crates/entity-core/tests/service_values.rs` is built from. Its
/// creation input is read as the **fields**, exactly as a `kernel/1` creation's is, and the record
/// it produces therefore carries no `arguments`.
fn registry() -> Registry {
    let document = json!({
        "entity": "probe",
        "version": 1,
        "semantics": "service/1",
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "held", "states": ["held"] }
    });
    let definition = serde_json::from_value(document).expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    registry
}

fn entry(registry: &Registry, fields: Value) -> RecordedEntry {
    let decision = Runtime::new(registry)
        .create("probe", 1, "p-1", fields)
        .expect("creation succeeds");
    RecordedEntry::Decision(RecordedCommit::new(decision, &recording("r-1")).expect("valid"))
}

/// The framing is chosen from `definition.semantics` alone, while the creation path, `replay` and
/// the anchored verifier all additionally require `!create.outcomes.is_empty()` before they read
/// the input as arguments. Where the two disagree, the reconstruction writes `"arguments":{}` for
/// a request whose whole content was its fields — so two different creations reconstruct to the
/// same bytes, and a retry carrying different fields is accepted as the same request.
#[test]
fn a_service_1_creation_without_branches_reconstructs_the_request_the_caller_sent() {
    let registry = registry();
    let one = entry(&registry, json!({ "title": "one" }));
    let two = entry(&registry, json!({ "title": "two" }));

    assert_eq!(request_domain(&one), "er.request/2");

    let first = original_request_comparison_bytes(&one).expect("encodes");
    let second = original_request_comparison_bytes(&two).expect("encodes");
    assert_ne!(
        String::from_utf8(first.clone()).expect("utf-8"),
        String::from_utf8(second).expect("utf-8"),
        "two creations that differ in what the caller sent reconstruct to different requests"
    );

    let text = String::from_utf8(first).expect("utf-8");
    assert!(
        text.contains("\"one\""),
        "the reconstruction carries what the caller sent: {text}"
    );
}
