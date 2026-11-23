//! File Store v3: current state apart from an append-only history, definitions by digest, and the
//! out-of-place migration from v2.
//!
//! `fixtures/file-store-0.33.1/` is a v2 store written by the released 0.33.1 library, with what
//! 0.33.1 read back from it (`expected.json`); its README says how it was made.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use entity_core::{
    replay, DecisionCommand, DecisionRecord, DomainEvent, EntityDefinition, EntityInstance,
    Registry, Runtime,
};
use entity_store::asynchronous::{
    definition_comparison_bytes, definition_digest, record_comparison_bytes_in, HistoryOrigin,
    LegacyEvidence, RecordForm, RecordedEntry,
};
use entity_store::{
    migrate_file_store_v2, Envelope, EventProvider, Expect, FileStore, HistoryProvider,
    LegacyStoreSource, RecordedCommit, RecordedObservation, Recording, StateProvider, Store,
    StoreError,
};
use serde::Deserialize;
use serde_json::{json, Value};

const V2_MARKER: &str = "entity.file-store/2\n";
const V3_MARKER: &str = "entity.file-store/3\n";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/file-store-0.33.1")
}

fn scratch(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("file-store-v3")
        .join(name);
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("scratch root");
    root
}

/// Copies a directory tree byte for byte; the committed fixture is never opened as a store.
fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("copy destination");
    for entry in fs::read_dir(from).expect("fixture listed") {
        let entry = entry.expect("fixture entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("fixture entry type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("fixture file copied");
        }
    }
}

/// Every file under `root`, relative path to bytes.
fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, at: &Path, into: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(at).expect("listed") {
            let entry = entry.expect("entry");
            let path = entry.path();
            if entry.file_type().expect("type").is_dir() {
                walk(root, &path, into);
            } else {
                into.insert(
                    path.strip_prefix(root).expect("under root").to_owned(),
                    fs::read(&path).expect("read"),
                );
            }
        }
    }
    let mut all = BTreeMap::new();
    walk(root, root, &mut all);
    all
}

#[derive(Deserialize)]
struct Expected {
    definitions: Vec<Value>,
    subjects: Vec<ExpectedSubject>,
}

#[derive(Deserialize)]
struct ExpectedSubject {
    entity: String,
    id: String,
    instance: EntityInstance,
    records: Vec<Envelope<DecisionRecord>>,
    observations: Vec<RecordedObservation>,
    events: Vec<DomainEvent>,
    record_bytes_embedded: Vec<String>,
    record_bytes_by_digest: Vec<String>,
}

fn expected() -> Expected {
    serde_json::from_slice(&fs::read(fixture().join("expected.json")).expect("expected.json"))
        .expect("expected.json parses")
}

fn registry() -> Registry {
    let mut registry = Registry::new();
    for definition in expected().definitions {
        registry
            .register(serde_json::from_value(definition).expect("fixture definition"))
            .expect("it validates");
    }
    registry
}

fn recording(record_id: &str) -> Recording {
    Recording {
        record_id: record_id.to_owned(),
        recorded_at: "2026-11-23T12:00:00Z".to_owned(),
        correlation: None,
        causation: None,
        actor: Some("v3-test".to_owned()),
    }
}

/// `store` serves exactly what 0.33.1 read from the fixture: identities, state, history, record
/// bytes in both record forms, observations and events.
fn assert_serves_what_0_33_1_read(store: &FileStore, label: &str) {
    let expected = expected();
    for entity in ["counter", "note"] {
        let mut ids: Vec<String> = expected
            .subjects
            .iter()
            .filter(|subject| subject.entity == entity)
            .map(|subject| subject.id.clone())
            .collect();
        ids.sort();
        assert_eq!(
            store.ids(entity).expect("ids"),
            ids,
            "{label}: ids of {entity}"
        );
    }
    for subject in &expected.subjects {
        let (entity, id) = (subject.entity.as_str(), subject.id.as_str());
        assert_eq!(
            store.load(entity, id).expect("load").as_ref(),
            Some(&subject.instance),
            "{label}: state of {entity}/{id}"
        );
        let records = store.records(entity, id).expect("records");
        assert_eq!(
            records, subject.records,
            "{label}: history of {entity}/{id}"
        );
        for (position, envelope) in records.iter().enumerate() {
            let entry = RecordedEntry::Decision(RecordedCommit {
                instance: envelope.record.result.clone(),
                envelope: envelope.clone(),
            });
            for (form, bytes) in [
                (
                    RecordForm::DefinitionEmbedded,
                    &subject.record_bytes_embedded[position],
                ),
                (
                    RecordForm::DefinitionByDigest,
                    &subject.record_bytes_by_digest[position],
                ),
            ] {
                assert_eq!(
                    String::from_utf8(record_comparison_bytes_in(form, &entry).expect("encodes"))
                        .expect("UTF-8"),
                    *bytes,
                    "{label}: {form:?} bytes of {entity}/{id} record {position}"
                );
            }
        }
        assert_eq!(
            store.observations(entity, id).expect("observations"),
            subject.observations,
            "{label}: observations of {entity}/{id}"
        );
        assert_eq!(
            store.events(entity, id).expect("events"),
            subject.events,
            "{label}: events of {entity}/{id}"
        );
        let loaded = store
            .load_with_history(entity, id)
            .expect("load with history")
            .expect("stored");
        assert_eq!(loaded.instance, subject.instance, "{label}: {entity}/{id}");
        assert_eq!(
            loaded.records.as_ref(),
            Some(&subject.records),
            "{label}: {entity}/{id}"
        );
    }
}

#[test]
fn a_store_written_by_0_33_1_is_still_read_and_written_as_v2() {
    let root = scratch("v2-kept").join("store");
    copy_tree(&fixture().join("store"), &root);
    let mut store = FileStore::open(&root);
    assert_serves_what_0_33_1_read(&store, "v2 read by this release");

    let registry = registry();
    let held = store.load("counter", "c2").expect("load").expect("stored");
    let viewed = Runtime::new(&registry)
        .execute(&held, "view", json!({}))
        .expect("view is permitted");
    store
        .commit_recorded(
            &RecordedCommit::new(viewed, &recording("c2-view")).expect("valid"),
            Expect::Revision(1),
        )
        .expect("a v2 store accepts a recorded write");
    assert_eq!(
        fs::read_to_string(root.join(".entity-store-format")).expect("marker"),
        V2_MARKER,
        "a v2 root is never converted in place"
    );
    let document: Value = serde_json::from_slice(
        &fs::read(root.join("subjects/636f756e746572/6332.json")).expect("v2 subject"),
    )
    .expect("v2 subject parses");
    assert_eq!(document["format"], "entity.file-subject/2");
    assert_eq!(
        document["records"].as_array().expect("records").len(),
        2,
        "the write went into the v2 subject document"
    );
}

#[test]
fn migrating_a_0_33_1_store_to_v3_keeps_every_record_its_bytes_and_its_history() {
    let base = scratch("migrate");
    let source = base.join("v2");
    copy_tree(&fixture().join("store"), &source);
    let before = files(&source);
    let expected = expected();
    let legacy_events: usize = expected
        .subjects
        .iter()
        .map(|subject| {
            subject.events.len()
                - subject
                    .records
                    .iter()
                    .map(|envelope| envelope.record.events.len())
                    .sum::<usize>()
        })
        .sum();

    let dry = migrate_file_store_v2(&source, base.join("dry"), true).expect("dry run validates");
    assert_eq!(
        (dry.subjects, dry.events, dry.dry_run),
        (expected.subjects.len(), legacy_events, true)
    );
    assert!(
        !base.join("dry").exists(),
        "a dry run writes no destination"
    );

    let destination = base.join("v3");
    let report = migrate_file_store_v2(&source, &destination, false).expect("migrated");
    assert_eq!(
        (report.subjects, report.events, report.dry_run),
        (expected.subjects.len(), legacy_events, false)
    );
    assert_eq!(files(&source), before, "the v2 source is never modified");
    assert_eq!(
        fs::read_to_string(destination.join(".entity-store-format")).expect("marker"),
        V3_MARKER
    );

    let store = FileStore::open(&destination);
    assert_serves_what_0_33_1_read(&store, "v3 after migration");

    // Verifies: every history that starts with a creation replays to its last record's result,
    // and the migrated store is acquirable as complete subjects.
    for subject in &expected.subjects {
        let records = store
            .records(&subject.entity, &subject.id)
            .expect("records");
        if records.first().is_some_and(|envelope| {
            matches!(envelope.record.command, DecisionCommand::Create { .. })
        }) {
            let decisions: Vec<DecisionRecord> = records
                .iter()
                .map(|envelope| envelope.record.clone())
                .collect();
            assert_eq!(
                replay(&decisions).expect("the history replays"),
                records.last().expect("one record").record.result,
                "{}/{}",
                subject.entity,
                subject.id
            );
        }
    }
    let mut acquiring = FileStore::open(&destination);
    let snapshot = acquiring
        .acquire_legacy("file/migrated-v3")
        .expect("a v3 store is acquirable");
    assert_eq!(snapshot.histories.len(), expected.subjects.len());
    // Source locators name the subject as they did on v2, whatever file v3 keeps it in.
    let c1 = snapshot
        .histories
        .iter()
        .find(|history| history.subject.id == "c1")
        .expect("c1 acquired");
    let HistoryOrigin::Imported(anchor) = &c1.origin else {
        panic!("an acquired subject is imported");
    };
    let locators: Vec<&str> = anchor
        .evidence
        .iter()
        .filter_map(|evidence| match evidence {
            LegacyEvidence::Envelope(envelope) => Some(envelope.source_locator.as_str()),
            LegacyEvidence::Event(_) | LegacyEvidence::Decision(_) => None,
        })
        .collect();
    assert_eq!(locators.len(), 10, "nine records and one observation");
    assert_eq!(locators[0], "subjects/636f756e746572/6331.json#records/0");
    assert_eq!(
        locators[9],
        "subjects/636f756e746572/6331.json#observations/0"
    );

    // One document per definition, named by its digest; no subject file holds a definition.
    let definitions: Vec<PathBuf> = fs::read_dir(destination.join("definitions"))
        .expect("definitions directory")
        .map(|entry| entry.expect("entry").path())
        .collect();
    assert_eq!(
        definitions.len(),
        expected.definitions.len(),
        "{definitions:?}"
    );
    for path in &definitions {
        let bytes = fs::read(path).expect("definition document");
        assert_eq!(
            definition_digest(&bytes),
            format!(
                "sha256:{}",
                path.file_stem().expect("name").to_str().expect("UTF-8")
            ),
            "a definition document is named by its digest"
        );
    }
    for (path, bytes) in files(&destination.join("subjects")) {
        let text = String::from_utf8(bytes).expect("UTF-8 subject file");
        assert!(
            !text.contains("\"lifecycle\""),
            "{} holds a definition",
            path.display()
        );
    }
}

/// Bytes this thread has written since it started, from `/proc/thread-self/io`, which other test
/// threads do not move.
#[cfg(target_os = "linux")]
fn thread_written_bytes() -> u64 {
    let text = fs::read_to_string("/proc/thread-self/io").expect("/proc/thread-self/io");
    text.lines()
        .find_map(|line| line.strip_prefix("wchar:"))
        .expect("wchar line")
        .trim()
        .parse()
        .expect("a number")
}

#[cfg(target_os = "linux")]
#[test]
fn one_recorded_execute_on_v3_writes_the_same_bytes_at_8_and_32_prior_records() {
    let registry = registry();
    let root = scratch("write-bytes").join("store");
    let created = Runtime::new(&registry)
        .create("counter", 1, "c", json!({"name": "measured"}))
        .expect("create");
    let commit = RecordedCommit::new(created, &recording("create")).expect("valid");
    FileStore::open(&root)
        .commit_recorded(&commit, Expect::Absent)
        .expect("created");
    let mut instance = commit.instance;
    let log = root.join("subjects/636f756e746572/63.log");
    let mut written = BTreeMap::new();
    let mut appended = BTreeMap::new();
    for prior in 1..=32u64 {
        let viewed = Runtime::new(&registry)
            .execute(&instance, "view", json!({}))
            .expect("view");
        let commit =
            RecordedCommit::new(viewed, &recording(&format!("view-{prior}"))).expect("valid");
        let log_before = fs::metadata(&log).expect("history file").len();
        // A fresh handle per command, as each `entity execute` process opens one.
        let mut store = FileStore::open(&root);
        let start = thread_written_bytes();
        store
            .commit_recorded(&commit, Expect::Revision(instance.revision))
            .expect("recorded");
        written.insert(prior, thread_written_bytes() - start);
        appended.insert(
            prior,
            fs::metadata(&log).expect("history file").len() - log_before,
        );
        instance = commit.instance;
    }
    let record = appended[&32];
    assert!(
        written[&32].abs_diff(written[&8]) <= record,
        "bytes written by one execute at 8 prior records ({}) and at 32 ({}) differ by more than \
         one record ({record} bytes)",
        written[&8],
        written[&32]
    );
    assert!(
        written[&32] < 3 * record,
        "one execute writes its record and the state, not the history: {} bytes for a {record}-byte \
         record",
        written[&32]
    );
    assert_eq!(
        FileStore::open(&root)
            .records("counter", "c")
            .expect("records")
            .len(),
        33
    );
}

#[test]
fn thirty_two_records_of_one_type_keep_one_definition_document() {
    let registry = registry();
    let root = scratch("one-definition").join("store");
    let mut store = FileStore::open(&root);
    let runtime = Runtime::new(&registry);
    let mut written = 0;
    for subject in 0..4 {
        let id = format!("c{subject}");
        let created = runtime
            .create("counter", 1, &id, json!({"name": id}))
            .expect("create");
        let commit = RecordedCommit::new(created, &recording(&format!("{id}-0"))).expect("valid");
        store
            .commit_recorded(&commit, Expect::Absent)
            .expect("created");
        written += 1;
        let mut instance = commit.instance;
        for step in 1..8 {
            let viewed = runtime.execute(&instance, "view", json!({})).expect("view");
            let commit =
                RecordedCommit::new(viewed, &recording(&format!("{id}-{step}"))).expect("valid");
            store
                .commit_recorded(&commit, Expect::Revision(instance.revision))
                .expect("recorded");
            written += 1;
            instance = commit.instance;
        }
    }
    assert_eq!(written, 32);

    let reader = FileStore::open(&root);
    let records = reader.records("counter", "c3").expect("records");
    let definition: &EntityDefinition = records[0]
        .record
        .definition
        .as_ref()
        .expect("a record served with its definition");
    let document = definition_comparison_bytes(definition).expect("encodes");
    let stored: Vec<Vec<u8>> = files(&root.join("definitions")).into_values().collect();
    assert_eq!(
        stored,
        std::slice::from_ref(&document),
        "32 records of one type hold exactly one definition document"
    );
    let definition_bytes: usize = stored.iter().map(Vec::len).sum();
    assert_eq!(definition_bytes, document.len());
    for (path, bytes) in files(&root.join("subjects")) {
        assert!(
            !String::from_utf8(bytes)
                .expect("UTF-8")
                .contains("\"lifecycle\""),
            "{} holds a copy of the definition",
            path.display()
        );
    }
    for subject in 0..4 {
        for envelope in reader
            .records("counter", &format!("c{subject}"))
            .expect("records")
        {
            assert_eq!(envelope.record.definition.as_ref(), Some(definition));
        }
    }
}

/// A v3 store of one counter with a creation and one view, and the paths of its files.
fn two_records(name: &str) -> (PathBuf, Registry, EntityInstance) {
    let registry = registry();
    let root = scratch(name).join("store");
    let runtime = Runtime::new(&registry);
    let created = runtime
        .create("counter", 1, "c", json!({"name": "torn"}))
        .expect("create");
    let commit = RecordedCommit::new(created, &recording("create")).expect("valid");
    let mut store = FileStore::open(&root);
    store
        .commit_recorded(&commit, Expect::Absent)
        .expect("created");
    let viewed = runtime
        .execute(&commit.instance, "view", json!({}))
        .expect("view");
    let commit = RecordedCommit::new(viewed, &recording("view")).expect("valid");
    store
        .commit_recorded(&commit, Expect::Revision(1))
        .expect("viewed");
    (root, registry, commit.instance)
}

const LOG: &str = "subjects/636f756e746572/63.log";
const STATE: &str = "subjects/636f756e746572/63.json";

#[test]
fn a_torn_history_tail_is_ignored_by_readers_and_cut_by_the_next_write() {
    let (root, registry, instance) = two_records("torn-tail");
    let log = root.join(LOG);
    let intact = fs::read(&log).expect("history");
    let mut torn = intact.clone();
    torn.extend_from_slice(b"{\"record\":{\"record_id\":\"lost");
    fs::write(&log, &torn).expect("a write interrupted mid-record");

    let store = FileStore::open(&root);
    assert_eq!(
        store.load("counter", "c").expect("load"),
        Some(instance.clone())
    );
    assert_eq!(store.records("counter", "c").expect("records").len(), 2);
    assert_eq!(store.ids("counter").expect("ids"), ["c"]);

    let closed = Runtime::new(&registry)
        .execute(&instance, "close", json!({}))
        .expect("close");
    let mut writer = FileStore::open(&root);
    writer
        .commit_recorded(
            &RecordedCommit::new(closed, &recording("close")).expect("valid"),
            Expect::Revision(2),
        )
        .expect("the next write succeeds");
    let after = fs::read(&log).expect("history");
    assert!(
        after.starts_with(&intact)
            && !after[intact.len()..].starts_with(b"{\"record\":{\"record_id\":\"lost"),
        "the next write cuts the torn tail before it appends"
    );
    assert!(after.ends_with(b"\n"));
    let records = FileStore::open(&root)
        .records("counter", "c")
        .expect("records");
    assert_eq!(
        records
            .iter()
            .map(|envelope| envelope.record_id.as_str())
            .collect::<Vec<_>>(),
        ["create", "view", "close"]
    );
}

#[test]
fn a_state_document_behind_its_history_is_rebuilt_from_the_history() {
    let (root, registry, instance) = two_records("stale-state");
    let state = root.join(STATE);
    let stale = fs::read(&state).expect("state document");
    let closed = Runtime::new(&registry)
        .execute(&instance, "close", json!({}))
        .expect("close");
    let closed_instance = closed.instance.clone();
    FileStore::open(&root)
        .commit_recorded(
            &RecordedCommit::new(closed, &recording("close")).expect("valid"),
            Expect::Revision(2),
        )
        .expect("closed");

    // A crash after the history append, before the state replacement reached the disk.
    fs::write(&state, &stale).expect("state rolled back");
    assert_eq!(
        FileStore::open(&root).load("counter", "c").expect("load"),
        Some(closed_instance.clone()),
        "the history is the authority; a state document behind it is not served"
    );
    fs::write(&state, b"{\"format\":\"entity.file-subj").expect("state torn");
    assert_eq!(
        FileStore::open(&root).load("counter", "c").expect("load"),
        Some(closed_instance.clone())
    );
    fs::remove_file(&state).expect("state lost");
    let store = FileStore::open(&root);
    assert_eq!(
        store.load("counter", "c").expect("load"),
        Some(closed_instance)
    );
    assert_eq!(store.ids("counter").expect("ids"), ["c"]);
}

#[test]
fn a_definition_document_that_does_not_hash_to_its_name_is_refused() {
    let (root, _, _) = two_records("tampered-definition");
    let path = fs::read_dir(root.join("definitions"))
        .expect("definitions")
        .next()
        .expect("one definition")
        .expect("entry")
        .path();
    let mut bytes = fs::read(&path).expect("definition");
    let at = bytes
        .windows(b"\"closed\"".len())
        .position(|window| window == b"\"closed\"")
        .expect("a state name in the definition");
    bytes[at + 1] = b'C';
    fs::write(&path, bytes).expect("tampered");
    let refused = FileStore::open(&root).records("counter", "c");
    assert!(
        matches!(&refused, Err(StoreError::Backend(detail))
            if detail.contains("does not hash to") && detail.contains("definition")),
        "{refused:?}"
    );
}

#[test]
fn a_history_line_that_does_not_parse_is_refused_rather_than_skipped() {
    let (root, _, _) = two_records("corrupt-line");
    let log = root.join(LOG);
    let text = fs::read_to_string(&log).expect("history");
    let mut lines: Vec<&str> = text.lines().collect();
    lines[1] = "{\"record\": not json}";
    fs::write(&log, lines.join("\n") + "\n").expect("corrupted");
    let refused = FileStore::open(&root).records("counter", "c");
    assert!(
        matches!(&refused, Err(StoreError::Backend(detail)) if detail.contains("line 2")),
        "{refused:?}"
    );
}

#[cfg(unix)]
#[test]
fn v3_subject_files_are_private_and_a_restricted_state_keeps_its_restriction() {
    use std::os::unix::fs::PermissionsExt;
    let mode = |path: &Path| fs::metadata(path).expect("metadata").permissions().mode() & 0o777;
    let (root, registry, instance) = two_records("private-v3");
    assert_eq!(mode(&root.join(LOG)), 0o600, "history");
    assert_eq!(mode(&root.join(STATE)), 0o600, "state");
    fs::set_permissions(root.join(STATE), fs::Permissions::from_mode(0o400)).expect("restricted");
    fs::set_permissions(root.join(LOG), fs::Permissions::from_mode(0o640)).expect("widened");
    let closed = Runtime::new(&registry)
        .execute(&instance, "close", json!({}))
        .expect("close");
    FileStore::open(&root)
        .commit_recorded(
            &RecordedCommit::new(closed, &recording("close")).expect("valid"),
            Expect::Revision(2),
        )
        .expect("closed");
    assert_eq!(
        mode(&root.join(STATE)),
        0o400,
        "the replaced state keeps its bits"
    );
    assert_eq!(
        mode(&root.join(LOG)),
        0o640,
        "the history is appended in place"
    );
}

#[test]
fn plain_commits_and_observations_round_trip_on_v3_in_order() {
    let registry = registry();
    let root = scratch("plain-v3").join("store");
    let runtime = Runtime::new(&registry);
    let mut store = FileStore::open(&root);
    let created = runtime
        .create("note", 1, "n", json!({"text": "plain"}))
        .expect("create");
    store
        .commit(&created, Expect::Absent)
        .expect("plain create");
    let finished = runtime
        .execute(&created.instance, "finish", json!({}))
        .expect("finish");
    store
        .commit(&finished, Expect::Revision(1))
        .expect("plain finish");
    let observation = RecordedObservation {
        entity: "note".to_owned(),
        id: "n".to_owned(),
        revision: 2,
        envelope: recording("seen").seal(json!({"n": 1})).expect("valid"),
    };
    store.observe(&observation).expect("observed");
    store
        .observe(&observation)
        .expect("an identical observation is a retry");

    let reader = FileStore::open(&root);
    assert_eq!(
        reader.load("note", "n").expect("load"),
        Some(finished.instance.clone())
    );
    let mut events = created.record.events.clone();
    events.extend(finished.record.events.clone());
    assert_eq!(reader.events("note", "n").expect("events"), events);
    assert_eq!(
        reader.observations("note", "n").expect("observations"),
        [observation]
    );
    assert!(reader.records("note", "n").expect("records").is_empty());
}
