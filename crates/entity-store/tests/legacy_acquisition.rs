//! Exact read-only acquisition of the retained File Store format.

use std::path::{Path, PathBuf};

use entity_core::{Registry, Runtime};
use entity_store::{
    asynchronous::{
        HistoryOrigin, KnownLegacyOrder, LegacyEvidence, LegacyOrderDeclaration, RecordedEntry,
    },
    Expect, FileStore, LegacyStoreSnapshot, LegacyStoreSource, RecordedCommit, RecordedObservation,
    Recording, Store,
};

fn scratch(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("legacy-acquisition")
        .join(name);
    let _ = std::fs::remove_dir_all(&root);
    root
}

fn registry() -> Registry {
    let definition = serde_json::from_value(serde_json::json!({
        "entity": "ticket",
        "version": 1,
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "open", "states": ["open"] },
        "operations": {
            "touch": {
                "transitions": [{ "from": "open", "to": "open" }],
                "emits": [{ "type": "TicketTouched", "payload": { "ticket": "$id" } }]
            }
        }
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    registry
}

fn recording(record_id: &str) -> Recording {
    Recording {
        record_id: record_id.to_owned(),
        recorded_at: "2026-09-16T00:00:00Z".to_owned(),
        correlation: Some("legacy-acquisition".to_owned()),
        causation: None,
        actor: None,
    }
}

#[test]
fn acquiring_an_absent_file_source_creates_no_bytes() {
    let root = scratch("absent");
    let mut source = FileStore::open(&root);
    let snapshot = source
        .acquire_legacy("file/absent")
        .expect("an absent source is an empty boundary");
    assert!(snapshot.histories.is_empty());
    assert!(
        !root.exists(),
        "read-only acquisition must not create a root"
    );
}

#[test]
fn file_acquisition_retains_complete_envelopes_and_only_known_order() {
    let root = scratch("mixed");
    let registry = registry();
    let runtime = Runtime::new(&registry);
    let mut source = FileStore::open(&root);
    let created = runtime
        .create("ticket", 1, "one", serde_json::json!({"title":"one"}))
        .expect("creation");
    source
        .commit(&created, Expect::Absent)
        .expect("legacy creation");
    let touched = runtime
        .execute(&created.instance, "touch", serde_json::json!({}))
        .expect("legacy touch");
    source
        .commit(&touched, Expect::Revision(1))
        .expect("legacy event write");
    let recorded_touch = runtime
        .execute(&touched.instance, "touch", serde_json::json!({}))
        .expect("recorded touch");
    let recorded = RecordedCommit::new(recorded_touch, &recording("touch-3"))
        .expect("complete decision envelope");
    source
        .commit_recorded(&recorded, Expect::Revision(2))
        .expect("recorded decision");
    let observation = RecordedObservation {
        entity: "ticket".to_owned(),
        id: "one".to_owned(),
        revision: 3,
        envelope: recording("observation-3")
            .seal(serde_json::json!({"seen":true}))
            .expect("observation envelope"),
    };
    source.observe(&observation).expect("observation");

    let snapshot = source
        .acquire_legacy("file/source-a")
        .expect("consistent source capture");
    assert_eq!(snapshot.histories.len(), 1);
    let history = &snapshot.histories[0];
    assert_eq!(history.subject.entity, "ticket");
    assert_eq!(history.subject.id, "one");
    assert!(history.records.is_empty());
    let HistoryOrigin::Imported(anchor) = &history.origin else {
        panic!("legacy acquisition must declare its boundary")
    };
    assert_eq!(anchor.instance, recorded.instance);
    assert_eq!(anchor.order, LegacyOrderDeclaration::PerKindOnly);
    assert_eq!(anchor.evidence.len(), 3);
    let LegacyEvidence::Envelope(decision) = &anchor.evidence[0] else {
        panic!("the complete decision envelope is retained")
    };
    assert_eq!(decision.entry, RecordedEntry::Decision(recorded));
    assert_eq!(decision.source_id, "file/source-a");
    assert_eq!(decision.known_order, KnownLegacyOrder::PerKind(0));
    let LegacyEvidence::Envelope(saved_observation) = &anchor.evidence[1] else {
        panic!("the complete observation envelope is retained")
    };
    assert_eq!(
        saved_observation.entry,
        RecordedEntry::Observation(observation)
    );
    assert_eq!(saved_observation.known_order, KnownLegacyOrder::PerKind(0));
    let LegacyEvidence::Event(event) = &anchor.evidence[2] else {
        panic!("the bare legacy event remains explicit")
    };
    assert_eq!(event.event_type, "TicketTouched");
    assert_eq!(event.revision, 2);
}

#[test]
fn acquisition_document_refuses_one_global_record_identity_in_two_subjects() {
    let registry = registry();
    let mut histories = Vec::new();
    for id in ["first", "second"] {
        let root = scratch(&format!("duplicate-global-{id}"));
        let mut source = FileStore::open(&root);
        let decision = Runtime::new(&registry)
            .create("ticket", 1, id, serde_json::json!({"title":id}))
            .expect("creation");
        let recorded = RecordedCommit::new(decision, &recording("same-global-record"))
            .expect("recorded creation");
        source
            .commit_recorded(&recorded, Expect::Absent)
            .expect("isolated source accepts its record");
        histories.extend(
            source
                .acquire_legacy("file/combined")
                .expect("isolated acquisition")
                .histories,
        );
    }
    assert!(
        LegacyStoreSnapshot::new("file/combined", histories).is_err(),
        "one imported document cannot reserve a global record identity twice"
    );
}

#[test]
fn file_acquisition_refuses_a_stale_publication_intent() {
    fn hex(value: &str) -> String {
        value
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    let root = scratch("stale-publication");
    let mut source = FileStore::open(&root);
    let decision = Runtime::new(&registry())
        .create(
            "ticket",
            1,
            "settled",
            serde_json::json!({"title":"settled"}),
        )
        .expect("creation");
    source
        .commit(&decision, Expect::Absent)
        .expect("settled subject");
    let entity_directory = root.join("subjects").join(hex("ticket"));
    std::fs::write(
        entity_directory.join(format!("{}.json.writing.999999.1", hex("unsettled"))),
        b"unsettled bytes",
    )
    .expect("stale publication intent");
    assert!(
        source.acquire_legacy("file/stale-publication").is_err(),
        "acquisition must not silently turn an interrupted publication into absence"
    );
}

/// A pre-0.15 split-file store holding one ticket, migrated to v2 at `<name>/migrated`.
fn migrated(name: &str) -> PathBuf {
    let base = scratch(name);
    let legacy = base.join("legacy");
    std::fs::create_dir_all(legacy.join("ticket")).expect("legacy entity directory");
    let decision = Runtime::new(&registry())
        .create("ticket", 1, "one", serde_json::json!({"title":"one"}))
        .expect("creation");
    std::fs::write(
        legacy.join("ticket").join("one.json"),
        serde_json::to_vec(&decision.instance).expect("instance serializes"),
    )
    .expect("legacy state");
    let destination = base.join("migrated");
    let report =
        entity_store::migrate_file_store_v1(&legacy, &destination, false).expect("migrated");
    assert_eq!(report.subjects, 1);
    destination
}

#[test]
fn a_freshly_migrated_file_store_can_be_acquired_at_once() {
    let root = migrated("migrated-acquisition");
    assert!(
        root.join(".entity-store-lock").is_file(),
        "the migrator publishes the coordination file with the store"
    );
    let snapshot = FileStore::open(&root)
        .acquire_legacy("file/migrated")
        .expect("a migrated store is acquirable without a write first");
    assert_eq!(snapshot.histories.len(), 1);
    assert_eq!(snapshot.histories[0].subject.id, "one");
}

#[test]
fn acquiring_a_v2_store_without_a_lock_file_creates_the_lock_writers_use() {
    let root = migrated("lockless-acquisition");
    // A store migrated by an earlier release, or made by a writer older than 0.17.7.
    std::fs::remove_file(root.join(".entity-store-lock")).expect("lock removed");
    let snapshot = FileStore::open(&root)
        .acquire_legacy("file/lockless")
        .expect("an initialized v2 root without a lock is admitted");
    assert_eq!(snapshot.histories.len(), 1);
    let lock = root.join(".entity-store-lock");
    assert_eq!(
        std::fs::metadata(&lock).expect("lock created").len(),
        0,
        "acquisition creates the lock without advancing the writers' epoch"
    );
}

/// A pre-0.15 split-file store at `<name>/legacy` whose ticket `one` is saved as `state` and whose
/// event log holds `events`.
fn legacy_store(
    name: &str,
    state: &entity_core::EntityInstance,
    events: &[entity_core::DomainEvent],
) -> PathBuf {
    let legacy = scratch(name).join("legacy");
    std::fs::create_dir_all(legacy.join("ticket")).expect("legacy entity directory");
    std::fs::write(
        legacy.join("ticket").join("one.json"),
        serde_json::to_vec(state).expect("instance serializes"),
    )
    .expect("legacy state");
    if !events.is_empty() {
        let log: String = events
            .iter()
            .map(|event| serde_json::to_string(event).expect("event serializes") + "\n")
            .collect();
        std::fs::write(legacy.join("ticket").join("one.events.jsonl"), log).expect("legacy log");
    }
    legacy
}

/// Every path under `root` with its bytes, sorted: what a migration must leave as it found it.
fn tree(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("listing") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                found.push((path.clone(), Vec::new()));
                pending.push(path);
            } else {
                let bytes = std::fs::read(&path).expect("reading");
                found.push((path, bytes));
            }
        }
    }
    found.sort();
    found
}

#[test]
fn a_destination_inside_the_legacy_source_is_refused_before_anything_is_written() {
    let created = Runtime::new(&registry())
        .create("ticket", 1, "one", serde_json::json!({"title":"one"}))
        .expect("creation");
    let legacy = legacy_store("nested-destination", &created.instance, &[]);
    let base = legacy.parent().expect("scratch base").to_path_buf();
    let before = tree(&legacy);
    for destination in [
        legacy.join("converted"),
        legacy.join("ticket").join("converted"),
        // `absent` does not exist; creating it and stepping back out lands in the source.
        base.join("absent").join("..").join("legacy").join("converted"),
    ] {
        for dry_run in [true, false] {
            let error = entity_store::migrate_file_store_v1(&legacy, &destination, dry_run)
                .expect_err("a destination inside the source is refused");
            assert!(
                error.to_string().contains("lies inside the legacy source"),
                "{} (dry run {dry_run}): {error}",
                destination.display()
            );
        }
    }
    assert_eq!(tree(&legacy), before, "the source is left exactly as it was");
    assert!(!base.join("absent").exists(), "no directory was created on the way");
    let report = entity_store::migrate_file_store_v1(&legacy, base.join("converted"), false)
        .expect("a sibling destination still migrates");
    assert_eq!(report.subjects, 1);
    assert_eq!(tree(&legacy), before, "the source is left exactly as it was");
}


/// Set by `a_dry_run_holds_one_legacy_subject_at_a_time` for the child process it measures.
const DRY_RUN_PROBE: &str = "ENTITY_STORE_DRY_RUN_PROBE";

/// One line of `/proc/self/status`, in KiB.
#[cfg(target_os = "linux")]
fn status_kib(field: &str) -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").expect("process status");
    status
        .lines()
        .find_map(|line| line.strip_prefix(field)?.strip_prefix(':'))
        .and_then(|value| value.trim().strip_suffix("kB")?.trim().parse().ok())
        .unwrap_or_else(|| panic!("{field} in /proc/self/status"))
}

/// Runs only in the child process `a_dry_run_holds_one_legacy_subject_at_a_time` starts: a dry run
/// over the store it names, printing how far it raised the peak resident memory.
#[cfg(target_os = "linux")]
#[test]
fn dry_run_memory_probe() {
    let Some(legacy) = std::env::var_os(DRY_RUN_PROBE) else {
        return;
    };
    let legacy = PathBuf::from(legacy);
    // Writing 5 resets the peak to the current resident size, so the peak below is the dry run's.
    let _ = std::fs::write("/proc/self/clear_refs", "5");
    let before = status_kib("VmHWM");
    let report = entity_store::migrate_file_store_v1(&legacy, legacy.with_file_name("unwritten"), true)
        .expect("dry run");
    let peak = status_kib("VmHWM");
    println!("dry-run subjects {} grew-kib {}", report.subjects, peak.saturating_sub(before));
}

#[cfg(target_os = "linux")]
#[test]
fn a_dry_run_holds_one_legacy_subject_at_a_time() {
    const SUBJECTS: usize = 64;
    let base = scratch("dry-run-memory");
    let legacy = base.join("legacy");
    std::fs::create_dir_all(legacy.join("ticket")).expect("legacy entity directory");
    let created = Runtime::new(&registry())
        .create("ticket", 1, "0", serde_json::json!({"title": "x".repeat(256 * 1024)}))
        .expect("creation");
    let mut instance = created.instance;
    for index in 0..SUBJECTS {
        instance.id = index.to_string();
        std::fs::write(
            legacy.join("ticket").join(format!("{index}.json")),
            serde_json::to_vec(&instance).expect("instance serializes"),
        )
        .expect("legacy state");
    }
    // Measured in a child running the probe alone, so no other test shares its peak.
    let output = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args(["--exact", "dry_run_memory_probe", "--nocapture", "--test-threads=1"])
        .env(DRY_RUN_PROBE, &legacy)
        .output()
        .expect("probe runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "probe failed: {stdout}{}", String::from_utf8_lossy(&output.stderr));
    let line = stdout
        .lines()
        .find_map(|line| line.split_once("dry-run subjects ").map(|(_, report)| report))
        .unwrap_or_else(|| panic!("probe reported nothing: {stdout}"));
    let (subjects, grown) = line.split_once(" grew-kib ").expect("probe line");
    assert_eq!(subjects, SUBJECTS.to_string());
    let grown: u64 = grown.parse().expect("KiB");
    // 64 documents of 256 KiB are 16 MiB: holding them all raised the peak by more than that, holding
    // one at a time by under 3 MiB.
    assert!(
        grown < 8 * 1024,
        "a dry run over {SUBJECTS} documents of 256 KiB raised peak memory by {grown} KiB; it holds one subject at a time"
    );
    std::fs::remove_dir_all(&base).expect("fixture removed");
}
