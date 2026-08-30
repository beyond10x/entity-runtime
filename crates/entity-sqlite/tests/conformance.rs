//! The transactional provider, held to the same suite as every other — and to the promise the
//! others cannot make.

use entity_core::{Decision, DomainEvent, Registry, Runtime};
use entity_sqlite::SqliteStore;
use entity_store::{conformance, EventProvider, Expect, StateProvider, Store, StoreError};
use serde_json::json;

/// The one definition these tests use.
fn registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "ticket",
        "version": 1,
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "open", "states": ["open", "closed"] },
        "create": { "emit": { "type": "TicketOpened", "payload": { "ticket": "$id" } } },
        "operations": {
            "close": {
                "transitions": [{ "from": "open", "to": "closed" }],
                "emits": [{ "type": "TicketClosed", "payload": { "ticket": "$id" } }]
            }
        }
    }))
    .expect("parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("validates");
    registry
}

#[test]
fn the_sqlite_provider_conforms() {
    let mut store = SqliteStore::in_memory().expect("a database");
    let report = conformance::run(&mut store);
    assert!(report.is_clean(), "SqliteStore:\n{}", report.summary());
    conformance::verify_recorded(&mut store).expect("SqliteStore recorded history");
    let batch = conformance::run_atomic(&mut store);
    assert!(batch.is_clean(), "SqliteStore batch:\n{}", batch.summary());
}

#[test]
fn it_survives_being_closed_and_reopened() {
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("reopen.sqlite3");
    let _ = std::fs::remove_file(&path);

    let definition = serde_json::from_value(json!({
        "entity": "ticket",
        "version": 1,
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "open", "states": ["open", "closed"] },
        "operations": {
            "close": {
                "transitions": [{ "from": "open", "to": "closed" }],
                "emits": [{ "type": "TicketClosed", "payload": { "ticket": "$id" } }]
            }
        }
    }))
    .expect("parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("validates");
    let runtime = Runtime::new(&registry);

    {
        let mut store = SqliteStore::open(&path).expect("a database");
        let created = runtime
            .create("ticket", 1, "one", json!({ "title": "A ticket" }))
            .expect("permitted");
        store.commit(&created, Expect::Absent).expect("accepted");
        let closed = runtime
            .execute(&created.instance, "close", json!({}))
            .expect("permitted");
        store
            .commit(&closed, Expect::Revision(1))
            .expect("accepted");
    }

    let reopened = SqliteStore::open(&path).expect("a database");
    let loaded = reopened
        .load("ticket", "one")
        .expect("answers")
        .expect("held");
    assert_eq!(loaded.lifecycle_state, "closed");
    assert_eq!(loaded.revision, 2);
    assert_eq!(reopened.events("ticket", "one").expect("events").len(), 1);
}

#[test]
fn a_refused_commit_rolls_back_both_halves() {
    // A refusal at the pre-check proves nothing about rollback: it happens before either write, so
    // there are no halves to roll back — and that assertion passes verbatim against `FileStore`,
    // the provider whose own documentation says it cannot make this promise. This asserts the case
    // that does: the instance write lands, the event write then fails, and neither survives.
    //
    // The event write is made to fail by a trigger on the events table, added through a second
    // connection once the instance holds two revisions: the instance row has been written by the
    // time the append runs, and the trigger refuses it. (It used to be made to fail by planting an
    // event at a `(revision, position)` the commit would reuse; positions now continue from what
    // the log holds, so nothing planted can collide.)
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("rollback.sqlite3");
    let _ = std::fs::remove_file(&path);
    let mut store = SqliteStore::open(&path).expect("a database");
    let registry = registry();
    let runtime = Runtime::new(&registry);

    let created = runtime
        .create("ticket", 1, "one", json!({ "title": "A ticket" }))
        .expect("permitted");
    store.commit(&created, Expect::Absent).expect("accepted");
    let closed = runtime
        .execute(&created.instance, "close", json!({}))
        .expect("permitted");
    store
        .commit(&closed, Expect::Revision(1))
        .expect("accepted");

    rusqlite::Connection::open(&path)
        .expect("a second connection")
        .execute_batch(
            "CREATE TRIGGER refuse_events BEFORE INSERT ON events \
             BEGIN SELECT RAISE(ABORT, 'the events table refuses'); END;",
        )
        .expect("the trigger is created");

    // Revision 3, carrying the event the trigger will refuse.
    let mut moved = closed.instance.clone();
    moved.revision = 3;
    let colliding = Decision::legacy_import(
        moved,
        vec![DomainEvent {
            entity: "ticket".to_owned(),
            version: 1,
            id: "one".to_owned(),
            revision: 3,
            event_type: "TicketClosed".to_owned(),
            from_state: Some("closed".to_owned()),
            to_state: "closed".to_owned(),
            changed: serde_json::Map::new(),
            args: serde_json::Map::new(),
            payload: json!({ "ticket": "one" }),
        }],
    );

    let error = store
        .commit(&colliding, Expect::Revision(2))
        .expect_err("the event write is refused by the trigger");
    assert!(
        !matches!(error, StoreError::RevisionConflict { .. }),
        "this is not a conflict: the check passed and the failure came after it: {error}"
    );

    let after = store.load("ticket", "one").expect("answers").expect("held");
    assert_eq!(
        after.revision, 2,
        "the instance write was rolled back with the event write that failed"
    );
    assert_eq!(
        store.events("ticket", "one").expect("events").len(),
        2,
        "and the refused event was not left behind beside the two that were already there"
    );
}
