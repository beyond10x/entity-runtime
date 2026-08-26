//! One suite, run against every provider in this crate.
//!
//! Two of the three, to be exact: `entity-sqlite` depends on this crate, so it cannot be a
//! dependency of these tests. It runs the same suite against itself, in
//! `crates/entity-sqlite/tests/conformance.rs`. The suite is the shared thing; where it is invoked
//! from is not.
//!
//! The SPI's whole value is that a caller can swap the thing underneath. That is a claim about
//! *agreement*, and a claim about agreement checked against one implementation is not checked at
//! all — so every case here runs against both, and a provider that drifted fails the same case the
//! other passes.
//!
//! `CARGO_TARGET_TMPDIR` rather than `/tmp`: the tmpfs on at least one machine here drops writes
//! under pressure, and a store test that loses a file tests nothing while looking like it passed.

use std::path::{Path, PathBuf};

use entity_core::{Decision, Registry, Runtime};
use entity_store::{
    EventProvider, Expect, FileStore, MemoryStore, StateProvider, Store, StoreError,
};

fn registry() -> Registry {
    let definition = serde_json::from_value(serde_json::json!({
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
    .expect("the definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("it validates");
    registry
}

fn opening(registry: &Registry) -> Decision {
    Runtime::new(registry)
        .create(
            "ticket",
            1,
            "one",
            serde_json::json!({ "title": "A ticket" }),
        )
        .expect("creation is permitted")
}

fn scratch(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("providers")
        .join(name);
    let _ = std::fs::remove_dir_all(&root);
    root
}

/// Runs `case` against every provider, naming the one that failed.
fn for_each_provider(name: &str, case: impl Fn(&mut dyn Store, &str)) {
    let mut memory = MemoryStore::new();
    case(&mut memory, "MemoryStore");

    let mut file = FileStore::open(scratch(name));
    case(&mut file, "FileStore");
}

#[test]
fn a_committed_instance_reads_back_with_its_events() {
    let registry = registry();
    for_each_provider("read-back", |store, provider| {
        let created = opening(&registry);
        store
            .commit(&created, Expect::Absent)
            .unwrap_or_else(|error| panic!("{provider}: creation must be accepted: {error}"));

        let loaded = store
            .load("ticket", "one")
            .unwrap_or_else(|error| panic!("{provider}: {error}"))
            .unwrap_or_else(|| panic!("{provider}: the instance must be there"));
        assert_eq!(loaded, created.instance, "{provider}");

        let closed = Runtime::new(&registry)
            .execute(&loaded, "close", serde_json::json!({}))
            .expect("closing an open ticket is permitted");
        store
            .commit(&closed, Expect::Revision(1))
            .unwrap_or_else(|error| panic!("{provider}: {error}"));

        let events = store
            .events("ticket", "one")
            .unwrap_or_else(|error| panic!("{provider}: {error}"));
        assert_eq!(events.len(), 1, "{provider}");
        assert_eq!(events[0].event_type, "TicketClosed", "{provider}");
        assert_eq!(events[0].revision, 2, "{provider}");
    });
}

#[test]
fn every_provider_refuses_a_stale_write_the_same_way() {
    let registry = registry();
    for_each_provider("stale", |store, provider| {
        let created = opening(&registry);
        store.commit(&created, Expect::Absent).expect("accepted");

        let loaded = store.load("ticket", "one").expect("answers").expect("held");
        let first = Runtime::new(&registry)
            .execute(&loaded, "close", serde_json::json!({}))
            .expect("permitted");
        let second = Runtime::new(&registry)
            .execute(&loaded, "close", serde_json::json!({}))
            .expect("permitted");

        store
            .commit(&first, Expect::Revision(1))
            .unwrap_or_else(|error| panic!("{provider}: the first writer wins: {error}"));

        match store.commit(&second, Expect::Revision(1)) {
            Err(StoreError::RevisionConflict {
                expected, found, ..
            }) => {
                assert_eq!(expected, Expect::Revision(1), "{provider}");
                assert_eq!(found, Some(2), "{provider}");
            }
            Ok(()) => panic!("{provider}: a stale write must not be accepted"),
            Err(other) => panic!("{provider}: expected a conflict, got {other}"),
        }

        // And exactly one event landed, not two.
        assert_eq!(
            store.events("ticket", "one").expect("events").len(),
            1,
            "{provider}"
        );
    });
}

#[test]
fn every_provider_leaves_a_refused_commit_with_no_trace() {
    let registry = registry();
    for_each_provider("no-trace", |store, provider| {
        let created = opening(&registry);
        store.commit(&created, Expect::Absent).expect("accepted");

        let loaded = store.load("ticket", "one").expect("answers").expect("held");
        let closed = Runtime::new(&registry)
            .execute(&loaded, "close", serde_json::json!({}))
            .expect("permitted");

        store
            .commit(&closed, Expect::Revision(99))
            .expect_err("99 is not what is stored");

        let after = store.load("ticket", "one").expect("answers").expect("held");
        assert_eq!(after.revision, 1, "{provider}: the state did not move");
        assert_eq!(after.lifecycle_state, "open", "{provider}");
        assert!(
            store.events("ticket", "one").expect("events").is_empty(),
            "{provider}: a refused commit appended nothing"
        );
    });
}

#[test]
fn every_provider_answers_absent_for_something_nobody_stored() {
    for_each_provider("absent", |store, provider| {
        assert_eq!(
            store.load("ticket", "nothing").expect("answers"),
            None,
            "{provider}"
        );
        assert!(
            store
                .events("ticket", "nothing")
                .expect("answers")
                .is_empty(),
            "{provider}"
        );
    });
}

#[test]
fn the_file_store_survives_being_reopened() {
    // The difference that matters between the two providers, asserted rather than assumed: one
    // forgets when the process exits and the other does not.
    let registry = registry();
    let root = scratch("reopen");

    {
        let mut store = FileStore::open(&root);
        let created = opening(&registry);
        store.commit(&created, Expect::Absent).expect("accepted");
        let loaded = store.load("ticket", "one").expect("answers").expect("held");
        let closed = Runtime::new(&registry)
            .execute(&loaded, "close", serde_json::json!({}))
            .expect("permitted");
        store
            .commit(&closed, Expect::Revision(1))
            .expect("accepted");
    }

    let reopened = FileStore::open(&root);
    let loaded = reopened
        .load("ticket", "one")
        .expect("answers")
        .expect("held");
    assert_eq!(loaded.lifecycle_state, "closed");
    assert_eq!(loaded.revision, 2);
    assert_eq!(reopened.events("ticket", "one").expect("events").len(), 1);
}

#[test]
fn a_retried_commit_appends_its_events_once() {
    // `FileStore` writes events before the state, so a state write that fails leaves the
    // expectation unchanged — and the retry any caller is entitled to make used to append the same
    // events a second time, producing a log that no longer folds. ENOSPC or EIO is enough.
    let directory = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("retry-once");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a directory");

    // Its own definition: this case needs an event at revision 1, and the shared fixture emits
    // none on create.
    let definition = serde_json::from_value(serde_json::json!({
        "entity": "ticket",
        "version": 1,
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "open", "states": ["open", "closed"] },
        "create": { "emit": { "type": "TicketOpened", "payload": { "ticket": "$id" } } },
        "operations": {
            "close": { "transitions": [{ "from": "open", "to": "closed" }] }
        }
    }))
    .expect("the definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("it validates");
    let runtime = Runtime::new(&registry);
    let mut store = FileStore::open(&directory);

    let created = runtime
        .create(
            "ticket",
            1,
            "one",
            serde_json::json!({ "title": "A ticket" }),
        )
        .expect("permitted");

    // The first attempt: make the state write fail while the event append succeeds.
    use std::os::unix::fs::PermissionsExt as _;
    let entity_directory = directory.join("ticket");
    std::fs::create_dir_all(&entity_directory).expect("a directory");
    // Seed the events file so appending needs no new directory entry.
    std::fs::write(entity_directory.join("one.events.jsonl"), "").expect("seeded");
    let mut locked = std::fs::metadata(&entity_directory)
        .expect("metadata")
        .permissions();
    locked.set_mode(0o555);
    std::fs::set_permissions(&entity_directory, locked).expect("read-only");

    store
        .commit(&created, Expect::Absent)
        .expect_err("the state write cannot land");

    let mut open = std::fs::metadata(&entity_directory)
        .expect("metadata")
        .permissions();
    open.set_mode(0o755);
    std::fs::set_permissions(&entity_directory, open).expect("writable again");

    // The retry, which is what a caller seeing a backend failure is entitled to do.
    store.commit(&created, Expect::Absent).expect("accepted");

    let events = store.events("ticket", "one").expect("events");
    let revisions: Vec<u64> = events.iter().map(|event| event.revision).collect();
    assert_eq!(
        revisions,
        vec![1],
        "the retry appended nothing the log already had"
    );
}
