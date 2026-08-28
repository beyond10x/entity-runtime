//! The transactional provider, held to the same suite as every other — and to the promise the
//! others cannot make.

use entity_core::{Decision, DomainEvent, Registry, Runtime};
use entity_sqlite::SqliteStore;
use entity_store::{
    conformance, EventProvider, Expect, FileStore, StateProvider, Store, StoreError,
};
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
    // The event write is made to fail by handing the commit an event whose `(entity, id, revision,
    // position)` is already in the table. The instance row has been written by then.
    let mut store = SqliteStore::in_memory().expect("a database");
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

    // Revision 3, carrying an event that collides with the one revision 2 already wrote.
    let mut moved = closed.instance.clone();
    moved.revision = 3;
    let colliding = Decision {
        instance: moved,
        events: vec![DomainEvent {
            entity: "ticket".to_owned(),
            version: 1,
            id: "one".to_owned(),
            revision: 2,
            event_type: "TicketClosed".to_owned(),
            from_state: Some("open".to_owned()),
            to_state: "closed".to_owned(),
            changed: serde_json::Map::new(),
            args: serde_json::Map::new(),
            payload: json!({ "ticket": "one" }),
        }],
    };

    let error = store
        .commit(&colliding, Expect::Revision(2))
        .expect_err("the event write collides with the row already there");
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
        "and the colliding event was not left behind beside the two that were already there"
    );
}

#[test]
fn the_rollback_case_is_one_a_non_transactional_store_actually_fails() {
    // The guard on the test above: it must assert something only a transactional store can do.
    // `FileStore` appends events *before* the state write, so a torn write there leaves the event
    // of a commit that did not land. If this test ever starts passing, the one above has stopped
    // being evidence.
    let directory = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("torn-file-store");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a directory");

    let mut store = FileStore::open(&directory);
    let registry = registry();
    let runtime = Runtime::new(&registry);

    let created = runtime
        .create("ticket", 1, "one", json!({ "title": "A ticket" }))
        .expect("permitted");
    store.commit(&created, Expect::Absent).expect("accepted");

    // The state write cannot land, while the event append still can: the instance's directory is
    // made read-only, so the existing events file can still be opened for append but the temporary
    // file the state write renames from cannot be created.
    use std::os::unix::fs::PermissionsExt as _;
    let instance_directory = directory.join("ticket");
    let mut permissions = std::fs::metadata(&instance_directory)
        .expect("metadata")
        .permissions();
    permissions.set_mode(0o555);
    std::fs::set_permissions(&instance_directory, permissions).expect("read-only");

    let closed = runtime
        .execute(&created.instance, "close", json!({}))
        .expect("permitted");
    store
        .commit(&closed, Expect::Revision(1))
        .expect_err("the state write cannot land");

    let events = store.events("ticket", "one").expect("events");

    // Put it back before asserting, so a failure here does not leave an unremovable directory.
    let mut permissions = std::fs::metadata(&instance_directory)
        .expect("metadata")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&instance_directory, permissions).expect("writable again");

    assert_eq!(
        events.len(),
        2,
        "FileStore kept the event of a commit that did not land — the promise it documents that it \
         cannot make, and the one the SQLite test above is evidence for"
    );
}
