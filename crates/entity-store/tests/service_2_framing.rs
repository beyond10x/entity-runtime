//! Literal compatibility controls for the `service/2` record and request domains.

use entity_core::{Registry, Runtime};
use entity_store::{
    asynchronous::{
        batch_comparison_bytes, original_request_comparison_bytes, read_record_in_domain,
        record_comparison_bytes, record_domain, record_framing, request_domain, AppendMember,
        BatchKey, RecordedEntry,
    },
    Expect, RecordedCommit, Recording,
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

fn entry(semantics: &str, id: &str, record_id: &str) -> RecordedEntry {
    let definition = serde_json::from_value(json!({
        "entity": format!("probe-{semantics}"),
        "version": 1,
        "semantics": semantics,
        "schema": { "fields": {
            "fixed": { "type": "string", "required": true },
            "note": { "type": "string" }
        }},
        "lifecycle": { "initial": "Held", "states": ["Held"] },
        "create": if semantics == "service/2" { json!({
            "arguments": { "fields": {
                "fixed": { "type": "string", "required": true },
                "bound": { "type": "object", "required": true,
                    "properties": { "note": { "type": "string" } } }
            }},
            "outcomes": [{
                "name": "accepted", "effect": "creates",
                "set": { "fixed": "$args.fixed" },
                "set_if_present": { "note": { "argument": "bound.note" } }
            }]
        }) } else { json!({}) }
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition registers");
    let input = if semantics == "service/2" {
        json!({"fixed": "fixed", "bound": {"note": "present"}})
    } else {
        json!({"fixed": "fixed"})
    };
    let decision = Runtime::new(&registry)
        .create(&format!("probe-{semantics}"), 1, id, input)
        .expect("creation succeeds");
    RecordedEntry::Decision(RecordedCommit::new(decision, &recording(record_id)).expect("records"))
}

#[test]
fn an_er_record_2_reader_refuses_er_record_3_before_reading_its_payload() {
    let unreadable = br#"["er.record/3",{"kind":"#;
    let error = read_record_in_domain("er.record/2", unreadable)
        .expect_err("a reader capped at /2 refuses the /3 tag");
    let message = error.to_string();
    assert!(message.contains("er.record/2") && message.contains("er.record/3"));
}

#[test]
fn an_er_request_2_reader_refuses_er_request_3_before_reading_its_payload() {
    let unreadable = br#"["er.request/3",{"kind":"#;
    let error = read_record_in_domain("er.request/2", unreadable)
        .expect_err("a reader capped at /2 refuses the /3 tag");
    let message = error.to_string();
    assert!(message.contains("er.request/2") && message.contains("er.request/3"));
}

#[test]
fn existing_record_and_request_literal_tags_are_unchanged() {
    assert_eq!(
        record_framing(br#"["er.record/1",{}]"#).unwrap(),
        "er.record/1"
    );
    assert_eq!(
        record_framing(br#"["er.record/2",{}]"#).unwrap(),
        "er.record/2"
    );
    assert_eq!(
        record_framing(br#"["er.request/1",{}]"#).unwrap(),
        "er.request/1"
    );
    assert_eq!(
        record_framing(br#"["er.request/2",{}]"#).unwrap(),
        "er.request/2"
    );
}

#[test]
fn service_2_uses_literal_record_3_and_request_3_framing() {
    let entry = entry("service/2", "p-3", "r-3");
    assert_eq!(record_domain(&entry), "er.record/3");
    assert_eq!(request_domain(&entry), "er.request/3");
    let record = record_comparison_bytes(&entry).expect("record vector");
    let request = original_request_comparison_bytes(&entry).expect("request vector");
    assert!(read_record_in_domain("er.record/2", &record).is_err());
    assert!(read_record_in_domain("er.request/2", &request).is_err());
    let record_literal = include_bytes!("fixtures/service_2_record_3.json");
    assert_eq!(
        record.as_slice(),
        record_literal.strip_suffix(b"\n").unwrap_or(record_literal)
    );
    assert_eq!(
        request,
        br#"["er.request/3",{"arguments":{"bound":{"note":"present"},"fixed":"fixed"},"definition_version":1,"kind":"create","recording":{"actor":null,"causation":null,"correlation":null,"record_id":"r-3","recorded_at":"2026-09-16T00:00:00Z"},"subject":["probe-service/2","p-3"]}]"#
    );
}

#[test]
fn mixed_record_domains_remain_members_of_er_batch_1() {
    let kernel = {
        let definition = serde_json::from_value(json!({
            "entity": "kernel", "version": 1,
            "schema": { "fields": { "fixed": { "type": "string", "required": true } } },
            "lifecycle": { "initial": "Held", "states": ["Held"] }
        }))
        .unwrap();
        let mut registry = Registry::new();
        registry.register(definition).unwrap();
        let decision = Runtime::new(&registry)
            .create("kernel", 1, "k-1", json!({"fixed": "fixed"}))
            .unwrap();
        RecordedEntry::Decision(RecordedCommit::new(decision, &recording("r-1")).unwrap())
    };
    let service_1 = entry("service/1", "p-2", "r-2");
    let service_2 = entry("service/2", "p-3", "r-3");
    let members = [kernel, service_1, service_2].map(|entry| {
        let request = original_request_comparison_bytes(&entry).unwrap();
        AppendMember::new(Expect::Absent, entry, request)
    });
    let bytes = batch_comparison_bytes(&BatchKey::Named("mixed".to_owned()), &members).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert_eq!(record_framing(text.as_bytes()).unwrap(), "er.batch/1");
    for domain in ["er.record/1", "er.record/2", "er.record/3"] {
        assert!(text.contains(domain), "missing {domain}: {text}");
    }
}
