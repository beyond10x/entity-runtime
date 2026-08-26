//! Two stores and a declared rule for when they disagree.
//!
//! The cases worth pinning are the ones where a hybrid store usually goes quietly wrong: silence
//! read as absence, and a losing write vanishing without a record.

use entity_core::{Decision, Registry, Runtime};
use entity_remote::{
    Authority, Hybrid, LoopbackTransport, OnDivergence, Policy, ReadPath, RemoteStore,
    WhenUnreachable,
};
use entity_store::{conformance, Expect, MemoryStore, StateProvider, Store};
use serde_json::json;

type Remote = RemoteStore<LoopbackTransport<MemoryStore>>;

fn remote() -> Remote {
    RemoteStore::new(LoopbackTransport::new(MemoryStore::new()))
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

fn opened(registry: &Registry) -> Decision {
    Runtime::new(registry)
        .create("ticket", 1, "one", json!({ "title": "A ticket" }))
        .expect("permitted")
}

#[test]
fn a_hybrid_with_the_remote_as_authority_conforms_like_any_other_store() {
    let mut store = Hybrid::new(
        MemoryStore::new(),
        remote(),
        Policy::new(
            Authority::Remote,
            ReadPath::RemoteFirst,
            WhenUnreachable::Refuse,
            OnDivergence::Refuse,
        ),
    );
    let report = conformance::run(&mut store);
    assert!(report.is_clean(), "Hybrid:\n{}", report.summary());
}

#[test]
fn a_silent_remote_refuses_rather_than_answering_absent() {
    // The default that must not exist. A `None` here reads exactly like "there is no such thing",
    // and a caller acts on it: creates a duplicate, or tells somebody their record is gone.
    let registry = registry();
    let mut store = Hybrid::new(
        MemoryStore::new(),
        remote(),
        Policy::new(
            Authority::Remote,
            ReadPath::RemoteFirst,
            WhenUnreachable::Refuse,
            OnDivergence::Refuse,
        ),
    );
    store
        .commit(&opened(&registry), Expect::Absent)
        .expect("accepted");

    store.remote().transport().go_dark("connection refused");
    let error = store
        .load("ticket", "one")
        .expect_err("nothing was learned, so nothing may be reported");
    assert!(error.is_unreachable(), "{error}");
}

#[test]
fn serving_a_stale_copy_is_a_choice_and_the_answer_says_it_was_stale() {
    // The other legitimate answer — and the difference is that somebody typed it, and the result
    // carries the fact at the point of use rather than in a log nobody reads.
    let registry = registry();
    let mut store = Hybrid::new(
        MemoryStore::new(),
        remote(),
        Policy::new(
            Authority::Remote,
            ReadPath::RemoteFirst,
            WhenUnreachable::ServeStale,
            OnDivergence::Refuse,
        ),
    );
    store
        .commit(&opened(&registry), Expect::Absent)
        .expect("accepted");

    let fresh = store.load_read("ticket", "one").expect("answers");
    assert!(fresh.is_fresh());
    assert!(fresh.value.is_some());

    store.remote().transport().go_dark("connection refused");
    let stale = store
        .load_read("ticket", "one")
        .expect("the local copy answers");
    assert!(
        stale.was_stale,
        "the answer must carry that it may be behind"
    );
    assert!(stale.value.is_some(), "and it is still an answer");
}

#[test]
fn with_the_remote_as_authority_a_refused_remote_write_never_reaches_the_local_copy() {
    // A cache holding something the record of truth refused is worse than an empty cache: it is
    // confidently wrong, and every read of it is wrong the same way.
    let registry = registry();
    let mut store = Hybrid::new(
        MemoryStore::new(),
        remote(),
        Policy::new(
            Authority::Remote,
            ReadPath::RemoteFirst,
            WhenUnreachable::Refuse,
            OnDivergence::Refuse,
        ),
    );
    let created = opened(&registry);
    store.commit(&created, Expect::Absent).expect("accepted");

    // A second creation: the remote refuses, so the local copy must not take it either.
    store
        .commit(&created, Expect::Absent)
        .expect_err("the authority refused");

    let local_revision = store
        .local()
        .load("ticket", "one")
        .expect("answers")
        .expect("held")
        .revision;
    assert_eq!(
        local_revision, 1,
        "the cache did not move on a refused write"
    );
}

#[test]
fn with_the_local_store_as_authority_a_losing_replica_write_is_recorded_and_not_swallowed() {
    // A conflict resolved silently is data loss with good manners. The local write stands — it is
    // the authority — and the disagreement is written down where somebody can act on it.
    let registry = registry();
    let mut store = Hybrid::new(
        MemoryStore::new(),
        remote(),
        Policy::new(
            Authority::Local,
            ReadPath::LocalFirst,
            WhenUnreachable::ServeStale,
            OnDivergence::RecordDivergence,
        ),
    );

    store.remote().transport().go_dark("the replica is down");
    store
        .commit(&opened(&registry), Expect::Absent)
        .expect("the authority accepted it");

    assert_eq!(store.divergences().len(), 1);
    let divergence = &store.divergences()[0];
    assert_eq!(divergence.id, "one");
    assert_eq!(divergence.local_revision, 1);
    assert!(
        divergence.detail.contains("could not be reached"),
        "the record says why: {}",
        divergence.detail
    );

    // And the write really did land locally.
    assert!(store
        .local()
        .load("ticket", "one")
        .expect("answers")
        .is_some());
}

#[test]
fn refusing_on_divergence_lets_no_write_stand_unreplicated() {
    // The stricter half of the same choice, and it is a choice: this deployment would rather fail
    // than hold something the replica never saw.
    let registry = registry();
    let mut store = Hybrid::new(
        MemoryStore::new(),
        remote(),
        Policy::new(
            Authority::Local,
            ReadPath::LocalFirst,
            WhenUnreachable::Refuse,
            OnDivergence::Refuse,
        ),
    );

    store.remote().transport().go_dark("the replica is down");
    let error = store
        .commit(&opened(&registry), Expect::Absent)
        .expect_err("the replica did not take it");
    assert!(error.is_unreachable(), "{error}");
    assert!(
        store.divergences().is_empty(),
        "nothing was recorded as diverged"
    );
}

#[test]
fn a_laptop_that_wrote_while_the_replica_was_down_catches_up_when_it_returns() {
    // The whole point of allowing a divergence: the work is not lost and does not need a person.
    let registry = registry();
    let mut store = Hybrid::new(
        MemoryStore::new(),
        remote(),
        Policy::new(
            Authority::Local,
            ReadPath::LocalFirst,
            WhenUnreachable::ServeStale,
            OnDivergence::RecordDivergence,
        ),
    );

    store.remote().transport().go_dark("on a train");
    store
        .commit(&opened(&registry), Expect::Absent)
        .expect("the authority accepted it");
    assert_eq!(store.divergences().len(), 1);
    assert!(
        store.remote().load("ticket", "one").is_err(),
        "the replica has not seen it yet"
    );

    store.remote().transport().come_back();
    let outstanding = store.catch_up();

    assert_eq!(outstanding, 0, "nothing is still outstanding");
    assert!(store.divergences().is_empty());
    let replicated = store
        .remote()
        .load("ticket", "one")
        .expect("answers")
        .expect("the replica has it now");
    assert_eq!(replicated.revision, 1);
}

#[test]
fn catch_up_keeps_what_it_could_not_replay_rather_than_reporting_success() {
    // A reconciliation that cleared its own list on a partial success would report success and lose
    // the rest — which is the failure this whole path exists to prevent, reintroduced at the end.
    let registry = registry();
    let mut store = Hybrid::new(
        MemoryStore::new(),
        remote(),
        Policy::new(
            Authority::Local,
            ReadPath::LocalFirst,
            WhenUnreachable::ServeStale,
            OnDivergence::RecordDivergence,
        ),
    );

    store.remote().transport().go_dark("on a train");
    store
        .commit(&opened(&registry), Expect::Absent)
        .expect("accepted");
    assert_eq!(store.divergences().len(), 1);

    // Still dark: catching up cannot succeed, and must say so.
    let outstanding = store.catch_up();
    assert_eq!(outstanding, 1, "it is still outstanding");
    assert_eq!(store.divergences().len(), 1, "and still recorded");
}

#[test]
fn catch_up_replays_what_the_local_store_holds_now_and_not_the_write_that_diverged() {
    // The local side may have moved on. Replaying the old revision would push the replica to a
    // state the authority has already left, which is a different wrong answer from being behind.
    let registry = registry();
    let runtime = Runtime::new(&registry);
    let mut store = Hybrid::new(
        MemoryStore::new(),
        remote(),
        Policy::new(
            Authority::Local,
            ReadPath::LocalFirst,
            WhenUnreachable::ServeStale,
            OnDivergence::RecordDivergence,
        ),
    );

    store.remote().transport().go_dark("on a train");
    let created = opened(&registry);
    store.commit(&created, Expect::Absent).expect("accepted");
    let closed = runtime
        .execute(&created.instance, "close", json!({}))
        .expect("permitted");
    store
        .commit(&closed, Expect::Revision(1))
        .expect("accepted");
    assert_eq!(store.divergences().len(), 2, "both writes diverged");

    store.remote().transport().come_back();
    assert_eq!(store.catch_up(), 0);

    let replicated = store
        .remote()
        .load("ticket", "one")
        .expect("answers")
        .expect("held");
    assert_eq!(
        replicated.revision, 2,
        "the replica lands on where the authority is now, not on where it was"
    );
    assert_eq!(replicated.lifecycle_state, "closed");
}
