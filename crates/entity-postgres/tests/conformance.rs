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
    asynchronous::{HistoryOrigin, KnownLegacyOrder, LegacyEvidence, LegacyOrderDeclaration},
    conformance, AtomicBatchStore, AtomicCommit, Expect, LegacyStoreSource, RecordedCommit,
    RecordedObservation, Recording, StateProvider, Store, StoreError,
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

fn legacy_recording(record_id: &str) -> Recording {
    Recording {
        record_id: record_id.to_owned(),
        recorded_at: "2026-09-16T00:00:00Z".to_owned(),
        correlation: Some("postgres-legacy-acquisition".to_owned()),
        causation: None,
        actor: None,
    }
}

#[test]
fn legacy_acquisition_retains_exact_available_postgres_evidence() {
    let Some(url) = url() else { return };
    let (mut store, schema) = fresh(&url, "legacy_acquisition");
    let registry = registry();
    let runtime = Runtime::new(&registry);
    let created = runtime
        .create("ticket", 1, "legacy", json!({"title":"legacy"}))
        .expect("creation");
    store
        .commit(&created, Expect::Absent)
        .expect("bare legacy creation");
    let closed = runtime
        .execute(&created.instance, "close", json!({}))
        .expect("recorded transition");
    let recorded = RecordedCommit::new(closed, &legacy_recording("legacy-close"))
        .expect("complete recorded decision");
    store
        .commit_recorded(&recorded, Expect::Revision(1))
        .expect("recorded decision");
    let observation = RecordedObservation {
        entity: "ticket".to_owned(),
        id: "legacy".to_owned(),
        revision: 2,
        envelope: legacy_recording("legacy-observation")
            .seal(json!({"seen":true}))
            .expect("observation envelope"),
    };
    store.observe(&observation).expect("observation");

    let snapshot = store
        .acquire_legacy("postgres/source-a")
        .expect("repeatable-read acquisition");
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
    assert_eq!(decision.entry.record_id(), "legacy-close");
    assert_eq!(decision.source_id, "postgres/source-a");
    assert_eq!(decision.known_order, KnownLegacyOrder::PerKind(0));
    let LegacyEvidence::Envelope(saved_observation) = &anchor.evidence[1] else {
        panic!("observation envelope expected")
    };
    assert_eq!(saved_observation.entry.record_id(), "legacy-observation");
    assert_eq!(saved_observation.known_order, KnownLegacyOrder::PerKind(0));
    assert!(matches!(anchor.evidence[2], LegacyEvidence::Event(_)));
    store.drop_schema(&schema).expect("dropped");
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

/// A ticket that can be closed and reopened, so one subject reaches revision 3.
fn reopenable_registry() -> Registry {
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
            },
            "reopen": {
                "transitions": [{ "from": "closed", "to": "open" }],
                "emits": [{ "type": "TicketReopened", "payload": { "ticket": "$id" } }]
            }
        }
    }))
    .expect("parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("validates");
    registry
}

/// Forces the interleaving a read of one subject's events must survive: revision 1 is committed,
/// then the reader starts while another transaction holds `history` exclusively. A reader that
/// reads the bare-event table and the recorded-decision table in two statements finishes the first
/// and waits on the second; meanwhile a bare revision 2 and then a recorded revision 3 commit. The
/// reader must return all three revisions — never 1 and 3 with the middle missing.
fn assert_events_read_survives_commits_between_its_sources(
    label: &str,
    read: impl FnOnce(&mut PostgresStore) -> Result<Vec<entity_core::DomainEvent>, StoreError> + Send,
) {
    let Some(url) = url() else { return };
    let (mut writer, schema) = fresh(&url, label);
    let mut reader = PostgresStore::connect_in_schema(&url, &schema).expect("a reader connection");
    let registry = reopenable_registry();
    let created = Runtime::new(&registry)
        .create("ticket", 1, "raced", json!({ "title": "A ticket" }))
        .expect("permitted");
    writer.commit(&created, Expect::Absent).expect("revision 1");
    let closed = Runtime::new(&registry)
        .execute(&created.instance, "close", json!({}))
        .expect("permitted");
    let reopened = Runtime::new(&registry)
        .execute(&closed.instance, "reopen", json!({}))
        .expect("permitted");
    let recorded = RecordedCommit::new(reopened.clone(), &recording("reopened")).expect("seals");

    // The lock holder writes the recorded revision itself, as `commit_recorded` stores it, because
    // no other connection can write `history` while it holds the table.
    let mut holder = postgres::Client::connect(&url, postgres::NoTls).expect("a holder connection");
    holder
        .batch_execute(&format!("SET search_path TO \"{schema}\""))
        .expect("schema selected");
    let mut lock = holder.transaction().expect("holder transaction");
    lock.batch_execute("LOCK TABLE history IN ACCESS EXCLUSIVE MODE")
        .expect("history held");

    let events = std::thread::scope(|scope| {
        let reading = scope.spawn(|| read(&mut reader));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            let waiting: i64 = lock
                .query_one(
                    "SELECT count(*) FROM pg_locks WHERE NOT granted \
                     AND relation = 'history'::regclass",
                    &[],
                )
                .expect("pg_locks answers")
                .get(0);
            if waiting > 0 {
                break;
            }
            assert!(
                !reading.is_finished(),
                "the reader finished without ever waiting on history"
            );
            assert!(
                std::time::Instant::now() < deadline,
                "the reader never waited on history"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        writer
            .commit(&closed, Expect::Revision(1))
            .expect("bare revision 2 commits while the reader waits");
        lock.execute(
            "UPDATE instances SET revision = 3, document = $1 \
             WHERE entity = 'ticket' AND id = 'raced'",
            &[&serde_json::to_string(&reopened.instance).expect("serialises")],
        )
        .expect("instance at revision 3");
        lock.execute(
            "INSERT INTO history (entity, id, position, kind, record_id, document) \
             VALUES ('ticket', 'raced', 0, 'decision', $1, $2)",
            &[
                &recorded.envelope.record_id,
                &serde_json::to_string(&recorded.envelope).expect("serialises"),
            ],
        )
        .expect("recorded revision 3");
        lock.commit().expect("revisions 2 and 3 committed");
        reading.join().expect("reader thread")
    })
    .expect("the read answers");

    let revisions: Vec<u64> = events.iter().map(|event| event.revision).collect();
    assert_eq!(
        revisions,
        [1, 2, 3],
        "a history with recorded revision 3 must include bare revision 2: {events:?}"
    );
    writer.drop_schema(&schema).expect("dropped");
}

#[test]
fn an_events_read_never_returns_a_later_recorded_revision_without_the_bare_one_before_it() {
    use entity_store::EventProvider;
    assert_events_read_survives_commits_between_its_sources("events_snapshot", |store| {
        store.events("ticket", "raced")
    });
}

#[test]
fn a_session_events_read_never_returns_a_later_recorded_revision_without_the_bare_one_before_it() {
    assert_events_read_survives_commits_between_its_sources("session_events_snapshot", |store| {
        store.with_transaction(|session| session.events("ticket", "raced"))
    });
}

#[test]
fn legacy_acquisition_scans_history_a_fixed_number_of_times_whatever_the_subject_count() {
    // The server counts every scan of a table, and a closing backend flushes its counts. Bare
    // commits never read `history`, but creating it builds its two indexes, and each build is a
    // scan counted to the creating backend. So the writer closes first, its counts are taken as
    // the baseline, and what the acquiring backend adds on closing is what acquisition cost.
    let Some(url) = url() else { return };
    let (mut writer, schema) = fresh(&url, "acquisition_statements");
    let registry = registry();
    for n in 0..32 {
        let created = Runtime::new(&registry)
            .create(
                "ticket",
                1,
                format!("t{n:03}"),
                json!({ "title": "A ticket" }),
            )
            .expect("permitted");
        writer.commit(&created, Expect::Absent).expect("accepted");
    }
    drop(writer);

    let mut observer = postgres::Client::connect(&url, postgres::NoTls).expect("observer");
    let mut history_scans = || -> i64 {
        observer
            .query_one(
                "SELECT COALESCE(seq_scan, 0) + COALESCE(idx_scan, 0) FROM pg_stat_user_tables \
                 WHERE schemaname = $1 AND relname = 'history'",
                &[&schema],
            )
            .expect("statistics answer")
            .get(0)
    };
    let wait_above = |floor: i64, scans: &mut dyn FnMut() -> i64| -> i64 {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            let seen = scans();
            if seen > floor {
                return seen;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "a closed backend's statistics never arrived"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    };
    let baseline = wait_above(0, &mut history_scans);

    let mut acquirer = PostgresStore::connect_in_schema(&url, &schema).expect("acquirer");
    let snapshot = acquirer.acquire_legacy("postgres/count").expect("acquired");
    assert_eq!(snapshot.histories.len(), 32, "every subject acquired");
    drop(acquirer);
    let scans = wait_above(baseline, &mut history_scans) - baseline;

    assert_eq!(
        scans, 2,
        "history is read once for the coordinates and once for its rows, not once per subject"
    );
    observer
        .batch_execute(&format!("DROP SCHEMA \"{schema}\" CASCADE"))
        .expect("dropped");
}

#[test]
fn legacy_acquisition_refuses_evidence_without_its_terminal_instance() {
    let Some(url) = url() else { return };
    let (mut store, schema) = fresh(&url, "acquisition_orphan");
    let mut raw = postgres::Client::connect(&url, postgres::NoTls).expect("raw connection");
    raw.batch_execute(&format!(
        "SET search_path TO \"{schema}\";
         INSERT INTO events (entity, id, revision, position, document)
         VALUES ('ticket', 'orphan', 1, 0, '{{}}');"
    ))
    .expect("orphan fixture");
    let refusal = store
        .acquire_legacy("postgres/orphan")
        .expect_err("evidence without its terminal instance cannot be silently omitted");
    assert!(
        matches!(&refusal, StoreError::Backend(message)
            if message.contains("\"ticket\"/\"orphan\" has no terminal instance")),
        "the refusal names the subject whose terminal instance is missing: {refusal}"
    );
    store.drop_schema(&schema).expect("dropped");
}

/// The server's identifier limit in bytes (`max_identifier_length`, 63 unless rebuilt).
fn identifier_limit(raw: &mut postgres::Client) -> usize {
    let limit: String = raw
        .query_one("SELECT current_setting('max_identifier_length')", &[])
        .expect("the server reports its identifier limit")
        .get(0);
    limit.parse().expect("a byte count")
}

#[test]
fn a_schema_name_longer_than_the_server_limit_is_refused_rather_than_truncated() {
    let Some(url) = url() else { return };
    let mut raw = postgres::Client::connect(&url, postgres::NoTls).expect("raw connection");
    let limit = identifier_limit(&mut raw);
    // A name exactly at the limit is kept whole; a longer one sharing its first `limit` bytes is
    // the name PostgreSQL would truncate to it.
    let mut kept = format!("entity_test_{}_long_", std::process::id());
    while kept.len() < limit {
        kept.push('x');
    }
    let longer = format!("{kept}_another_plan");
    let mut store =
        PostgresStore::connect_in_schema(&url, &kept).expect("a name at the limit is kept whole");
    let created = Runtime::new(&registry())
        .create("ticket", 1, "kept", json!({"title": "Kept"}))
        .expect("permitted");
    store.commit(&created, Expect::Absent).expect("accepted");
    let says_truncated = |refusal: &StoreError, bytes: usize| {
        let expected =
            format!("is {bytes} bytes; this server truncates identifiers to {limit} bytes");
        matches!(refusal, StoreError::Backend(message) if message.contains(&expected))
    };

    let refusal = PostgresStore::connect_in_schema(&url, &longer)
        .expect_err("a longer name must not open the schema of its truncation");
    assert!(
        says_truncated(&refusal, longer.len()),
        "the refusal names the byte length and the server's limit: {refusal}"
    );
    let refusal = store
        .drop_schema(&longer)
        .expect_err("a longer name must not drop the schema of its truncation");
    assert!(
        says_truncated(&refusal, longer.len()),
        "the refusal names the byte length and the server's limit: {refusal}"
    );
    assert_eq!(
        store
            .load("ticket", "kept")
            .expect("the kept schema answers"),
        Some(created.instance),
        "the schema a longer name truncates to keeps its data"
    );

    // Bytes, not characters: two-byte characters reach the limit at half as many characters.
    let multibyte = "é".repeat(limit / 2 + 1);
    let refusal = PostgresStore::connect_in_schema(&url, &multibyte)
        .expect_err("a name over the limit in bytes is refused whatever its character count");
    assert!(
        says_truncated(&refusal, multibyte.len()),
        "the refusal counts bytes: {refusal}"
    );
    store.drop_schema(&kept).expect("dropped");
}

#[test]
fn a_schema_name_that_resolves_to_another_schema_is_refused_and_leaves_nothing_behind() {
    let Some(url) = url() else { return };
    // In a search path, `$user` names the session user's schema, never a schema called `$user`:
    // the session would work in whatever schema the user's name selects, or in none.
    let refusal = PostgresStore::connect_in_schema(&url, "$user")
        .expect_err("a name the search path resolves elsewhere must not be used");
    assert!(
        matches!(&refusal, StoreError::Backend(message)
            if message.contains("schema \"$user\" resolves to") && message.contains("not to itself")),
        "the refusal says the name resolved to another schema: {refusal}"
    );
    let mut raw = postgres::Client::connect(&url, postgres::NoTls).expect("raw connection");
    let left: i64 = raw
        .query_one(
            "SELECT count(*) FROM pg_namespace WHERE nspname = '$user'",
            &[],
        )
        .expect("catalog answers")
        .get(0);
    assert_eq!(left, 0, "the refused schema's creation is rolled back");
}

/// A store in an existing `schema` whose lock waits end after `lock_timeout`, and the process id
/// of its server backend.
fn store_with_lock_timeout(url: &str, schema: &str, lock_timeout: &str) -> (PostgresStore, i32) {
    let mut client = postgres::Client::connect(url, postgres::NoTls).expect("client");
    client
        .batch_execute(&format!(
            "SET search_path TO \"{schema}\"; SET lock_timeout = '{lock_timeout}';"
        ))
        .expect("session configured");
    let pid: i32 = client
        .query_one("SELECT pg_backend_pid()", &[])
        .expect("backend pid")
        .get(0);
    (PostgresStore::from_client(client).expect("store"), pid)
}

#[test]
fn a_record_lock_serializes_one_record_id_within_a_schema_and_not_across_schemas() {
    let Some(url) = url() else { return };
    let (mut east, east_schema) = fresh(&url, "record_lock_east");
    let (mut west, west_schema) = fresh(&url, "record_lock_west");
    let registry = registry();
    let runtime = Runtime::new(&registry);
    let held = runtime
        .create("ticket", 1, "held", json!({"title": "Held"}))
        .expect("permitted");
    east.commit(&held, Expect::Absent).expect("accepted");
    let closed = runtime
        .execute(&held.instance, "close", json!({}))
        .expect("permitted");
    let blocked = RecordedCommit::new(closed, &recording("shared-receipt")).expect("seals");
    let receipt_for = |id: &str| {
        let created = runtime
            .create("ticket", 1, id, json!({"title": "Other"}))
            .expect("permitted");
        RecordedCommit::new(created, &recording("shared-receipt")).expect("seals")
    };
    let (west_commit, east_commit) = (receipt_for("west"), receipt_for("other"));

    // A second connection holds the row of `held`, so the recorded write of it takes its record
    // lock and then waits on the row, holding that lock until the row is released.
    let mut row_holder = postgres::Client::connect(&url, postgres::NoTls).expect("row holder");
    row_holder
        .batch_execute(&format!(
            "BEGIN; SET LOCAL search_path TO \"{east_schema}\";
             SELECT 1 FROM instances WHERE entity = 'ticket' AND id = 'held' FOR UPDATE;"
        ))
        .expect("row held");
    let (mut lock_holder, holder_pid) = store_with_lock_timeout(&url, &east_schema, "30s");
    let (mut west_writer, _) = store_with_lock_timeout(&url, &west_schema, "5s");
    let (mut east_writer, _) = store_with_lock_timeout(&url, &east_schema, "300ms");
    let mut observer = postgres::Client::connect(&url, postgres::NoTls).expect("observer");

    let (held_write, across, within) = std::thread::scope(|scope| {
        let held_write = scope.spawn(|| lock_holder.commit_recorded(&blocked, Expect::Revision(1)));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let (holds, waits): (bool, bool) = {
                let row = observer
                    .query_one(
                        "SELECT bool_or(locktype = 'advisory' AND granted), bool_or(NOT granted) \
                         FROM pg_locks WHERE pid = $1",
                        &[&holder_pid],
                    )
                    .expect("pg_locks answers");
                (
                    row.get::<_, Option<bool>>(0).unwrap_or(false),
                    row.get::<_, Option<bool>>(1).unwrap_or(false),
                )
            };
            if holds && waits {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the recorded write never held its record lock while waiting on the row"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // Results are collected before any assertion, so the row is always released and the
        // held write always ends.
        let across = west_writer.commit_recorded(&west_commit, Expect::Absent);
        let started = std::time::Instant::now();
        let within = east_writer.commit_recorded(&east_commit, Expect::Absent);
        let waited = started.elapsed();
        row_holder.batch_execute("COMMIT").expect("row released");
        (
            held_write.join().expect("no panic"),
            across,
            (within, waited),
        )
    });

    held_write.expect("the held write completes once the row is released");
    across.expect("the same record id in another schema does not wait for this schema's lock");
    let (within, waited) = within;
    let refusal = within.expect_err("the same record id in the same schema waits for the lock");
    // The driver's message says only "db error"; the step and the wait say it was the lock.
    assert!(
        matches!(&refusal, StoreError::Backend(message)
            if message.starts_with("locking record identity: "))
            && waited >= std::time::Duration::from_millis(300),
        "the second writer in one schema waited out its 300 ms lock timeout on the record lock, \
         not another step ({waited:?}): {refusal}"
    );
    west.drop_schema(&west_schema).expect("dropped");
    east.drop_schema(&east_schema).expect("dropped");
}

/// A creation of `id` whose record carries `count` events at revision 1, each numbered in its
/// `changed` map so their stored order can be read back.
fn creation_with_events(id: &str, count: usize) -> entity_core::Decision {
    let mut decision = Runtime::new(&registry())
        .create("ticket", 1, id, json!({"title": "Many"}))
        .expect("permitted");
    let first = decision.record.events[0].clone();
    decision.record.events = (0..count)
        .map(|n| {
            let mut event = first.clone();
            event.changed.insert("n".to_owned(), json!(n));
            event
        })
        .collect();
    decision
}

#[test]
fn a_commit_reads_one_base_position_per_revision_however_many_events_it_appends() {
    // The server counts every scan of a table to the backend that ran it and adds the counts
    // when that backend closes. A first writer leaves one event and a baseline; the second
    // writer's scans of `events` are what appending 128 events cost.
    let Some(url) = url() else { return };
    let (mut first, schema) = fresh(&url, "event_positions");
    first
        .commit(&creation_with_events("first", 1), Expect::Absent)
        .expect("accepted");
    drop(first);
    let mut observer = postgres::Client::connect(&url, postgres::NoTls).expect("observer");
    let mut counts_after = |inserted: i64| -> i64 {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            let row = observer
                .query_one(
                    "SELECT COALESCE(n_tup_ins, 0), \
                            COALESCE(seq_scan, 0) + COALESCE(idx_scan, 0) \
                     FROM pg_stat_user_tables WHERE schemaname = $1 AND relname = 'events'",
                    &[&schema],
                )
                .expect("statistics answer");
            if row.get::<_, i64>(0) >= inserted {
                return row.get(1);
            }
            assert!(
                std::time::Instant::now() < deadline,
                "a closed backend's statistics never arrived"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    };
    let baseline = counts_after(1);

    let mut writer = PostgresStore::connect_in_schema(&url, &schema).expect("writer");
    writer
        .commit(&creation_with_events("many", 128), Expect::Absent)
        .expect("accepted");
    drop(writer);
    let scans = counts_after(129) - baseline;

    let mut raw = postgres::Client::connect(&url, postgres::NoTls).expect("raw connection");
    let stored: Vec<(i64, String)> = raw
        .query(
            &format!(
                "SELECT position, document FROM \"{schema}\".events \
                 WHERE entity = 'ticket' AND id = 'many' AND revision = 1 ORDER BY position"
            ),
            &[],
        )
        .expect("events answer")
        .into_iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect();
    let numbered: Vec<(i64, u64)> = stored
        .iter()
        .map(|(position, document)| {
            let event: entity_core::DomainEvent = serde_json::from_str(document).expect("an event");
            (*position, event.changed["n"].as_u64().expect("numbered"))
        })
        .collect();
    let expected: Vec<(i64, u64)> = (0..128_i64).map(|n| (n, n.unsigned_abs())).collect();
    assert_eq!(
        numbered, expected,
        "positions are consecutive from 0 in the order the decision emitted the events"
    );
    assert_eq!(
        scans, 1,
        "the base position is read once for the revision, not once per event"
    );
    raw.batch_execute(&format!("DROP SCHEMA \"{schema}\" CASCADE"))
        .expect("dropped");
}
