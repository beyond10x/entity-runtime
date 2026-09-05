//! The Postgres provider, held to the same suite as every other — under a server, when there is one.
//!
//! Every test here opens a schema of its own on the server `ENTITY_POSTGRES_URL` names and drops it
//! afterwards, so a shared database and parallel tests do not see each other. When the variable is
//! unset, each test **returns after saying so**; the gate's `postgres-check` step says the same in
//! one line, so a green gate cannot read as a tested provider. When the variable is set and the
//! server does not answer, the tests fail: a variable somebody set is a claim that the server is
//! there.

use std::sync::atomic::{AtomicUsize, Ordering};

use entity_core::{Registry, Runtime};
use entity_postgres::PostgresStore;
use entity_query::{DocumentQuery, DocumentQueryProvider};
use entity_store::{
    conformance, AtomicBatchStore, AtomicCommit, Expect, StateProvider, Store, StoreError,
};
use serde_json::json;

static SCHEMAS: AtomicUsize = AtomicUsize::new(0);

/// The server, or `None` after saying why the test is not running.
fn url() -> Option<String> {
    match std::env::var("ENTITY_POSTGRES_URL") {
        Ok(url) if !url.trim().is_empty() => Some(url),
        _ => {
            eprintln!("skipped: ENTITY_POSTGRES_URL unset, so no server to hold this provider to");
            None
        }
    }
}

/// A store in a schema of its own, and the schema's name so the test can drop it.
fn fresh(url: &str, label: &str) -> (PostgresStore, String) {
    let schema = format!(
        "entity_test_{}_{}_{label}",
        std::process::id(),
        SCHEMAS.fetch_add(1, Ordering::Relaxed)
    );
    let store = PostgresStore::connect_in_schema(url, &schema).unwrap_or_else(|error| {
        panic!("ENTITY_POSTGRES_URL is set and the server refused: {error}")
    });
    (store, schema)
}

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
fn the_postgres_provider_conforms() {
    let Some(url) = url() else { return };
    let (mut store, schema) = fresh(&url, "conforms");
    let report = conformance::run(&mut store);
    let recorded = conformance::verify_recorded(&mut store);
    let batch = conformance::run_atomic(&mut store);
    store.drop_schema(&schema).expect("dropped");
    assert!(report.is_clean(), "PostgresStore:\n{}", report.summary());
    recorded.expect("PostgresStore recorded history");
    assert_eq!(report.outcomes.len(), 10);
    assert!(
        batch.is_clean(),
        "PostgresStore batch:\n{}",
        batch.summary()
    );
}

/// The runtime's `Broken`, written over this provider: every write accepted whatever was expected.
struct Broken(PostgresStore);

impl StateProvider for Broken {
    fn load(
        &self,
        entity: &str,
        id: &str,
    ) -> Result<Option<entity_core::EntityInstance>, StoreError> {
        self.0.load(entity, id)
    }
    fn ids(&self, entity: &str) -> Result<Vec<String>, StoreError> {
        self.0.ids(entity)
    }
}
impl entity_store::EventProvider for Broken {
    fn events(&self, entity: &str, id: &str) -> Result<Vec<entity_core::DomainEvent>, StoreError> {
        self.0.events(entity, id)
    }
}
impl Store for Broken {
    fn commit(&mut self, decision: &entity_core::Decision, _: Expect) -> Result<(), StoreError> {
        let expect = match self
            .0
            .revision_of(&decision.instance.entity, &decision.instance.id)?
        {
            Some(held) => Expect::Revision(held),
            None => Expect::Absent,
        };
        self.0.commit(decision, expect)
    }
}

impl AtomicBatchStore for Broken {
    fn commit_batch(&mut self, commits: &[AtomicCommit]) -> Result<(), StoreError> {
        // Deliberately non-atomic: each prefix is its own transaction, so the shared batch suite
        // must localise the rollback defect just as the ordinary suite localises ignored expects.
        for commit in commits {
            self.0.commit(&commit.decision, commit.expect)?;
        }
        Ok(())
    }
}

#[test]
fn a_broken_copy_of_the_provider_is_caught() {
    let Some(url) = url() else { return };
    let (store, schema) = fresh(&url, "broken");
    let mut broken = Broken(store);
    let report = conformance::run(&mut broken);
    let batch = conformance::run_atomic(&mut broken);
    broken.0.drop_schema(&schema).expect("dropped");
    assert!(
        !report.is_clean(),
        "a provider ignoring revisions must not pass"
    );
    let caught: Vec<&str> = report.failures().iter().map(|o| o.case).collect();
    assert!(
        caught.contains(&"a stale write is refused"),
        "the stale-write case is the one that must catch it: {caught:?}"
    );
    assert!(
        report.failures().len() < report.outcomes.len(),
        "and it localises the defect"
    );
    let caught: Vec<&str> = batch
        .failures()
        .iter()
        .map(|outcome| outcome.case)
        .collect();
    assert!(
        caught.contains(&"a conflict rolls every earlier batch entry back"),
        "the batch suite must catch a provider that commits its prefix: {caught:?}"
    );
}

#[test]
fn two_writers_from_one_revision_leave_exactly_one_accepted() {
    // R-84 under real concurrency: two connections, two threads, one instance at revision 1, both
    // executing `close` from it. One lands; the other is told which revision it lost to. Never a
    // silent last-writer-wins, never a `Backend` failure that tells the caller to stop retrying.
    let Some(url) = url() else { return };
    let (mut first, schema) = fresh(&url, "two_writers");
    let registry = registry();
    let created = Runtime::new(&registry)
        .create("ticket", 1, "contested", json!({ "title": "A ticket" }))
        .expect("permitted");
    first.commit(&created, Expect::Absent).expect("accepted");

    let mut second = PostgresStore::connect_in_schema(&url, &schema).expect("a second connection");
    let closed = Runtime::new(&registry)
        .execute(&created.instance, "close", json!({}))
        .expect("permitted");

    let (a, b) = std::thread::scope(|scope| {
        let a = scope.spawn(|| first.commit(&closed, Expect::Revision(1)));
        let b = scope.spawn(|| second.commit(&closed, Expect::Revision(1)));
        (a.join().expect("thread a"), b.join().expect("thread b"))
    });
    let outcomes = [a, b];
    let accepted = outcomes.iter().filter(|o| o.is_ok()).count();
    assert_eq!(accepted, 1, "exactly one writer lands: {outcomes:?}");
    let refused = outcomes
        .iter()
        .find_map(|o| o.as_ref().err())
        .expect("one was refused");
    assert!(
        matches!(refused, StoreError::RevisionConflict { found: Some(2), .. }),
        "the loser is told the revision it lost to, as a conflict: {refused}"
    );

    let held = first
        .load("ticket", "contested")
        .expect("answers")
        .expect("held");
    assert_eq!(held.revision, 2, "one write landed, not two");
    first.drop_schema(&schema).expect("dropped");
}

#[test]
fn two_creators_of_one_identity_leave_exactly_one_accepted() {
    // The case a row lock cannot serialise: nothing exists yet to lock. Both read absent, both
    // insert, the primary key refuses the second — which arrives as the same conflict, naming the
    // revision the first landed.
    let Some(url) = url() else { return };
    let (mut first, schema) = fresh(&url, "two_creators");
    let mut second = PostgresStore::connect_in_schema(&url, &schema).expect("a second connection");
    let registry = registry();
    let created = Runtime::new(&registry)
        .create("ticket", 1, "raced", json!({ "title": "A ticket" }))
        .expect("permitted");

    let (a, b) = std::thread::scope(|scope| {
        let a = scope.spawn(|| first.commit(&created, Expect::Absent));
        let b = scope.spawn(|| second.commit(&created, Expect::Absent));
        (a.join().expect("thread a"), b.join().expect("thread b"))
    });
    let outcomes = [a, b];
    assert_eq!(
        outcomes.iter().filter(|o| o.is_ok()).count(),
        1,
        "{outcomes:?}"
    );
    let refused = outcomes
        .iter()
        .find_map(|o| o.as_ref().err())
        .expect("one was refused");
    assert!(
        matches!(
            refused,
            StoreError::RevisionConflict {
                expected: Expect::Absent,
                found: Some(1),
                ..
            }
        ),
        "{refused}"
    );
    first.drop_schema(&schema).expect("dropped");
}

#[test]
fn migrate_is_idempotent_and_a_store_survives_being_reopened() {
    let Some(url) = url() else { return };
    let (mut store, schema) = fresh(&url, "reopen");
    store.migrate().expect("a second migrate changes nothing");
    let registry = registry();
    let created = Runtime::new(&registry)
        .create("ticket", 1, "kept", json!({ "title": "A ticket" }))
        .expect("permitted");
    store.commit(&created, Expect::Absent).expect("accepted");
    drop(store);

    let reopened = PostgresStore::connect_in_schema(&url, &schema).expect("reopens");
    assert_eq!(reopened.ids("ticket").expect("answers"), ["kept"]);
    let held = reopened
        .load("ticket", "kept")
        .expect("answers")
        .expect("held");
    assert_eq!(held, created.instance);
    let mut reopened = reopened;
    reopened.drop_schema(&schema).expect("dropped");
}

#[test]
fn postgres_document_queries_bind_serialized_containment_as_text() {
    let Some(url) = url() else { return };
    let (mut store, schema) = fresh(&url, "document_query_parameter");
    let created = Runtime::new(&registry())
        .create("ticket", 1, "matched", json!({ "title": "A ticket" }))
        .expect("permitted");
    store.commit(&created, Expect::Absent).expect("accepted");

    let page = store
        .query_documents(&DocumentQuery::for_entity("ticket").matching("title", json!("A ticket")))
        .expect("serialized JSON text is cast to JSONB by PostgreSQL");

    assert_eq!(page.items, [created.instance]);
    store.drop_schema(&schema).expect("dropped");
}

#[test]
fn a_session_reserves_disjoint_sequence_ranges_and_reads_its_events() {
    let Some(url) = url() else { return };
    let (mut store, schema) = fresh(&url, "session_primitives");
    let created = Runtime::new(&registry())
        .create("ticket", 1, "session", json!({ "title": "A ticket" }))
        .expect("permitted");
    store.commit(&created, Expect::Absent).expect("accepted");

    let first = store
        .with_transaction(|session| {
            session.lock_identity("aep.locator", "ep://beyond10x/plan/story/session")?;
            assert_eq!(session.events("ticket", "session")?, created.events);
            session.reserve_sequence("aep", 8)
        })
        .expect("first reservation");
    let second = store
        .with_transaction(|session| session.reserve_sequence("aep", 8))
        .expect("second reservation");

    assert_eq!(first, 0);
    assert_eq!(second, 8);
    store.drop_schema(&schema).expect("dropped");
}

#[test]
fn a_server_that_does_not_answer_is_unreachable_and_never_an_empty_store() {
    // No variable needed: a port nothing listens on. Silence is the third value, not absence.
    let error = PostgresStore::connect_no_tls("postgres://nobody:nothing@127.0.0.1:1/nowhere")
        .expect_err("nothing listens on port 1");
    assert!(error.is_unreachable(), "{error}");
}

fn recording(id: &str) -> entity_store::Recording {
    entity_store::Recording {
        record_id: id.into(),
        recorded_at: "2026-09-05T12:00:00Z".into(),
        correlation: None,
        causation: None,
        actor: None,
    }
}

#[test]
fn a_caught_session_batch_conflict_rolls_back_its_prefix_but_keeps_the_transaction_usable() {
    let Some(url) = url() else { return };
    let (mut store, schema) = fresh(&url, "caught_batch");
    let created = Runtime::new(&registry())
        .create("ticket", 1, "one", json!({"title": "One"}))
        .unwrap();
    let commits = vec![
        AtomicCommit {
            decision: created.clone(),
            expect: Expect::Absent,
        },
        AtomicCommit {
            decision: created.clone(),
            expect: Expect::Absent,
        },
    ];
    store
        .with_transaction(|session| {
            let result = session.commit_batch(&commits);
            assert!(
                matches!(
                    result,
                    Err(StoreError::RevisionConflict { found: Some(1), .. })
                ),
                "{result:?}"
            );
            assert_eq!(
                session.load_for_update("ticket", "one")?,
                None,
                "the caught error must not retain the successful prefix"
            );
            assert!(session.events("ticket", "one")?.is_empty());
            session.reserve_sequence("still-usable", 1)?;
            Ok(())
        })
        .unwrap();
    assert_eq!(store.load("ticket", "one").unwrap(), None);
    store.drop_schema(&schema).unwrap();
}

#[test]
fn session_events_include_recorded_and_plain_writes_in_revision_order() {
    use entity_store::{EventProvider, RecordedCommit};
    let Some(url) = url() else { return };
    let (mut store, schema) = fresh(&url, "mixed_session");
    let registry = registry();
    let created = Runtime::new(&registry)
        .create("ticket", 1, "one", json!({"title": "One"}))
        .unwrap();
    store
        .commit_recorded(
            &RecordedCommit::new(created.clone(), &recording("created")).unwrap(),
            Expect::Absent,
        )
        .unwrap();
    let closed = Runtime::new(&registry)
        .execute(&created.instance, "close", json!({}))
        .unwrap();
    store.commit(&closed, Expect::Revision(1)).unwrap();
    let expected: Vec<_> = created.events.into_iter().chain(closed.events).collect();
    assert_eq!(store.events("ticket", "one").unwrap(), expected);
    store
        .with_transaction(|session| {
            assert_eq!(session.events("ticket", "one")?, expected);
            Ok(())
        })
        .unwrap();
    store.drop_schema(&schema).unwrap();
}

#[test]
fn concurrent_identical_decisions_and_observations_are_idempotent() {
    use entity_store::{HistoryProvider, RecordedCommit, RecordedObservation};
    let Some(url) = url() else { return };
    let (mut first, schema) = fresh(&url, "record_retries");
    let mut second = PostgresStore::connect_in_schema(&url, &schema).unwrap();
    let created = Runtime::new(&registry())
        .create("ticket", 1, "one", json!({"title": "One"}))
        .unwrap();
    let commit = RecordedCommit::new(created, &recording("created")).unwrap();
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let a = scope.spawn(|| {
            barrier.wait();
            first.commit_recorded(&commit, Expect::Absent)
        });
        let b = scope.spawn(|| {
            barrier.wait();
            second.commit_recorded(&commit, Expect::Absent)
        });
        a.join().unwrap().expect("first identical decision");
        b.join().unwrap().expect("second identical decision");
    });
    let observation = RecordedObservation {
        entity: "ticket".into(),
        id: "one".into(),
        revision: 1,
        envelope: recording("observed")
            .seal(json!({"evidence": true}))
            .unwrap(),
    };
    std::thread::scope(|scope| {
        let a = scope.spawn(|| {
            barrier.wait();
            first.observe(&observation)
        });
        let b = scope.spawn(|| {
            barrier.wait();
            second.observe(&observation)
        });
        a.join().unwrap().expect("first identical observation");
        b.join().unwrap().expect("second identical observation");
    });
    assert_eq!(first.records("ticket", "one").unwrap(), [commit.envelope]);
    assert_eq!(first.observations("ticket", "one").unwrap(), [observation]);
    first.drop_schema(&schema).unwrap();
}

#[test]
fn memory_and_postgres_match_equivalent_numeric_json_values() {
    use entity_store::MemoryStore;
    let Some(url) = url() else { return };
    let (mut store, schema) = fresh(&url, "numeric_query");
    let mut memory = MemoryStore::new();
    let mut registry = Registry::new();
    registry.register(serde_json::from_value(json!({"entity": "numbers", "schema": {"fields": {"value": {"type": "json"}}}, "lifecycle": {"initial": "open", "states": ["open"]}})).unwrap()).unwrap();
    for (id, value) in [
        ("one", json!({"amount": 100})),
        ("two", json!([0.1, 2])),
        ("three", json!(0)),
    ] {
        let decision = Runtime::new(&registry)
            .create("numbers", 1, id, json!({"value": value}))
            .unwrap();
        store.commit(&decision, Expect::Absent).unwrap();
        memory.commit(&decision, Expect::Absent).unwrap();
    }
    for (value, expected) in [
        (json!({"amount": 100.0}), "one"),
        (json!([1e-1]), "two"),
        (json!(-0.0), "three"),
    ] {
        let query = DocumentQuery::for_entity("numbers").matching("value", value);
        let postgres = store.query_documents(&query).unwrap();
        assert_eq!(
            postgres
                .items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            [expected]
        );
        assert_eq!(memory.query_documents(&query).unwrap(), postgres);
    }
    store.drop_schema(&schema).unwrap();
}
