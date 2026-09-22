//! Actual-provider composition acceptance for `service/3` fulfillment records.
#![cfg(any(feature = "file", feature = "sqlite", feature = "postgres"))]

use std::{collections::BTreeMap, sync::Arc};

use entity_core::{OperationFieldAction, Registry};
use entity_eventlog::{
    AsyncBindingProvisioner, Authority, ErRecordedProjector, EventlogBackend,
    EventlogBindingProvisioner, EventlogOperationContext, EventlogRecordedStore,
};
use entity_executor::{BatchAction, CreateRequest, ExecuteRequest, Executor};
use entity_store::{
    Recording,
    asynchronous::{
        AsyncRecordedReader, AsyncStateReader, AsyncStoreError, BatchKey, RecordLookup, Subject,
        original_request_comparison_bytes, record_comparison_bytes, record_domain, request_domain,
        verify_subject_history,
    },
};
use eventlog_core::{CaptureLimits, TenantId};
use serde_json::json;
use time::OffsetDateTime;

#[cfg(all(feature = "sync-bridge", feature = "sqlite"))]
use std::num::NonZeroU16;

#[cfg(all(feature = "sync-bridge", feature = "sqlite"))]
use entity_eventlog::sync::{
    BridgeConfig, CallWait, EventlogRecordedStoreOwner, RecordedEventlogBridge, ShutdownMode,
    ShutdownOutcome, SyncExecutionError,
};

const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 256,
    max_blobs: 1_024,
    max_projection_rows: 1_024,
    max_payload_bytes: 8 * 1024 * 1024,
};

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

fn registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "invoice",
        "version": 1,
        "semantics": "service/3",
        "schema": { "fields": {
            "title": { "type": "string", "required": true },
            "issued_at": { "type": "string", "required": true },
            "note": { "type": "string" }
        }},
        "lifecycle": { "initial": "Draft", "states": ["Draft"] },
        "operations": { "Issue": {
            "outcomes": [{
                "name": "issued",
                "effect": "updates",
                "fulfills": {
                    "title": { "actions": "required" },
                    "issued_at": { "actions": "required" },
                    "note": { "actions": "optional" }
                }
            }]
        }}
    }))
    .expect("service/3 definition parses");
    let mut registry = Registry::new();
    registry
        .register(definition)
        .expect("service/3 definition validates");
    registry
}

fn context(label: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "service-3-composition-test".into(),
        actor: "entity-eventlog-test".into(),
        request_id: format!("request-{label}"),
        trace_id: format!("trace-{label}"),
        causation_id: None,
        causation_depth: 0,
        occurred_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn recording(record_id: &str) -> Recording {
    Recording {
        record_id: record_id.into(),
        recorded_at: "2026-09-16T10:00:00Z".into(),
        correlation: Some("service-3-composition".into()),
        causation: None,
        actor: None,
    }
}

fn create(id: &str, record_id: &str) -> CreateRequest {
    CreateRequest {
        subject: Subject::new("invoice", id).expect("subject"),
        definition_version: 1,
        fields: json!({
            "title": format!("title-{id}"),
            "issued_at": "pending",
            "note": "remove-me"
        }),
        recording: recording(record_id),
    }
}

fn fulfillments(issued_at: &str) -> BTreeMap<String, OperationFieldAction> {
    BTreeMap::from([
        ("title".into(), OperationFieldAction::Preserve),
        (
            "issued_at".into(),
            OperationFieldAction::Set {
                value: json!(issued_at),
            },
        ),
        ("note".into(), OperationFieldAction::Remove),
    ])
}

fn execute(id: &str, record_id: &str, issued_at: &str) -> ExecuteRequest {
    ExecuteRequest {
        subject: Subject::new("invoice", id).expect("subject"),
        expected_revision: 1,
        operation: "Issue".into(),
        arguments: json!({}),
        fulfillments: fulfillments(issued_at),
        recording: recording(record_id),
    }
}

async fn prepare<B>(backend: Arc<B>, label: &str) -> (Arc<dyn EventlogBackend>, Authority)
where
    B: EventlogBackend + 'static,
{
    let tenant = TenantId::new(format!("service-3-adapter-{label}")).expect("tenant");
    let stream_identity = backend
        .stream_identity(&tenant)
        .await
        .expect("stream identity");
    let projector = Arc::new(ErRecordedProjector::new());
    backend
        .create_projections(projector.clone())
        .await
        .expect("projection admission");
    backend
        .attach_inline_existing(projector)
        .await
        .expect("projection attachment");
    let authority = Authority {
        logical_scope: format!("service-3-scope-{label}"),
        tenant: tenant.as_str().into(),
        stream_identity,
    };
    let erased: Arc<dyn EventlogBackend> = backend;
    EventlogBindingProvisioner::new(erased.clone(), LIMITS)
        .provision_binding(authority.clone(), context(&format!("{label}-binding")))
        .await
        .expect("binding provisioned");
    (erased, authority)
}

async fn assert_history(
    store: &EventlogRecordedStore,
    id: &str,
    issued_at: &str,
) -> entity_store::asynchronous::SubjectHistory {
    let subject = Subject::new("invoice", id).expect("subject");
    let terminal = store
        .load(&subject)
        .await
        .expect("state read")
        .expect("state exists");
    assert_eq!(terminal.revision, 2);
    assert_eq!(terminal.fields["title"], json!(format!("title-{id}")));
    assert_eq!(terminal.fields["issued_at"], json!(issued_at));
    assert!(!terminal.fields.contains_key("note"));

    let history = store.history(&subject).await.expect("history read");
    assert_eq!(history.records.len(), 2);
    verify_subject_history(&history, &terminal).expect("history verifies from genesis");
    let executed = &history.records[1];
    assert_eq!(record_domain(&executed.entry), "er.record/4");
    assert_eq!(request_domain(&executed.entry), "er.request/4");
    assert_eq!(
        executed.record_bytes,
        record_comparison_bytes(&executed.entry).expect("record/4 bytes")
    );
    assert_eq!(
        executed.request_bytes,
        original_request_comparison_bytes(&executed.entry).expect("request/4 bytes")
    );
    let commit = match &executed.entry {
        entity_store::asynchronous::RecordedEntry::Decision(commit) => commit,
        other => panic!("service/3 execution is a decision, got {other:?}"),
    };
    assert_eq!(
        commit.envelope.record.removed,
        ["note".to_owned()].into_iter().collect()
    );
    history
}

async fn exercise_provider<B>(backend: Arc<B>, label: &str)
where
    B: EventlogBackend + 'static,
{
    let (backend, authority) = prepare(backend, label).await;
    let store = EventlogRecordedStore::open(backend.clone(), authority.clone(), LIMITS)
        .await
        .expect("adapter opens");
    let registry = registry();

    let direct_id = format!("{label}-direct");
    let direct_record = format!("{label}-direct-execute");
    let direct_request = execute(&direct_id, &direct_record, "2026-09-16T10:30:00Z");
    let operation = store.operation(context(&format!("{label}-direct")));
    Executor::new(&registry, &operation)
        .create(create(&direct_id, &format!("{label}-direct-create")))
        .await
        .expect("direct creation commits");
    let committed = Executor::new(&registry, &operation)
        .execute(direct_request.clone())
        .await
        .expect("Set/Preserve/Remove commits");
    let original_receipt = committed.receipt().expect("commit receipt").clone();
    assert!(!committed.replayed());

    let retry_operation = store.operation(context(&format!("{label}-direct-retry")));
    let replay = Executor::new(&registry, &retry_operation)
        .execute(direct_request.clone())
        .await
        .expect("exact request/4 retry replays");
    assert!(replay.replayed());
    assert_eq!(replay.receipt(), Some(&original_receipt));

    let changed = execute(&direct_id, &direct_record, "different");
    let conflict = Executor::new(&registry, &retry_operation)
        .execute(changed)
        .await
        .expect_err("changed fulfillment under one record id conflicts");
    assert!(matches!(
        conflict.store_error(),
        Some(AsyncStoreError::RecordConflict { record_id }) if record_id == &direct_record
    ));
    let direct_history = assert_history(&store, &direct_id, "2026-09-16T10:30:00Z").await;
    assert!(matches!(
        store.lookup_record(&direct_record).await.expect("record lookup"),
        Some(RecordLookup::Committed(found)) if found == direct_history.records[1]
    ));

    let left = format!("{label}-batch-left");
    let right = format!("{label}-batch-right");
    let batch_key = BatchKey::Named(format!("{label}-service-3-batch"));
    let actions = vec![
        BatchAction::Create(create(&left, &format!("{label}-left-create"))),
        BatchAction::Execute(execute(
            &left,
            &format!("{label}-left-execute"),
            "2026-09-16T11:00:00Z",
        )),
        BatchAction::Create(create(&right, &format!("{label}-right-create"))),
        BatchAction::Execute(execute(
            &right,
            &format!("{label}-right-execute"),
            "2026-09-16T11:30:00Z",
        )),
    ];
    let batch_operation = store.operation(context(&format!("{label}-batch")));
    let batch = Executor::new(&registry, &batch_operation)
        .batch(batch_key.clone(), actions.clone())
        .await
        .expect("atomic service/3 batch commits");
    let batch_receipt = batch.receipt().expect("batch receipt").clone();
    assert!(!batch.replayed());
    let batch_replay = Executor::new(&registry, &batch_operation)
        .batch(batch_key.clone(), actions.clone())
        .await
        .expect("exact atomic batch retry replays");
    assert!(batch_replay.replayed());
    assert_eq!(batch_replay.receipt(), Some(&batch_receipt));

    let mut changed_batch = actions;
    let BatchAction::Execute(changed_left) = &mut changed_batch[1] else {
        unreachable!("second action is execute")
    };
    changed_left.fulfillments = fulfillments("different");
    let conflict = Executor::new(&registry, &batch_operation)
        .batch(batch_key.clone(), changed_batch)
        .await
        .expect_err("changed fulfillment under one batch key conflicts");
    assert!(matches!(
        conflict.store_error(),
        Some(AsyncStoreError::RecordConflict { record_id })
            if record_id == &format!("{label}-left-execute")
    ));

    let reopened = EventlogRecordedStore::open(backend, authority, LIMITS)
        .await
        .expect("adapter reopens after service/3 records");
    let reopened_direct = assert_history(&reopened, &direct_id, "2026-09-16T10:30:00Z").await;
    assert_eq!(reopened_direct, direct_history);
    assert_history(&reopened, &left, "2026-09-16T11:00:00Z").await;
    assert_history(&reopened, &right, "2026-09-16T11:30:00Z").await;
    let stored_batch = reopened
        .lookup_batch(&batch_key)
        .await
        .expect("batch lookup")
        .expect("batch remains present");
    assert_eq!(stored_batch.records.len(), 4);
    assert_eq!(
        stored_batch.records[1].request_bytes,
        original_request_comparison_bytes(&stored_batch.records[1].entry)
            .expect("left request/4 bytes")
    );
    assert_eq!(
        stored_batch.records[3].record_bytes,
        record_comparison_bytes(&stored_batch.records[3].entry).expect("right record/4 bytes")
    );
}

#[cfg(feature = "file")]
#[test]
fn file_provider_preserves_complete_service_3_composition() {
    block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        let backend = Arc::new(
            eventlog_file::FileEventStore::open(directory.path())
                .await
                .expect("file provider"),
        );
        exercise_provider(backend, "file").await;
    });
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_memory_provider_preserves_complete_service_3_composition() {
    block_on(async {
        let backend = Arc::new(
            eventlog_sqlite::SqliteEventStore::in_memory("service_memory")
                .await
                .expect("SQLite memory provider"),
        );
        exercise_provider(backend, "sqlite-memory").await;
    });
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_file_provider_preserves_complete_service_3_composition() {
    block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("service-3.sqlite");
        let backend = Arc::new(
            eventlog_sqlite::SqliteEventStore::open(
                path.to_str().expect("UTF-8 path"),
                "service_file",
            )
            .await
            .expect("SQLite file provider"),
        );
        exercise_provider(backend, "sqlite-file").await;
    });
}

#[cfg(feature = "postgres")]
#[test]
fn postgres_provider_preserves_complete_service_3_composition_when_assigned() {
    block_on(async {
        let Ok(url) = std::env::var("ENTITY_POSTGRES_URL") else {
            eprintln!("PostgreSQL service/3 test skipped: ENTITY_POSTGRES_URL is not assigned");
            return;
        };
        let suffix: String = OffsetDateTime::now_utc()
            .unix_timestamp_nanos()
            .unsigned_abs()
            .to_string()
            .bytes()
            .map(|digit| char::from(b'a' + digit - b'0'))
            .collect();
        let prefix = format!("es_{suffix}");
        let backend = Arc::new(
            eventlog_postgres::PostgresEventStore::connect_local(
                &url,
                &prefix,
                eventlog_postgres::PoolOptions::default(),
            )
            .await
            .expect("PostgreSQL provider"),
        );
        exercise_provider(backend.clone(), &format!("postgres-{suffix}")).await;
        backend.shutdown().await.expect("PostgreSQL shutdown");
    });
}

#[cfg(all(feature = "sync-bridge", feature = "sqlite"))]
#[test]
fn sqlite_bridge_crosses_service_3_retry_history_and_reopen() {
    block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory
            .path()
            .join("service-3-bridge.sqlite")
            .to_str()
            .expect("UTF-8 path")
            .to_owned();
        let prefix = "service_bridge".to_owned();
        let backend = Arc::new(
            eventlog_sqlite::SqliteEventStore::open(&path, &prefix)
                .await
                .expect("SQLite setup provider"),
        );
        let (backend, authority) = prepare(backend, "sqlite-bridge").await;
        drop(backend);

        let owner = || EventlogRecordedStoreOwner::Sqlite {
            path: path.clone(),
            prefix: prefix.clone(),
            authority: authority.clone(),
            limits: LIMITS,
        };
        let mut bridge = RecordedEventlogBridge::start(
            registry(),
            owner(),
            BridgeConfig {
                queue_capacity: NonZeroU16::new(2).expect("capacity"),
            },
        )
        .expect("bridge starts");
        let id = "bridge-invoice";
        bridge
            .operation(context("bridge-create"))
            .create(create(id, "bridge-service-3-create"), CallWait::Forever)
            .expect("bridge creation commits");
        let request = execute(id, "bridge-service-3-execute", "2026-09-16T12:00:00Z");
        let committed = bridge
            .operation(context("bridge-execute"))
            .execute(request.clone(), CallWait::Forever)
            .expect("bridge Set/Preserve/Remove commits");
        let receipt = committed.receipt().expect("receipt").clone();
        let subject = Subject::new("invoice", id).expect("subject");
        let terminal = bridge
            .load(&subject, CallWait::Forever)
            .expect("bridge state")
            .expect("state exists");
        let history = bridge
            .history(&subject, CallWait::Forever)
            .expect("bridge history");
        verify_subject_history(&history, &terminal).expect("bridge history verifies");
        assert_eq!(request_domain(&history.records[1].entry), "er.request/4");
        assert_eq!(record_domain(&history.records[1].entry), "er.record/4");
        assert_eq!(
            bridge.shutdown(ShutdownMode::Drain, CallWait::Forever),
            ShutdownOutcome::Joined { provider: Ok(()) }
        );

        let mut reopened = RecordedEventlogBridge::start(
            registry(),
            owner(),
            BridgeConfig {
                queue_capacity: NonZeroU16::new(2).expect("capacity"),
            },
        )
        .expect("bridge reopens");
        let replay = reopened
            .operation(context("bridge-retry-after-reopen"))
            .execute(request, CallWait::Forever)
            .expect("exact bridge retry after reopen");
        assert!(replay.replayed());
        assert_eq!(replay.receipt(), Some(&receipt));
        let changed = execute(id, "bridge-service-3-execute", "different");
        let conflict = reopened
            .operation(context("bridge-conflict"))
            .execute(changed, CallWait::Forever)
            .expect_err("changed bridge fulfillment conflicts");
        assert!(matches!(
            conflict,
            SyncExecutionError::Execution(error)
                if matches!(
                    error.store_error(),
                    Some(AsyncStoreError::RecordConflict { record_id })
                        if record_id == "bridge-service-3-execute"
                )
        ));
        assert_eq!(
            reopened.shutdown(ShutdownMode::Drain, CallWait::Forever),
            ShutdownOutcome::Joined { provider: Ok(()) }
        );
    });
}
