//! The transactional provider, held to the same suite as every other — and to the promise the
//! others cannot make.

use entity_core::{Registry, Runtime};
use entity_sqlite::SqliteStore;
use entity_store::{conformance, EventProvider, Expect, StateProvider, Store};
use serde_json::json;

#[test]
fn the_sqlite_provider_conforms() {
    let mut store = SqliteStore::in_memory().expect("a database");
    let report = conformance::run(&mut store);
    assert!(report.is_clean(), "SqliteStore:\n{}", report.summary());
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
    // The promise `FileStore` states it cannot make. There it appends events, then writes state,
    // and a crash between the two leaves a recorded fact whose state did not land. Here the check
    // and both writes are one transaction, so a refusal cannot leave half of it behind.
    let mut store = SqliteStore::in_memory().expect("a database");

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

    let created = runtime
        .create("ticket", 1, "one", json!({ "title": "A ticket" }))
        .expect("permitted");
    store.commit(&created, Expect::Absent).expect("accepted");
    let closed = runtime
        .execute(&created.instance, "close", json!({}))
        .expect("permitted");

    store
        .commit(&closed, Expect::Revision(42))
        .expect_err("42 is not what is stored");

    let after = store.load("ticket", "one").expect("answers").expect("held");
    assert_eq!(after.revision, 1, "the state did not move");
    assert_eq!(after.lifecycle_state, "open");
    assert!(
        store.events("ticket", "one").expect("events").is_empty(),
        "and no event was left behind by the half that would have run first"
    );
}
