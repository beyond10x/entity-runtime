//! Real-provider lifecycle coverage for the recorded adapter and synchronous owner.
#![cfg(any(feature = "file", feature = "sqlite", feature = "postgres"))]

use std::sync::Arc;

use entity_core::{Registry, Runtime};
use entity_eventlog::{
    AsyncBindingProvisioner, AsyncImportedAnchorWriter, Authority, ErRecordedProjector,
    EventlogBackend, EventlogBindingProvisioner, EventlogOperationContext, EventlogRecordedStore,
};
use entity_executor::{BatchAction, CreateRequest, ExecuteRequest, Executor};
use entity_store::{
    RecordedObservation, Recording,
    asynchronous::{
        AppendOutcome, AsyncRecordedReader, AsyncStateReader, BatchKey, CommitReceipt,
        HistoryOrigin, ImportedRecordEvidence, KnownLegacyOrder, LegacyAnchor, LegacyCompleteness,
        LegacyEvidence, LegacyOrderDeclaration, RecordLookup, RecordedEntry, Subject,
        SubjectHistory,
    },
};
use eventlog_core::{CaptureLimits, EventStore, InlineProjectionAdmin, TenantId};
use serde_json::json;
use time::OffsetDateTime;

#[cfg(all(feature = "sync-bridge", feature = "sqlite"))]
use std::num::NonZeroU16;

#[cfg(all(feature = "sync-bridge", feature = "sqlite"))]
use entity_eventlog::sync::{
    BridgeConfig, BridgeRejection, CallWait, EventlogRecordedStoreOwner, RecordedEventlogBridge,
    ShutdownMode, ShutdownOutcome, SyncReadError,
};

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
        subject: "adapter-provider-test".into(),
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

fn recording(record_id: &str) -> Recording {
    Recording {
        record_id: record_id.into(),
        recorded_at: "2026-09-16T00:00:00Z".into(),
        correlation: Some("adapter-provider-contract".into()),
        causation: None,
        actor: None,
    }
}

fn create(label: &str) -> CreateRequest {
    CreateRequest {
        subject: Subject::new("ticket", label).expect("subject"),
        definition_version: 1,
        fields: json!({"title":label}),
        recording: recording(&format!("{label}-create")),
    }
}

fn create_with_record(subject_id: &str, record_id: &str) -> CreateRequest {
    CreateRequest {
        subject: Subject::new("ticket", subject_id).expect("subject"),
        definition_version: 1,
        fields: json!({"title":subject_id}),
        recording: recording(record_id),
    }
}

fn touch(label: &str) -> ExecuteRequest {
    ExecuteRequest {
        subject: Subject::new("ticket", label).expect("subject"),
        expected_revision: 1,
        operation: "touch".into(),
        arguments: json!({}),
        recording: recording(&format!("{label}-touch")),
    }
}

fn observe(label: &str) -> RecordedObservation {
    RecordedObservation {
        entity: "ticket".into(),
        id: label.into(),
        revision: 1,
        envelope: recording(&format!("{label}-observe"))
            .seal(json!({"source":"provider-contract"}))
            .expect("observation envelope"),
    }
}

async fn exercise_provider<B>(backend: Arc<B>, label: &str)
where
    B: EventlogBackend,
{
    let tenant = TenantId::new(format!("adapter-{label}")).expect("valid tenant");
    let stream_identity = backend
        .stream_identity(&tenant)
        .await
        .expect("tenant identity");
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
        logical_scope: format!("scope-{label}"),
        tenant: tenant.as_str().to_owned(),
        stream_identity,
    };
    let erased: Arc<dyn EventlogBackend> = backend;
    let provisioner = EventlogBindingProvisioner::new(erased.clone(), LIMITS);
    let first = provisioner
        .provision_binding(authority.clone(), context(label))
        .await
        .expect("binding provisioned");
    assert!(!first.replayed);
    assert_eq!(first.authority, authority);
    let replay = provisioner
        .provision_binding(authority.clone(), context(label))
        .await
        .expect("binding replayed");
    assert!(replay.replayed);
    assert_eq!(replay.physical, first.physical);
    let mut conflicting_authority = authority.clone();
    conflicting_authority.logical_scope.push_str("-different");
    assert!(matches!(
        provisioner
            .provision_binding(conflicting_authority, context(label))
            .await,
        Err(entity_eventlog::ProvisionBindingFailure::Conflict { .. })
    ));

    let store = EventlogRecordedStore::open(erased, authority.clone(), LIMITS)
        .await
        .expect("ordinary open");
    let snapshot = store
        .complete_snapshot(&authority.logical_scope)
        .await
        .expect("complete empty snapshot");
    assert!(snapshot.histories.is_empty());

    let registry = registry();
    let operation = store.operation(context(&format!("{label}-mixed")));
    let key = BatchKey::Named(format!("{label}-mixed"));
    let actions = vec![
        BatchAction::Create(create(label)),
        BatchAction::Observe(observe(label)),
        BatchAction::Execute(touch(label)),
    ];
    let first = Executor::new(&registry, &operation)
        .batch(key.clone(), actions.clone())
        .await
        .expect("mixed batch");
    let receipt = match &first {
        AppendOutcome::Committed {
            receipt: CommitReceipt::Batch(receipt),
            replayed: false,
        } => receipt,
        other => panic!("unexpected first outcome: {other:?}"),
    };
    assert_eq!(receipt.members.len(), 3);
    assert_eq!(receipt.members[0].revision, 1);
    assert_eq!(receipt.members[1].revision, 1);
    assert_eq!(receipt.members[2].revision, 2);
    assert!(
        receipt
            .members
            .windows(2)
            .all(|pair| pair[0].position.store < pair[1].position.store)
    );

    let single_key_subject = format!("{label}-single-key");
    let single_key = Executor::new(&registry, &operation)
        .create(create_with_record(
            &single_key_subject,
            &format!("{label}-mixed"),
        ))
        .await
        .expect("single-record key remains separate from the equal named key");
    assert!(matches!(
        single_key,
        AppendOutcome::Committed {
            replayed: false,
            ..
        }
    ));

    let race_record_id = format!("{label}-race-record");
    let first_race = create_with_record(&format!("{label}-race-a"), &race_record_id);
    let second_race = create_with_record(&format!("{label}-race-b"), &race_record_id);
    let first_executor = Executor::new(&registry, &operation);
    let second_executor = Executor::new(&registry, &operation);
    let (left, right) = tokio::join!(
        first_executor.create(first_race),
        second_executor.create(second_race)
    );
    assert_eq!(
        usize::from(left.is_ok()) + usize::from(right.is_ok()),
        1,
        "one absent global record identity winner"
    );
    let replay = Executor::new(&registry, &operation)
        .batch(key.clone(), actions)
        .await
        .expect("exact mixed-batch replay");
    assert!(replay.replayed());
    assert_eq!(replay.receipt(), first.receipt());
    let state = store
        .load(&Subject::new("ticket", label).expect("subject"))
        .await
        .expect("state read")
        .expect("created state");
    assert_eq!(state.revision, 2, "zero-event decision advances once");
    let history = store
        .history(&Subject::new("ticket", label).expect("subject"))
        .await
        .expect("history");
    assert_eq!(history.records.len(), 3, "observation remains recorded");
    assert!(
        store
            .lookup_batch(&key)
            .await
            .expect("batch lookup")
            .is_some()
    );

    let legacy_id = format!("{label}-legacy");
    let decision = Runtime::new(&registry)
        .create("ticket", 1, &legacy_id, json!({"title":legacy_id}))
        .expect("legacy decision");
    let commit =
        entity_store::RecordedCommit::new(decision, &recording(&format!("{label}-legacy-create")))
            .expect("legacy record");
    let legacy_record_id = commit.envelope.record_id.clone();
    let imported_history = SubjectHistory {
        subject: Subject::new("ticket", &legacy_id).expect("legacy subject"),
        origin: HistoryOrigin::Imported(LegacyAnchor {
            instance: commit.instance.clone(),
            completeness: LegacyCompleteness::AvailableEvidenceOnly,
            order: LegacyOrderDeclaration::PerKindOnly,
            evidence: vec![LegacyEvidence::Envelope(
                ImportedRecordEvidence::new(
                    RecordedEntry::Decision(commit),
                    format!("source-{label}"),
                    "records/0",
                    KnownLegacyOrder::PerKind(0),
                )
                .expect("imported evidence"),
            )],
        }),
        records: Vec::new(),
    };
    let imported = operation
        .import_anchor(imported_history.clone())
        .await
        .expect("import anchor");
    assert!(!imported.replayed);
    assert!(
        operation
            .import_anchor(imported_history.clone())
            .await
            .expect("import replay")
            .replayed
    );
    assert!(matches!(
        store
            .lookup_record(&legacy_record_id)
            .await
            .expect("imported record lookup"),
        Some(RecordLookup::Imported(_))
    ));

    let imported_suffix = Executor::new(&registry, &operation)
        .execute(ExecuteRequest {
            subject: Subject::new("ticket", &legacy_id).expect("legacy subject"),
            expected_revision: 1,
            operation: "touch".into(),
            arguments: json!({}),
            recording: recording(&format!("{label}-legacy-touch")),
        })
        .await
        .expect("suffix after imported anchor");
    assert!(matches!(
        imported_suffix,
        AppendOutcome::Committed {
            replayed: false,
            ..
        }
    ));
    let replay_after_suffix = operation
        .import_anchor(imported_history)
        .await
        .expect("import replay after suffix");
    assert!(replay_after_suffix.replayed);
    let imported_history = store
        .history(&Subject::new("ticket", &legacy_id).expect("legacy subject"))
        .await
        .expect("imported history with suffix");
    assert!(matches!(
        imported_history.origin,
        HistoryOrigin::Imported(_)
    ));
    assert_eq!(imported_history.records.len(), 1);

    let rebuilt = store
        .rebuild_indexes()
        .await
        .expect("authoritative rebuild");
    assert_eq!(rebuilt.applied, 8);
    assert_eq!(
        store
            .complete_snapshot(&authority.logical_scope)
            .await
            .expect("snapshot after rebuild")
            .histories
            .len(),
        4
    );
}

#[cfg(feature = "file")]
#[test]
fn file_provider_provisions_replays_opens_and_rebuilds() {
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
fn sqlite_memory_provider_provisions_replays_opens_and_rebuilds() {
    block_on(async {
        let backend = Arc::new(
            eventlog_sqlite::SqliteEventStore::in_memory("adapter_memory")
                .await
                .expect("SQLite memory provider"),
        );
        exercise_provider(backend, "sqlite-memory").await;
    });
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_file_provider_provisions_replays_opens_and_rebuilds() {
    block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("adapter.sqlite");
        let backend = Arc::new(
            eventlog_sqlite::SqliteEventStore::open(
                path.to_str().expect("UTF-8 path"),
                "adapter_file",
            )
            .await
            .expect("SQLite file provider"),
        );
        exercise_provider(backend, "sqlite-file").await;
    });
}

#[cfg(all(feature = "sync-bridge", feature = "sqlite"))]
#[test]
fn synchronous_bridge_owns_reopened_provider_and_reports_retirement() {
    block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("bridge.sqlite");
        let path = path.to_str().expect("UTF-8 path").to_owned();
        let prefix = "adapter_bridge".to_owned();
        let tenant = TenantId::new("adapter-bridge").expect("valid tenant");
        let backend = Arc::new(
            eventlog_sqlite::SqliteEventStore::open(&path, &prefix)
                .await
                .expect("setup provider"),
        );
        let stream_identity = backend
            .stream_identity(&tenant)
            .await
            .expect("tenant identity");
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
            logical_scope: "bridge-scope".into(),
            tenant: tenant.as_str().to_owned(),
            stream_identity,
        };
        let erased: Arc<dyn EventlogBackend> = backend;
        EventlogBindingProvisioner::new(erased, LIMITS)
            .provision_binding(authority.clone(), context("bridge"))
            .await
            .expect("binding provisioned");

        let owner = EventlogRecordedStoreOwner::Sqlite {
            path,
            prefix,
            authority,
            limits: LIMITS,
        };
        let mut bridge = RecordedEventlogBridge::start(
            entity_core::Registry::new(),
            owner,
            BridgeConfig {
                queue_capacity: NonZeroU16::new(2).expect("nonzero"),
            },
        )
        .expect("bridge started");
        let absent = bridge
            .load(
                &entity_store::asynchronous::Subject {
                    entity: "Missing".into(),
                    id: "none".into(),
                },
                CallWait::Forever,
            )
            .expect("synchronous read");
        assert!(absent.is_none());
        let joined = ShutdownOutcome::Joined { provider: Ok(()) };
        assert_eq!(
            bridge.shutdown(ShutdownMode::Drain, CallWait::Forever),
            joined
        );
        assert_eq!(
            bridge.shutdown(ShutdownMode::CancelQueued, CallWait::Forever),
            joined,
            "repeat shutdown returns the cached retirement result"
        );
        assert!(matches!(
            bridge.load(
                &Subject::new("ticket", "closed").expect("subject"),
                CallWait::Forever
            ),
            Err(SyncReadError::Rejected(BridgeRejection::Closed))
        ));
        assert!(matches!(
            bridge.operation(context("closed-empty")).batch(
                BatchKey::Named("closed-empty".into()),
                Vec::new(),
                CallWait::Forever
            ),
            Ok(AppendOutcome::Empty)
        ));
    });
}

#[cfg(all(feature = "sync-bridge", feature = "sqlite"))]
async fn prepared_sqlite_owner(
    path: String,
    prefix: String,
    label: &str,
) -> EventlogRecordedStoreOwner {
    let tenant = TenantId::new(format!("adapter-bridge-{label}")).expect("valid tenant");
    let backend = Arc::new(
        eventlog_sqlite::SqliteEventStore::open(&path, &prefix)
            .await
            .expect("setup provider"),
    );
    let stream_identity = backend
        .stream_identity(&tenant)
        .await
        .expect("tenant identity");
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
        logical_scope: format!("bridge-scope-{label}"),
        tenant: tenant.as_str().to_owned(),
        stream_identity,
    };
    let erased: Arc<dyn EventlogBackend> = backend;
    EventlogBindingProvisioner::new(erased, LIMITS)
        .provision_binding(authority.clone(), context(label))
        .await
        .expect("binding provisioned");
    EventlogRecordedStoreOwner::Sqlite {
        path,
        prefix,
        authority,
        limits: LIMITS,
    }
}

#[cfg(all(feature = "sync-bridge", feature = "sqlite"))]
#[test]
fn synchronous_bridge_operates_without_a_caller_runtime() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory
        .path()
        .join("bridge-outside.sqlite")
        .to_str()
        .expect("UTF-8 path")
        .to_owned();
    let owner = block_on(prepared_sqlite_owner(
        path,
        "adapter_bridge_outside".into(),
        "outside",
    ));
    let mut bridge = RecordedEventlogBridge::start(
        entity_core::Registry::new(),
        owner,
        BridgeConfig {
            queue_capacity: NonZeroU16::new(1).expect("nonzero"),
        },
    )
    .expect("bridge started");
    assert!(
        bridge
            .load(
                &Subject::new("ticket", "outside").expect("subject"),
                CallWait::Forever,
            )
            .expect("read")
            .is_none()
    );
    assert_eq!(
        bridge.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
}

#[cfg(all(feature = "sync-bridge", feature = "sqlite"))]
#[test]
fn synchronous_bridge_operates_inside_a_multithread_runtime() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory
        .path()
        .join("bridge-multithread.sqlite")
        .to_str()
        .expect("UTF-8 path")
        .to_owned();
    let owner = block_on(prepared_sqlite_owner(
        path,
        "adapter_bridge_multithread".into(),
        "multithread",
    ));
    let mut bridge = RecordedEventlogBridge::start(
        entity_core::Registry::new(),
        owner,
        BridgeConfig {
            queue_capacity: NonZeroU16::new(1).expect("nonzero"),
        },
    )
    .expect("bridge started");
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("multi-thread runtime")
        .block_on(async {
            assert!(
                bridge
                    .load(
                        &Subject::new("ticket", "multithread").expect("subject"),
                        CallWait::Forever,
                    )
                    .expect("read")
                    .is_none()
            );
        });
    assert_eq!(
        bridge.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
}

#[cfg(feature = "postgres")]
#[test]
fn postgres_provider_provisions_replays_opens_and_rebuilds_when_assigned() {
    block_on(async {
        let Ok(url) = std::env::var("ENTITY_POSTGRES_URL") else {
            eprintln!("PostgreSQL adapter test skipped: ENTITY_POSTGRES_URL is not assigned");
            return;
        };
        let suffix: String = OffsetDateTime::now_utc()
            .unix_timestamp_nanos()
            .unsigned_abs()
            .to_string()
            .bytes()
            .map(|digit| char::from(b'a' + digit - b'0'))
            .collect();
        let prefix = format!("ea_{suffix}");
        let backend = Arc::new(
            eventlog_postgres::PostgresEventStore::connect_local(
                &url,
                &prefix,
                eventlog_postgres::PoolOptions::default(),
            )
            .await
            .expect("PostgreSQL provider"),
        );
        exercise_provider(backend.clone(), "postgres").await;
        backend.shutdown().await.expect("PostgreSQL shutdown");
    });
}
