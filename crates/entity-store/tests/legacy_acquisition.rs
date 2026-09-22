//! Exact read-only acquisition of the retained File Store format.

use std::path::{Path, PathBuf};

use entity_core::{Registry, Runtime};
use entity_store::{
    asynchronous::{
        HistoryOrigin, KnownLegacyOrder, LegacyEvidence, LegacyOrderDeclaration, RecordedEntry,
    },
    Expect, FileStore, LegacyStoreSnapshot, LegacyStoreSource, RecordedCommit, RecordedObservation,
    Recording, Store,
};

fn scratch(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("legacy-acquisition")
        .join(name);
    let _ = std::fs::remove_dir_all(&root);
    root
}

fn registry() -> Registry {
    let definition = serde_json::from_value(serde_json::json!({
        "entity": "ticket",
        "version": 1,
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "open", "states": ["open"] },
        "operations": {
            "touch": {
                "transitions": [{ "from": "open", "to": "open" }],
                "emits": [{ "type": "TicketTouched", "payload": { "ticket": "$id" } }]
            }
        }
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    registry
}

fn recording(record_id: &str) -> Recording {
    Recording {
        record_id: record_id.to_owned(),
        recorded_at: "2026-09-16T00:00:00Z".to_owned(),
        correlation: Some("legacy-acquisition".to_owned()),
        causation: None,
        actor: None,
    }
}

#[test]
fn acquiring_an_absent_file_source_creates_no_bytes() {
    let root = scratch("absent");
    let mut source = FileStore::open(&root);
    let snapshot = source
        .acquire_legacy("file/absent")
        .expect("an absent source is an empty boundary");
    assert!(snapshot.histories.is_empty());
    assert!(
        !root.exists(),
        "read-only acquisition must not create a root"
    );
}

#[test]
fn file_acquisition_retains_complete_envelopes_and_only_known_order() {
    let root = scratch("mixed");
    let registry = registry();
    let runtime = Runtime::new(&registry);
    let mut source = FileStore::open(&root);
    let created = runtime
        .create("ticket", 1, "one", serde_json::json!({"title":"one"}))
        .expect("creation");
    source
        .commit(&created, Expect::Absent)
        .expect("legacy creation");
    let touched = runtime
        .execute(&created.instance, "touch", serde_json::json!({}))
        .expect("legacy touch");
    source
        .commit(&touched, Expect::Revision(1))
        .expect("legacy event write");
    let recorded_touch = runtime
        .execute(&touched.instance, "touch", serde_json::json!({}))
        .expect("recorded touch");
    let recorded = RecordedCommit::new(recorded_touch, &recording("touch-3"))
        .expect("complete decision envelope");
    source
        .commit_recorded(&recorded, Expect::Revision(2))
        .expect("recorded decision");
    let observation = RecordedObservation {
        entity: "ticket".to_owned(),
        id: "one".to_owned(),
        revision: 3,
        envelope: recording("observation-3")
            .seal(serde_json::json!({"seen":true}))
            .expect("observation envelope"),
    };
    source.observe(&observation).expect("observation");

    let snapshot = source
        .acquire_legacy("file/source-a")
        .expect("consistent source capture");
    assert_eq!(snapshot.histories.len(), 1);
    let history = &snapshot.histories[0];
    assert_eq!(history.subject.entity, "ticket");
    assert_eq!(history.subject.id, "one");
    assert!(history.records.is_empty());
    let HistoryOrigin::Imported(anchor) = &history.origin else {
        panic!("legacy acquisition must declare its boundary")
    };
    assert_eq!(anchor.instance, recorded.instance);
    assert_eq!(anchor.order, LegacyOrderDeclaration::PerKindOnly);
    assert_eq!(anchor.evidence.len(), 3);
    let LegacyEvidence::Envelope(decision) = &anchor.evidence[0] else {
        panic!("the complete decision envelope is retained")
    };
    assert_eq!(decision.entry, RecordedEntry::Decision(recorded));
    assert_eq!(decision.source_id, "file/source-a");
    assert_eq!(decision.known_order, KnownLegacyOrder::PerKind(0));
    let LegacyEvidence::Envelope(saved_observation) = &anchor.evidence[1] else {
        panic!("the complete observation envelope is retained")
    };
    assert_eq!(
        saved_observation.entry,
        RecordedEntry::Observation(observation)
    );
    assert_eq!(saved_observation.known_order, KnownLegacyOrder::PerKind(0));
    let LegacyEvidence::Event(event) = &anchor.evidence[2] else {
        panic!("the bare legacy event remains explicit")
    };
    assert_eq!(event.event_type, "TicketTouched");
    assert_eq!(event.revision, 2);
}

#[test]
fn acquisition_document_refuses_one_global_record_identity_in_two_subjects() {
    let registry = registry();
    let mut histories = Vec::new();
    for id in ["first", "second"] {
        let root = scratch(&format!("duplicate-global-{id}"));
        let mut source = FileStore::open(&root);
        let decision = Runtime::new(&registry)
            .create("ticket", 1, id, serde_json::json!({"title":id}))
            .expect("creation");
        let recorded = RecordedCommit::new(decision, &recording("same-global-record"))
            .expect("recorded creation");
        source
            .commit_recorded(&recorded, Expect::Absent)
            .expect("isolated source accepts its record");
        histories.extend(
            source
                .acquire_legacy("file/combined")
                .expect("isolated acquisition")
                .histories,
        );
    }
    assert!(
        LegacyStoreSnapshot::new("file/combined", histories).is_err(),
        "one imported document cannot reserve a global record identity twice"
    );
}

#[test]
fn file_acquisition_refuses_a_stale_publication_intent() {
    fn hex(value: &str) -> String {
        value
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    let root = scratch("stale-publication");
    let mut source = FileStore::open(&root);
    let decision = Runtime::new(&registry())
        .create(
            "ticket",
            1,
            "settled",
            serde_json::json!({"title":"settled"}),
        )
        .expect("creation");
    source
        .commit(&decision, Expect::Absent)
        .expect("settled subject");
    let entity_directory = root.join("subjects").join(hex("ticket"));
    std::fs::write(
        entity_directory.join(format!("{}.json.writing.999999.1", hex("unsettled"))),
        b"unsettled bytes",
    )
    .expect("stale publication intent");
    assert!(
        source.acquire_legacy("file/stale-publication").is_err(),
        "acquisition must not silently turn an interrupted publication into absence"
    );
}
