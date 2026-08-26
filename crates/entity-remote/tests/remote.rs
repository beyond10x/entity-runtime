//! The remote store, held to the same suite as every local one — and to the one thing only it has
//! to get right: telling *unreachable* from *absent*.

use entity_core::{Decision, DomainEvent, EntityInstance, Registry, Runtime};
use entity_remote::{Answer, Ask, LoopbackTransport, RemoteStore, Request, WIRE_VERSION};
use entity_store::{
    conformance, EventProvider, Expect, MemoryStore, StateProvider, Store, StoreError,
};
use serde_json::json;

fn remote() -> RemoteStore<LoopbackTransport<MemoryStore>> {
    RemoteStore::new(LoopbackTransport::new(MemoryStore::new()))
}

#[test]
fn a_remote_store_conforms_like_a_local_one() {
    // Every case runs through a full JSON round trip in both directions, so the wire shape is
    // exercised rather than described.
    let mut store = remote();
    let report = conformance::run(&mut store);
    assert!(report.is_clean(), "RemoteStore:\n{}", report.summary());
}

#[test]
fn a_remote_that_did_not_answer_is_unreachable_and_never_absent() {
    // The failure this exists to prevent: a caller reads silence as "no such thing", creates a
    // duplicate, or tells somebody their record is gone because a switch was rebooting.
    let registry = registry();
    let mut store = remote();

    let created = Runtime::new(&registry)
        .create("ticket", 1, "one", json!({ "title": "A ticket" }))
        .expect("permitted");
    store.commit(&created, Expect::Absent).expect("accepted");
    assert!(store.load("ticket", "one").expect("answers").is_some());

    store.transport().go_dark("connection refused");

    let error = store
        .load("ticket", "one")
        .expect_err("a store that cannot be reached has not answered");
    assert!(
        error.is_unreachable(),
        "silence must not be reported as absent: {error}"
    );
    assert!(
        error.to_string().contains("the loopback store"),
        "the refusal names which side did not answer: {error}"
    );

    // And the same for a write: nothing was learned, so nothing may be assumed.
    let closed = Runtime::new(&registry)
        .execute(&created.instance, "close", json!({}))
        .expect("permitted");
    assert!(store
        .commit(&closed, Expect::Revision(1))
        .expect_err("unreachable")
        .is_unreachable());

    store.transport().come_back();
    assert!(store.load("ticket", "one").expect("answers").is_some());
}

#[test]
fn a_conflict_crosses_the_wire_as_a_conflict_and_not_as_a_failure() {
    // A refusal is something the store decided; a failure is the exchange breaking. Collapsing them
    // would make a retry loop out of a conflict that no amount of retrying resolves.
    let registry = registry();
    let mut store = remote();
    let runtime = Runtime::new(&registry);

    let created = runtime
        .create("ticket", 1, "one", json!({ "title": "A ticket" }))
        .expect("permitted");
    store.commit(&created, Expect::Absent).expect("accepted");

    let first = runtime
        .execute(&created.instance, "close", json!({}))
        .expect("permitted");
    let second = first.clone();
    store.commit(&first, Expect::Revision(1)).expect("accepted");

    let error = store
        .commit(&second, Expect::Revision(1))
        .expect_err("the second writer is stale");
    assert!(
        !error.is_unreachable(),
        "a conflict is not a network problem"
    );
    assert!(
        error
            .to_string()
            .contains("expected revision 1, found revision 2"),
        "the conflict arrives intact, with both numbers: {error}"
    );
}

#[test]
fn a_request_at_a_wire_version_this_build_does_not_know_is_refused_by_name() {
    // Refused rather than parsed as far as it goes: a partial read of a protocol nobody agreed on
    // is how two deployments come to disagree quietly.
    let transport = LoopbackTransport::new(MemoryStore::new());
    let mut request = Request::new(Ask::Load {
        entity: "ticket".to_owned(),
        id: "one".to_owned(),
    });
    assert_eq!(request.version, WIRE_VERSION);
    request.version = "entity.store/99".to_owned();

    use entity_remote::Transport as _;
    let answered = transport.call(&request).expect("the peer answered");
    let Answer::Refused { detail } = answered else {
        panic!("an unknown version is refused by name, not {answered:?}");
    };
    assert!(detail.contains("entity.store/99"), "{detail}");
    assert!(detail.contains(WIRE_VERSION), "{detail}");
}

#[test]
fn a_version_refusal_is_not_reported_as_unreachable() {
    // The peer answered. Calling that unreachable would have a `ServeStale` hybrid serve a stale
    // copy for ever against a remote that is up and saying no.
    struct WrongVersion(LoopbackTransport<MemoryStore>);
    impl entity_remote::Transport for WrongVersion {
        fn name(&self) -> String {
            "wrong-version".to_owned()
        }
        fn call(&self, request: &Request) -> Result<Answer, String> {
            let mut bumped = request.clone();
            bumped.version = "entity.store/99".to_owned();
            self.0.call(&bumped)
        }
    }

    let store = RemoteStore::new(WrongVersion(LoopbackTransport::new(MemoryStore::new())));
    let error = store
        .load("ticket", "one")
        .expect_err("a version this build does not speak is refused");
    assert!(
        !error.is_unreachable(),
        "a peer that answered is not unreachable: {error}"
    );
    assert!(
        error.to_string().contains("entity.store/99"),
        "the refusal names the version: {error}"
    );
}

#[test]
fn an_unreachable_store_on_the_far_side_stays_unreachable_on_this_one() {
    // The third value has to survive the wire. Flattened into a backend failure, every
    // `WhenUnreachable` policy downstream stops applying to it.
    struct DeadStore;
    impl entity_store::StateProvider for DeadStore {
        fn load(&self, _: &str, _: &str) -> Result<Option<EntityInstance>, StoreError> {
            Err(StoreError::Unreachable {
                provider: "the-far-side-store".to_owned(),
                detail: "no route to host".to_owned(),
            })
        }
    }
    impl entity_store::EventProvider for DeadStore {
        fn events(&self, _: &str, _: &str) -> Result<Vec<DomainEvent>, StoreError> {
            Err(StoreError::Unreachable {
                provider: "the-far-side-store".to_owned(),
                detail: "no route to host".to_owned(),
            })
        }
    }
    impl Store for DeadStore {
        fn commit(&mut self, _: &Decision, _: Expect) -> Result<(), StoreError> {
            Err(StoreError::Unreachable {
                provider: "the-far-side-store".to_owned(),
                detail: "no route to host".to_owned(),
            })
        }
    }

    let store = RemoteStore::new(LoopbackTransport::new(DeadStore));
    let error = store
        .load("ticket", "one")
        .expect_err("the far side could not reach its own store");
    assert!(
        error.is_unreachable(),
        "unreachable one hop out is still unreachable here: {error}"
    );
    assert!(
        error.to_string().contains("the-far-side-store"),
        "the far side's provider name survives: {error}"
    );
}

#[test]
fn events_cross_the_wire_intact() {
    let registry = registry();
    let mut store = remote();
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

    let events = store.events("ticket", "one").expect("answers");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "TicketClosed");
    assert_eq!(events[0].to_state, "closed");
    assert_eq!(events[0].from_state.as_deref(), Some("open"));
}

fn registry() -> Registry {
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
    registry
}
