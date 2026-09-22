//! Literal compatibility controls for `service/3` record and request domains.

use std::collections::BTreeMap;

use entity_core::{LoadedDecision, OperationFieldAction, PreloadDecision, Registry, Runtime};
use entity_store::{
    asynchronous::{
        batch_comparison_bytes, original_request_comparison_bytes, read_record_in_domain,
        record_comparison_bytes, record_domain, record_framing, request_domain, AppendMember,
        BatchKey, RecordedEntry,
    },
    Expect, RecordedCommit, Recording,
};
use serde_json::json;

fn entry() -> RecordedEntry {
    let definition = serde_json::from_value(json!({
        "entity": "invoice", "version": 1, "semantics": "service/3",
        "schema": { "fields": {
            "issued_at": { "type": "string", "required": true },
            "note": { "type": "string" }
        }},
        "lifecycle": { "initial": "Draft", "states": ["Draft"] },
        "operations": { "Issue": { "outcomes": [{
            "name": "issued", "effect": "updates",
            "fulfills": {
                "issued_at": { "actions": "required" },
                "note": { "actions": "optional" }
            }
        }]}}
    }))
    .unwrap();
    let mut registry = Registry::new();
    registry.register(definition).unwrap();
    let runtime = Runtime::new(&registry);
    let created = runtime
        .create(
            "invoice",
            1,
            "i-1",
            json!({"issued_at": "pending", "note": "remove"}),
        )
        .unwrap();
    let PreloadDecision::Load(prepared) = runtime
        .decide_before_load("invoice", 1, "i-1", "Issue", json!({}))
        .unwrap()
    else {
        panic!("subject is required")
    };
    let LoadedDecision::NeedsFulfillment(prepared) =
        prepared.select_with(&created.instance).unwrap()
    else {
        panic!("selected outcome requires fulfillment")
    };
    let actions = BTreeMap::from([
        (
            "issued_at".to_owned(),
            OperationFieldAction::Set {
                value: json!("2026-09-16T10:00:00Z"),
            },
        ),
        ("note".to_owned(), OperationFieldAction::Remove),
    ]);
    let decision = prepared.complete(actions).unwrap().into_decision().unwrap();
    RecordedEntry::Decision(
        RecordedCommit::new(
            decision,
            &Recording {
                record_id: "r-2".to_owned(),
                recorded_at: "2026-09-16T10:00:00Z".to_owned(),
                correlation: None,
                causation: None,
                actor: None,
            },
        )
        .unwrap(),
    )
}

#[test]
fn old_readers_refuse_record_and_request_4_at_the_literal_tag() {
    for (old, new) in [
        ("er.record/3", b"[\"er.record/4\",{\"kind\":\"".as_slice()),
        ("er.request/3", b"[\"er.request/4\",{\"kind\":\"".as_slice()),
    ] {
        let error = read_record_in_domain(old, new).expect_err("old reader refuses /4");
        let message = error.to_string();
        assert!(message.contains(old) && message.contains(&old.replace('3', "4")));
    }
}

#[test]
fn service_3_uses_record_4_and_request_4_with_exact_ordered_actions() {
    let entry = entry();
    assert_eq!(record_domain(&entry), "er.record/4");
    assert_eq!(request_domain(&entry), "er.request/4");
    let record = record_comparison_bytes(&entry).unwrap();
    assert!(read_record_in_domain("er.record/3", &record).is_err());
    let record_text = String::from_utf8(record).unwrap();
    assert!(record_text.contains(r#""removed":["note"]"#));
    assert!(record_text.contains(
        r#""fulfillments":{"issued_at":{"set":{"value":"2026-09-16T10:00:00Z"}},"note":"remove"}"#
    ));
    let request = original_request_comparison_bytes(&entry).unwrap();
    assert_eq!(
        request,
        br#"["er.request/4",{"arguments":{},"expected_revision":1,"fulfillments":{"issued_at":{"set":{"value":"2026-09-16T10:00:00Z"}},"note":"remove"},"kind":"execute","operation":"Issue","recording":{"actor":null,"causation":null,"correlation":null,"record_id":"r-2","recorded_at":"2026-09-16T10:00:00Z"},"subject":["invoice","i-1"]}]"#
    );
    let member = AppendMember::new(Expect::Absent, entry, request);
    let batch =
        batch_comparison_bytes(&BatchKey::Named("service-3".to_owned()), &[member]).unwrap();
    assert_eq!(record_framing(&batch).unwrap(), "er.batch/1");
    assert!(String::from_utf8(batch).unwrap().contains("er.record/4"));
}
