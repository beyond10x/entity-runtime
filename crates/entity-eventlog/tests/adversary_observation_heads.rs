//! Adversarial cases for "an observation does not make a head": a stale observation replayed
//! after the decision the other branch made, and a batch that writes one multi-tip subject twice.
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
    asynchronous::{AppendOutcome, AsyncStateReader, BatchKey, Subject},
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
        subject: "adversary-observation".into(),
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

fn recording(id: &str) -> Recording {
    Recording {
        record_id: id.to_owned(),
        recorded_at: "2026-09-24T00:00:00Z".into(),
        correlation: None,
        causation: None,
        actor: None,
    }
}

fn merge_into(from: &Path, into: &Path) {
    for entry in fs::read_dir(from).expect("readable") {
        let path = entry.expect("entry").path();
        let target = into.join(path.file_name().expect("name"));
        if path.is_dir() {
            fs::create_dir_all(&target).expect("directory");
            merge_into(&path, &target);
        } else if path.file_name().is_some_and(|name| name == ".lock") {
        } else if let Ok(existing) = fs::read(&target) {
            assert_eq!(existing, fs::read(&path).expect("readable"));
        } else {
            fs::copy(&path, &target).expect("copy");
        }
    }
}

fn branch_of(base: &Path) -> tempfile::TempDir {
    let branch = tempfile::tempdir().expect("directory");
    merge_into(base, branch.path());
    branch
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

async fn open(root: &Path, authority: &Authority) -> EventlogRecordedStore {
    let backend: Arc<dyn EventlogBackend> = backend(root).await;
    EventlogRecordedStore::open(backend, authority.clone(), LIMITS)
        .await
        .expect("the store opens")
}

fn touch(record: &str, expected_revision: u64) -> ExecuteRequest {
    ExecuteRequest {
        subject: Subject::new("ticket", "x").expect("subject"),
        expected_revision,
        operation: "touch".into(),
        arguments: json!({}),
        fulfillments: Default::default(),
        recording: recording(record),
    }
}

fn observation(revision: u64, record: &str) -> entity_store::RecordedObservation {
    entity_store::RecordedObservation {
        entity: "ticket".into(),
        id: "x".into(),
        revision,
        envelope: recording(record)
            .seal(json!({ "evidence": record }))
            .expect("observation envelope"),
    }
}

async fn run(
    root: &Path,
    authority: &Authority,
    key: &str,
    actions: Vec<BatchAction>,
) -> Result<AppendOutcome, entity_executor::ExecutionError> {
    let store = open(root, authority).await;
    let registry = registry();
    let operation = store.operation(context(key));
    Executor::new(&registry, &operation)
        .batch(BatchKey::Named(key.to_owned()), actions)
        .await
}

/// A provisioned store in which `x` was created at revision 1.
async fn store_with_x() -> (tempfile::TempDir, Authority) {
    let base = tempfile::tempdir().expect("directory");
    let tenant = TenantId::new("adversary-observation").expect("tenant");
    let backend = backend(base.path()).await;
    let stream_identity = backend.stream_identity(&tenant).await.expect("identity");
    let authority = Authority {
        logical_scope: "scope-adversary-observation".into(),
        tenant: tenant.as_str().to_owned(),
        stream_identity,
    };
    let erased: Arc<dyn EventlogBackend> = backend;
    EventlogBindingProvisioner::new(erased, LIMITS)
        .provision_binding(authority.clone(), context("provision"))
        .await
        .expect("binding provisioned");
    run(
        base.path(),
        &authority,
        "create-x",
        vec![BatchAction::Create(CreateRequest {
            subject: Subject::new("ticket", "x").expect("subject"),
            definition_version: 1,
            fields: json!({ "title": "x" }),
            recording: recording("x-create"),
        })],
    )
    .await
    .expect("x is created");
    (base, authority)
}

/// The stale-observation shape the existing suite does not build: one branch decides first, and
/// only afterwards the other branch observes the revision both started from. The observation is
/// of revision 1, which is stale once the branches merge, and it is the later record in time.
#[test]
fn a_stale_observation_recorded_after_the_other_branch_decided_does_not_break_the_merge() {
    block_on(async {
        let (base, authority) = store_with_x().await;
        let ours = branch_of(base.path());
        let theirs = branch_of(base.path());
        run(
            theirs.path(),
            &authority,
            "theirs-touch",
            vec![BatchAction::Execute(touch("theirs-touch", 1))],
        )
        .await
        .expect("their decision");
        std::thread::sleep(std::time::Duration::from_millis(20));
        run(
            ours.path(),
            &authority,
            "ours-seen",
            vec![BatchAction::Observe(observation(1, "ours-seen"))],
        )
        .await
        .expect("our observation of revision 1");
        merge_into(theirs.path(), ours.path());

        let opened = TreeEventStore::open(ours.path()).await;
        let opened = match opened {
            Ok(store) => Arc::new(store),
            Err(error) => panic!("the merged tree does not open: {error:?}"),
        };
        let projector = Arc::new(ErRecordedProjector::new());
        opened
            .create_projections(projector.clone())
            .await
            .expect("projection admission");
        let attached = opened.attach_inline_existing(projector).await;
        assert!(
            attached.is_ok(),
            "a stale observation replayed after the other branch's decision breaks the \
             projection of the merged store: {attached:?}"
        );
        let erased: Arc<dyn EventlogBackend> = opened;
        let store = EventlogRecordedStore::open(erased, authority.clone(), LIMITS).await;
        let store = match store {
            Ok(store) => store,
            Err(error) => panic!(
                "a stale observation merged beside a decision made the store refuse to open: \
                 {error:?}"
            ),
        };
        let x = store
            .operation(context("read"))
            .load(&Subject::new("ticket", "x").expect("subject"))
            .await;
        assert!(
            matches!(x, Ok(Some(ref state)) if state.revision == 2),
            "the merged subject does not serve the decision's revision: {x:?}"
        );
    });
}

/// After an observation merged beside a decision the stream has two tips and one head. A batch
/// that writes `x` twice is the adapter's "the first write is that append; the rest follow it as
/// usual" path: the second member is appended with an exact expectation on a stream the provider
/// still sees with two heads before the group.
#[test]
fn a_batch_writing_a_multi_tip_subject_twice_commits() {
    block_on(async {
        let (base, authority) = store_with_x().await;
        let ours = branch_of(base.path());
        let theirs = branch_of(base.path());
        run(
            ours.path(),
            &authority,
            "ours-seen",
            vec![BatchAction::Observe(observation(1, "ours-seen"))],
        )
        .await
        .expect("our observation");
        std::thread::sleep(std::time::Duration::from_millis(20));
        run(
            theirs.path(),
            &authority,
            "theirs-touch",
            vec![BatchAction::Execute(touch("theirs-touch", 1))],
        )
        .await
        .expect("their decision");
        merge_into(theirs.path(), ours.path());

        let outcome = run(
            ours.path(),
            &authority,
            "twice",
            vec![
                BatchAction::Execute(touch("twice-1", 2)),
                BatchAction::Execute(touch("twice-2", 3)),
            ],
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
            "a batch of two writes to an unforked subject whose stream has two tips was \
             refused: {outcome:?}"
        );
        let x = open(ours.path(), &authority)
            .await
            .operation(context("read"))
            .load(&Subject::new("ticket", "x").expect("subject"))
            .await;
        assert!(
            matches!(x, Ok(Some(ref state)) if state.revision == 4),
            "the two writes did not both land: {x:?}"
        );
    });
}

/// An observation, then a decision, in one batch on a multi-tip subject.
#[test]
fn a_batch_observing_then_deciding_on_a_multi_tip_subject_commits() {
    block_on(async {
        let (base, authority) = store_with_x().await;
        let ours = branch_of(base.path());
        let theirs = branch_of(base.path());
        run(
            ours.path(),
            &authority,
            "ours-seen",
            vec![BatchAction::Observe(observation(1, "ours-seen"))],
        )
        .await
        .expect("our observation");
        std::thread::sleep(std::time::Duration::from_millis(20));
        run(
            theirs.path(),
            &authority,
            "theirs-touch",
            vec![BatchAction::Execute(touch("theirs-touch", 1))],
        )
        .await
        .expect("their decision");
        merge_into(theirs.path(), ours.path());

        let outcome = run(
            ours.path(),
            &authority,
            "seen-then-touch",
            vec![
                BatchAction::Observe(observation(2, "seen-2")),
                BatchAction::Execute(touch("touch-3", 2)),
            ],
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
            "an observation then a decision on an unforked multi-tip subject was refused: \
             {outcome:?}"
        );
    });
}

/// A single observation of the current revision on a multi-tip subject, then three-way tips.
#[test]
fn an_observation_on_a_three_tip_unforked_subject_commits_and_serves_the_decision() {
    block_on(async {
        let (base, authority) = store_with_x().await;
        let a = branch_of(base.path());
        let b = branch_of(base.path());
        let c = branch_of(base.path());
        run(
            a.path(),
            &authority,
            "a-seen",
            vec![BatchAction::Observe(observation(1, "a-seen"))],
        )
        .await
        .expect("a observes");
        std::thread::sleep(std::time::Duration::from_millis(20));
        run(
            b.path(),
            &authority,
            "b-seen",
            vec![BatchAction::Observe(observation(1, "b-seen"))],
        )
        .await
        .expect("b observes");
        std::thread::sleep(std::time::Duration::from_millis(20));
        run(
            c.path(),
            &authority,
            "c-touch",
            vec![BatchAction::Execute(touch("c-touch", 1))],
        )
        .await
        .expect("c decides");
        merge_into(b.path(), a.path());
        merge_into(c.path(), a.path());

        let outcome = run(
            a.path(),
            &authority,
            "seen-2",
            vec![BatchAction::Observe(observation(2, "seen-2"))],
        )
        .await;
        assert!(
            matches!(outcome, Ok(AppendOutcome::Committed { .. })),
            "an observation of the current revision over three tips was refused: {outcome:?}"
        );
        let x = open(a.path(), &authority)
            .await
            .operation(context("read"))
            .load(&Subject::new("ticket", "x").expect("subject"))
            .await;
        assert!(
            matches!(x, Ok(Some(ref state)) if state.revision == 2),
            "three-way tips do not serve the one decision: {x:?}"
        );
    });
}
