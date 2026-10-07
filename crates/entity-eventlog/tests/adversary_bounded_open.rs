//! Adversary pass on the bounded `ProviderTracked` open (GitHub #55,
//! `docs/design/recorded-open-checkpoint-v0.1.md`).
//!
//! Each case drives a claim of the design against the code: what a handle opened from a persisted
//! checkpoint answers when a foreign SQL write lands between the provider's continuity answer and
//! the row read, what its row-backed reads do with a raw edit of an index row, and whether a
//! checkpoint taken before a disable and re-enable, or before a foreign write, is ever continued.
//!
//! The foreign SQL writes go through a second connection opened by the `sqlite3` command-line
//! shell, as `review_open_checkpoint.rs` does: this crate has no SQL client of its own.
#![cfg(all(feature = "sqlite", feature = "sync-bridge"))]

use std::{
    num::NonZeroU16,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};

use entity_core::Registry;
use entity_eventlog::{
    Authority, CapturePolicy, ErRecordedProjector, EventlogBackend, EventlogOperationContext,
    EventlogRecordedStore, OpenVerification, RecordedProviderFacade,
    sync::{
        BridgeConfig, CallWait, EventlogRecordedStoreProvisioner, ProvisionAuthority, ShutdownMode,
        ShutdownOutcome,
    },
};
use entity_executor::{BatchAction, CreateRequest, ExecuteRequest, Executor};
use entity_store::{
    Recording,
    asynchronous::{
        AsyncRecordedReader, AsyncStateReader, AsyncStoreError, BatchKey, RecordLookup, Subject,
    },
};
use eventlog_core::{
    AppendGroup, AppendGroupResult, AppendResult, AtomicEventStore, BoxFuture, Capabilities,
    CaptureCheckpoint, CaptureError, CaptureLimits, CaptureUsage, CatchUpProgress, Claim,
    ClaimedCommand, CommandMeta, ConsistentTenantCapture, DurableCaptureCheckpoint, EventLogError,
    EventStore, Expected, FeedPage, Guard, InlineProjectionAdmin, InlineRebuildResult, NewEvent,
    ProjectionPage, ProjectionSpec, Projector, Read, ReadResult, RecordedEvent, Snapshot,
    SnapshotGeneration, StreamId, StreamSlice, TenantCapture, TenantCaptureUpdate, TenantId,
};
use serde_json::{Value, json};
use time::OffsetDateTime;

const PREFIX: &str = "adv_bounded";
const SUBJECT_TABLE: &str = "adv_bounded_p_er_subjects_v1";
const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 512,
    max_blobs: 4_096,
    max_projection_rows: 4_096,
    max_payload_bytes: 16 * 1024 * 1024,
};

fn registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "ticket",
        "version": 1,
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "open", "states": ["open", "closed"] },
        "operations": {
            "touch": { "transitions": [{ "from": "open", "to": "open" }], "emits": [] },
            "close": { "transitions": [{ "from": "open", "to": "closed" }], "emits": [] }
        }
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    registry
}

fn context(label: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "adversary-bounded-open".to_owned(),
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
        recorded_at: "2026-10-07T00:00:00Z".to_owned(),
        correlation: None,
        causation: None,
        actor: None,
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

fn ticket(id: &str) -> Subject {
    Subject::new("ticket", id).expect("subject")
}

fn create(id: &str, title: &str) -> BatchAction {
    BatchAction::Create(CreateRequest {
        subject: ticket(id),
        definition_version: 1,
        fields: json!({ "title": title }),
        recording: recording(&format!("create-{id}")),
    })
}

fn execute(id: &str, revision: u64, operation: &str, record: &str) -> BatchAction {
    BatchAction::Execute(ExecuteRequest {
        subject: ticket(id),
        expected_revision: revision,
        operation: operation.to_owned(),
        arguments: json!({}),
        fulfillments: Default::default(),
        recording: recording(record),
    })
}

/// A provisioned SQLite file with every handle on it closed.
fn provisioned(path: &Path) -> Authority {
    let mut facade = RecordedProviderFacade::provision(
        registry(),
        EventlogRecordedStoreProvisioner::Sqlite {
            path: path.to_string_lossy().into_owned(),
            prefix: PREFIX.to_owned(),
            authority: ProvisionAuthority {
                logical_scope: "adversary-scope".to_owned(),
                tenant: "adversary-tenant".to_owned(),
                expected_stream_identity: None,
            },
            limits: LIMITS,
        },
        context("provision"),
        bridge(),
    )
    .expect("SQLite authority provisioned");
    let authority = facade.authority().clone();
    assert_eq!(
        facade.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
    authority
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

async fn open(
    path: &Path,
    authority: &Authority,
    policy: CapturePolicy,
) -> Result<EventlogRecordedStore, AsyncStoreError> {
    EventlogRecordedStore::open_with_policy(attached(path).await, authority.clone(), LIMITS, policy)
        .await
}

async fn durable_continuity(path: &Path, enable: bool) {
    let store =
        eventlog_sqlite::SqliteEventStore::open_existing(path.to_str().expect("utf-8"), PREFIX)
            .await
            .expect("store opens");
    if enable {
        store
            .enable_durable_continuity()
            .await
            .expect("durable continuity installs");
    } else {
        store
            .disable_durable_continuity()
            .await
            .expect("durable continuity is removed");
    }
}

async fn run(store: &EventlogRecordedStore, key: &str, actions: Vec<BatchAction>) {
    let registry = registry();
    let operation = store.operation(context(key));
    Executor::new(&registry, &operation)
        .batch(BatchKey::Named(key.to_owned()), actions)
        .await
        .map(|_| ())
        .expect("batch commits");
}

async fn write(path: &Path, authority: &Authority, key: &str, actions: Vec<BatchAction>) {
    let store = open(path, authority, CapturePolicy::FullVerification)
        .await
        .expect("full open");
    run(&store, key, actions).await;
}

/// Opens tracked, persists the observation it verified, and closes.
async fn checkpoint(path: &Path, authority: &Authority) {
    let store = open(path, authority, CapturePolicy::ProviderTracked)
        .await
        .expect("tracked open");
    assert!(
        store
            .write_open_checkpoint()
            .await
            .expect("checkpoint write"),
        "a checkpoint was persisted"
    );
}

/// Runs SQL through a second SQLite connection and returns what it printed.
fn second_connection(path: &Path, sql: &str) -> String {
    let output = Command::new("sqlite3")
        .arg("-cmd")
        .arg(".timeout 5000")
        .arg(path)
        .arg(sql)
        .output()
        .expect("this case needs the sqlite3 shell on PATH");
    assert!(
        output.status.success(),
        "sqlite3 refused {sql}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf-8")
}

/// The body of the store's only subject row, as stored.
fn only_subject_row(path: &Path) -> String {
    let rows = second_connection(path, &format!("SELECT body FROM {SUBJECT_TABLE}"));
    let rows: Vec<&str> = rows.lines().collect();
    assert_eq!(rows.len(), 1, "one subject row: {rows:?}");
    rows[0].to_owned()
}

/// Replaces the store's only subject row with `body` through a second connection.
fn replace_subject_row(path: &Path, body: &str) {
    second_connection(
        path,
        &format!(
            "UPDATE {SUBJECT_TABLE} SET body = '{}'",
            body.replace('\'', "''")
        ),
    );
}

/// A store holding ticket `a` at revision 2, and the subject row it had at revision 1: a genuine,
/// well-formed row whose state source is a bound, digest-correct record blob.
async fn two_revisions(path: &Path) -> (Authority, String) {
    let authority = provisioned(path);
    durable_continuity(path, true).await;
    write(path, &authority, "first", vec![create("a", "alpha")]).await;
    let stale = only_subject_row(path);
    assert!(stale.contains("\"revision\":1"), "{stale}");
    write(
        path,
        &authority,
        "second",
        vec![execute("a", 1, "touch", "touch-a")],
    )
    .await;
    (authority, stale)
}

/// Passes every call to a real provider; runs a one-shot hook just before a subject index row is
/// read, standing in for a foreign connection that writes between the provider's continuity
/// answer and the read that answer is supposed to cover.
struct Interposing<B> {
    inner: Arc<B>,
    /// The index the hook waits for, and the hook.
    hook: Mutex<Option<RowHook>>,
    /// Runs once just before the snapshot generation is read: another process's write to the
    /// open checkpoint landing inside a drain's check-then-write.
    snapshot_hook: Mutex<Option<SnapshotHook>>,
}

/// The index a row hook waits for, and the hook.
type RowHook = (&'static str, Box<dyn FnOnce() + Send>);

type SnapshotHook =
    Box<dyn FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Send>;

impl<B> Interposing<B> {
    fn new(inner: Arc<B>) -> Self {
        Self {
            inner,
            hook: Mutex::new(None),
            snapshot_hook: Mutex::new(None),
        }
    }

    fn arm(&self, hook: impl FnOnce() + Send + 'static) {
        self.arm_on("er_subjects_v1", hook);
    }

    /// Runs `hook` once, just before the next read of a row of the index `projection`.
    fn arm_on(&self, projection: &'static str, hook: impl FnOnce() + Send + 'static) {
        *self.hook.lock().expect("hook") = Some((projection, Box::new(hook)));
    }

    fn arm_snapshot(&self, hook: SnapshotHook) {
        *self.snapshot_hook.lock().expect("hook") = Some(hook);
    }
}

impl<B: EventlogBackend> EventStore for Interposing<B> {
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }

    fn append<'a>(
        &'a self,
        stream: &'a StreamId,
        expected: Expected,
        events: &'a [NewEvent],
        meta: &'a CommandMeta,
    ) -> BoxFuture<'a, Result<AppendResult, EventLogError>> {
        self.inner.append(stream, expected, events, meta)
    }

    fn recorded_claim<'a>(
        &'a self,
        tenant: &'a TenantId,
        claim: &'a Claim,
    ) -> BoxFuture<'a, Result<Option<ClaimedCommand>, EventLogError>> {
        self.inner.recorded_claim(tenant, claim)
    }

    fn recorded_command<'a>(
        &'a self,
        stream: &'a StreamId,
        key: &'a str,
        hash: &'a str,
    ) -> BoxFuture<'a, Result<Option<AppendResult>, EventLogError>> {
        self.inner.recorded_command(stream, key, hash)
    }

    fn read_stream<'a>(
        &'a self,
        stream: &'a StreamId,
        after: u64,
        limit: usize,
    ) -> BoxFuture<'a, Result<StreamSlice, EventLogError>> {
        self.inner.read_stream(stream, after, limit)
    }

    fn stream_version<'a>(
        &'a self,
        stream: &'a StreamId,
    ) -> BoxFuture<'a, Result<Option<u64>, EventLogError>> {
        self.inner.stream_version(stream)
    }

    fn read_feed<'a>(
        &'a self,
        tenant: &'a TenantId,
        after: u64,
        limit: usize,
    ) -> BoxFuture<'a, Result<FeedPage, EventLogError>> {
        self.inner.read_feed(tenant, after, limit)
    }

    fn redact<'a>(
        &'a self,
        stream: &'a StreamId,
        version: u64,
        reason: &'a str,
    ) -> BoxFuture<'a, Result<RecordedEvent, EventLogError>> {
        self.inner.redact(stream, version, reason)
    }

    fn save_snapshot<'a>(
        &'a self,
        stream: &'a StreamId,
        snapshot: &'a Snapshot,
    ) -> BoxFuture<'a, Result<(), EventLogError>> {
        self.inner.save_snapshot(stream, snapshot)
    }

    fn snapshot_generation<'a>(
        &'a self,
        stream: &'a StreamId,
    ) -> BoxFuture<'a, Result<Option<SnapshotGeneration>, EventLogError>> {
        Box::pin(async move {
            let hook = self.snapshot_hook.lock().expect("hook").take();
            if let Some(hook) = hook {
                hook().await;
            }
            self.inner.snapshot_generation(stream).await
        })
    }

    fn save_snapshot_checked<'a>(
        &'a self,
        stream: &'a StreamId,
        snapshot: &'a Snapshot,
        generation: &'a SnapshotGeneration,
    ) -> BoxFuture<'a, Result<bool, EventLogError>> {
        self.inner
            .save_snapshot_checked(stream, snapshot, generation)
    }

    fn load_snapshot<'a>(
        &'a self,
        stream: &'a StreamId,
    ) -> BoxFuture<'a, Result<Option<Snapshot>, EventLogError>> {
        self.inner.load_snapshot(stream)
    }

    fn forget_tenant<'a>(
        &'a self,
        tenant: &'a TenantId,
    ) -> BoxFuture<'a, Result<(), EventLogError>> {
        self.inner.forget_tenant(tenant)
    }

    fn append_guarded<'a>(
        &'a self,
        stream: &'a StreamId,
        expected: Expected,
        events: &'a [NewEvent],
        meta: &'a CommandMeta,
        guard: Arc<dyn Guard>,
    ) -> BoxFuture<'a, Result<AppendResult, EventLogError>> {
        self.inner
            .append_guarded(stream, expected, events, meta, guard)
    }

    fn create_projections(
        &self,
        projector: Arc<dyn Projector>,
    ) -> BoxFuture<'_, Result<(), EventLogError>> {
        self.inner.create_projections(projector)
    }

    fn register_inline(
        &self,
        projector: Arc<dyn Projector>,
    ) -> BoxFuture<'_, Result<(), EventLogError>> {
        self.inner.register_inline(projector)
    }

    fn is_inline<'a>(&'a self, name: &'a str) -> BoxFuture<'a, bool> {
        self.inner.is_inline(name)
    }

    fn run_catch_up<'a>(
        &'a self,
        projector: Arc<dyn Projector>,
        tenant: &'a TenantId,
        batch: usize,
    ) -> BoxFuture<'a, Result<CatchUpProgress, EventLogError>> {
        self.inner.run_catch_up(projector, tenant, batch)
    }

    fn rebuild_projection<'a>(
        &'a self,
        projector: Arc<dyn Projector>,
        tenant: &'a TenantId,
    ) -> BoxFuture<'a, Result<u64, EventLogError>> {
        self.inner.rebuild_projection(projector, tenant)
    }

    fn projection_get<'a>(
        &'a self,
        projection: &'a ProjectionSpec,
        tenant: &'a TenantId,
        key: &'a str,
    ) -> BoxFuture<'a, Result<Option<Value>, EventLogError>> {
        Box::pin(async move {
            let hook = {
                let mut armed = self.hook.lock().expect("hook");
                match armed.as_ref() {
                    Some((name, _)) if *name == projection.name => armed.take(),
                    _ => None,
                }
            };
            if let Some((_, hook)) = hook {
                hook();
            }
            self.inner.projection_get(projection, tenant, key).await
        })
    }

    fn projection_find<'a>(
        &'a self,
        projection: &'a ProjectionSpec,
        tenant: &'a TenantId,
        field: &'a str,
        value: &'a str,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<Value>, EventLogError>> {
        self.inner
            .projection_find(projection, tenant, field, value, limit)
    }

    fn projection_list<'a>(
        &'a self,
        projection: &'a ProjectionSpec,
        tenant: &'a TenantId,
        after: Option<&'a str>,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<(String, Value)>, EventLogError>> {
        self.inner.projection_list(projection, tenant, after, limit)
    }

    fn projection_page<'a>(
        &'a self,
        projection: &'a ProjectionSpec,
        tenant: &'a TenantId,
        prefix: Option<&'a str>,
        cursor: Option<&'a str>,
        limit: usize,
    ) -> BoxFuture<'a, Result<ProjectionPage, EventLogError>> {
        self.inner
            .projection_page(projection, tenant, prefix, cursor, limit)
    }

    fn stream_identity<'a>(
        &'a self,
        tenant: &'a TenantId,
    ) -> BoxFuture<'a, Result<String, EventLogError>> {
        self.inner.stream_identity(tenant)
    }

    fn put_blob<'a>(
        &'a self,
        tenant: &'a TenantId,
        digest: &'a str,
        bytes: &'a [u8],
    ) -> BoxFuture<'a, Result<(), EventLogError>> {
        self.inner.put_blob(tenant, digest, bytes)
    }

    fn get_blob<'a>(
        &'a self,
        tenant: &'a TenantId,
        digest: &'a str,
    ) -> BoxFuture<'a, Result<Option<Vec<u8>>, EventLogError>> {
        self.inner.get_blob(tenant, digest)
    }

    fn delete_blob<'a>(
        &'a self,
        tenant: &'a TenantId,
        digest: &'a str,
    ) -> BoxFuture<'a, Result<(), EventLogError>> {
        self.inner.delete_blob(tenant, digest)
    }

    fn read_many<'a>(
        &'a self,
        reads: &'a [Read],
    ) -> BoxFuture<'a, Result<Vec<ReadResult>, EventLogError>> {
        self.inner.read_many(reads)
    }
}

impl<B: EventlogBackend> AtomicEventStore for Interposing<B> {
    fn append_group_guarded<'a>(
        &'a self,
        group: &'a AppendGroup,
        admission: Arc<dyn Guard>,
    ) -> BoxFuture<'a, Result<AppendGroupResult, EventLogError>> {
        self.inner.append_group_guarded(group, admission)
    }

    fn append_group_guarded_with_blobs<'a>(
        &'a self,
        group: &'a AppendGroup,
        admission: Arc<dyn Guard>,
        blobs: &'a [(String, Vec<u8>)],
    ) -> BoxFuture<'a, Result<AppendGroupResult, EventLogError>> {
        self.inner
            .append_group_guarded_with_blobs(group, admission, blobs)
    }
}

impl<B: EventlogBackend> ConsistentTenantCapture for Interposing<B> {
    fn capture_tenant_since<'a>(
        &'a self,
        tenant: &'a TenantId,
        projections: &'a [ProjectionSpec],
        limits: CaptureLimits,
        previous: Option<&'a CaptureCheckpoint>,
    ) -> BoxFuture<'a, Result<TenantCaptureUpdate, CaptureError>> {
        self.inner
            .capture_tenant_since(tenant, projections, limits, previous)
    }

    fn durable_checkpoint(
        &self,
        checkpoint: &CaptureCheckpoint,
    ) -> Option<DurableCaptureCheckpoint> {
        self.inner.durable_checkpoint(checkpoint)
    }

    fn restore_checkpoint(&self, durable: &DurableCaptureCheckpoint) -> Option<CaptureCheckpoint> {
        self.inner.restore_checkpoint(durable)
    }

    fn checkpoint_usage(&self, checkpoint: &CaptureCheckpoint) -> Option<CaptureUsage> {
        self.inner.checkpoint_usage(checkpoint)
    }

    fn capture_tenant<'a>(
        &'a self,
        tenant: &'a TenantId,
        projections: &'a [ProjectionSpec],
        limits: CaptureLimits,
    ) -> BoxFuture<'a, Result<TenantCapture, CaptureError>> {
        self.inner.capture_tenant(tenant, projections, limits)
    }
}

impl<B: EventlogBackend> InlineProjectionAdmin for Interposing<B> {
    fn attach_inline_existing(
        &self,
        projector: Arc<dyn Projector>,
    ) -> BoxFuture<'_, Result<(), EventLogError>> {
        self.inner.attach_inline_existing(projector)
    }

    fn rebuild_inline_projection<'a>(
        &'a self,
        projector_name: &'a str,
        tenant: &'a TenantId,
    ) -> BoxFuture<'a, Result<InlineRebuildResult, EventLogError>> {
        self.inner.rebuild_inline_projection(projector_name, tenant)
    }
}

/// Design table, "SQL edit made while a bounded-opened handle is live | seen by the in-process
/// proof, as today", and § *How a bounded handle answers*: "No read is answered from content
/// nobody verified". A bounded handle asks the provider for continuity first and reads the row
/// afterwards, in another transaction; a foreign SQL write landing between the two is served.
/// Here the write puts back the subject's genuine revision-1 row, whose state source is a bound,
/// digest-correct record blob, so every check the row read makes passes and the handle answers a
/// rolled-back state that no verification covered. A whole-model handle answers the same read from
/// its verified model.
#[test]
fn a_foreign_sql_write_between_the_continuity_answer_and_the_row_read_is_not_served() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path: PathBuf = directory.path().join("store.sqlite3");
    block_on(async {
        let (authority, stale) = two_revisions(&path).await;
        checkpoint(&path, &authority).await;
        let backend = Arc::new(Interposing::new(attached(&path).await));
        let store = EventlogRecordedStore::open_with_policy(
            backend.clone(),
            authority.clone(),
            LIMITS,
            CapturePolicy::ProviderTracked,
        )
        .await
        .expect("bounded open");
        assert_eq!(store.open_verification(), OpenVerification::Checkpoint);
        assert_eq!(
            store
                .load(&ticket("a"))
                .await
                .expect("load")
                .expect("a")
                .revision,
            2,
            "before the foreign write the rows answer the verified state"
        );
        let foreign = path.clone();
        backend.arm(move || replace_subject_row(&foreign, &stale));
        let served = store.load(&ticket("a")).await;
        let next = store.load(&ticket("a")).await;
        assert!(
            matches!(&served, Ok(Some(state)) if state.revision == 2)
                || matches!(&served, Err(AsyncStoreError::ProviderIntegrity { .. })),
            "a bounded handle served a row a foreign connection wrote after the provider's \
             continuity answer: {:?}; the handle's own next read: {:?}",
            served.map(|state| state.map(|s| s.revision)),
            next.map(|state| state.map(|s| s.revision))
        );
    });
}

/// The rest of the class the case above belongs to: every answer a checkpoint-opened handle builds
/// from reads made after the provider's continuity answer. A foreign SQL write to the record row,
/// and then to the batch row, that the next lookup reads lands between the answer and the read; the
/// lookup answers the verified position or refuses, and never serves the written one.
#[test]
fn a_foreign_sql_write_between_the_continuity_answer_and_a_record_or_batch_row_read_is_not_served()
{
    for (index, table) in [
        ("er_records_v1", "adv_bounded_p_er_records_v1"),
        ("er_batches_v1", "adv_bounded_p_er_batches_v1"),
    ] {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
        let path: PathBuf = directory.path().join("store.sqlite3");
        block_on(async {
            let (authority, _) = two_revisions(&path).await;
            checkpoint(&path, &authority).await;
            let backend = Arc::new(Interposing::new(attached(&path).await));
            let store = EventlogRecordedStore::open_with_policy(
                backend.clone(),
                authority.clone(),
                LIMITS,
                CapturePolicy::ProviderTracked,
            )
            .await
            .expect("bounded open");
            assert_eq!(store.open_verification(), OpenVerification::Checkpoint);
            let foreign = path.clone();
            let edit = format!(
                "UPDATE {table} SET body = replace(body, '\"stream_version\":2}}', \
                 '\"stream_version\":7}}') WHERE body LIKE '%touch-a%'"
            );
            backend.arm_on(index, move || {
                second_connection(&foreign, &edit);
            });
            let served: Result<Vec<u64>, AsyncStoreError> = if index == "er_records_v1" {
                store
                    .lookup_record("touch-a")
                    .await
                    .map(|found| match found {
                        Some(RecordLookup::Committed(record)) => vec![record.position.subject],
                        _ => Vec::new(),
                    })
            } else {
                store
                    .lookup_batch(&BatchKey::Named("second".into()))
                    .await
                    .map(|found| {
                        found.map_or_else(Vec::new, |batch| {
                            batch
                                .records
                                .iter()
                                .map(|record| record.position.subject)
                                .collect()
                        })
                    })
            };
            assert_eq!(
                second_connection(
                    &path,
                    &format!(
                        "SELECT count(*) FROM {table} WHERE body LIKE '%\"stream_version\":7}}%'"
                    )
                )
                .trim(),
                "1",
                "the foreign write to the {index} row landed during the lookup"
            );
            assert!(
                matches!(&served, Ok(positions) if positions == &[2])
                    || matches!(&served, Err(AsyncStoreError::ProviderIntegrity { .. })),
                "a bounded handle served a {index} row a foreign connection wrote after the \
                 provider's continuity answer: {served:?}"
            );
        });
    }
}

/// The per-entity reads behind a bounded handle's histories and a write's preflight and read-back
/// are several provider calls too. A foreign connection that rolls subject `a` back consistently
/// while they run, deleting its last event and the record and batch rows of that event and putting
/// back its revision-1 subject row, leaves nothing for the per-entity verifier to disagree with.
/// Only the provider, asked again, sees the write; the complete verification that follows finds
/// the head behind the position the handle verified and refuses.
#[test]
fn a_consistent_foreign_rollback_during_a_bounded_per_entity_read_is_not_served() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path: PathBuf = directory.path().join("store.sqlite3");
    block_on(async {
        let (authority, stale) = two_revisions(&path).await;
        checkpoint(&path, &authority).await;
        let backend = Arc::new(Interposing::new(attached(&path).await));
        let store = EventlogRecordedStore::open_with_policy(
            backend.clone(),
            authority.clone(),
            LIMITS,
            CapturePolicy::ProviderTracked,
        )
        .await
        .expect("bounded open");
        assert_eq!(store.open_verification(), OpenVerification::Checkpoint);
        assert_eq!(
            store
                .history(&ticket("a"))
                .await
                .expect("history")
                .records
                .len(),
            2,
            "before the foreign write the per-entity read answers the verified history"
        );
        let foreign = path.clone();
        let rollback = format!(
            "DELETE FROM {PREFIX}_events WHERE global_seq = (SELECT MAX(global_seq) FROM \
             {PREFIX}_events); UPDATE {SUBJECT_TABLE} SET body = '{}'; DELETE FROM \
             {PREFIX}_p_er_records_v1 WHERE body LIKE '%touch-a%'; DELETE FROM \
             {PREFIX}_p_er_batches_v1 WHERE body LIKE '%touch-a%';",
            stale.replace('\'', "''")
        );
        // The binding row is the first row a per-entity read reads, before any stream.
        backend.arm_on("er_binding_v1", move || {
            second_connection(&foreign, &rollback);
        });
        let served = store.history(&ticket("a")).await;
        assert_eq!(
            second_connection(&path, &format!("SELECT count(*) FROM {PREFIX}_events")).trim(),
            "2",
            "the foreign rollback landed during the read"
        );
        assert!(
            matches!(&served, Ok(history) if history.records.len() == 2)
                || matches!(&served, Err(AsyncStoreError::ProviderIntegrity { .. })),
            "a bounded handle served a history a foreign connection rolled back during its \
             per-entity read: {:?}",
            served.map(|history| history.records.len())
        );
    });
}

/// Rewrites `from` to `to` (equal lengths) in every record index row of `bytes` that names
/// `record_id`, and nowhere else: the nearest preceding row tag must be the record index's, and
/// the occurrence must lie between that tag and the row's record id.
fn rewrite_record_row(bytes: &mut [u8], record_id: &str, from: &[u8], to: &[u8]) -> usize {
    assert_eq!(from.len(), to.len());
    let anchor = format!("\"record_id\":\"{record_id}\"");
    let tag = b"\"er.eventlog.";
    let record_tag = b"\"er.eventlog.record-index/1\"";
    let find_all = |haystack: &[u8], needle: &[u8]| -> Vec<usize> {
        haystack
            .windows(needle.len())
            .enumerate()
            .filter(|(_, window)| *window == needle)
            .map(|(index, _)| index)
            .collect()
    };
    let mut rewritten = 0;
    for at in find_all(bytes, anchor.as_bytes()) {
        let start = at.saturating_sub(2_000);
        let Some(tag_at) = find_all(&bytes[start..at], tag).last().map(|i| start + i) else {
            continue;
        };
        if !bytes[tag_at..].starts_with(record_tag) {
            continue;
        }
        if let Some(found) = find_all(&bytes[tag_at..at], from)
            .last()
            .map(|i| tag_at + i)
        {
            bytes[found..found + to.len()].copy_from_slice(to);
            rewritten += 1;
        }
    }
    rewritten
}

/// The documented limit of a bounded open (design § *What a bounded open does not detect*, and the
/// row "each state, record and batch read" of § *No silent unverified read*): a raw edit of the
/// file that bypasses SQLite is invisible to the provider. A blob edit is refused by the read that
/// reads the blob, because every blob is held to its digest; an index row is not a blob, so a raw
/// edit of the record row is served by the bounded lookup, and only a `FullVerification` open or a
/// complete read refuses it. This case holds that behaviour, so a change to it is a change to the
/// documented boundary.
#[test]
fn a_raw_edit_of_a_record_row_is_served_by_a_bounded_lookup_and_refused_only_by_complete_verification()
 {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path: PathBuf = directory.path().join("store.sqlite3");
    let (authority, verified) = block_on(async {
        let (authority, _) = two_revisions(&path).await;
        checkpoint(&path, &authority).await;
        let full = open(&path, &authority, CapturePolicy::FullVerification)
            .await
            .expect("full open");
        let Some(RecordLookup::Committed(record)) =
            full.lookup_record("touch-a").await.expect("lookup")
        else {
            panic!("touch-a is committed");
        };
        (authority, record.position)
    });
    assert_eq!(verified.subject, 2, "touch-a is a's second record");
    let mut edits = 0;
    for file in [path.clone(), path.with_extension("sqlite3-wal")] {
        let Ok(mut bytes) = std::fs::read(&file) else {
            continue;
        };
        let count = rewrite_record_row(
            &mut bytes,
            "touch-a",
            b"\"stream_version\":2}",
            b"\"stream_version\":7}",
        );
        if count > 0 {
            std::fs::write(&file, bytes).expect("raw edit");
        }
        edits += count;
    }
    assert!(edits >= 1, "the record row of touch-a was edited in place");
    block_on(async {
        let full = open(&path, &authority, CapturePolicy::FullVerification).await;
        assert!(
            matches!(full, Err(AsyncStoreError::ProviderIntegrity { .. })),
            "the raw edit took effect: a complete verification refuses the file: {:?}",
            full.map(|store| store.open_verification())
        );
        let bounded = open(&path, &authority, CapturePolicy::ProviderTracked)
            .await
            .expect("the provider sees no SQL write, so the open starts from the checkpoint");
        assert_eq!(bounded.open_verification(), OpenVerification::Checkpoint);
        let served = bounded.lookup_record("touch-a").await;
        assert!(
            matches!(
                &served,
                Ok(Some(RecordLookup::Committed(record)))
                    if record.position.subject == 7 && record.receipt.position.subject == 7
            ),
            "design § What a bounded open does not detect: a raw edit of an index row that \
             bypasses SQLite is served by a bounded read and refused only by a FullVerification \
             open or a complete read; the bounded lookup answered {:?}",
            served.map(|found| match found {
                Some(RecordLookup::Committed(record)) =>
                    Some((record.position, record.receipt.position)),
                _ => None,
            })
        );
        let complete = bounded.complete_snapshot(&authority.logical_scope).await;
        assert!(
            matches!(complete, Err(AsyncStoreError::ProviderIntegrity { .. })),
            "a complete read of the bounded handle refuses the edited row: {:?}",
            complete.map(|snapshot| snapshot.histories.len())
        );
    });
}

/// Design § *Installing durable continuity*: disable removes the provider's change recording, so a
/// SQL write while it is off leaves no mark. A checkpoint persisted before the disable must never
/// be continued after a re-enable: the open verifies completely and refuses the edited rows.
#[test]
fn a_checkpoint_from_before_a_disable_and_re_enable_is_never_continued() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path: PathBuf = directory.path().join("store.sqlite3");
    block_on(async {
        let (authority, stale) = two_revisions(&path).await;
        checkpoint(&path, &authority).await;
        durable_continuity(&path, false).await;
        replace_subject_row(&path, &stale);
        durable_continuity(&path, true).await;
        let reopened = open(&path, &authority, CapturePolicy::ProviderTracked).await;
        assert!(
            matches!(reopened, Err(AsyncStoreError::ProviderIntegrity { .. })),
            "a checkpoint from before the disable was continued over a write nobody recorded: {:?}",
            reopened.map(|store| store.open_verification())
        );
    });
}

/// Design § *What it binds*: "a foreign write after the handle's last read and before its drain
/// stays visible to the next open". Here the handle opened from a checkpoint, moved past it with
/// its own write (so its drain writes a new record), and a foreign SQL write lands before that
/// drain. The next open must not start from the record the drain wrote.
#[test]
fn a_bounded_handle_that_moved_never_persists_a_foreign_write_made_before_its_drain() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path: PathBuf = directory.path().join("store.sqlite3");
    block_on(async {
        let (authority, stale) = two_revisions(&path).await;
        checkpoint(&path, &authority).await;
        let bounded = open(&path, &authority, CapturePolicy::ProviderTracked)
            .await
            .expect("bounded open");
        assert_eq!(bounded.open_verification(), OpenVerification::Checkpoint);
        run(
            &bounded,
            "third",
            vec![execute("a", 2, "touch", "touch-a-again")],
        )
        .await;
        assert_eq!(
            bounded
                .load(&ticket("a"))
                .await
                .expect("load")
                .expect("a")
                .revision,
            3
        );
        replace_subject_row(&path, &stale);
        assert!(
            bounded.write_open_checkpoint().await.expect("write"),
            "the handle moved past the loaded record, so its drain writes one"
        );
        drop(bounded);
        let reopened = open(&path, &authority, CapturePolicy::ProviderTracked).await;
        assert!(
            matches!(reopened, Err(AsyncStoreError::ProviderIntegrity { .. })),
            "the next open started from a record that absorbed the foreign write: {:?}",
            reopened.map(|store| store.open_verification())
        );
    });
}

/// The documented limit of the discard rule (design § *When it is written*, fifth condition, and
/// the rustdoc of `EventlogRecordedStore::write_open_checkpoint` and both
/// `discard_open_checkpoint`s): the drain reads the persisted record, then the snapshot
/// generation, then writes, and Eventlog's checked save compares a generation that never changes
/// once minted, so the snapshot port offers no compare-and-set. A discard run while a handle is
/// live breaks the discard's documented precondition; when it lands inside that handle's drain,
/// between the check and the write, the drain writes over the tombstone. This case holds that
/// outcome, so a change to it is a change to the documented boundary.
#[test]
fn a_discard_landing_inside_a_live_handles_drain_is_overwritten_as_documented() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path: PathBuf = directory.path().join("store.sqlite3");
    block_on(async {
        let (authority, _) = two_revisions(&path).await;
        checkpoint(&path, &authority).await;
        let backend = Arc::new(Interposing::new(attached(&path).await));
        let live = EventlogRecordedStore::open_with_policy(
            backend.clone(),
            authority.clone(),
            LIMITS,
            CapturePolicy::ProviderTracked,
        )
        .await
        .expect("bounded open");
        assert_eq!(live.open_verification(), OpenVerification::Checkpoint);
        run(
            &live,
            "third",
            vec![execute("a", 2, "touch", "touch-a-again")],
        )
        .await;
        live.load(&ticket("a")).await.expect("load");
        let operator = attached(&path).await;
        let discarded = authority.clone();
        backend.arm_snapshot(Box::new(move || {
            Box::pin(async move {
                assert!(
                    EventlogRecordedStore::discard_open_checkpoint(operator.as_ref(), &discarded)
                        .await
                        .expect("discard"),
                    "the operator's discard replaced the checkpoint with a tombstone"
                );
            })
        }));
        let wrote = live.write_open_checkpoint().await.expect("write call");
        drop(live);
        let next = open(&path, &authority, CapturePolicy::ProviderTracked)
            .await
            .expect("tracked open");
        assert!(
            wrote && next.open_verification() == OpenVerification::Checkpoint,
            "design § When it is written, the documented limit: the snapshot port has no \
             compare-and-set, so a discard run while a handle is live and landing inside its \
             drain is overwritten by that drain (wrote: {wrote}, next open {:?})",
            next.open_verification()
        );
    });
}
