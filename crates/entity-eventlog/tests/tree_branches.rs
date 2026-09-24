//! A recorded store kept as a tree of files merges the way its files do: two branches that
//! recorded decisions for different subjects merge into one store that opens and holds both.
#![cfg(feature = "tree")]

use std::{fs, path::Path, sync::Arc};

use entity_core::Registry;
use entity_eventlog::{
    AsyncBindingProvisioner, Authority, ErRecordedProjector, EventlogBackend,
    EventlogBindingProvisioner, EventlogOperationContext, EventlogRecordedStore,
};
use entity_executor::{BatchAction, CreateRequest, ExecuteRequest, Executor};
use entity_store::{
    Recording,
    asynchronous::{AppendOutcome, AsyncRecordedReader, BatchKey, Subject},
};
use eventlog_core::{CaptureLimits, EventStore, InlineProjectionAdmin, TenantId};
use eventlog_tree::TreeEventStore;
use serde_json::json;
use time::OffsetDateTime;

const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 128,
    max_blobs: 512,
    max_projection_rows: 512,
    max_payload_bytes: 4 * 1024 * 1024,
};

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

fn context(label: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "tree-branch-test".into(),
        actor: "entity-eventlog-test".into(),
        request_id: format!("request-{label}"),
        trace_id: format!("trace-{label}"),
        causation_id: None,
        causation_depth: 0,
        occurred_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "ticket",
        "version": 1,
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "open", "states": ["open"] },
        "operations": {
            "touch": {
                "transitions": [{ "from": "open", "to": "open" }],
                "arguments": { "fields": {} },
                "emits": []
            }
        }
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    registry
}

fn create(id: &str) -> CreateRequest {
    CreateRequest {
        subject: Subject::new("ticket", id).expect("subject"),
        definition_version: 1,
        fields: json!({ "title": id }),
        recording: Recording {
            record_id: format!("{id}-create"),
            recorded_at: "2026-09-24T00:00:00Z".into(),
            correlation: None,
            causation: None,
            actor: None,
        },
    }
}

/// Copy every file under `from` into `into` that `into` does not hold, as a merge of branches
/// that added different files does; a file both hold must be identical.
fn merge_into(from: &Path, into: &Path) {
    for entry in fs::read_dir(from).expect("readable") {
        let path = entry.expect("entry").path();
        let target = into.join(path.file_name().expect("name"));
        if path.is_dir() {
            fs::create_dir_all(&target).expect("directory");
            merge_into(&path, &target);
        } else if path.file_name().is_some_and(|name| name == ".lock") {
        } else if let Ok(existing) = fs::read(&target) {
            assert_eq!(
                existing,
                fs::read(&path).expect("readable"),
                "{} conflicts",
                target.display()
            );
        } else {
            fs::copy(&path, &target).expect("copy");
        }
    }
}

async fn backend_for(root: &Path) -> Arc<TreeEventStore> {
    backend(root).await
}

async fn backend(root: &Path) -> Arc<TreeEventStore> {
    let backend = Arc::new(TreeEventStore::open(root).await.expect("tree store"));
    let projector = Arc::new(ErRecordedProjector::new());
    backend
        .create_projections(projector.clone())
        .await
        .expect("projection admission");
    backend
        .attach_inline_existing(projector)
        .await
        .expect("projection attachment");
    backend
}

async fn record(root: &Path, authority: &Authority, id: &str) {
    let backend: Arc<dyn EventlogBackend> = backend(root).await;
    let store = EventlogRecordedStore::open(backend, authority.clone(), LIMITS)
        .await
        .expect("ordinary open");
    let registry = registry();
    let operation = store.operation(context(id));
    let outcome = Executor::new(&registry, &operation)
        .batch(
            BatchKey::Named(format!("create-{id}")),
            vec![BatchAction::Create(create(id))],
        )
        .await
        .expect("a batch that creates one ticket");
    assert!(matches!(
        outcome,
        AppendOutcome::Committed {
            replayed: false,
            ..
        }
    ));
}

#[test]
fn two_branches_that_recorded_different_subjects_merge_into_one_store() {
    block_on(async {
        let base = tempfile::tempdir().expect("directory");
        let tenant = TenantId::new("tree-branches").expect("tenant");
        let authority = {
            let backend = backend(base.path()).await;
            let stream_identity = backend.stream_identity(&tenant).await.expect("identity");
            let authority = Authority {
                logical_scope: "scope-tree-branches".into(),
                tenant: tenant.as_str().to_owned(),
                stream_identity,
            };
            let erased: Arc<dyn EventlogBackend> = backend;
            EventlogBindingProvisioner::new(erased, LIMITS)
                .provision_binding(authority.clone(), context("provision"))
                .await
                .expect("binding provisioned");
            authority
        };
        let ours = tempfile::tempdir().expect("directory");
        let theirs = tempfile::tempdir().expect("directory");
        merge_into(base.path(), ours.path());
        merge_into(base.path(), theirs.path());

        record(ours.path(), &authority, "ours").await;
        record(theirs.path(), &authority, "theirs").await;
        merge_into(theirs.path(), ours.path());

        let backend: Arc<dyn EventlogBackend> = backend(ours.path()).await;
        let store = EventlogRecordedStore::open(backend, authority.clone(), LIMITS)
            .await
            .expect("the merged store opens");
        let snapshot = store
            .complete_snapshot(&authority.logical_scope)
            .await
            .expect("a complete snapshot of the merged store");
        let subjects: Vec<String> = snapshot
            .histories
            .iter()
            .map(|subject| format!("{subject:?}"))
            .collect();
        assert_eq!(
            snapshot.histories.len(),
            2,
            "the merged store lost a branch: {subjects:?}"
        );
    });
}

fn touch(id: &str, record: &str) -> ExecuteRequest {
    ExecuteRequest {
        subject: Subject::new("ticket", id).expect("subject"),
        expected_revision: 1,
        operation: "touch".into(),
        arguments: json!({}),
        fulfillments: Default::default(),
        recording: Recording {
            record_id: record.to_owned(),
            recorded_at: "2026-09-24T00:00:00Z".into(),
            correlation: None,
            causation: None,
            actor: None,
        },
    }
}

async fn touch_in(root: &Path, authority: &Authority, id: &str, record: &str) {
    let backend: Arc<dyn EventlogBackend> = backend(root).await;
    let store = EventlogRecordedStore::open(backend, authority.clone(), LIMITS)
        .await
        .expect("ordinary open");
    let registry = registry();
    let operation = store.operation(context(record));
    Executor::new(&registry, &operation)
        .batch(
            BatchKey::Named(record.to_owned()),
            vec![BatchAction::Execute(touch(id, record))],
        )
        .await
        .expect("a touch");
}

/// Build a store in which `x` was created, then two branches each changed `x` and one of them
/// also created `y`, and merge the branches' files. Returns the merged directory.
async fn forked_store() -> (tempfile::TempDir, Authority) {
    let base = tempfile::tempdir().expect("directory");
    let tenant = TenantId::new("tree-branches").expect("tenant");
    let authority = {
        let backend = backend(base.path()).await;
        let stream_identity = backend.stream_identity(&tenant).await.expect("identity");
        let authority = Authority {
            logical_scope: "scope-tree-branches".into(),
            tenant: tenant.as_str().to_owned(),
            stream_identity,
        };
        let erased: Arc<dyn EventlogBackend> = backend;
        EventlogBindingProvisioner::new(erased, LIMITS)
            .provision_binding(authority.clone(), context("provision"))
            .await
            .expect("binding provisioned");
        authority
    };
    record(base.path(), &authority, "x").await;
    let ours = tempfile::tempdir().expect("directory");
    let theirs = tempfile::tempdir().expect("directory");
    merge_into(base.path(), ours.path());
    merge_into(base.path(), theirs.path());
    touch_in(ours.path(), &authority, "x", "ours-touch").await;
    touch_in(theirs.path(), &authority, "x", "theirs-touch").await;
    record(theirs.path(), &authority, "y").await;
    merge_into(theirs.path(), ours.path());
    (ours, authority)
}

#[test]
fn a_subject_both_branches_changed_is_forked_and_the_rest_of_the_store_still_serves() {
    use entity_store::asynchronous::{AsyncStateReader, AsyncStoreError};
    block_on(async {
        let (merged, authority) = forked_store().await;
        let backend: Arc<dyn EventlogBackend> = backend(merged.path()).await;
        let store = EventlogRecordedStore::open(backend, authority.clone(), LIMITS)
            .await
            .expect("a store with one forked subject still opens");
        let reader = store.operation(context("read"));
        let y = reader
            .load(&Subject::new("ticket", "y").expect("subject"))
            .await
            .expect("an unforked subject reads");
        assert!(y.is_some(), "the other branch's subject was lost");
        let x = reader
            .load(&Subject::new("ticket", "x").expect("subject"))
            .await;
        assert!(
            matches!(x, Err(AsyncStoreError::Forked { ref heads, .. }) if heads.len() == 2),
            "a subject both branches changed was served as one state: {x:?}"
        );
        drop(reader);
        drop(store);
        let refused = {
            let backend: Arc<dyn EventlogBackend> = backend_for(merged.path()).await;
            let store = EventlogRecordedStore::open(backend, authority.clone(), LIMITS)
                .await
                .expect("open");
            let registry = registry();
            let operation = store.operation(context("after-fork"));
            let mut after = touch("x", "after-fork");
            after.expected_revision = 2;
            Executor::new(&registry, &operation)
                .batch(
                    BatchKey::Named("after-fork".into()),
                    vec![BatchAction::Execute(after)],
                )
                .await
        };
        assert!(
            matches!(refused, Err(ref error) if format!("{error:?}").contains("Forked")),
            "an ordinary write extended a forked subject: {refused:?}"
        );
    });
}

#[test]
fn a_merge_decision_joins_a_forked_subject_and_it_serves_again_after_reopening() {
    use entity_executor::MergeRequest;
    use entity_store::asynchronous::{AsyncStateReader, branch_heads};
    block_on(async {
        let (merged, authority) = forked_store().await;
        let x = Subject::new("ticket", "x").expect("subject");
        {
            let backend: Arc<dyn EventlogBackend> = backend(merged.path()).await;
            let store = EventlogRecordedStore::open(backend, authority.clone(), LIMITS)
                .await
                .expect("open");
            let operation = store.operation(context("merge"));
            let history = operation
                .history(&x)
                .await
                .expect("the forked history reads");
            let heads = branch_heads(&history).expect("its heads");
            assert_eq!(heads.len(), 2);
            let mut execute = touch("x", "merge-x");
            execute.expected_revision = 2;
            let registry = registry();
            let outcome = Executor::new(&registry, &operation)
                .batch(
                    BatchKey::Named("merge-x".into()),
                    vec![BatchAction::Merge(MergeRequest {
                        execute,
                        first: heads[0].0.clone(),
                    })],
                )
                .await
                .expect("a merge decision over both heads");
            assert!(matches!(
                outcome,
                AppendOutcome::Committed {
                    replayed: false,
                    ..
                }
            ));
            let joined = operation.load(&x).await.expect("the joined subject reads");
            assert_eq!(
                joined.map(|state| state.revision),
                Some(3),
                "the merge did not pass both branches"
            );
        }
        let backend: Arc<dyn EventlogBackend> = backend(merged.path()).await;
        let store = EventlogRecordedStore::open(backend, authority.clone(), LIMITS)
            .await
            .expect("the joined store reopens");
        let reader = store.operation(context("reread"));
        let state = reader
            .load(&x)
            .await
            .expect("the joined subject reads after reopening");
        assert_eq!(state.map(|state| state.revision), Some(3));
        let snapshot = store
            .complete_snapshot(&authority.logical_scope)
            .await
            .expect("a complete snapshot of the joined store");
        assert_eq!(snapshot.histories.len(), 2);
    });
}

#[test]
fn a_refused_command_is_recorded_once_and_changes_no_subject() {
    use entity_store::asynchronous::{AsyncRefusalRecorder, AsyncStateReader, RefusalReason};
    block_on(async {
        let (merged, authority) = forked_store().await;
        let refuse = |root: std::path::PathBuf, authority: Authority| async move {
            let backend: Arc<dyn EventlogBackend> = backend(&root).await;
            let store = EventlogRecordedStore::open(backend, authority, LIMITS)
                .await
                .expect("open");
            let operation = store.operation(context("refused"));
            let registry = registry();
            // A stale expectation on an unforked subject, and an ordinary write to a forked one.
            let mut stale = touch("y", "stale-y");
            stale.expected_revision = 7;
            let stale = Executor::recording_refusals(&registry, &operation, &operation)
                .batch(
                    BatchKey::Named("stale-y".into()),
                    vec![BatchAction::Execute(stale)],
                )
                .await;
            let mut forked = touch("x", "forked-x");
            forked.expected_revision = 2;
            let forked = Executor::recording_refusals(&registry, &operation, &operation)
                .batch(
                    BatchKey::Named("forked-x".into()),
                    vec![BatchAction::Execute(forked)],
                )
                .await;
            (stale.is_err(), forked.is_err())
        };
        assert_eq!(
            refuse(merged.path().to_owned(), authority.clone()).await,
            (true, true)
        );
        // The same commands again: refused again, and recorded once each.
        assert_eq!(
            refuse(merged.path().to_owned(), authority.clone()).await,
            (true, true)
        );

        let backend: Arc<dyn EventlogBackend> = backend(merged.path()).await;
        let store = EventlogRecordedStore::open(backend, authority.clone(), LIMITS)
            .await
            .expect("the store reopens with its refusals");
        let operation = store.operation(context("read"));
        let refusals = operation.refusals().await.expect("refusals read");
        assert_eq!(
            refusals.len(),
            2,
            "a refusal was lost or recorded twice: {refusals:?}"
        );
        assert!(refusals.iter().any(|refusal| matches!(
            refusal.reason,
            RefusalReason::Conflict {
                expected: Some(7),
                found: Some(1),
                ..
            }
        )));
        assert!(
            refusals
                .iter()
                .any(|refusal| matches!(refusal.reason, RefusalReason::Forked { .. }))
        );
        let y = operation
            .load(&Subject::new("ticket", "y").expect("subject"))
            .await
            .expect("y reads");
        assert_eq!(
            y.map(|state| state.revision),
            Some(1),
            "a refusal moved a subject"
        );
    });
}

/// A provisioned store in which `x` was created, and its authority.
async fn store_with_x() -> (tempfile::TempDir, Authority) {
    let base = tempfile::tempdir().expect("directory");
    let tenant = TenantId::new("tree-branches").expect("tenant");
    let backend = backend(base.path()).await;
    let stream_identity = backend.stream_identity(&tenant).await.expect("identity");
    let authority = Authority {
        logical_scope: "scope-tree-branches".into(),
        tenant: tenant.as_str().to_owned(),
        stream_identity,
    };
    let erased: Arc<dyn EventlogBackend> = backend;
    EventlogBindingProvisioner::new(erased, LIMITS)
        .provision_binding(authority.clone(), context("provision"))
        .await
        .expect("binding provisioned");
    record(base.path(), &authority, "x").await;
    (base, authority)
}

/// A checkout of `base`, as a branch taken from it.
fn branch_of(base: &Path) -> tempfile::TempDir {
    let branch = tempfile::tempdir().expect("directory");
    merge_into(base, branch.path());
    branch
}

/// Record an observation of `x` at `revision` — evidence about a state, not a change to it.
async fn observe_in(root: &Path, authority: &Authority, revision: u64, record: &str) {
    let backend: Arc<dyn EventlogBackend> = backend(root).await;
    let store = EventlogRecordedStore::open(backend, authority.clone(), LIMITS)
        .await
        .expect("ordinary open");
    let registry = registry();
    let operation = store.operation(context(record));
    let observation = entity_store::RecordedObservation {
        entity: "ticket".into(),
        id: "x".into(),
        revision,
        envelope: Recording {
            record_id: record.to_owned(),
            recorded_at: "2026-09-24T00:00:00Z".into(),
            correlation: None,
            causation: None,
            actor: None,
        }
        .seal(json!({ "evidence": record }))
        .expect("observation envelope"),
    };
    Executor::new(&registry, &operation)
        .observe(observation)
        .await
        .expect("an observation of the current revision");
}

/// The record ids of `x`'s history in `root`, and the state it serves.
async fn x_in(
    root: &Path,
    authority: &Authority,
) -> (
    Vec<String>,
    Result<Option<entity_core::EntityInstance>, entity_store::asynchronous::AsyncStoreError>,
) {
    use entity_store::asynchronous::AsyncStateReader;
    let backend: Arc<dyn EventlogBackend> = backend(root).await;
    let store = EventlogRecordedStore::open(backend, authority.clone(), LIMITS)
        .await
        .expect("the merged store opens");
    let operation = store.operation(context("read-x"));
    let x = Subject::new("ticket", "x").expect("subject");
    let history = operation.history(&x).await.expect("x's history reads");
    let ids = history
        .records
        .iter()
        .map(|record| record.entry.record_id().to_owned())
        .collect();
    (ids, operation.load(&x).await)
}

/// `x` created, then one branch observed it and the other changed it, merged.
async fn observed_beside_a_decision() -> (tempfile::TempDir, Authority) {
    let (base, authority) = store_with_x().await;
    let ours = branch_of(base.path());
    let theirs = branch_of(base.path());
    observe_in(ours.path(), &authority, 1, "ours-seen").await;
    touch_in(theirs.path(), &authority, "x", "theirs-touch").await;
    merge_into(theirs.path(), ours.path());
    (ours, authority)
}

#[test]
fn an_observation_on_one_branch_and_a_decision_on_the_other_do_not_fork_the_subject() {
    block_on(async {
        let (merged, authority) = observed_beside_a_decision().await;
        let (ids, x) = x_in(merged.path(), &authority).await;
        assert!(
            matches!(x, Ok(Some(ref state)) if state.revision == 2),
            "an observation beside a decision forked the subject or lost the decision: {x:?}"
        );
        for id in ["x-create", "ours-seen", "theirs-touch"] {
            assert!(
                ids.iter().any(|held| held == id),
                "the merge lost {id}: the history holds {ids:?}"
            );
        }
    });
}

#[test]
fn two_branches_that_only_observed_one_revision_do_not_fork_the_subject() {
    use entity_store::asynchronous::branch_heads;
    block_on(async {
        let (base, authority) = store_with_x().await;
        let ours = branch_of(base.path());
        let theirs = branch_of(base.path());
        observe_in(ours.path(), &authority, 1, "ours-seen").await;
        observe_in(theirs.path(), &authority, 1, "theirs-seen").await;
        merge_into(theirs.path(), ours.path());

        let (ids, x) = x_in(ours.path(), &authority).await;
        assert!(
            matches!(x, Ok(Some(ref state)) if state.revision == 1),
            "two observations of one revision forked the subject or moved it: {x:?}"
        );
        assert!(
            ids.iter().any(|id| id == "ours-seen") && ids.iter().any(|id| id == "theirs-seen"),
            "the merge lost an observation: the history holds {ids:?}"
        );
        let backend: Arc<dyn EventlogBackend> = backend(ours.path()).await;
        let store = EventlogRecordedStore::open(backend, authority.clone(), LIMITS)
            .await
            .expect("open");
        let history = store
            .operation(context("heads"))
            .history(&Subject::new("ticket", "x").expect("subject"))
            .await
            .expect("history");
        let heads = branch_heads(&history).expect("heads");
        assert_eq!(
            heads.len(),
            1,
            "an observation made a head of its own: {heads:?}"
        );
    });
}

#[test]
fn a_decision_after_an_observation_merged_beside_a_decision_extends_the_subject() {
    block_on(async {
        let (merged, authority) = observed_beside_a_decision().await;
        {
            let backend: Arc<dyn EventlogBackend> = backend(merged.path()).await;
            let store = EventlogRecordedStore::open(backend, authority.clone(), LIMITS)
                .await
                .expect("open");
            let registry = registry();
            let operation = store.operation(context("after-merge"));
            let mut after = touch("x", "after-merge");
            after.expected_revision = 2;
            let outcome = Executor::new(&registry, &operation)
                .batch(
                    BatchKey::Named("after-merge".into()),
                    vec![BatchAction::Execute(after)],
                )
                .await;
            assert!(
                matches!(
                    outcome,
                    Ok(AppendOutcome::Committed {
                        replayed: false,
                        ..
                    })
                ),
                "an ordinary write after an observation merged beside a decision was refused: \
                 {outcome:?}"
            );
        }
        let (ids, x) = x_in(merged.path(), &authority).await;
        assert!(
            matches!(x, Ok(Some(ref state)) if state.revision == 3),
            "the write after the merge did not extend the subject once reopened: {x:?}"
        );
        assert!(
            ids.iter().any(|id| id == "ours-seen"),
            "the observation was lost: {ids:?}"
        );
    });
}

/// `x` created, then each branch changed it and one branch also observed its own change, merged.
async fn decided_on_both_branches_one_observed() -> (tempfile::TempDir, Authority) {
    let (base, authority) = store_with_x().await;
    let ours = branch_of(base.path());
    let theirs = branch_of(base.path());
    touch_in(ours.path(), &authority, "x", "ours-touch").await;
    observe_in(ours.path(), &authority, 2, "ours-seen").await;
    touch_in(theirs.path(), &authority, "x", "theirs-touch").await;
    merge_into(theirs.path(), ours.path());
    (ours, authority)
}

#[test]
fn a_decision_on_each_branch_still_forks_and_its_heads_are_the_decisions() {
    use entity_store::asynchronous::{AsyncStoreError, branch_heads};
    block_on(async {
        let (merged, authority) = decided_on_both_branches_one_observed().await;
        let (_, x) = x_in(merged.path(), &authority).await;
        assert!(
            matches!(x, Err(AsyncStoreError::Forked { ref heads, .. }) if heads.len() == 2),
            "a decision on each branch was served as one state: {x:?}"
        );
        let backend: Arc<dyn EventlogBackend> = backend(merged.path()).await;
        let store = EventlogRecordedStore::open(backend, authority.clone(), LIMITS)
            .await
            .expect("open");
        let history = store
            .operation(context("heads"))
            .history(&Subject::new("ticket", "x").expect("subject"))
            .await
            .expect("history");
        let digest_of = |id: &str| {
            history
                .records
                .iter()
                .find(|record| record.entry.record_id() == id)
                .and_then(|record| record.lineage.as_ref())
                .map(|lineage| lineage.digest.clone())
                .unwrap_or_else(|| panic!("{id} carries no lineage"))
        };
        let mut decisions = vec![digest_of("ours-touch"), digest_of("theirs-touch")];
        decisions.sort();
        let heads: Vec<String> = branch_heads(&history)
            .expect("heads")
            .into_iter()
            .map(|(digest, _)| digest)
            .collect();
        assert_eq!(
            heads, decisions,
            "the heads of a fork are its decisions, not the observation after one"
        );
    });
}

#[test]
fn an_observation_after_a_merge_decision_attaches_to_the_merge() {
    use entity_executor::MergeRequest;
    use entity_store::asynchronous::branch_heads;
    block_on(async {
        let (merged, authority) = decided_on_both_branches_one_observed().await;
        let x = Subject::new("ticket", "x").expect("subject");
        {
            let backend: Arc<dyn EventlogBackend> = backend(merged.path()).await;
            let store = EventlogRecordedStore::open(backend, authority.clone(), LIMITS)
                .await
                .expect("open");
            let operation = store.operation(context("merge"));
            let history = operation.history(&x).await.expect("history");
            let heads = branch_heads(&history).expect("heads");
            let mut execute = touch("x", "merge-x");
            execute.expected_revision = 2;
            let registry = registry();
            let outcome = Executor::new(&registry, &operation)
                .batch(
                    BatchKey::Named("merge-x".into()),
                    vec![BatchAction::Merge(MergeRequest {
                        execute,
                        first: heads[0].0.clone(),
                    })],
                )
                .await;
            assert!(
                matches!(
                    outcome,
                    Ok(AppendOutcome::Committed {
                        replayed: false,
                        ..
                    })
                ),
                "a merge over a fork whose one branch ends in an observation was refused: \
                 {outcome:?}"
            );
        }
        observe_in(merged.path(), &authority, 3, "seen-after-merge").await;

        let backend: Arc<dyn EventlogBackend> = backend(merged.path()).await;
        let store = EventlogRecordedStore::open(backend, authority.clone(), LIMITS)
            .await
            .expect("reopen");
        let operation = store.operation(context("after"));
        let history = operation.history(&x).await.expect("history");
        let lineage_of = |id: &str| {
            history
                .records
                .iter()
                .find(|record| record.entry.record_id() == id)
                .and_then(|record| record.lineage.as_deref().cloned())
                .unwrap_or_else(|| panic!("{id} is absent or carries no lineage"))
        };
        let merge = lineage_of("merge-x");
        let seen = lineage_of("seen-after-merge");
        assert_eq!(
            seen.parents,
            vec![merge.digest.clone()],
            "the observation does not follow the merge decision it observed"
        );
        let heads = branch_heads(&history).expect("heads");
        assert_eq!(
            heads.iter().map(|(digest, _)| digest).collect::<Vec<_>>(),
            vec![&merge.digest],
            "the merge decision is not the one head after it was observed"
        );
        assert!(
            history
                .records
                .iter()
                .any(|record| record.entry.record_id() == "ours-seen"),
            "the observation before the merge was lost"
        );
        use entity_store::asynchronous::AsyncStateReader;
        let state = operation.load(&x).await;
        assert!(
            matches!(state, Ok(Some(ref state)) if state.revision == 3),
            "an observation moved the merged subject or forked it: {state:?}"
        );
    });
}

/// Touch `x` at `expected_revision` in `root`.
async fn touch_at(root: &Path, authority: &Authority, record: &str, expected_revision: u64) {
    let backend: Arc<dyn EventlogBackend> = backend(root).await;
    let store = EventlogRecordedStore::open(backend, authority.clone(), LIMITS)
        .await
        .expect("ordinary open");
    let registry = registry();
    let operation = store.operation(context(record));
    let mut request = touch("x", record);
    request.expected_revision = expected_revision;
    Executor::new(&registry, &operation)
        .batch(
            BatchKey::Named(record.to_owned()),
            vec![BatchAction::Execute(request)],
        )
        .await
        .expect("a touch");
}

/// A fork whose longer branch observed its own head after the other branch decided replays that
/// observation after the other branch's lower-revision decision: the observation is newer than
/// the subject row the replay holds. The store still opens and the subject reads as forked.
#[test]
fn a_fork_whose_observation_replays_after_the_other_branch_still_opens_as_forked() {
    use entity_store::asynchronous::AsyncStoreError;
    block_on(async {
        let (base, authority) = store_with_x().await;
        let ours = branch_of(base.path());
        let theirs = branch_of(base.path());
        touch_at(ours.path(), &authority, "ours-2", 1).await;
        touch_at(ours.path(), &authority, "ours-3", 2).await;
        std::thread::sleep(std::time::Duration::from_millis(20));
        touch_at(theirs.path(), &authority, "theirs-2", 1).await;
        std::thread::sleep(std::time::Duration::from_millis(20));
        observe_in(ours.path(), &authority, 3, "ours-seen-3").await;
        merge_into(theirs.path(), ours.path());

        let (ids, x) = x_in(ours.path(), &authority).await;
        assert!(
            matches!(x, Err(AsyncStoreError::Forked { ref heads, .. }) if heads.len() == 2),
            "a fork with an observation replayed after the other branch did not read as forked: \
             {x:?}"
        );
        assert!(
            ids.iter().any(|id| id == "ours-seen-3"),
            "the observation was lost: {ids:?}"
        );
    });
}
