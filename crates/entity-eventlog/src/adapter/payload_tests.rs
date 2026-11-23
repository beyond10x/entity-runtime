//! Review A2: a write is held to the payload-byte cap its own reader reads the store back with.
//!
//! `CaptureLimits::max_payload_bytes` bounds every capture: the compact length of every event
//! body and projection row body plus every blob's bytes (Eventlog's `CaptureBudget`). A write
//! admitted past it commits a store the handle that wrote it cannot read back, so its receipt is
//! lost and every later open at the same limits is refused. Each test commits one subject, sets
//! the cap a few bytes above what the store then holds, and asks for one more write: it is refused
//! as `BatchExceedsReadBounds { bound: "max_payload_bytes" }`, nothing reaches the store, and an
//! open at the same limits still reads it. The same write is then committed under a wide cap to
//! show that the refusal's `would_hold` is an upper bound on what it would have held.
//!
//! In this module rather than in `tests/`: the recount case reaches into the handle's held
//! counts, and the crate's test binaries are not to grow by one.

use std::{path::Path, sync::Arc};

use crate::{
    AsyncBindingProvisioner, AsyncImportedAnchorWriter, Authority, CapturePolicy,
    ErRecordedProjector, EventlogBindingProvisioner, EventlogOperationContext,
    EventlogRecordedStore, ImportAnchorFailure, OpenVerification, projection_specs,
};
use entity_core::{Registry, Runtime};
use entity_executor::{BatchAction, CreateRequest, ExecutionError, Executor};
use entity_store::{
    RecordedCommit, Recording,
    asynchronous::{
        AsyncStateReader, AsyncStoreError, BatchKey, HistoryOrigin, ImportedRecordEvidence,
        KnownLegacyOrder, LegacyAnchor, LegacyCompleteness, LegacyEvidence, LegacyOrderDeclaration,
        RecordedEntry, Subject, SubjectHistory, WriteFailure,
    },
};
use eventlog_core::{
    CaptureLimits, ConsistentTenantCapture, EventStore, InlineProjectionAdmin, TenantId,
};
use serde_json::{Value, json};
use time::OffsetDateTime;

const PREFIX: &str = "payload_budget";
const TENANT: &str = "payload-tenant";
const WIDE: CaptureLimits = CaptureLimits {
    max_events: 512,
    max_blobs: 4_096,
    max_projection_rows: 4_096,
    max_payload_bytes: 16 * 1024 * 1024,
};
/// How far above the store's payload the tight cap sits: far below what one more subject adds.
const SLACK: u64 = 64;

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
        subject: "payload-budget".to_owned(),
        actor: "entity-eventlog-test".to_owned(),
        request_id: format!("request-{label}"),
        trace_id: format!("trace-{label}"),
        causation_id: None,
        causation_depth: 0,
        occurred_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn recording(id: &str) -> Recording {
    Recording {
        record_id: id.to_owned(),
        recorded_at: "2026-11-23T00:00:00Z".to_owned(),
        correlation: None,
        causation: None,
        actor: None,
    }
}

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

fn ticket(id: &str) -> Subject {
    Subject::new("ticket", id).expect("subject")
}

fn create(id: &str) -> BatchAction {
    BatchAction::Create(CreateRequest {
        subject: ticket(id),
        definition_version: 1,
        fields: json!({ "title": id }),
        recording: recording(&format!("create-{id}")),
    })
}

fn imported(id: &str) -> SubjectHistory {
    let registry = registry();
    let decision = Runtime::new(&registry)
        .create("ticket", 1, id, json!({ "title": id }))
        .expect("legacy decision");
    let commit =
        RecordedCommit::new(decision, &recording(&format!("legacy-{id}"))).expect("legacy record");
    SubjectHistory {
        subject: ticket(id),
        origin: HistoryOrigin::Imported(LegacyAnchor {
            instance: commit.instance.clone(),
            completeness: LegacyCompleteness::AvailableEvidenceOnly,
            order: LegacyOrderDeclaration::PerKindOnly,
            evidence: vec![LegacyEvidence::Envelope(
                ImportedRecordEvidence::new(
                    RecordedEntry::Decision(commit),
                    "payload-source".to_owned(),
                    format!("records/{id}"),
                    KnownLegacyOrder::PerKind(0),
                )
                .expect("imported evidence"),
            )],
        }),
        records: Vec::new(),
    }
}

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

/// A directory for one store, inside the build directory.
fn directory() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    std::fs::create_dir_all(&root).expect("build directory");
    tempfile::tempdir_in(root).expect("directory")
}

/// A provisioned SQLite file holding only its binding.
async fn provisioned(path: &Path) -> Authority {
    let backend = Arc::new(
        eventlog_sqlite::SqliteEventStore::open(path.to_str().expect("utf-8"), PREFIX)
            .await
            .expect("store"),
    );
    let tenant = TenantId::new(TENANT).expect("tenant");
    let authority = Authority {
        logical_scope: "payload-scope".to_owned(),
        tenant: TENANT.to_owned(),
        stream_identity: backend.stream_identity(&tenant).await.expect("identity"),
    };
    let projector = Arc::new(ErRecordedProjector::new());
    backend
        .create_projections(projector.clone())
        .await
        .expect("projections");
    backend
        .attach_inline_existing(projector)
        .await
        .expect("attach");
    EventlogBindingProvisioner::new(backend, WIDE)
        .provision_binding(authority.clone(), context("binding"))
        .await
        .expect("binding");
    authority
}

async fn open(
    path: &Path,
    authority: &Authority,
    limits: CaptureLimits,
    policy: CapturePolicy,
) -> Result<EventlogRecordedStore, AsyncStoreError> {
    EventlogRecordedStore::open_with_policy(attached(path).await, authority.clone(), limits, policy)
        .await
}

async fn write(
    store: &EventlogRecordedStore,
    key: &str,
    actions: Vec<BatchAction>,
) -> Result<(), ExecutionError> {
    let registry = registry();
    let operation = store.operation(context(key));
    Executor::new(&registry, &operation)
        .batch(BatchKey::Named(key.to_owned()), actions)
        .await
        .map(|_| ())
}

/// What the store holds, read with wide limits, and its payload as Eventlog's `CaptureBudget`
/// charges it: compact event bodies, blob bytes and compact row bodies.
async fn held(path: &Path) -> (usize, usize, usize, u64) {
    let capture = attached(path)
        .await
        .capture_tenant(
            &TenantId::new(TENANT).expect("tenant"),
            projection_specs(),
            WIDE,
        )
        .await
        .expect("observer capture");
    let compact = |value: &Value| serde_json::to_vec(value).expect("encodes").len() as u64;
    let rows: Vec<&Value> = capture
        .projections
        .iter()
        .flat_map(|projection| projection.rows.iter().map(|(_, body)| body))
        .collect();
    let payload = capture
        .events
        .iter()
        .map(|event| compact(&event.data))
        .sum::<u64>()
        + capture
            .blobs
            .iter()
            .map(|blob| blob.bytes.len() as u64)
            .sum::<u64>()
        + rows.iter().map(|body| compact(body)).sum::<u64>();
    (
        capture.events.len(),
        capture.blobs.len(),
        rows.len(),
        payload,
    )
}

fn tight(payload: u64) -> CaptureLimits {
    CaptureLimits {
        max_payload_bytes: payload + SLACK,
        ..WIDE
    }
}

fn payload_refusal(error: &AsyncStoreError) -> Option<(u64, u64)> {
    match error {
        AsyncStoreError::BatchExceedsReadBounds {
            bound,
            limit,
            would_hold,
        } if bound == "max_payload_bytes" => Some((*limit, *would_hold)),
        _ => None,
    }
}

#[test]
fn a_write_past_the_payload_cap_is_refused_leaving_nothing_and_the_store_readable() {
    for policy in [
        CapturePolicy::FullVerification,
        CapturePolicy::ProviderTracked,
    ] {
        let directory = directory();
        let path = directory.path().join("store.sqlite3");
        block_on(async {
            let authority = provisioned(&path).await;
            let wide = open(&path, &authority, WIDE, policy)
                .await
                .expect("wide open");
            write(&wide, "seed", vec![create("a")])
                .await
                .expect("the seed commits");
            drop(wide);
            let before = held(&path).await;
            let limits = tight(before.3);

            let writer = open(&path, &authority, limits, policy)
                .await
                .expect("the store fits its cap");
            let outcome = write(&writer, "over", vec![create("b")]).await;
            let refused = match &outcome {
                Err(ExecutionError::Write(WriteFailure::NotCommitted(error))) => {
                    payload_refusal(error)
                }
                _ => None,
            };
            let Some((limit, would_hold)) = refused else {
                panic!(
                    "{policy:?}: a write past max_payload_bytes={} was not refused as \
                     BatchExceedsReadBounds {{ bound: \"max_payload_bytes\" }}: {outcome:?}; the \
                     store now holds {:?}, it held {before:?}",
                    limits.max_payload_bytes,
                    held(&path).await
                );
            };
            assert_eq!(limit, limits.max_payload_bytes);
            assert_eq!(
                held(&path).await,
                before,
                "{policy:?}: the refusal left nothing"
            );
            drop(writer);

            let reader = open(&path, &authority, limits, policy)
                .await
                .expect("an open at the same limits still reads the store");
            assert!(reader.load(&ticket("a")).await.expect("load").is_some());
            assert!(reader.load(&ticket("b")).await.expect("load").is_none());
            drop(reader);

            let wide = open(&path, &authority, WIDE, policy)
                .await
                .expect("wide open");
            write(&wide, "over", vec![create("b")])
                .await
                .expect("the same write commits under a wide cap");
            let after = held(&path).await;
            eprintln!(
                "{policy:?}: write of one subject: held {} -> {} payload bytes, bound {would_hold}",
                before.3, after.3
            );
            assert!(
                would_hold >= after.3,
                "{policy:?}: the write was admitted at {would_hold} bytes and holds {}",
                after.3
            );
        });
    }
}

#[test]
fn an_import_batch_past_the_payload_cap_is_refused_leaving_nothing_and_the_store_readable() {
    let directory = directory();
    let path = directory.path().join("store.sqlite3");
    block_on(async {
        let authority = provisioned(&path).await;
        let wide = open(&path, &authority, WIDE, CapturePolicy::FullVerification)
            .await
            .expect("wide open");
        wide.operation(context("seed"))
            .import_anchors(vec![imported("a")])
            .await
            .expect("the seed imports");
        drop(wide);
        let before = held(&path).await;
        let limits = tight(before.3);
        let batch = || vec![imported("b"), imported("c")];

        let importer = open(&path, &authority, limits, CapturePolicy::FullVerification)
            .await
            .expect("the store fits its cap");
        let outcome = importer
            .operation(context("over"))
            .import_anchors(batch())
            .await;
        let refused = match &outcome {
            Err(ImportAnchorFailure::NotCommitted(error)) => payload_refusal(error),
            _ => None,
        };
        let Some((limit, would_hold)) = refused else {
            panic!(
                "an import batch past max_payload_bytes={} was not refused as \
                 BatchExceedsReadBounds {{ bound: \"max_payload_bytes\" }}: {outcome:?}; the store \
                 now holds {:?}, it held {before:?}",
                limits.max_payload_bytes,
                held(&path).await
            );
        };
        assert_eq!(limit, limits.max_payload_bytes);
        assert_eq!(held(&path).await, before, "the refusal left nothing");
        drop(importer);

        let reader = open(&path, &authority, limits, CapturePolicy::FullVerification)
            .await
            .expect("an open at the same limits still reads the store");
        assert!(reader.load(&ticket("a")).await.expect("load").is_some());
        assert!(reader.load(&ticket("b")).await.expect("load").is_none());
        drop(reader);

        let wide = open(&path, &authority, WIDE, CapturePolicy::FullVerification)
            .await
            .expect("wide open");
        wide.operation(context("over"))
            .import_anchors(batch())
            .await
            .expect("the same batch imports under a wide cap");
        let after = held(&path).await;
        eprintln!(
            "import of two subjects: held {} -> {} payload bytes, bound {would_hold}",
            before.3, after.3
        );
        assert!(
            would_hold >= after.3,
            "the batch was admitted at {would_hold} bytes and holds {}",
            after.3
        );
    });
}

/// The payload a handle holds is the provider's own count: a complete capture counts it to the
/// byte the store's `CaptureBudget` does, and a handle opened from a checkpoint or advanced by a
/// verified suffix holds the provider's usage, which agrees.
#[test]
fn held_payload_is_the_providers_count_from_a_capture_and_from_a_checkpoint() {
    let directory = directory();
    let path = directory.path().join("store.sqlite3");
    block_on(async {
        let authority = provisioned(&path).await;
        attached(&path)
            .await
            .enable_durable_continuity()
            .await
            .expect("durable continuity");
        let first = open(&path, &authority, WIDE, CapturePolicy::ProviderTracked)
            .await
            .expect("first open");
        write(&first, "seed", vec![create("a"), create("b")])
            .await
            .expect("the seed commits");
        assert!(first.write_open_checkpoint().await.expect("checkpoint"));
        // A tracked handle's own write is verified as a suffix, whose usage replaces its bound.
        let tracked_after_write = payload(&first);
        drop(first);
        let counted = held(&path).await.3;
        let whole = open(&path, &authority, WIDE, CapturePolicy::FullVerification)
            .await
            .expect("whole open");
        let bounded = open(&path, &authority, WIDE, CapturePolicy::ProviderTracked)
            .await
            .expect("bounded open");
        assert_eq!(bounded.open_verification(), OpenVerification::Checkpoint);
        // A complete capture is counted when a write first needs it, not by the open.
        assert_eq!(
            payload(&whole),
            (None, 0),
            "complete capture, before a write"
        );
        whole.count_payload().await.expect("counted");
        assert_eq!(payload(&whole), (Some(counted), 0), "complete capture");
        assert_eq!(payload(&bounded), (Some(counted), 0), "checkpoint usage");
        assert_eq!(tracked_after_write, (Some(counted), 0), "verified suffix");
    });
}

fn payload(store: &EventlogRecordedStore) -> (Option<u64>, u64) {
    let held = store.held.lock().expect("held");
    (held.payload, held.payload_added)
}

/// A payload charge the handle only estimated does not refuse a write the exact charge admits:
/// the write is checked again against a fresh count.
#[test]
fn an_estimated_payload_is_counted_exactly_before_it_refuses_a_write() {
    let directory = directory();
    let path = directory.path().join("store.sqlite3");
    block_on(async {
        let authority = provisioned(&path).await;
        let store = open(&path, &authority, WIDE, CapturePolicy::FullVerification)
            .await
            .expect("open");
        write(&store, "seed", vec![create("a")])
            .await
            .expect("the seed commits");
        let exact = held(&path).await.3;
        {
            // The handle's own write left the bound it was admitted with, not the exact count;
            // here that estimate is grown to the cap, as a run of such writes would grow it.
            let mut held = store.held.lock().expect("held");
            let total = held.payload.unwrap_or_default() + held.payload_added;
            assert!(
                held.payload_added > 0 && total >= exact,
                "after its own write the handle holds {:?} + {} bytes; exact: {exact}",
                held.payload,
                held.payload_added
            );
            eprintln!("after its own write: held {total} bytes estimated, {exact} exact");
            held.payload_added = WIDE.max_payload_bytes;
        }
        let outcome = write(&store, "fits", vec![create("b")]).await;
        assert!(
            outcome.is_ok(),
            "a write that fits the exact count of {exact} bytes was refused on an estimate: \
             {outcome:?}"
        );
        assert!(store.load(&ticket("b")).await.expect("load").is_some());
    });
}
