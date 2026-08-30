//! The conformance suite, run against the providers this crate ships — and against one that is
//! deliberately wrong, so the suite's reach is measured rather than assumed.

use std::path::Path;

use entity_store::conformance::{self, Broken};
use entity_store::{FileStore, MemoryStore};

#[test]
fn the_memory_provider_conforms() {
    let mut store = MemoryStore::new();
    let report = conformance::run(&mut store);
    assert!(report.is_clean(), "MemoryStore:\n{}", report.summary());
    conformance::verify_recorded(&mut store).expect("MemoryStore recorded history");
    assert_eq!(report.outcomes.len(), 10);
    let batch = conformance::run_atomic(&mut store);
    assert!(batch.is_clean(), "MemoryStore batch:\n{}", batch.summary());
}

#[test]
fn the_file_provider_conforms() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("conformance-file");
    let _ = std::fs::remove_dir_all(&root);
    let mut store = FileStore::open(&root);
    let report = conformance::run(&mut store);
    assert!(report.is_clean(), "FileStore:\n{}", report.summary());
    conformance::verify_recorded(&mut store).expect("FileStore recorded history");
}

#[test]
fn a_broken_provider_is_caught() {
    // The case that gives the suite its meaning. `Broken` ignores the revision it is handed and
    // writes anyway, so a concurrent writer silently replaces another's work — the single defect a
    // store must not have — and it lists an id it does not hold, which sends a hydrating shell to
    // fetch an instance that is not there. A suite that passed it would be telling nobody anything.
    let mut store = Broken::default();
    let report = conformance::run(&mut store);

    assert!(
        !report.is_clean(),
        "the suite must catch a provider that ignores the revision it was given"
    );
    let caught: Vec<&str> = report.failures().iter().map(|o| o.case).collect();
    assert!(
        caught.contains(&"a stale write is refused"),
        "the stale-write case is the one that must catch it, and it did not: {caught:?}"
    );
    assert!(
        caught.contains(&"a second creation of one identity is refused"),
        "a second creation replacing the first must also be caught: {caught:?}"
    );
    assert!(
        caught.contains(&"what a store holds is listed, sorted, and only that"),
        "a provider listing an id it does not hold must be caught: {caught:?}"
    );

    // And it fails *only* where it is broken: a suite that failed everything against one defect
    // would be no more informative than one that passed everything.
    assert!(
        report.failures().len() < report.outcomes.len(),
        "the suite must localise the defect, not condemn the whole provider"
    );

    let batch = conformance::run_atomic(&mut store);
    let caught: Vec<&str> = batch
        .failures()
        .iter()
        .map(|outcome| outcome.case)
        .collect();
    assert!(
        caught.contains(&"a conflict rolls every earlier batch entry back"),
        "the batch suite must catch a provider that publishes an accepted prefix: {caught:?}"
    );
    assert!(
        batch.failures().len() < batch.outcomes.len(),
        "the batch suite must localise its deliberate defect"
    );
}
