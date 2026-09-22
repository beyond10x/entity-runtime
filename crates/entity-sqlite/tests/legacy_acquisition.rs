//! Consistent acquisition of the retained SQLite provider format.

use std::path::{Path, PathBuf};

use entity_core::{Registry, Runtime};
use entity_sqlite::SqliteStore;
use entity_store::{
    asynchronous::{HistoryOrigin, KnownLegacyOrder, LegacyEvidence, LegacyOrderDeclaration},
    Expect, LegacyStoreSource, RecordedCommit, RecordedObservation, Recording, Store,
};

fn scratch(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("sqlite-legacy-acquisition")
        .join(format!("{name}.db"));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("scratch parent");
    }
    let _ = std::fs::remove_file(&path);
    path
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
        correlation: Some("sqlite-acquisition".to_owned()),
        causation: None,
        actor: None,
    }
}

#[test]
fn sqlite_acquisition_retains_mixed_history_without_inventing_event_order() {
    let path = scratch("mixed");
    let registry = registry();
    let runtime = Runtime::new(&registry);
    let mut source = SqliteStore::open(&path).expect("SQLite source");
    let created = runtime
        .create("ticket", 1, "one", serde_json::json!({"title":"one"}))
        .expect("creation");
    source
        .commit(&created, Expect::Absent)
        .expect("legacy creation");
    let legacy_touch = runtime
        .execute(&created.instance, "touch", serde_json::json!({}))
        .expect("legacy touch");
    source
        .commit(&legacy_touch, Expect::Revision(1))
        .expect("legacy write");
    let recorded_touch = runtime
        .execute(&legacy_touch.instance, "touch", serde_json::json!({}))
        .expect("recorded touch");
    let recorded =
        RecordedCommit::new(recorded_touch, &recording("touch-3")).expect("complete decision");
    source
        .commit_recorded(&recorded, Expect::Revision(2))
        .expect("recorded write");
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
        .acquire_legacy("sqlite/source-a")
        .expect("consistent capture");
    let [history] = snapshot.histories.as_slice() else {
        panic!("one complete subject expected")
    };
    let HistoryOrigin::Imported(anchor) = &history.origin else {
        panic!("imported boundary expected")
    };
    assert_eq!(anchor.instance, recorded.instance);
    assert_eq!(anchor.order, LegacyOrderDeclaration::PerKindOnly);
    assert_eq!(anchor.evidence.len(), 3);
    let LegacyEvidence::Envelope(decision) = &anchor.evidence[0] else {
        panic!("decision envelope expected")
    };
    assert_eq!(decision.known_order, KnownLegacyOrder::PerKind(0));
    let LegacyEvidence::Envelope(saved_observation) = &anchor.evidence[1] else {
        panic!("observation envelope expected")
    };
    assert_eq!(saved_observation.known_order, KnownLegacyOrder::PerKind(0));
    assert!(matches!(anchor.evidence[2], LegacyEvidence::Event(_)));
}

#[test]
fn sqlite_acquisition_refuses_evidence_without_terminal_state() {
    let path = scratch("orphan");
    drop(SqliteStore::open(&path).expect("schema"));
    let connection = rusqlite::Connection::open(&path).expect("raw fixture");
    connection
        .execute(
            "INSERT INTO events (entity, id, revision, position, document)
             VALUES ('ticket', 'orphan', 1, 0, '{}')",
            [],
        )
        .expect("orphan fixture");
    drop(connection);
    let mut source = SqliteStore::open(&path).expect("source");
    assert!(
        source.acquire_legacy("sqlite/orphan").is_err(),
        "evidence without its terminal instance cannot be silently omitted"
    );
}
