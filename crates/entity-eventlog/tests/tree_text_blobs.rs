//! A recorded store on a tree written in the first layout (`eventlog-tree/1`) reads the same
//! after Eventlog's migration keeps each long text once (`eventlog-tree/2`): every record and
//! anchor blob is still found under its framed key with the bytes that key was taken over, and the
//! store keeps recording.
#![cfg(feature = "tree")]

use std::{fs, path::Path, sync::Arc};

use entity_core::Registry;
use entity_eventlog::{
    AsyncBindingProvisioner, Authority, ErRecordedProjector, EventlogBackend,
    EventlogBindingProvisioner, EventlogOperationContext, EventlogRecordedStore,
};
use entity_executor::{BatchAction, CreateRequest, Executor};
use entity_store::{
    Recording,
    asynchronous::{AppendOutcome, AsyncRecordedReader, BatchKey, Subject},
};
use eventlog_core::{CaptureLimits, EventStore, InlineProjectionAdmin, TenantId};
use eventlog_tree::{MigrationMode, TreeEventStore, migrate};
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
        subject: "tree-text-test".into(),
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
        "operations": {}
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    registry
}

fn long_title(id: &str) -> String {
    format!(
        "{id}: {}",
        "a sentence long enough to be kept once. ".repeat(60)
    )
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
    let outcome = record_outcome(root, authority, id).await;
    assert!(matches!(outcome, AppendOutcome::Committed { .. }));
}

/// Create ticket `id` through a fresh handle, as a process that reopens the store does, and
/// return what the store answered.
async fn record_outcome(root: &Path, authority: &Authority, id: &str) -> AppendOutcome {
    let backend: Arc<dyn EventlogBackend> = backend(root).await;
    let store = EventlogRecordedStore::open(backend, authority.clone(), LIMITS)
        .await
        .expect("ordinary open");
    let registry = registry();
    let operation = store.operation(context(id));
    Executor::new(&registry, &operation)
        .batch(
            BatchKey::Named(format!("create-{id}")),
            vec![BatchAction::Create(CreateRequest {
                subject: Subject::new("ticket", id).expect("subject"),
                definition_version: 1,
                fields: json!({ "title": long_title(id) }),
                recording: Recording {
                    record_id: format!("{id}-create"),
                    recorded_at: "2026-09-27T00:00:00Z".into(),
                    correlation: None,
                    causation: None,
                    actor: None,
                },
            })],
        )
        .await
        .expect("a batch that creates one ticket")
}

/// The group file format `store.json` declares, `None` when it declares none (Eventlog 0.10.0
/// and earlier wrote no `groups` key).
fn declared_group_format(root: &Path) -> Option<String> {
    let text = fs::read_to_string(root.join("store.json")).expect("store.json readable");
    let value: serde_json::Value = serde_json::from_str(&text).expect("store.json is JSON");
    value
        .get("groups")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

async fn provisioned(root: &Path, tenant: &str) -> Authority {
    let tenant = TenantId::new(tenant).expect("tenant");
    let backend = backend(root).await;
    let stream_identity = backend.stream_identity(&tenant).await.expect("identity");
    let authority = Authority {
        logical_scope: format!("scope-{}", tenant.as_str()),
        tenant: tenant.as_str().to_owned(),
        stream_identity,
    };
    let erased: Arc<dyn EventlogBackend> = backend;
    EventlogBindingProvisioner::new(erased, LIMITS)
        .provision_binding(authority.clone(), context("provision"))
        .await
        .expect("binding provisioned");
    authority
}

async fn snapshot(root: &Path, authority: &Authority) -> String {
    let _ = fs::remove_dir_all(root.join(".cache"));
    let backend: Arc<dyn EventlogBackend> = backend(root).await;
    let store = EventlogRecordedStore::open(backend, authority.clone(), LIMITS)
        .await
        .expect("the store opens");
    let snapshot = store
        .complete_snapshot(&authority.logical_scope)
        .await
        .expect("a complete snapshot");
    format!("{:?}", snapshot.histories)
}

fn files_under(root: &Path, kind: &str) -> usize {
    let mut count = 0;
    let mut stack = vec![root.to_owned()];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory).expect("readable").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.components().any(|part| part.as_os_str() == kind) {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn a_first_layout_store_reads_the_same_after_its_texts_are_kept_once_and_keeps_recording() {
    block_on(async {
        let directory = tempfile::tempdir().expect("directory");
        let root = directory.path();
        fs::write(
            root.join("store.json"),
            br#"{"format":"eventlog-tree/1","identity":"store-1"}"#,
        )
        .expect("a first-layout store.json");
        let tenant = TenantId::new("tree-texts").expect("tenant");
        let authority = {
            let backend = backend(root).await;
            let stream_identity = backend.stream_identity(&tenant).await.expect("identity");
            let authority = Authority {
                logical_scope: "scope-tree-texts".into(),
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
        record(root, &authority, "first").await;
        assert_eq!(
            files_under(root, "texts"),
            0,
            "a first-layout store wrote texts"
        );
        let before = snapshot(root, &authority).await;

        let report = migrate(root, MigrationMode::Apply).expect("the migration");
        assert!(
            report.blobs_split > 0,
            "no recorded blob held a long text: {report:?}"
        );
        assert_eq!(report.verified_blobs, Some(report.blobs));
        assert!(files_under(root, "texts") > 0);
        assert_eq!(
            snapshot(root, &authority).await,
            before,
            "the migrated store reads its history differently"
        );

        record(root, &authority, "second").await;
        let after = snapshot(root, &authority).await;
        assert!(after.contains(&long_title("second")) && after.contains(&long_title("first")));
    });
}

#[test]
fn a_new_tree_store_keeps_each_recorded_group_with_its_fingerprint() {
    block_on(async {
        let directory = tempfile::tempdir().expect("directory");
        let root = directory.path();
        let authority = provisioned(root, "tree-groups-new").await;
        assert_eq!(
            declared_group_format(root).as_deref(),
            Some("eventlog-tree/group/2"),
            "a store Entity Runtime provisioned does not declare the fingerprinted group format"
        );
        let first = record_outcome(root, &authority, "first").await;
        assert!(
            matches!(
                first,
                AppendOutcome::Committed {
                    replayed: false,
                    ..
                }
            ),
            "{first:?}"
        );
        let retry = record_outcome(root, &authority, "first").await;
        assert!(
            matches!(retry, AppendOutcome::Committed { replayed: true, .. }),
            "the exact retry after a reopen was not answered from the recorded group: {retry:?}"
        );
    });
}

#[test]
fn a_tree_store_written_before_eventlog_0_11_is_fingerprinted_by_migrate_and_keeps_its_retries() {
    block_on(async {
        let directory = tempfile::tempdir().expect("directory");
        let root = directory.path();
        fs::write(
            root.join("store.json"),
            br#"{"format":"eventlog-tree/2","identity":"store-1"}"#,
        )
        .expect("a store.json as Eventlog 0.10.0 wrote it");
        let authority = provisioned(root, "tree-groups-old").await;
        record(root, &authority, "first").await;
        assert_eq!(
            declared_group_format(root),
            None,
            "a store written before Eventlog 0.11.0 changed its group format by itself"
        );
        let before = snapshot(root, &authority).await;

        let report = migrate(root, MigrationMode::Apply).expect("the migration");
        assert!(!report.group_fingerprints_before, "{report:?}");
        assert!(
            report.groups_fingerprinted >= 2,
            "the binding and the record group were not both fingerprinted: {report:?}"
        );
        assert_eq!(report.groups_without_fingerprint, 0, "{report:?}");
        assert_eq!(
            declared_group_format(root).as_deref(),
            Some("eventlog-tree/group/2")
        );
        assert_eq!(
            snapshot(root, &authority).await,
            before,
            "the migrated store reads its history differently"
        );

        let retry = record_outcome(root, &authority, "first").await;
        assert!(
            matches!(retry, AppendOutcome::Committed { replayed: true, .. }),
            "the exact retry of a group recorded before the migration was not replayed: {retry:?}"
        );
        record(root, &authority, "second").await;
        let after = snapshot(root, &authority).await;
        assert!(after.contains(&long_title("second")) && after.contains(&long_title("first")));
    });
}
