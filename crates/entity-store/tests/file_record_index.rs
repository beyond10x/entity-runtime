//! The File Store's record-id index: built once per handle from disk, kept current by the
//! handle's own writes, and never a substitute for the store-global rule it serves (R-88).
//!
//! `CARGO_TARGET_TMPDIR` rather than `/tmp`, for the reason `both_providers.rs` gives.

use std::path::{Path, PathBuf};

use entity_core::{Decision, Registry, Runtime};
use entity_store::{Expect, FileStore, RecordedCommit, Recording, Store, StoreError};

fn registry() -> Registry {
    let definition = serde_json::from_value(serde_json::json!({
        "entity": "ticket",
        "version": 1,
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "open", "states": ["open", "closed"] },
        "operations": {
            "close": { "transitions": [{ "from": "open", "to": "closed" }] }
        }
    }))
    .expect("the definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("it validates");
    registry
}

fn opening(registry: &Registry, id: &str) -> Decision {
    Runtime::new(registry)
        .create("ticket", 1, id, serde_json::json!({ "title": "A ticket" }))
        .expect("creation is permitted")
}

fn recorded(decision: Decision, record_id: &str) -> RecordedCommit {
    RecordedCommit::new(
        decision,
        &Recording {
            record_id: record_id.to_owned(),
            recorded_at: "2026-09-03T12:00:00Z".to_owned(),
            correlation: None,
            causation: None,
            actor: None,
        },
    )
    .expect("valid metadata")
}

fn scratch(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("file-record-index")
        .join(name);
    let _ = std::fs::remove_dir_all(&root);
    root
}

#[test]
fn a_fresh_handle_finds_a_record_id_another_handle_wrote_and_refuses_its_reuse() {
    let registry = registry();
    let root = scratch("fresh-handle");
    let mut first = FileStore::open(&root);
    first
        .commit_recorded(&recorded(opening(&registry, "one"), "r-1"), Expect::Absent)
        .expect("first accepted");

    // A handle opened afterwards knows nothing yet; its first lookup reads the store once.
    let mut second = FileStore::open(&root);
    let reuse = second.commit_recorded(&recorded(opening(&registry, "two"), "r-1"), Expect::Absent);
    assert!(
        matches!(reuse, Err(StoreError::RecordConflict { ref record_id }) if record_id == "r-1"),
        "a record id reused for different bytes is a conflict across handles: {reuse:?}"
    );
    second
        .commit_recorded(&recorded(opening(&registry, "one"), "r-1"), Expect::Absent)
        .expect("identical bytes are an idempotent success across handles");
}

#[test]
fn a_handle_remembers_its_own_writes_without_rereading_the_store() {
    let registry = registry();
    let root = scratch("own-writes");
    let mut store = FileStore::open(&root);
    store
        .commit_recorded(&recorded(opening(&registry, "one"), "r-1"), Expect::Absent)
        .expect("first accepted");
    store
        .commit_recorded(&recorded(opening(&registry, "two"), "r-2"), Expect::Absent)
        .expect("second accepted");
    // The index was built before `r-2` existed on disk; a lookup for it must still find it.
    let reuse = store.commit_recorded(
        &recorded(opening(&registry, "three"), "r-2"),
        Expect::Absent,
    );
    assert!(
        matches!(reuse, Err(StoreError::RecordConflict { ref record_id }) if record_id == "r-2"),
        "a write the handle made is indexed without a second scan: {reuse:?}"
    );
    // And a clone carries what the original knew.
    let mut cloned = store.clone();
    let reuse =
        cloned.commit_recorded(&recorded(opening(&registry, "four"), "r-1"), Expect::Absent);
    assert!(
        matches!(reuse, Err(StoreError::RecordConflict { .. })),
        "{reuse:?}"
    );
}

#[test]
fn an_existing_handle_invalidates_its_index_after_another_writer() {
    let registry = registry();
    let root = scratch("changed-epoch");
    let mut first = FileStore::open(&root);
    first
        .commit_recorded(&recorded(opening(&registry, "one"), "one"), Expect::Absent)
        .unwrap();
    first
        .commit_recorded(&recorded(opening(&registry, "one"), "one"), Expect::Absent)
        .unwrap();
    let mut second = first.clone();
    second
        .commit_recorded(&recorded(opening(&registry, "two"), "two"), Expect::Absent)
        .unwrap();
    let result = first.commit_recorded(
        &recorded(opening(&registry, "three"), "two"),
        Expect::Absent,
    );
    assert!(matches!(result, Err(StoreError::RecordConflict { record_id }) if record_id == "two"));
}

#[test]
fn separate_writers_preserve_exactly_one_winning_revision_and_all_its_records() {
    use entity_store::{HistoryProvider, StateProvider};
    let registry = registry();
    let root = scratch("parallel-writers");
    let creation = recorded(opening(&registry, "one"), "creation");
    FileStore::open(&root)
        .commit_recorded(&creation, Expect::Absent)
        .unwrap();
    let closed = Runtime::new(&registry)
        .execute(&creation.instance, "close", serde_json::json!({}))
        .unwrap();
    let barrier = std::sync::Barrier::new(8);
    let outcomes = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..8)
            .map(|n| {
                let root = &root;
                let barrier = &barrier;
                let commit = recorded(closed.clone(), &format!("writer-{n}"));
                scope.spawn(move || {
                    let mut store = FileStore::open(root);
                    barrier.wait();
                    store.commit_recorded(&commit, Expect::Revision(1))
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        outcomes.iter().filter(|outcome| outcome.is_ok()).count(),
        1,
        "{outcomes:?}"
    );
    for error in outcomes.into_iter().filter_map(Result::err) {
        assert!(
            matches!(error, StoreError::RevisionConflict { found: Some(2), .. }),
            "{error}"
        );
    }
    let store = FileStore::open(&root);
    assert_eq!(store.load("ticket", "one").unwrap().unwrap().revision, 2);
    assert_eq!(store.records("ticket", "one").unwrap().len(), 2);
}

#[test]
fn abandoned_subject_temporary_files_do_not_hide_ids_or_block_recorded_writes() {
    use entity_store::StateProvider;
    let registry = registry();
    let root = scratch("orphan");
    let mut store = FileStore::open(&root);
    store
        .commit_recorded(&recorded(opening(&registry, "one"), "one"), Expect::Absent)
        .unwrap();
    std::fs::write(
        root.join("subjects/7469636b6574/6f6e65.json.writing.123.456"),
        "partial JSON",
    )
    .unwrap();
    assert_eq!(store.ids("ticket").unwrap(), ["one"]);
    let mut reopened = FileStore::open(&root);
    reopened
        .commit_recorded(&recorded(opening(&registry, "two"), "two"), Expect::Absent)
        .unwrap();
    assert_eq!(reopened.ids("ticket").unwrap(), ["one", "two"]);
}

#[cfg(unix)]
#[test]
fn parent_and_marker_symlinks_are_refused_on_reads() {
    use entity_store::StateProvider;
    use std::os::unix::fs::symlink;
    for component in ["subjects", ".entity-store-format"] {
        let root = scratch(&format!("symlink-{}", component));
        let outside = scratch(&format!("outside-{}", component));
        let mut store = FileStore::open(&root);
        store
            .commit_recorded(
                &recorded(opening(&registry(), "one"), "one"),
                Expect::Absent,
            )
            .unwrap();
        std::fs::rename(root.join(component), &outside).unwrap();
        symlink(&outside, root.join(component)).unwrap();
        let error = store
            .load("ticket", "one")
            .expect_err("a symlink must not escape confinement");
        assert!(
            matches!(error, StoreError::Backend(ref detail) if detail.contains("symlink")),
            "{error}"
        );
        std::fs::remove_file(root.join(component)).unwrap();
        std::fs::rename(&outside, root.join(component)).unwrap();
    }
}

#[test]
fn separate_processes_serialize_revision_checks_and_survive_a_killed_lock_holder() {
    use entity_store::HistoryProvider;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    const CHILD_ROOT: &str = "ENTITY_FILE_TEST_CHILD_ROOT";
    const CHILD_MODE: &str = "ENTITY_FILE_TEST_CHILD_MODE";
    let test_name = "separate_processes_serialize_revision_checks_and_survive_a_killed_lock_holder";
    if let Ok(path) = std::env::var(CHILD_ROOT) {
        let root = PathBuf::from(path);
        let mode = std::env::var(CHILD_MODE).unwrap();
        if mode == "lock-holder" {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .append(true)
                .open(root.join(".entity-store-lock"))
                .unwrap();
            fs2::FileExt::lock_exclusive(&file).unwrap();
            std::fs::write(root.join("holder-ready"), "ready").unwrap();
            loop {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        let registry = registry();
        let created = opening(&registry, "one");
        let closed = Runtime::new(&registry)
            .execute(&created.instance, "close", serde_json::json!({}))
            .unwrap();
        let start = Instant::now();
        while !root.join("writers-ready").exists() {
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "parent did not start writers"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let result =
            FileStore::open(&root).commit_recorded(&recorded(closed, &mode), Expect::Revision(1));
        let outcome = match result {
            Ok(()) => "accepted",
            Err(StoreError::RevisionConflict { found: Some(2), .. }) => "conflict",
            other => panic!("unexpected child outcome: {other:?}"),
        };
        std::fs::write(root.join(format!("result-{mode}")), outcome).unwrap();
        return;
    }
    let root = scratch("processes");
    FileStore::open(&root)
        .commit_recorded(
            &recorded(opening(&registry(), "one"), "created"),
            Expect::Absent,
        )
        .unwrap();
    let child = |mode: &str| {
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test_name, "--nocapture"])
            .env(CHILD_ROOT, &root)
            .env(CHILD_MODE, mode)
            .stdout(Stdio::null())
            .spawn()
            .unwrap()
    };
    let mut holder = child("lock-holder");
    let start = Instant::now();
    while !root.join("holder-ready").exists() {
        if start.elapsed() >= Duration::from_secs(30) {
            let _ = holder.kill();
            let _ = holder.wait();
            panic!("child did not acquire the root lock");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut writers: Vec<_> = (0..4).map(|n| child(&format!("writer-{n}"))).collect();
    std::fs::write(root.join("writers-ready"), "ready").unwrap();
    holder.kill().unwrap();
    holder.wait().unwrap();
    for writer in &mut writers {
        assert!(writer.wait().unwrap().success());
    }
    let accepted = (0..4)
        .filter(|n| {
            std::fs::read_to_string(root.join(format!("result-writer-{n}"))).unwrap() == "accepted"
        })
        .count();
    assert_eq!(
        accepted, 1,
        "exactly one process may publish revision 2 after the killed holder releases its lock"
    );
    assert_eq!(
        FileStore::open(root)
            .records("ticket", "one")
            .unwrap()
            .len(),
        2
    );
}
