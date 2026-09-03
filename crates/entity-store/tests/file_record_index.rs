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
