//! Review probes for the recorded open checkpoint design (`docs/design/recorded-open-checkpoint-v0.1.md`).
//!
//! Each test settles one claim the design or its Eventlog issue makes about the pinned SQLite
//! provider, by running it rather than reading it. The trigger probes add a trigger through a
//! second connection with the `sqlite3` command-line shell, because this crate has no SQL client of
//! its own; they are ignored by default and run with `-- --ignored`.
#![cfg(all(feature = "sqlite", feature = "sync-bridge"))]

use std::{num::NonZeroU16, path::Path, process::Command, sync::Arc};

use entity_core::Registry;
use entity_eventlog::{
    Authority, CapturePolicy, ErRecordedProjector, EventlogOperationContext, EventlogRecordedStore,
    RecordedProviderFacade, projection_specs,
    sync::{
        BridgeConfig, BridgeStartError, CallWait, EventlogRecordedStoreOwner,
        EventlogRecordedStoreProvisioner, ProvisionAuthority, ShutdownMode, ShutdownOutcome,
    },
};
use entity_executor::{BatchAction, CreateRequest, ExecuteRequest, Executor};
use entity_store::{
    Recording,
    asynchronous::{AsyncRecordedReader, AsyncRefusalRecorder, AsyncStoreError, BatchKey, Subject},
};
use eventlog_core::{
    CaptureLimits, ConsistentTenantCapture, EventLogError, EventStore, InlineProjectionAdmin,
    Snapshot, StreamId, TenantId,
};
use serde_json::json;
use time::OffsetDateTime;

const PREFIX: &str = "review_checkpoint";
const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 512,
    max_blobs: 2_048,
    max_projection_rows: 2_048,
    max_payload_bytes: 8 * 1024 * 1024,
};

fn registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "ticket",
        "version": 1,
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "open", "states": ["open"] },
        "operations": {
            "touch": { "transitions": [{ "from": "open", "to": "open" }], "emits": [] }
        }
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    registry
}

fn context(label: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "review-open-checkpoint".to_owned(),
        actor: "entity-eventlog-test".to_owned(),
        request_id: format!("request-{label}"),
        trace_id: format!("trace-{label}"),
        causation_id: None,
        causation_depth: 0,
        occurred_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn bridge() -> BridgeConfig {
    BridgeConfig {
        queue_capacity: NonZeroU16::new(8).expect("nonzero"),
    }
}

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

/// A provisioned SQLite file holding one created ticket, with every handle on it closed.
fn provisioned(path: &Path) -> Authority {
    let mut facade = RecordedProviderFacade::provision(
        registry(),
        EventlogRecordedStoreProvisioner::Sqlite {
            path: path.to_string_lossy().into_owned(),
            prefix: PREFIX.to_owned(),
            authority: ProvisionAuthority {
                logical_scope: "review-scope".to_owned(),
                tenant: "review-tenant".to_owned(),
                expected_stream_identity: None,
            },
            limits: LIMITS,
        },
        context("provision"),
        bridge(),
    )
    .expect("SQLite authority provisioned");
    let authority = facade.authority().clone();
    facade
        .create(
            context("create"),
            CreateRequest {
                subject: Subject::new("ticket", "one").expect("subject"),
                definition_version: 1,
                fields: json!({"title":"one"}),
                recording: Recording {
                    record_id: "create-one".to_owned(),
                    recorded_at: "2026-10-06T00:00:00Z".to_owned(),
                    correlation: None,
                    causation: None,
                    actor: None,
                },
            },
            CallWait::Forever,
        )
        .expect("ticket created");
    assert_eq!(
        facade.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
    authority
}

/// The provider handle an owner opens: existing file, ER projector attached.
async fn attached(path: &Path) -> Arc<eventlog_sqlite::SqliteEventStore> {
    let backend = Arc::new(
        eventlog_sqlite::SqliteEventStore::open_existing(path.to_str().expect("utf-8"), PREFIX)
            .await
            .expect("provisioned SQLite file opens"),
    );
    backend
        .attach_inline_existing(Arc::new(ErRecordedProjector::new()))
        .await
        .expect("ER projector attaches");
    backend
}

/// Runs one statement through a second SQLite connection, as the tamper scenarios do.
fn second_connection(path: &Path, sql: &str) {
    let status = Command::new("sqlite3")
        .arg(path)
        .arg(sql)
        .status()
        .expect("the sqlite3 shell runs");
    assert!(status.success(), "sqlite3 refused {sql}: {status}");
}

fn start_tracked(
    path: &Path,
    authority: Authority,
) -> Result<RecordedProviderFacade, BridgeStartError> {
    RecordedProviderFacade::start_with_read_policy(
        registry(),
        EventlogRecordedStoreOwner::Sqlite {
            path: path.to_string_lossy().into_owned(),
            prefix: PREFIX.to_owned(),
            authority,
            limits: LIMITS,
        },
        bridge(),
        CapturePolicy::ProviderTracked,
    )
}

/// Design, "Where the checkpoint is persisted" and "When it is written": a snapshot of an
/// Entity-Runtime-owned stream is not captured material, and writing it through the provider's own
/// connection ends the handle's in-process continuity ("read from the code, not run").
#[test]
fn a_checkpoint_snapshot_is_not_captured_and_writing_it_ends_in_process_continuity() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path = directory.path().join("checkpoint.sqlite3");
    let authority = provisioned(&path);
    block_on(async {
        let tenant = TenantId::new(&authority.tenant).expect("tenant");
        let backend = attached(&path).await;
        let store = EventlogRecordedStore::open_with_policy(
            backend.clone(),
            authority.clone(),
            LIMITS,
            CapturePolicy::ProviderTracked,
        )
        .await
        .expect("tracked open verifies");
        let first = store
            .complete_snapshot(&authority.logical_scope)
            .await
            .expect("first complete read");
        assert_eq!(
            store.calls().captures,
            1,
            "an unchanged provider reuses the open's verification"
        );
        let before = backend
            .capture_tenant(&tenant, projection_specs(), LIMITS)
            .await
            .expect("capture before the snapshot");

        let stream = StreamId::new(tenant.clone(), "er.open-checkpoint", "review-authority")
            .expect("stream");
        let generation = backend
            .snapshot_generation(&stream)
            .await
            .expect("generation read")
            .expect("SQLite issues snapshot generations");
        let saved = backend
            .save_snapshot_checked(
                &stream,
                &Snapshot {
                    version: 0,
                    state_schema_version: 1,
                    state: json!({"format":"er.eventlog.open-checkpoint/1"}),
                    recorded_at: OffsetDateTime::UNIX_EPOCH,
                },
                &generation,
            )
            .await
            .expect("snapshot write");
        assert!(saved, "a fresh generation admits the snapshot");

        let after = backend
            .capture_tenant(&tenant, projection_specs(), LIMITS)
            .await
            .expect("capture after the snapshot");
        assert_eq!(after, before, "a snapshot never enters the tenant capture");

        let second = store
            .complete_snapshot(&authority.logical_scope)
            .await
            .expect("second complete read");
        assert_eq!(second, first, "the verified history is unchanged");
        assert_eq!(
            store.calls().captures,
            2,
            "the provider's own snapshot write ended in-process continuity, so the read recaptured"
        );

        let reopened = EventlogRecordedStore::open_with_policy(
            attached(&path).await,
            authority.clone(),
            LIMITS,
            CapturePolicy::ProviderTracked,
        )
        .await
        .expect("a store holding a checkpoint snapshot still opens");
        assert_eq!(
            reopened
                .complete_snapshot(&authority.logical_scope)
                .await
                .expect("reopened read"),
            first
        );
    });
}

/// Eventlog issue, "SQLite behaviour": the epoch is kept by provider-owned triggers on captured
/// tables, and only `capture.rs:144-158` and `:639-647` are named as checks to change. The pinned
/// provider's open refuses any trigger on the blob table (`lib.rs:953-963`, reached from
/// `open_existing` through `require_existing_schema`), so Entity Runtime 0.26.0 cannot open a store
/// once a newer provider has installed one.
#[test]
#[ignore = "review probe: needs the sqlite3 shell"]
fn a_trigger_on_the_blob_table_makes_the_pinned_provider_refuse_to_open_the_store() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path = directory.path().join("blob-trigger.sqlite3");
    let authority = provisioned(&path);
    second_connection(
        &path,
        &format!("CREATE TRIGGER review_epoch AFTER UPDATE ON {PREFIX}_blobs BEGIN SELECT 1; END;"),
    );
    let refused = block_on(eventlog_sqlite::SqliteEventStore::open_existing(
        path.to_str().expect("utf-8"),
        PREFIX,
    ));
    match refused {
        Err(EventLogError::Invalid(message)) => {
            assert_eq!(message, "unsupported SQLite blob trigger or table behavior");
        }
        Err(other) => panic!("open refused for another reason: {other}"),
        Ok(_) => panic!("the pinned provider opened a store with a trigger on its blob table"),
    }
    match start_tracked(&path, authority) {
        Err(BridgeStartError::Open(AsyncStoreError::Backend(message))) => {
            assert_eq!(message, "unsupported SQLite blob trigger or table behavior");
        }
        Err(_) => panic!("the facade refused for another reason"),
        Ok(_) => panic!("Entity Runtime opened a store with a trigger on its blob table"),
    }
}

/// The same for a projection table: attaching the ER projector runs `admit_projection`
/// (`inline_admin.rs:106`, the trigger check at `capture.rs:639-647`), so the open refuses.
#[test]
#[ignore = "review probe: needs the sqlite3 shell"]
fn a_trigger_on_a_projection_table_makes_the_pinned_provider_refuse_to_attach_the_projector() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path = directory.path().join("projection-trigger.sqlite3");
    let authority = provisioned(&path);
    second_connection(
        &path,
        &format!(
            "CREATE TRIGGER review_epoch AFTER UPDATE ON {PREFIX}_p_er_subjects_v1 \
             BEGIN SELECT 1; END;"
        ),
    );
    match start_tracked(&path, authority) {
        Err(BridgeStartError::Open(AsyncStoreError::Backend(message))) => {
            assert_eq!(
                message,
                "projection admission does not match durable structure"
            );
        }
        Err(_) => panic!("the facade refused for another reason"),
        Ok(_) => panic!("Entity Runtime attached to a projection table carrying a trigger"),
    }
}

/// Eventlog issue: "keep refusing every other trigger and foreign key as today
/// (`capture.rs:144-158` for continuity ...)". Today a trigger on the event table refuses nothing:
/// the capture succeeds and the provider issues no checkpoint, so every tracked read recaptures.
#[test]
#[ignore = "review probe: needs the sqlite3 shell"]
fn a_trigger_on_the_event_table_withholds_continuity_rather_than_refusing_the_capture() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path = directory.path().join("event-trigger.sqlite3");
    let authority = provisioned(&path);
    second_connection(
        &path,
        &format!(
            "CREATE TRIGGER review_epoch AFTER UPDATE ON {PREFIX}_events BEGIN SELECT 1; END;"
        ),
    );
    block_on(async {
        let store = EventlogRecordedStore::open_with_policy(
            attached(&path).await,
            authority.clone(),
            LIMITS,
            CapturePolicy::ProviderTracked,
        )
        .await
        .expect("a foreign trigger on the event table does not refuse the open");
        store
            .complete_snapshot(&authority.logical_scope)
            .await
            .expect("complete read");
        let calls = store.calls();
        assert_eq!(
            (calls.captures, calls.model_builds),
            (2, 2),
            "with a trigger present the provider issues no checkpoint, so the read rebuilt: {calls:?}"
        );
    });
}

fn touch(expected_revision: u64, record_id: &str) -> ExecuteRequest {
    ExecuteRequest {
        subject: Subject::new("ticket", "one").expect("subject"),
        expected_revision,
        operation: "touch".to_owned(),
        arguments: json!({}),
        fulfillments: Default::default(),
        recording: Recording {
            record_id: record_id.to_owned(),
            recorded_at: "2026-10-06T00:00:01Z".to_owned(),
            correlation: None,
            causation: None,
            actor: None,
        },
    }
}

/// Design, *Cost*: recording a refusal binds its blob with a standalone `put_blob` and appends its
/// event outside an atomic group, so it ends in-process continuity and the next read verifies
/// completely, where the handle's own committed write is advanced as a delta.
#[test]
fn a_recorded_refusal_ends_tracked_continuity_where_an_own_write_is_advanced() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path = directory.path().join("refusal.sqlite3");
    let authority = provisioned(&path);
    let registry = registry();
    block_on(async {
        let store = EventlogRecordedStore::open_with_policy(
            attached(&path).await,
            authority.clone(),
            LIMITS,
            CapturePolicy::ProviderTracked,
        )
        .await
        .expect("tracked open verifies");
        let operation = store.operation(context("writes"));

        Executor::recording_refusals(&registry, &operation, &operation)
            .batch(
                BatchKey::Named("committed-touch".into()),
                vec![BatchAction::Execute(touch(1, "touch-one"))],
            )
            .await
            .expect("a touch at the held revision commits");
        let before = store.calls();
        store
            .complete_snapshot(&authority.logical_scope)
            .await
            .expect("read after the committed write");
        let after = store.calls();
        assert_eq!(
            (after.captures, after.model_builds),
            (before.captures, before.model_builds),
            "the handle's own atomic write is continued, not recaptured: {before:?} -> {after:?}"
        );

        let refused = Executor::recording_refusals(&registry, &operation, &operation)
            .batch(
                BatchKey::Named("stale-touch".into()),
                vec![BatchAction::Execute(touch(7, "touch-stale"))],
            )
            .await;
        assert!(
            matches!(refused, Err(ref error) if error.is_revision_conflict()),
            "a stale expectation is refused as a revision conflict: {refused:?}"
        );
        let before = store.calls();
        store
            .complete_snapshot(&authority.logical_scope)
            .await
            .expect("read after the recorded refusal");
        let after = store.calls();
        assert_eq!(
            (after.captures, after.model_builds),
            (before.captures + 1, before.model_builds + 1),
            "recording the refusal ended continuity, so the read verified completely: {before:?} -> {after:?}"
        );
        assert_eq!(
            operation.refusals().await.expect("refusals read").len(),
            1,
            "the refusal was recorded in the store"
        );
    });
}
