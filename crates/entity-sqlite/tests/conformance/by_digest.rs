//! New decision records name their definition by digest, and the definition is kept once per
//! store; a store written by 0.33.1, whose records embed it, opens and reads as it did.

use std::path::{Path, PathBuf};

use entity_core::{replay, DecisionRecord, Runtime};
use entity_sqlite::SqliteStore;
use entity_store::{
    stored_record::stored_definition, Envelope, EventProvider, Expect, HistoryProvider,
    RecordedCommit, Recording, StateProvider, Store, StoreError,
};
use serde_json::json;

use super::registry;

/// The digest of the `ticket` definition every record of the 0.33.1 fixture embeds, pinned so a
/// change of the definition document or its digest is seen here.
const TICKET_DIGEST: &str =
    "sha256:595a1dfe070a1f6e88c534dfe599b70585a922da39a104e1c1e007c08ed93239";

/// The prefix of a decision document that names its definition by digest.
const BY_DIGEST: &str = "[\"er.stored-record/1\",";

fn scratch(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("sqlite-by-digest")
        .join(format!("{name}.sqlite3"));
    std::fs::create_dir_all(path.parent().expect("parent")).expect("scratch directory");
    let _ = std::fs::remove_file(&path);
    path
}

fn recording(record_id: &str) -> Recording {
    Recording {
        record_id: record_id.to_owned(),
        recorded_at: "2026-11-23T12:00:00Z".to_owned(),
        correlation: None,
        causation: None,
        actor: None,
    }
}

/// Every `(record_id, document)` of `history`, in table order.
fn history_rows(path: &Path) -> Vec<(String, String)> {
    let connection = rusqlite::Connection::open(path).expect("a second connection");
    let mut statement = connection
        .prepare("SELECT record_id, document FROM history ORDER BY entity, id, position")
        .expect("prepared");
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("history answers")
        .map(|row| row.expect("a row"))
        .collect()
}

#[test]
fn thirty_two_records_of_one_type_keep_one_copy_of_the_definition() {
    let path = scratch("thirty-two");
    let registry = registry();
    let runtime = Runtime::new(&registry);
    let mut store = SqliteStore::open(&path).expect("a database");
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
    let connection = rusqlite::Connection::open(&path).expect("a second connection");
    let (copies, bytes): (i64, i64) = connection
        .query_row(
            "SELECT count(*), COALESCE(sum(length(document)), 0) FROM definitions",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("the store keeps its definitions in a table of their own");
    assert_eq!(
        (copies, bytes),
        (1, i64::try_from(one_copy.document.len()).expect("fits")),
        "32 records of one type keep one definition document"
    );
    let rows = history_rows(&path);
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
}

/// The store `fixture-gen` wrote with the released 0.33.1 `SqliteStore`: `t1` created and closed
/// by recorded commits and observed once, `t2` created by a recorded commit, `t3` by a plain
/// commit with its event in the `events` table.
const FIXTURE_0_33_1: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/store-0.33.1.sqlite3"
);

#[test]
fn a_store_written_by_0_33_1_reads_its_records_unchanged_and_takes_new_ones_by_digest() {
    let path = scratch("from-0.33.1");
    std::fs::copy(FIXTURE_0_33_1, &path).expect("fixture copied");
    let before = history_rows(&path);
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

    let mut store = SqliteStore::open(&path).expect("the 0.33.1 store opens");
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
    let after = history_rows(&path);
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
    assert_eq!(
        replay(&records(&t2)).expect("a mixed history replays"),
        store.load("ticket", "t2").expect("answers").expect("held")
    );
}

#[test]
fn a_decision_record_in_the_embedded_form_is_refused_once_the_store_names_definitions_by_digest() {
    let path = scratch("fenced");
    let registry = registry();
    let created = Runtime::new(&registry)
        .create("ticket", 1, "new", json!({"title": "T"}))
        .expect("permitted");
    let commit = RecordedCommit::new(created, &recording("r-new")).expect("seals");
    SqliteStore::open(&path)
        .expect("a database")
        .commit_recorded(&commit, Expect::Absent)
        .expect("accepted");

    let connection = rusqlite::Connection::open(&path).expect("a second connection");
    // What a 0.33.1 writer inserts for a recorded decision.
    let older = RecordedCommit::new(
        Runtime::new(&registry)
            .create("ticket", 1, "old", json!({"title": "T"}))
            .expect("permitted"),
        &recording("r-old"),
    )
    .expect("seals");
    let refused = connection
        .execute(
            "INSERT INTO history (entity, id, position, kind, record_id, document) \
             VALUES ('ticket', 'old', 0, 'decision', 'r-old', ?1)",
            [serde_json::to_string(&older.envelope).expect("serializes")],
        )
        .expect_err("an older writer's decision record is refused");
    assert!(
        refused.to_string().contains("names definitions by digest"),
        "{refused}"
    );
    // What a 0.33.1 reader does with a new record: it refuses it rather than misreading it. Serde
    // reads an envelope from an array by position, so the framing would be the record id and the
    // digest the timestamp; the envelope itself, where `correlation` belongs, is what fails.
    let (_, document) = history_rows(&path).pop().expect("the new record");
    let misread = serde_json::from_str::<Envelope<DecisionRecord>>(&document)
        .expect_err("an older reader cannot take a by-digest record for an envelope");
    assert!(
        misread
            .to_string()
            .contains("invalid type: map, expected a string"),
        "{misread}"
    );
}

#[test]
fn a_definition_changed_or_missing_under_its_digest_is_refused_on_read() {
    let path = scratch("integrity");
    let registry = registry();
    let created = Runtime::new(&registry)
        .create("ticket", 1, "one", json!({"title": "T"}))
        .expect("permitted");
    let store = {
        let mut store = SqliteStore::open(&path).expect("a database");
        store
            .commit_recorded(
                &RecordedCommit::new(created, &recording("r-one")).expect("seals"),
                Expect::Absent,
            )
            .expect("accepted");
        store
    };
    let connection = rusqlite::Connection::open(&path).expect("a second connection");
    connection
        .execute(
            "UPDATE definitions SET document = replace(document, '\"closed\"', '\"shut\"')",
            [],
        )
        .expect("tampered");
    let Err(StoreError::Backend(message)) = store.records("ticket", "one") else {
        panic!("a changed definition is refused");
    };
    assert!(message.contains("does not hash to its digest"), "{message}");
    connection
        .execute("DELETE FROM definitions", [])
        .expect("removed");
    let Err(StoreError::Backend(message)) = store.records("ticket", "one") else {
        panic!("a missing definition is refused");
    };
    assert!(
        message.contains("which this store does not hold"),
        "{message}"
    );
}
