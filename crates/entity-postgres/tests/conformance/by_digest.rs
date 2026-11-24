//! New decision records name their definition by digest, and the definition is kept once per
//! schema; a schema written by 0.33.1, whose records embed it, opens and reads as it did.

use entity_core::{replay, DecisionRecord, Runtime};
use entity_postgres::PostgresStore;
use entity_store::{
    stored_record::stored_definition, Envelope, EventProvider, Expect, HistoryProvider,
    RecordedCommit, Recording, StateProvider, Store, StoreError,
};
use serde_json::json;

use super::{fresh, registry, url};

/// The digest of the `ticket` definition every record of the 0.33.1 fixture embeds, pinned so a
/// change of the definition document or its digest is seen here.
const TICKET_DIGEST: &str =
    "sha256:595a1dfe070a1f6e88c534dfe599b70585a922da39a104e1c1e007c08ed93239";

/// The prefix of a decision document that names its definition by digest.
const BY_DIGEST: &str = "[\"er.stored-record/1\",";

fn recording(record_id: &str) -> Recording {
    Recording {
        record_id: record_id.to_owned(),
        recorded_at: "2026-11-23T12:00:00Z".to_owned(),
        correlation: None,
        causation: None,
        actor: None,
    }
}

fn raw(url: &str, schema: &str) -> postgres::Client {
    let mut client = postgres::Client::connect(url, postgres::NoTls).expect("raw connection");
    client
        .batch_execute(&format!("SET search_path TO \"{schema}\""))
        .expect("schema selected");
    client
}

/// Every `(record_id, document)` of `history`, in subject and position order.
fn history_rows(client: &mut postgres::Client) -> Vec<(String, String)> {
    client
        .query(
            "SELECT record_id, document FROM history ORDER BY entity, id, position",
            &[],
        )
        .expect("history answers")
        .into_iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect()
}

#[test]
fn thirty_two_records_of_one_type_keep_one_copy_of_the_definition() {
    let Some(url) = url() else { return };
    let (mut store, schema) = fresh(&url, "by_digest_thirty_two");
    let registry = registry();
    let runtime = Runtime::new(&registry);
    let mut committed = Vec::new();
    for n in 0..32 {
        let created = runtime
            .create("ticket", 1, format!("t{n:02}"), json!({"title": "T"}))
            .expect("permitted");
        let commit = RecordedCommit::new(created, &recording(&format!("r{n:02}"))).expect("seals");
        store
            .commit_recorded(&commit, Expect::Absent)
            .expect("accepted");
        committed.push(commit.envelope);
    }

    let definition = committed[0]
        .record
        .definition
        .as_ref()
        .expect("a decision carries its definition");
    let one_copy = stored_definition(definition).expect("serializes");
    let mut client = raw(&url, &schema);
    let row = client
        .query_one(
            "SELECT count(*), COALESCE(sum(octet_length(document)), 0)::BIGINT FROM definitions",
            &[],
        )
        .expect("the schema keeps its definitions in a table of their own");
    assert_eq!(
        (row.get::<_, i64>(0), row.get::<_, i64>(1)),
        (1, i64::try_from(one_copy.document.len()).expect("fits")),
        "32 records of one type keep one definition document"
    );
    let rows = history_rows(&mut client);
    assert_eq!(rows.len(), 32);
    for (record_id, document) in &rows {
        assert!(
            document.starts_with(BY_DIGEST) && document.contains(&one_copy.digest),
            "{record_id} names the definition by digest: {document}"
        );
        assert!(
            !document.contains("\"lifecycle\""),
            "{record_id} holds no copy of the definition: {document}"
        );
    }
    let read: Vec<Envelope<DecisionRecord>> = (0..32)
        .flat_map(|n| {
            store
                .records("ticket", &format!("t{n:02}"))
                .expect("history")
        })
        .collect();
    assert_eq!(
        read, committed,
        "every record reads back with its definition"
    );
    drop(client);
    store.drop_schema(&schema).expect("dropped");
}

/// `pg_dump` of the schema `fixture-gen` wrote with the released 0.33.1 `PostgresStore`, with the
/// schema's name replaced by `__SCHEMA__`: `t1` created and closed by recorded commits and observed
/// once, `t2` created by a recorded commit, `t3` by a plain commit with its event in `events`.
const FIXTURE_0_33_1: &str = include_str!("../fixtures/store-0.33.1.sql");

#[test]
fn a_schema_written_by_0_33_1_reads_its_records_unchanged_and_takes_new_ones_by_digest() {
    let Some(url) = url() else { return };
    let schema = format!("entity_test_{}_by_digest_from_0_33_1", std::process::id());
    let mut client = postgres::Client::connect(&url, postgres::NoTls).expect("raw connection");
    client
        .batch_execute(&FIXTURE_0_33_1.replace("__SCHEMA__", &schema))
        .expect("the 0.33.1 schema is restored");
    client
        .batch_execute(&format!("SET search_path TO \"{schema}\""))
        .expect("schema selected");
    let before = history_rows(&mut client);
    // What 0.33.1 itself reads: every decision document is its envelope.
    let as_written: Vec<Envelope<DecisionRecord>> = before
        .iter()
        .filter(|(record_id, _)| record_id.starts_with("r-"))
        .map(|(_, document)| serde_json::from_str(document).expect("an embedded envelope"))
        .collect();
    assert_eq!(
        as_written.len(),
        3,
        "the fixture holds three decision records"
    );

    let mut store =
        PostgresStore::connect_in_schema(&url, &schema).expect("the 0.33.1 schema opens");
    let t1 = store.records("ticket", "t1").expect("history");
    let t2 = store.records("ticket", "t2").expect("history");
    assert_eq!(
        [t1.clone(), t2.clone()].concat(),
        as_written,
        "the records 0.33.1 wrote read as 0.33.1 read them"
    );
    for envelope in t1.iter().chain(&t2) {
        let definition = envelope.record.definition.as_ref().expect("embedded");
        assert_eq!(
            stored_definition(definition).expect("serializes").digest,
            TICKET_DIGEST,
            "{}",
            envelope.record_id
        );
    }
    let records = |history: &[Envelope<DecisionRecord>]| -> Vec<DecisionRecord> {
        history
            .iter()
            .map(|envelope| envelope.record.clone())
            .collect()
    };
    assert_eq!(
        replay(&records(&t1)).expect("replays"),
        store.load("ticket", "t1").expect("answers").expect("held")
    );
    assert_eq!(
        store
            .observations("ticket", "t1")
            .expect("observations")
            .len(),
        1
    );
    assert_eq!(store.events("ticket", "t1").expect("events").len(), 2);
    assert_eq!(store.events("ticket", "t3").expect("events").len(), 1);

    // A retry of a record 0.33.1 wrote is the same record; other bytes under its id are not.
    let retry = RecordedCommit {
        instance: t1[0].record.result.clone(),
        envelope: t1[0].clone(),
    };
    store
        .commit_recorded(&retry, Expect::Absent)
        .expect("a retry of a 0.33.1 record is idempotent");
    let mut altered = retry.clone();
    altered.envelope.actor = Some("someone else".to_owned());
    assert_eq!(
        store.commit_recorded(&altered, Expect::Absent),
        Err(StoreError::RecordConflict {
            record_id: "r-t1-1".to_owned()
        })
    );

    // A new record of t2 names the definition by digest; its old record keeps its bytes.
    let registry = registry();
    let closed = Runtime::new(&registry)
        .execute(&t2[0].record.result, "close", json!({}))
        .expect("permitted");
    let commit = RecordedCommit::new(closed, &recording("r-t2-2")).expect("seals");
    store
        .commit_recorded(&commit, Expect::Revision(1))
        .expect("accepted");
    store
        .commit_recorded(&commit, Expect::Revision(1))
        .expect("a retry of a by-digest record is idempotent");
    let after = history_rows(&mut client);
    for row in &before {
        assert!(after.contains(row), "{} keeps its bytes", row.0);
    }
    let new_row = after
        .iter()
        .find(|(record_id, _)| record_id == "r-t2-2")
        .expect("the new record");
    assert!(new_row.1.starts_with(BY_DIGEST), "{}", new_row.1);
    assert!(new_row.1.contains(TICKET_DIGEST), "{}", new_row.1);
    let t2 = store.records("ticket", "t2").expect("history");
    assert_eq!(t2.len(), 2);
    assert_eq!(t2[1], commit.envelope);
    assert_eq!(store.events("ticket", "t2").expect("events").len(), 2);
    assert_eq!(
        replay(&records(&t2)).expect("a mixed history replays"),
        store.load("ticket", "t2").expect("answers").expect("held")
    );
    drop(client);
    store.drop_schema(&schema).expect("dropped");
}

#[test]
fn a_decision_record_in_the_embedded_form_is_refused_once_the_schema_names_definitions_by_digest() {
    let Some(url) = url() else { return };
    let (mut store, schema) = fresh(&url, "by_digest_fenced");
    let registry = registry();
    let created = Runtime::new(&registry)
        .create("ticket", 1, "new", json!({"title": "T"}))
        .expect("permitted");
    store
        .commit_recorded(
            &RecordedCommit::new(created, &recording("r-new")).expect("seals"),
            Expect::Absent,
        )
        .expect("accepted");

    let mut client = raw(&url, &schema);
    // What a 0.33.1 writer inserts for a recorded decision.
    let older = RecordedCommit::new(
        Runtime::new(&registry)
            .create("ticket", 1, "old", json!({"title": "T"}))
            .expect("permitted"),
        &recording("r-old"),
    )
    .expect("seals");
    let refused = client
        .execute(
            "INSERT INTO history (entity, id, position, kind, record_id, document) \
             VALUES ('ticket', 'old', 0, 'decision', 'r-old', $1)",
            &[&serde_json::to_string(&older.envelope).expect("serializes")],
        )
        .expect_err("an older writer's decision record is refused");
    let message = refused
        .as_db_error()
        .map(|error| error.message().to_owned())
        .unwrap_or_default();
    assert!(
        message.contains("names definitions by digest"),
        "{refused:?}"
    );
    // What a 0.33.1 reader does with a new record: it refuses it rather than misreading it. Serde
    // reads an envelope from an array by position, so the framing would be the record id and the
    // digest the timestamp; the envelope itself, where `correlation` belongs, is what fails.
    let (_, document) = history_rows(&mut client).pop().expect("the new record");
    let misread = serde_json::from_str::<Envelope<DecisionRecord>>(&document)
        .expect_err("an older reader cannot take a by-digest record for an envelope");
    assert!(
        misread
            .to_string()
            .contains("invalid type: map, expected a string"),
        "{misread}"
    );
    drop(client);
    store.drop_schema(&schema).expect("dropped");
}

#[test]
fn a_definition_changed_or_missing_under_its_digest_is_refused_on_read() {
    let Some(url) = url() else { return };
    let (mut store, schema) = fresh(&url, "by_digest_integrity");
    let registry = registry();
    let created = Runtime::new(&registry)
        .create("ticket", 1, "one", json!({"title": "T"}))
        .expect("permitted");
    store
        .commit_recorded(
            &RecordedCommit::new(created, &recording("r-one")).expect("seals"),
            Expect::Absent,
        )
        .expect("accepted");
    let mut client = raw(&url, &schema);
    client
        .execute(
            "UPDATE definitions SET document = replace(document, '\"closed\"', '\"shut\"')",
            &[],
        )
        .expect("tampered");
    let Err(StoreError::Backend(message)) = store.records("ticket", "one") else {
        panic!("a changed definition is refused");
    };
    assert!(message.contains("does not hash to its digest"), "{message}");
    client
        .execute("DELETE FROM definitions", &[])
        .expect("removed");
    let Err(StoreError::Backend(message)) = store.records("ticket", "one") else {
        panic!("a missing definition is refused");
    };
    assert!(
        message.contains("which this store does not hold"),
        "{message}"
    );
    drop(client);
    store.drop_schema(&schema).expect("dropped");
}
