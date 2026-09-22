//! Real-provider lifecycle coverage for the recorded adapter and synchronous owner.
#![cfg(any(feature = "file", feature = "sqlite", feature = "postgres"))]

use std::sync::Arc;

use entity_core::{Registry, Runtime};
use entity_eventlog::{
    AsyncBindingProvisioner, AsyncImportedAnchorWriter, Authority, ErRecordedProjector,
    EventlogBackend, EventlogBindingProvisioner, EventlogOperationContext, EventlogRecordedStore,
    ImportAnchorFailure, projection_specs,
};
use entity_executor::{BatchAction, CreateRequest, ExecuteRequest, Executor};
use entity_store::{
    RecordedObservation, Recording,
    asynchronous::{
        AppendOutcome, AsyncRecordedReader, AsyncStateReader, AsyncStoreError, BatchKey,
        CommitReceipt, HistoryOrigin, ImportedRecordEvidence, KnownLegacyOrder, LegacyAnchor,
        LegacyCompleteness, LegacyEvidence, LegacyOrderDeclaration, RecordLookup, RecordedEntry,
        Subject, SubjectHistory,
    },
};
use eventlog_core::{
    CaptureLimits, ConsistentTenantCapture, EventStore, InlineProjectionAdmin, TenantId,
};
use serde_json::json;
use time::OffsetDateTime;

#[cfg(all(feature = "sync-bridge", feature = "sqlite"))]
use std::num::NonZeroU16;

#[cfg(all(feature = "sync-bridge", feature = "sqlite"))]
use entity_eventlog::sync::{
    BridgeConfig, BridgeRejection, BridgeStartError, CallWait, EventlogRecordedStoreOwner,
    RecordedEventlogBridge, ShutdownMode, ShutdownOutcome, SyncReadError,
};

#[cfg(all(feature = "sync-bridge", feature = "sqlite"))]
use eventlog_core::{
    BoxFuture, EventLogError, ProjectionSpec, ProjectionStore, Projector, RecordedEvent,
};

const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 128,
    max_blobs: 512,
    max_projection_rows: 512,
    max_payload_bytes: 4 * 1024 * 1024,
};

#[cfg(all(feature = "sync-bridge", feature = "sqlite"))]
struct DriftedErProjector;

#[cfg(all(feature = "sync-bridge", feature = "sqlite"))]
impl Projector for DriftedErProjector {
    fn name(&self) -> &'static str {
        "er_recorded_v1"
    }

    fn projections(&self) -> &'static [ProjectionSpec] {
        static SPECS: std::sync::OnceLock<Vec<ProjectionSpec>> = std::sync::OnceLock::new();
        SPECS.get_or_init(|| {
            let mut specs = projection_specs().to_vec();
            specs[0].indexed = &["drifted_field"];
            specs
        })
    }

    fn apply<'a>(
        &'a self,
        _: &'a RecordedEvent,
        _: &'a mut dyn ProjectionStore,
    ) -> BoxFuture<'a, Result<(), EventLogError>> {
        Box::pin(async { Ok(()) })
    }
}

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
        fulfillments: Default::default(),
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
    let binding_physical = first.physical.clone();
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
            fulfillments: Default::default(),
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
    let recovered_binding = provisioner
        .recover_binding(authority.clone())
        .await
        .expect("binding recovery validates every later record and imported anchor")
        .expect("binding remains present");
    assert_eq!(recovered_binding.physical, binding_physical);
    let mut requested = authority.clone();
    requested.logical_scope.push_str("-after-complete-history");
    assert_eq!(
        provisioner.recover_binding(requested.clone()).await,
        Err(entity_eventlog::ProvisionBindingFailure::Conflict {
            requested,
            found: authority,
        })
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
async fn sqlite_authority(
    path: &str,
    prefix: &str,
    label: &str,
) -> (Arc<eventlog_sqlite::SqliteEventStore>, Authority) {
    let backend = Arc::new(
        eventlog_sqlite::SqliteEventStore::open(path, prefix)
            .await
            .expect("SQLite provider"),
    );
    let tenant = TenantId::new(format!("adapter-bridge-admission-{label}")).expect("tenant");
    let stream_identity = backend
        .stream_identity(&tenant)
        .await
        .expect("stream identity");
    (
        backend,
        Authority {
            logical_scope: format!("bridge-admission-{label}"),
            tenant: tenant.as_str().to_owned(),
            stream_identity,
        },
    )
}

#[cfg(all(feature = "sync-bridge", feature = "sqlite"))]
#[test]
fn synchronous_bridge_startup_refuses_missing_drifted_and_dirty_projection_admission() {
    block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");

        let missing_path = directory.path().join("bridge-missing.sqlite");
        let missing_path = missing_path.to_str().expect("UTF-8 path").to_owned();
        let missing_prefix = "adapter_bridge_missing";
        let (missing, authority) = sqlite_authority(&missing_path, missing_prefix, "missing").await;
        drop(missing);
        assert!(matches!(
            RecordedEventlogBridge::start(
                Registry::new(),
                EventlogRecordedStoreOwner::Sqlite {
                    path: missing_path.clone(),
                    prefix: missing_prefix.into(),
                    authority,
                    limits: LIMITS,
                },
                BridgeConfig {
                    queue_capacity: NonZeroU16::new(1).expect("capacity"),
                },
            ),
            Err(BridgeStartError::Open(AsyncStoreError::Backend(_)))
        ));
        let missing = eventlog_sqlite::SqliteEventStore::open(&missing_path, missing_prefix)
            .await
            .expect("reopen missing provider");
        assert!(!missing.is_inline("er_recorded_v1").await);

        let drifted_path = directory.path().join("bridge-drifted.sqlite");
        let drifted_path = drifted_path.to_str().expect("UTF-8 path").to_owned();
        let drifted_prefix = "adapter_bridge_drifted";
        let (drifted, authority) = sqlite_authority(&drifted_path, drifted_prefix, "drifted").await;
        drifted
            .create_projections(Arc::new(DriftedErProjector))
            .await
            .expect("drifted projection setup");
        drop(drifted);
        assert!(matches!(
            RecordedEventlogBridge::start(
                Registry::new(),
                EventlogRecordedStoreOwner::Sqlite {
                    path: drifted_path.clone(),
                    prefix: drifted_prefix.into(),
                    authority,
                    limits: LIMITS,
                },
                BridgeConfig {
                    queue_capacity: NonZeroU16::new(1).expect("capacity"),
                },
            ),
            Err(BridgeStartError::Open(AsyncStoreError::Backend(_)))
        ));
        let drifted = eventlog_sqlite::SqliteEventStore::open(&drifted_path, drifted_prefix)
            .await
            .expect("reopen drifted provider");
        assert!(!drifted.is_inline("er_recorded_v1").await);

        let dirty_path = directory.path().join("bridge-dirty.sqlite");
        let dirty_path = dirty_path.to_str().expect("UTF-8 path").to_owned();
        let dirty_prefix = "adapter_bridge_dirty";
        let (dirty, authority) = sqlite_authority(&dirty_path, dirty_prefix, "dirty").await;
        let projector = Arc::new(ErRecordedProjector::new());
        dirty
            .create_projections(projector.clone())
            .await
            .expect("projection setup");
        dirty
            .attach_inline_existing(projector)
            .await
            .expect("projection attachment");
        let erased: Arc<dyn EventlogBackend> = dirty.clone();
        EventlogBindingProvisioner::new(erased, LIMITS)
            .provision_binding(authority.clone(), context("dirty-binding"))
            .await
            .expect("binding provisioned");
        let tenant = TenantId::new(authority.tenant.clone()).expect("tenant");
        let captured = dirty
            .capture_tenant(&tenant, projection_specs(), LIMITS)
            .await
            .expect("capture before redaction");
        let binding_event = captured.events.first().expect("binding event");
        let stream = binding_event.stream().expect("binding stream");
        dirty
            .redact(
                &stream,
                binding_event.version,
                "controlled projection dirtying",
            )
            .await
            .expect("redaction dirtied attached projections");
        assert!(
            dirty
                .capture_tenant(&tenant, projection_specs(), LIMITS)
                .await
                .is_err(),
            "dirty/redacted authority is no longer capturable"
        );
        drop(dirty);
        assert!(matches!(
            RecordedEventlogBridge::start(
                Registry::new(),
                EventlogRecordedStoreOwner::Sqlite {
                    path: dirty_path,
                    prefix: dirty_prefix.into(),
                    authority,
                    limits: LIMITS,
                },
                BridgeConfig {
                    queue_capacity: NonZeroU16::new(1).expect("capacity"),
                },
            ),
            Err(BridgeStartError::Open(
                AsyncStoreError::ProviderIntegrity { .. }
            ))
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

#[cfg(all(feature = "sync-bridge", feature = "postgres"))]
#[test]
fn postgres_bridge_awaits_worker_owned_provider_retirement_when_assigned() {
    block_on(async {
        let Ok(url) = std::env::var("ENTITY_POSTGRES_URL") else {
            eprintln!("PostgreSQL bridge test skipped: ENTITY_POSTGRES_URL is not assigned");
            return;
        };
        let suffix: String = OffsetDateTime::now_utc()
            .unix_timestamp_nanos()
            .unsigned_abs()
            .to_string()
            .bytes()
            .map(|digit| char::from(b'a' + digit - b'0'))
            .collect();
        let prefix = format!("eb_{suffix}");
        let backend = Arc::new(
            eventlog_postgres::PostgresEventStore::connect_local(
                &url,
                &prefix,
                eventlog_postgres::PoolOptions::default(),
            )
            .await
            .expect("PostgreSQL setup provider"),
        );
        let tenant =
            TenantId::new(format!("adapter-bridge-postgres-{suffix}")).expect("valid tenant");
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
            logical_scope: format!("bridge-postgres-{suffix}"),
            tenant: tenant.as_str().to_owned(),
            stream_identity,
        };
        let erased: Arc<dyn EventlogBackend> = backend.clone();
        EventlogBindingProvisioner::new(erased, LIMITS)
            .provision_binding(authority.clone(), context("bridge-postgres"))
            .await
            .expect("binding provisioned");
        backend.shutdown().await.expect("setup provider shutdown");

        let mut bridge = RecordedEventlogBridge::start(
            Registry::new(),
            EventlogRecordedStoreOwner::PostgresLocal {
                url,
                prefix,
                authority,
                limits: LIMITS,
            },
            BridgeConfig {
                queue_capacity: NonZeroU16::new(1).expect("capacity"),
            },
        )
        .expect("PostgreSQL bridge started");
        assert_eq!(
            bridge.shutdown(ShutdownMode::Drain, CallWait::Forever),
            ShutdownOutcome::Joined { provider: Ok(()) }
        );
    });
}

// --- Batched imported boundaries ------------------------------------------------------------
//
// One capture and one append group for a whole batch, writing exactly the bytes N singular
// `import_anchor` calls write. The byte comparison is what makes the batch admissible: a faster
// import that writes different anchors, keys or receipts has changed what the migration moves.

#[cfg(feature = "file")]
fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).expect("destination directory");
    for entry in std::fs::read_dir(from).expect("readable source directory") {
        let entry = entry.expect("directory entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("entry type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copied file");
        }
    }
}

#[cfg(any(feature = "file", feature = "sqlite"))]
fn imported_batch_history_with(label: &str, record_id: &str, locator: &str) -> SubjectHistory {
    let registry = registry();
    let decision = Runtime::new(&registry)
        .create("ticket", 1, label, json!({ "title": label }))
        .expect("legacy decision");
    let commit =
        entity_store::RecordedCommit::new(decision, &recording(record_id)).expect("legacy record");
    SubjectHistory {
        subject: Subject::new("ticket", label).expect("legacy subject"),
        origin: HistoryOrigin::Imported(LegacyAnchor {
            instance: commit.instance.clone(),
            completeness: LegacyCompleteness::AvailableEvidenceOnly,
            order: LegacyOrderDeclaration::PerKindOnly,
            evidence: vec![LegacyEvidence::Envelope(
                ImportedRecordEvidence::new(
                    RecordedEntry::Decision(commit),
                    "import-batch-source".to_owned(),
                    locator.to_owned(),
                    KnownLegacyOrder::PerKind(0),
                )
                .expect("imported evidence"),
            )],
        }),
        records: Vec::new(),
    }
}

#[cfg(any(feature = "file", feature = "sqlite"))]
fn imported_batch_history(label: &str) -> SubjectHistory {
    imported_batch_history_with(label, &format!("{label}-create"), "records/0")
}

/// Everything about an event that the adapter decides, with the provider's own minted identity
/// and arrival order removed: two runs of the same input cannot share an event id or a wall clock.
#[cfg(feature = "file")]
fn event_content(capture: &eventlog_core::TenantCapture) -> Vec<serde_json::Value> {
    capture
        .events
        .iter()
        .map(|event| {
            json!({
                "stream_type": event.stream_type,
                "stream_id": event.stream_id,
                "version": event.version,
                "name": event.name,
                "schema_version": event.schema_version,
                "occurred_at": event.occurred_at.unix_timestamp_nanos().to_string(),
                "subject": event.subject,
                "actor": event.actor,
                "request_id": event.request_id,
                "trace_id": event.trace_id,
                "causation_id": event.causation_id,
                "causation_depth": event.causation_depth,
                "data": event.data,
            })
        })
        .collect()
}

/// Every blob the destination currently binds, by key.
#[cfg(feature = "file")]
async fn bound_digests(backend: &Arc<dyn EventlogBackend>, tenant: &TenantId) -> Vec<String> {
    backend
        .capture_tenant(tenant, projection_specs(), LIMITS)
        .await
        .expect("capture")
        .blobs
        .into_iter()
        .map(|blob| blob.digest)
        .collect()
}

#[cfg(feature = "file")]
async fn open_import_destination(
    directory: &std::path::Path,
    authority: &Authority,
) -> (Arc<dyn EventlogBackend>, EventlogRecordedStore) {
    let backend = Arc::new(
        eventlog_file::FileEventStore::open(directory)
            .await
            .expect("file provider"),
    );
    backend
        .attach_inline_existing(Arc::new(ErRecordedProjector::new()))
        .await
        .expect("projection attachment");
    let erased: Arc<dyn EventlogBackend> = backend;
    let store = EventlogRecordedStore::open(erased.clone(), authority.clone(), LIMITS)
        .await
        .expect("bound store");
    (erased, store)
}

#[cfg(feature = "file")]
async fn provisioned_file_destination(
    directory: &std::path::Path,
    label: &str,
) -> (Arc<dyn EventlogBackend>, Authority, TenantId) {
    let tenant = TenantId::new(format!("import-batch-{label}")).expect("valid tenant");
    let backend = Arc::new(
        eventlog_file::FileEventStore::open(directory)
            .await
            .expect("file provider"),
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
        logical_scope: format!("import-batch-{label}-scope"),
        tenant: tenant.as_str().to_owned(),
        stream_identity,
    };
    let erased: Arc<dyn EventlogBackend> = backend;
    EventlogBindingProvisioner::new(erased.clone(), LIMITS)
        .provision_binding(authority.clone(), context(label))
        .await
        .expect("binding provisioned");
    (erased, authority, tenant)
}

#[cfg(feature = "file")]
#[test]
fn a_batch_import_writes_the_same_bytes_as_singular_imports_from_one_capture() {
    block_on(async {
        let root = tempfile::tempdir().expect("temporary directory");
        let singular_directory = root.path().join("singular");
        let batch_directory = root.path().join("batch");

        // One provisioned destination, copied before the first import, so both runs carry the
        // same `Authority` — identical tenant, logical scope and stream identity. Without that the
        // anchors would differ for a reason that says nothing about batching.
        let (_, authority, tenant) =
            provisioned_file_destination(&singular_directory, "identity").await;
        copy_tree(&singular_directory, &batch_directory);

        let histories: Vec<SubjectHistory> = (0..4)
            .map(|ordinal| imported_batch_history(&format!("batch-subject-{ordinal}")))
            .collect();

        let (singular_backend, singular_store) =
            open_import_destination(&singular_directory, &authority).await;
        let singular_operation = singular_store.operation(context("identity-run"));
        let singular_before = singular_store.calls().captures;
        let mut singular_outcomes = Vec::new();
        for history in &histories {
            singular_outcomes.push(
                singular_operation
                    .import_anchor(history.clone())
                    .await
                    .expect("singular import"),
            );
        }
        let singular_captures = singular_store.calls().captures - singular_before;

        let (batch_backend, batch_store) =
            open_import_destination(&batch_directory, &authority).await;
        let batch_operation = batch_store.operation(context("identity-run"));
        let batch_before = batch_store.calls().captures;
        let batch_outcomes = batch_operation
            .import_anchors(histories.clone())
            .await
            .expect("batch import");
        let batch_captures = batch_store.calls().captures - batch_before;

        assert_eq!(
            singular_captures,
            2 * histories.len(),
            "each singular import takes a capture before and after"
        );
        assert_eq!(
            batch_captures, 2,
            "the batch takes one capture to verify against and one to verify with"
        );

        assert_eq!(
            batch_outcomes, singular_outcomes,
            "the batch returns the receipts the singular calls return"
        );

        let singular_capture = singular_backend
            .capture_tenant(&tenant, projection_specs(), LIMITS)
            .await
            .expect("singular capture");
        let batch_capture = batch_backend
            .capture_tenant(&tenant, projection_specs(), LIMITS)
            .await
            .expect("batch capture");

        assert_eq!(
            batch_capture.blobs, singular_capture.blobs,
            "every anchor and record blob, and the key it is stored under, is byte-identical"
        );
        assert_eq!(
            event_content(&batch_capture),
            event_content(&singular_capture),
            "the same subject streams carry the same events naming the same anchor blobs"
        );
        assert_eq!(
            batch_store
                .complete_snapshot(&authority.logical_scope)
                .await
                .expect("batch snapshot")
                .histories,
            singular_store
                .complete_snapshot(&authority.logical_scope)
                .await
                .expect("singular snapshot")
                .histories,
            "both destinations hold the same complete logical history"
        );

        // The singular path's own byte comparison, run against what the batch committed: an exact
        // replay is only reported when the committed anchor equals the bytes this call encodes.
        for history in &histories {
            assert!(
                batch_operation
                    .import_anchor(history.clone())
                    .await
                    .expect("singular replay of a batched anchor")
                    .replayed,
                "the singular path recognises the batched anchor as its own"
            );
        }

        // Cost does not grow with the batch.
        let wider: Vec<SubjectHistory> = (0..8)
            .map(|ordinal| imported_batch_history(&format!("wider-subject-{ordinal}")))
            .collect();
        let before = batch_store.calls().captures;
        assert_eq!(
            batch_operation
                .import_anchors(wider)
                .await
                .expect("wider batch")
                .len(),
            8
        );
        assert_eq!(
            batch_store.calls().captures - before,
            2,
            "twice the subjects still costs two captures"
        );
    });
}

#[cfg(feature = "file")]
#[test]
fn a_refused_member_leaves_no_part_of_the_batch_committed() {
    block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        let (erased, authority, tenant) =
            provisioned_file_destination(directory.path(), "atomicity").await;
        let store = EventlogRecordedStore::open(erased.clone(), authority.clone(), LIMITS)
            .await
            .expect("bound store");
        let operation = store.operation(context("atomicity-run"));

        operation
            .import_anchor(imported_batch_history("taken"))
            .await
            .expect("first subject is imported alone");

        // A member the pre-capture already refuses: the same subject under different anchor bytes.
        let refused = operation
            .import_anchors(vec![
                imported_batch_history("with-conflict-a"),
                imported_batch_history_with("taken", "taken-create", "records/1"),
                imported_batch_history("with-conflict-b"),
            ])
            .await
            .expect_err("a conflicting member refuses the batch");
        assert!(
            matches!(
                refused,
                ImportAnchorFailure::NotCommitted(AsyncStoreError::RevisionConflict { .. })
            ),
            "{refused:?}"
        );
        for absent in ["with-conflict-a", "with-conflict-b"] {
            assert!(
                store
                    .load(&Subject::new("ticket", absent).expect("subject"))
                    .await
                    .expect("state read")
                    .is_none(),
                "{absent} must not survive a refused batch"
            );
        }

        // A member the destination guard refuses: a record identity another subject already holds.
        // The batch's blobs travel inside the group's own transaction, so a guard that refuses
        // must leave no byte of the batch behind — not even a bound orphan blob for the member
        // the guard never objected to. Admission runs before any blob is bound; this is the
        // assertion that holds it there.
        let bound_before = bound_digests(&erased, &tenant).await;
        let refused = operation
            .import_anchors(vec![
                imported_batch_history("with-guard-a"),
                imported_batch_history_with("stolen", "taken-create", "records/0"),
            ])
            .await
            .expect_err("the destination guard refuses the batch");
        assert!(
            matches!(
                refused,
                ImportAnchorFailure::NotCommitted(AsyncStoreError::RecordConflict { .. })
            ),
            "{refused:?}"
        );
        assert!(
            store
                .load(&Subject::new("ticket", "with-guard-a").expect("subject"))
                .await
                .expect("state read")
                .is_none(),
            "a guard refusal rolls the whole group back"
        );
        assert_eq!(
            bound_digests(&erased, &tenant).await,
            bound_before,
            "a refused batch binds no blob"
        );

        // The only subject the destination holds is the one imported alone.
        assert_eq!(
            store
                .complete_snapshot(&authority.logical_scope)
                .await
                .expect("snapshot")
                .histories
                .len(),
            1
        );
    });
}

#[cfg(feature = "file")]
#[test]
fn a_batch_naming_one_subject_twice_with_different_anchors_is_refused_before_any_write() {
    block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        let (erased, authority, _) =
            provisioned_file_destination(directory.path(), "duplicate").await;
        let store = EventlogRecordedStore::open(erased, authority.clone(), LIMITS)
            .await
            .expect("bound store");
        let operation = store.operation(context("duplicate-run"));

        // Two mentions of one subject carrying the same bytes settle as an exact repeat, which is
        // what N singular calls do — `import_batch_record_identity.rs` holds that. Two mentions
        // carrying *different* bytes are two different boundaries claiming one subject. No
        // destination can hold both, so the adapter refuses without asking one.
        let before = store.calls().captures;
        let refused = operation
            .import_anchors(vec![
                imported_batch_history("twice"),
                imported_batch_history("other"),
                imported_batch_history_with("twice", "twice-create", "records/1"),
            ])
            .await
            .expect_err("one subject cannot carry two different anchors in one group");
        assert!(
            matches!(
                refused,
                ImportAnchorFailure::NotCommitted(AsyncStoreError::InvalidInput(_))
            ),
            "{refused:?}"
        );
        assert_eq!(
            store.calls().captures,
            before,
            "an input the adapter can refuse on its own does no provider work"
        );
        assert!(
            store
                .complete_snapshot(&authority.logical_scope)
                .await
                .expect("snapshot")
                .histories
                .is_empty()
        );
    });
}

#[cfg(feature = "file")]
#[test]
fn an_empty_batch_reaches_no_provider() {
    block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        let (erased, authority, _) = provisioned_file_destination(directory.path(), "empty").await;
        let store = EventlogRecordedStore::open(erased, authority, LIMITS)
            .await
            .expect("bound store");
        let operation = store.operation(context("empty-run"));
        let before = store.calls().captures;
        assert!(
            operation
                .import_anchors(Vec::new())
                .await
                .expect("an empty batch settles")
                .is_empty()
        );
        assert_eq!(store.calls().captures, before, "empty batches do no IO");
    });
}

/// A batch that *commits* on a provider taking the port's default blob path.
///
/// Every other `import_anchors` case either runs on the File provider, which overrides
/// `AtomicEventStore::append_group_guarded_with_blobs`, or — like
/// `import_batch_blob_binding.rs` — only exercises a batch the adapter refuses before it calls
/// the port at all. Neither notices if the default stops carrying a batch to the destination.
///
/// The default is the one thing under this call that this repository does not own. When it
/// changes, a batch import on SQLite and PostgreSQL either keeps working or stops dead, and
/// without this case it stops dead quietly: the byte-identity case is File-only and would stay
/// green. This is the alarm.
#[cfg(feature = "sqlite")]
#[test]
fn a_batch_import_commits_through_the_port_default_on_the_sqlite_provider() {
    block_on(async {
        let tenant = TenantId::new("import-batch-sqlite-commit").expect("valid tenant");
        let backend = Arc::new(
            eventlog_sqlite::SqliteEventStore::in_memory("import_batch_commit")
                .await
                .expect("SQLite memory provider"),
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
            logical_scope: "import-batch-sqlite-commit-scope".into(),
            tenant: tenant.as_str().to_owned(),
            stream_identity,
        };
        let erased: Arc<dyn EventlogBackend> = backend;
        EventlogBindingProvisioner::new(erased.clone(), LIMITS)
            .provision_binding(authority.clone(), context("sqlite-commit"))
            .await
            .expect("binding provisioned");
        let store = EventlogRecordedStore::open(erased, authority.clone(), LIMITS)
            .await
            .expect("bound store");
        let operation = store.operation(context("sqlite-commit-run"));

        let histories: Vec<SubjectHistory> = (0..3)
            .map(|ordinal| imported_batch_history(&format!("sqlite-batch-{ordinal}")))
            .collect();
        let before = store.calls().captures;
        let outcomes = operation
            .import_anchors(histories.clone())
            .await
            .expect("the batch commits through whatever blob path the provider offers");

        assert_eq!(outcomes.len(), 3);
        assert!(
            outcomes.iter().all(|outcome| !outcome.replayed),
            "three fresh boundaries are not replays: {outcomes:?}"
        );
        assert_eq!(
            store.calls().captures - before,
            2,
            "the fixed cost does not depend on which blob path the provider takes"
        );
        assert_eq!(
            store
                .complete_snapshot(&authority.logical_scope)
                .await
                .expect("snapshot")
                .histories
                .len(),
            3
        );
        // Every anchor is the one the singular path would recognise as its own.
        for history in &histories {
            assert!(
                operation
                    .import_anchor(history.clone())
                    .await
                    .expect("singular replay of a batched anchor")
                    .replayed
            );
        }
    });
}
