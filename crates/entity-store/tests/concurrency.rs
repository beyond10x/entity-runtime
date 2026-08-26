//! The rule this crate exists for: two writers from the same revision, one winner.
//!
//! Driven through the real kernel rather than by hand-building instances, because the thing under
//! test is the seam between what the kernel decides and what a store keeps — and a test that
//! constructed both halves itself would not have crossed it.

use entity_core::{Decision, Registry, Runtime};
use entity_store::{EventProvider, Expect, MemoryStore, StateProvider, Store, StoreError};

/// A definition with one state, one operation that moves it, and one event.
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

/// Creates `ticket:one` and commits it, returning the store and the created decision.
fn opened() -> (MemoryStore, Decision) {
    let registry = registry();
    let decision = Runtime::new(&registry)
        .create(
            "ticket",
            1,
            "one",
            serde_json::json!({ "title": "A ticket" }),
        )
        .expect("creation is permitted");

    let mut store = MemoryStore::new();
    store
        .commit(&decision, Expect::Absent)
        .expect("nothing was stored under this identity yet");
    (store, decision)
}

#[test]
fn a_creation_expects_nothing_and_a_second_creation_of_the_same_identity_is_refused() {
    let (mut store, decision) = opened();
    assert_eq!(store.len(), 1);

    let error = store
        .commit(&decision, Expect::Absent)
        .expect_err("something is stored under this identity now");
    match error {
        StoreError::RevisionConflict {
            found, expected, ..
        } => {
            assert_eq!(expected, Expect::Absent);
            assert_eq!(found, Some(1), "the refusal says what is actually there");
        }
        other => panic!("expected a revision conflict, got {other}"),
    }
}

#[test]
fn two_executions_from_one_revision_leave_exactly_one_accepted() {
    // The shape of a team: two people read the same ticket and both act on it. Without this check
    // the second write silently replaces the first, and nothing anywhere records that the first
    // ever happened — a lost update is invisible at the moment it happens and expensive later.
    let registry = registry();
    let (mut store, created) = opened();

    // Both decisions are computed from the *same* stored revision, which is what makes them
    // concurrent. Neither has seen the other.
    let loaded = store
        .load("ticket", "one")
        .expect("the store answers")
        .expect("it holds the ticket");
    assert_eq!(loaded.revision, 1);

    let first = Runtime::new(&registry)
        .execute(&loaded, "close", serde_json::json!({}))
        .expect("closing an open ticket is permitted");
    let second = Runtime::new(&registry)
        .execute(&loaded, "close", serde_json::json!({}))
        .expect("the kernel decides the same thing twice; it holds no state");

    assert_eq!(first.instance.revision, 2);
    assert_eq!(
        second.instance.revision, 2,
        "both were computed from revision 1"
    );

    store
        .commit(&first, Expect::Revision(loaded.revision))
        .expect("the first writer finds what it expected");

    let error = store
        .commit(&second, Expect::Revision(loaded.revision))
        .expect_err("the second writer must not overwrite the first");
    match error {
        StoreError::RevisionConflict {
            expected, found, ..
        } => {
            assert_eq!(expected, Expect::Revision(1));
            assert_eq!(found, Some(2), "re-read from here and the retry is obvious");
        }
        other => panic!("expected a revision conflict, got {other}"),
    }

    // Exactly one accepted, and the store holds exactly one event for it.
    assert_eq!(store.events("ticket", "one").expect("events").len(), 1);
    assert_eq!(
        created.events.len(),
        0,
        "creation emitted none in this definition"
    );
}

#[test]
fn a_refused_commit_changes_nothing() {
    // The kernel's own guarantee (R-04) continued across the boundary where it would otherwise be
    // lost: a store that had already written the state before checking would leave the instance
    // moved and the events missing.
    let registry = registry();
    let (mut store, _) = opened();
    let loaded = store.load("ticket", "one").expect("answers").expect("held");
    let closed = Runtime::new(&registry)
        .execute(&loaded, "close", serde_json::json!({}))
        .expect("permitted");

    let before = store.load("ticket", "one").expect("answers").expect("held");
    let events_before = store.events("ticket", "one").expect("events");

    store
        .commit(&closed, Expect::Revision(99))
        .expect_err("99 is not what is stored");

    assert_eq!(
        store.load("ticket", "one").expect("answers").expect("held"),
        before
    );
    assert_eq!(
        store.events("ticket", "one").expect("events"),
        events_before
    );
}

#[test]
fn state_and_events_arrive_together() {
    // R-80: the shell persists the instance and appends the events together. A store that wrote one
    // without the other would leave a state that moved with no event explaining it, and every
    // projection and replay downstream would be quietly wrong.
    let registry = registry();
    let (mut store, _) = opened();
    let loaded = store.load("ticket", "one").expect("answers").expect("held");
    let closed = Runtime::new(&registry)
        .execute(&loaded, "close", serde_json::json!({}))
        .expect("permitted");
    assert_eq!(closed.events.len(), 1);

    store
        .commit(&closed, Expect::Revision(1))
        .expect("accepted");

    let stored = store.load("ticket", "one").expect("answers").expect("held");
    let events = store.events("ticket", "one").expect("events");
    assert_eq!(stored.lifecycle_state, "closed");
    assert_eq!(stored.revision, 2);
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].revision, stored.revision,
        "the event records the revision the instance is now at"
    );
}

#[test]
fn an_instance_nobody_stored_is_absent_rather_than_an_error() {
    let store = MemoryStore::new();
    assert_eq!(store.load("ticket", "nothing").expect("answers"), None);
    assert_eq!(
        store.revision_of("ticket", "nothing").expect("answers"),
        None
    );
    assert!(store
        .events("ticket", "nothing")
        .expect("answers")
        .is_empty());
}
