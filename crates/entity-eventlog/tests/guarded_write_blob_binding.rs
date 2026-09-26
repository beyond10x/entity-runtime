//! The guarded writes other than the batch import hand their blobs to the provider with the group.
//!
//! `AtomicEventStore::append_group_guarded_with_blobs` commits a group and the blobs it references
//! in one transaction on a provider that overrides it — `eventlog-file` and, since Eventlog 0.5.0,
//! `eventlog-sqlite` — so a guard refusal binds no blob, and the whole write costs one durability
//! barrier. The batch import used it first. These cases hold the three other guarded writes to the
//! same contract: a recorded append, a binding and a singular import.
//!
//! [`Interposed`] forwards every call to a real provider and adds four knobs. It can run a rival
//! write immediately before this handle's first write reaches the provider, which is how a refusal
//! the adapter's own pre-read cannot see is reached deterministically. It can take the port's
//! default — refuse the blob-bearing group with `UNAVAILABLE` — which is what a provider without the
//! override (PostgreSQL) offers, so the adapter's fallback is exercised without a server. It can
//! submit every blob-bearing group twice, which is the retry the port describes as admitted again.
//! And it can fail the next `put_blob`, which is how the fallback's own error mapping is held.
//!
//! The refusal recorder is not here: it appends with `EventStore::append`, unguarded, to a stream
//! named by the digest of the one blob it writes, so nothing can refuse it after that blob is bound
//! except an append naming the same content.
#![cfg(all(feature = "sqlite", feature = "file", feature = "sync-bridge"))]

use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use entity_core::{Registry, Runtime};
use entity_eventlog::{
    AsyncBindingProvisioner, AsyncImportedAnchorWriter, Authority, ErRecordedProjector,
    EventlogBackend, EventlogBindingProvisioner, EventlogOperationContext, EventlogRecordedStore,
    ImportAnchorFailure, ProvisionBindingFailure, projection_specs,
};
use entity_executor::{BatchAction, CreateRequest, Executor};
use entity_store::{
    Recording,
    asynchronous::{
        AppendOutcome, AsyncStateReader, AsyncStoreError, BatchKey, HistoryOrigin,
        ImportedRecordEvidence, KnownLegacyOrder, LegacyAnchor, LegacyCompleteness, LegacyEvidence,
        LegacyOrderDeclaration, RecordedEntry, Subject, SubjectHistory, WriteFailure,
    },
};
use eventlog_core::{
    AppendGroup, AppendGroupResult, AppendResult, AtomicEventStore, BoxFuture, CaptureError,
    CaptureLimits, CatchUpProgress, Claim, ClaimedCommand, CommandMeta, ConsistentTenantCapture,
    EventLogError, EventStore, Expected, FeedPage, Guard, InlineProjectionAdmin,
    InlineRebuildResult, NewEvent, ProjectionSpec, Projector, RecordedEvent, Snapshot, StreamId,
    StreamSlice, TenantCapture, TenantId,
};
use serde_json::{Value, json};
use time::OffsetDateTime;

const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 128,
    max_blobs: 512,
    max_projection_rows: 512,
    max_payload_bytes: 4 * 1024 * 1024,
};

type Hook = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Which blob path the wrapped provider offers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Offer {
    /// The provider's own override: the group and its blobs in one transaction.
    Override,
    /// The port's default: `UNAVAILABLE`, having written nothing.
    DefaultOnly,
}

struct Interposed {
    inner: Arc<dyn EventlogBackend>,
    offer: Offer,
    replay: bool,
    /// Runs once, before the first `put_blob` or group this handle sends after it is armed.
    rival: Mutex<Option<Hook>>,
    /// What the tenant bound once the rival had committed, before this handle wrote anything.
    after_rival: Mutex<Option<BTreeSet<String>>>,
    fail_next_put: Mutex<Option<EventLogError>>,
    puts: AtomicUsize,
    /// The event ids each blob-bearing submission returned, in submission order.
    submissions: Mutex<Vec<Vec<String>>>,
}

impl Interposed {
    fn new(inner: Arc<dyn EventlogBackend>, offer: Offer) -> Self {
        Self {
            inner,
            offer,
            replay: false,
            rival: Mutex::new(None),
            after_rival: Mutex::new(None),
            fail_next_put: Mutex::new(None),
            puts: AtomicUsize::new(0),
            submissions: Mutex::new(Vec::new()),
        }
    }

    fn replaying(inner: Arc<dyn EventlogBackend>) -> Self {
        Self {
            replay: true,
            ..Self::new(inner, Offer::Override)
        }
    }

    fn arm(&self, rival: Hook) {
        *self.rival.lock().expect("rival") = Some(rival);
    }

    async fn run_rival(&self, tenant: &TenantId) {
        let rival = self.rival.lock().expect("rival").take();
        if let Some(rival) = rival {
            rival.await;
            *self.after_rival.lock().expect("after rival") = Some(bound(&self.inner, tenant).await);
        }
    }

    fn after_rival(&self) -> BTreeSet<String> {
        self.after_rival
            .lock()
            .expect("after rival")
            .clone()
            .expect("the rival ran before this handle's first write")
    }
}

impl AtomicEventStore for Interposed {
    fn append_group_guarded<'a>(
        &'a self,
        group: &'a AppendGroup,
        admission: Arc<dyn Guard>,
    ) -> BoxFuture<'a, Result<AppendGroupResult, EventLogError>> {
        Box::pin(async move {
            self.run_rival(&group.tenant).await;
            self.inner.append_group_guarded(group, admission).await
        })
    }

    fn append_group_guarded_with_blobs<'a>(
        &'a self,
        group: &'a AppendGroup,
        admission: Arc<dyn Guard>,
        blobs: &'a [(String, Vec<u8>)],
    ) -> BoxFuture<'a, Result<AppendGroupResult, EventLogError>> {
        Box::pin(async move {
            if self.offer == Offer::DefaultOnly {
                return Err(EventLogError::Invalid(eventlog_core::UNAVAILABLE.into()));
            }
            self.run_rival(&group.tenant).await;
            let first = self
                .inner
                .append_group_guarded_with_blobs(group, admission.clone(), blobs)
                .await?;
            self.record(&first);
            if !self.replay {
                return Ok(first);
            }
            let second = self
                .inner
                .append_group_guarded_with_blobs(group, admission, blobs)
                .await?;
            self.record(&second);
            Ok(second)
        })
    }
}

impl Interposed {
    fn record(&self, result: &AppendGroupResult) {
        self.submissions.lock().expect("submissions").push(
            result
                .appends
                .iter()
                .flat_map(|append| append.events.iter().map(|event| event.event_id.clone()))
                .collect(),
        );
    }
}

impl ConsistentTenantCapture for Interposed {
    fn capture_tenant<'a>(
        &'a self,
        tenant: &'a TenantId,
        projections: &'a [ProjectionSpec],
        limits: CaptureLimits,
    ) -> BoxFuture<'a, Result<TenantCapture, CaptureError>> {
        self.inner.capture_tenant(tenant, projections, limits)
    }
}

impl InlineProjectionAdmin for Interposed {
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

impl EventStore for Interposed {
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
        idempotency_key: &'a str,
        request_hash: &'a str,
    ) -> BoxFuture<'a, Result<Option<AppendResult>, EventLogError>> {
        self.inner
            .recorded_command(stream, idempotency_key, request_hash)
    }
    fn read_stream<'a>(
        &'a self,
        stream: &'a StreamId,
        after_version: u64,
        limit: usize,
    ) -> BoxFuture<'a, Result<StreamSlice, EventLogError>> {
        self.inner.read_stream(stream, after_version, limit)
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
        after_position: u64,
        limit: usize,
    ) -> BoxFuture<'a, Result<FeedPage, EventLogError>> {
        self.inner.read_feed(tenant, after_position, limit)
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
        self.inner.projection_get(projection, tenant, key)
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
        after_key: Option<&'a str>,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<(String, Value)>, EventLogError>> {
        self.inner
            .projection_list(projection, tenant, after_key, limit)
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
        Box::pin(async move {
            self.run_rival(tenant).await;
            self.puts.fetch_add(1, Ordering::SeqCst);
            if let Some(error) = self.fail_next_put.lock().expect("put failure").take() {
                return Err(error);
            }
            self.inner.put_blob(tenant, digest, bytes).await
        })
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
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

fn context(label: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "guarded-write-blob-test".into(),
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

fn recording(record_id: &str) -> Recording {
    Recording {
        record_id: record_id.into(),
        recorded_at: "2026-09-26T00:00:00Z".into(),
        correlation: None,
        causation: None,
        actor: None,
    }
}

fn create(subject_id: &str, record_id: &str) -> CreateRequest {
    CreateRequest {
        subject: Subject::new("ticket", subject_id).expect("subject"),
        definition_version: 1,
        fields: json!({ "title": subject_id }),
        recording: recording(record_id),
    }
}

fn imported(label: &str, record_id: &str) -> SubjectHistory {
    imported_titled(label, label, record_id)
}

fn imported_titled(label: &str, title: &str, record_id: &str) -> SubjectHistory {
    let registry = registry();
    let decision = Runtime::new(&registry)
        .create("ticket", 1, label, json!({ "title": title }))
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
                    "guarded-write-source".to_owned(),
                    "records/0",
                    KnownLegacyOrder::PerKind(0),
                )
                .expect("imported evidence"),
            )],
        }),
        records: Vec::new(),
    }
}

/// Every blob the tenant binds, by key.
async fn bound(backend: &Arc<dyn EventlogBackend>, tenant: &TenantId) -> BTreeSet<String> {
    backend
        .capture_tenant(tenant, projection_specs(), LIMITS)
        .await
        .expect("capture")
        .blobs
        .into_iter()
        .map(|blob| blob.digest)
        .collect()
}

#[derive(Clone, Copy, Debug)]
enum Provider {
    Sqlite,
    File,
}

const PROVIDERS: [Provider; 2] = [Provider::Sqlite, Provider::File];

/// A real provider with the recorded projections attached, and the authority a store binds to it.
struct Destination {
    inner: Arc<dyn EventlogBackend>,
    tenant: TenantId,
    authority: Authority,
    _directory: tempfile::TempDir,
}

async fn destination(provider: Provider, label: &str) -> Destination {
    let directory = tempfile::tempdir().expect("temporary directory");
    let tenant = TenantId::new(format!("guarded-{label}")).expect("valid tenant");
    let inner: Arc<dyn EventlogBackend> = match provider {
        Provider::Sqlite => Arc::new(
            eventlog_sqlite::SqliteEventStore::in_memory(&label.replace('-', "_"))
                .await
                .expect("SQLite memory provider"),
        ),
        Provider::File => Arc::new(
            eventlog_file::FileEventStore::open(directory.path())
                .await
                .expect("file provider"),
        ),
    };
    let stream_identity = inner
        .stream_identity(&tenant)
        .await
        .expect("tenant identity");
    let projector = Arc::new(ErRecordedProjector::new());
    inner
        .create_projections(projector.clone())
        .await
        .expect("projection admission");
    inner
        .attach_inline_existing(projector)
        .await
        .expect("projection attachment");
    let authority = Authority {
        logical_scope: format!("guarded-{label}-scope"),
        tenant: tenant.as_str().to_owned(),
        stream_identity,
    };
    Destination {
        inner,
        tenant,
        authority,
        _directory: directory,
    }
}

impl Destination {
    async fn bind(&self) {
        EventlogBindingProvisioner::new(self.inner.clone(), LIMITS)
            .provision_binding(self.authority.clone(), context("bind"))
            .await
            .expect("binding provisioned");
    }

    /// A second writer on the same destination, committing `request` under a named batch.
    fn rival_append(&self, key: &str, request: CreateRequest) -> Hook {
        let inner = self.inner.clone();
        let authority = self.authority.clone();
        let key = BatchKey::Named(key.to_owned());
        Box::pin(async move {
            let store = EventlogRecordedStore::open(inner, authority, LIMITS)
                .await
                .expect("rival store");
            let operation = store.operation(context("rival"));
            let registry = registry();
            Executor::new(&registry, &operation)
                .batch(key, vec![BatchAction::Create(request)])
                .await
                .expect("the rival commits first");
        })
    }
}

/// A named batch whose member's record identity a rival took after this handle read the store.
async fn refused_append(
    provider: Provider,
    offer: Offer,
) -> (ExecutionOutcome, Arc<Interposed>, Destination) {
    let label = format!("append-{provider:?}-{offer:?}").to_lowercase();
    let destination = destination(provider, &label).await;
    destination.bind().await;
    let interposed = Arc::new(Interposed::new(destination.inner.clone(), offer));
    interposed.arm(destination.rival_append("rival", create("rival", "contested")));
    let erased: Arc<dyn EventlogBackend> = interposed.clone();
    let store = EventlogRecordedStore::open(erased, destination.authority.clone(), LIMITS)
        .await
        .expect("bound store");
    let operation = store.operation(context(&label));
    let registry = registry();
    let refused = Executor::new(&registry, &operation)
        .batch(
            BatchKey::Named("late".into()),
            vec![BatchAction::Create(create("late", "contested"))],
        )
        .await
        .expect_err("the destination guard refuses a record identity the rival holds");
    (refused.store_error().cloned(), interposed, destination)
}

type ExecutionOutcome = Option<AsyncStoreError>;

#[test]
fn a_guard_refused_append_binds_no_blob_on_sqlite_and_file() {
    block_on(async {
        for provider in PROVIDERS {
            let (refused, interposed, destination) =
                refused_append(provider, Offer::Override).await;
            assert_eq!(
                refused,
                Some(AsyncStoreError::PreviouslyRecordedBatchEntries { indices: vec![0] }),
                "{provider:?}: the guard's own refusal reaches the caller"
            );
            assert_eq!(
                bound(&destination.inner, &destination.tenant).await,
                interposed.after_rival(),
                "{provider:?}: a refused append binds no blob of its own"
            );
        }
    });
}

/// A handle counts what its own writes bind against the bound it reads with. A refused append
/// bound nothing, so counting its blobs as held would refuse a later write the tenant has room for.
///
/// The rival writes through the same handle, so every blob the tenant binds is one the handle
/// counted: one binding blob, four for the rival's append and four for the later one — nine, which
/// is the bound. Counting the refused append's four as well would make it thirteen.
#[test]
fn a_refused_append_does_not_count_its_unbound_blobs_against_the_read_bound() {
    const TIGHT: CaptureLimits = CaptureLimits {
        max_blobs: 9,
        ..LIMITS
    };
    block_on(async {
        for provider in PROVIDERS {
            let label = format!("bound-{provider:?}").to_lowercase();
            let destination = destination(provider, &label).await;
            destination.bind().await;
            let interposed = Arc::new(Interposed::new(destination.inner.clone(), Offer::Override));
            let erased: Arc<dyn EventlogBackend> = interposed.clone();
            let store = Arc::new(
                EventlogRecordedStore::open(erased, destination.authority.clone(), TIGHT)
                    .await
                    .expect("bound store"),
            );
            let rival = store.clone();
            interposed.arm(Box::pin(async move {
                let operation = rival.operation(context("rival"));
                let registry = registry();
                Executor::new(&registry, &operation)
                    .batch(
                        BatchKey::Named("rival".into()),
                        vec![BatchAction::Create(create("rival", "contested"))],
                    )
                    .await
                    .expect("the rival commits first");
            }));
            let operation = store.operation(context(&label));
            let registry = registry();
            let refused = Executor::new(&registry, &operation)
                .batch(
                    BatchKey::Named("late".into()),
                    vec![BatchAction::Create(create("late", "contested"))],
                )
                .await
                .expect_err("the destination guard refuses a record identity the rival holds");
            assert_eq!(
                refused.store_error(),
                Some(&AsyncStoreError::PreviouslyRecordedBatchEntries { indices: vec![0] }),
                "{provider:?}"
            );
            Executor::new(&registry, &operation)
                .create(create("fresh", "fresh-create"))
                .await
                .expect("a write that fits the bound is admitted");
            assert_eq!(
                bound(&destination.inner, &destination.tenant).await.len(),
                9,
                "{provider:?}: the tenant holds exactly its read bound"
            );
        }
    });
}

#[test]
fn a_guard_refused_append_reports_the_same_refusal_through_the_fallback() {
    block_on(async {
        for provider in PROVIDERS {
            let (through_override, _, _) = refused_append(provider, Offer::Override).await;
            let (through_fallback, interposed, _) =
                refused_append(provider, Offer::DefaultOnly).await;
            assert_eq!(
                through_fallback, through_override,
                "{provider:?}: the fallback refuses exactly as the override does"
            );
            assert_eq!(
                interposed.puts.load(Ordering::SeqCst),
                4,
                "{provider:?}: the fallback uploads the batch, record, request and entry blobs \
                 on their own path"
            );
        }
    });
}

/// A singular import raced by a rival that imported the same subject, sharing a record identity,
/// through a *batch*. The batch committed under its own command key, so this import's key holds no
/// receipt and the destination guard is what answers on every path — the fallback, and the
/// override both before and after it asks again without blobs. The answer is the guard's
/// `RecordConflict`, and the override binds no blob of its own.
async fn import_raced_by_a_batch(
    provider: Provider,
    offer: Offer,
) -> (ImportAnchorFailure, BTreeSet<String>, BTreeSet<String>) {
    let label = format!("batch-raced-{provider:?}-{offer:?}").to_lowercase();
    let destination = destination(provider, &label).await;
    destination.bind().await;
    let interposed = Arc::new(Interposed::new(destination.inner.clone(), offer));
    let inner = destination.inner.clone();
    let authority = destination.authority.clone();
    interposed.arm(Box::pin(async move {
        let store = EventlogRecordedStore::open(inner, authority, LIMITS)
            .await
            .expect("rival store");
        store
            .operation(context("rival"))
            .import_anchors(vec![imported_titled("shared", "first", "shared-create")])
            .await
            .expect("the rival batch imports first");
    }));
    let erased: Arc<dyn EventlogBackend> = interposed.clone();
    let store = EventlogRecordedStore::open(erased, destination.authority.clone(), LIMITS)
        .await
        .expect("bound store");
    let refused = store
        .operation(context(&label))
        .import_anchor(imported_titled("shared", "second", "shared-create"))
        .await
        .expect_err("the rival holds the subject and its record identity");
    (
        refused,
        bound(&destination.inner, &destination.tenant).await,
        interposed.after_rival(),
    )
}

#[test]
fn an_import_raced_by_a_batch_hears_the_guard_through_override_and_fallback() {
    block_on(async {
        for provider in PROVIDERS {
            let (through_override, bound_after, after_rival) =
                import_raced_by_a_batch(provider, Offer::Override).await;
            let (through_fallback, _, _) =
                import_raced_by_a_batch(provider, Offer::DefaultOnly).await;
            assert_eq!(
                through_override,
                ImportAnchorFailure::NotCommitted(AsyncStoreError::RecordConflict {
                    record_id: "shared-create".into(),
                }),
                "{provider:?}: the guard answers when this command key holds no receipt"
            );
            assert_eq!(
                through_fallback, through_override,
                "{provider:?}: the fallback refuses exactly as the override does"
            );
            assert_eq!(
                bound_after, after_rival,
                "{provider:?}: the refused import binds no blob of its own"
            );
        }
    });
}

#[test]
fn a_guard_refused_import_binds_no_blob_on_sqlite_and_file() {
    block_on(async {
        for provider in PROVIDERS {
            let label = format!("import-{provider:?}").to_lowercase();
            let destination = destination(provider, &label).await;
            destination.bind().await;
            let store = EventlogRecordedStore::open(
                destination.inner.clone(),
                destination.authority.clone(),
                LIMITS,
            )
            .await
            .expect("bound store");
            let operation = store.operation(context(&label));
            operation
                .import_anchor(imported("taken", "taken-create"))
                .await
                .expect("the first subject is imported");
            let before = bound(&destination.inner, &destination.tenant).await;
            let refused = operation
                .import_anchor(imported("stolen", "taken-create"))
                .await
                .expect_err("the destination guard refuses a record identity already held");
            assert_eq!(
                refused,
                ImportAnchorFailure::NotCommitted(AsyncStoreError::RecordConflict {
                    record_id: "taken-create".into(),
                }),
                "{provider:?}: the guard's own refusal reaches the caller"
            );
            assert_eq!(
                bound(&destination.inner, &destination.tenant).await,
                before,
                "{provider:?}: a refused import binds no blob"
            );
        }
    });
}

#[test]
fn a_binding_lost_to_a_rival_binds_no_blob_on_sqlite_and_file() {
    block_on(async {
        for provider in PROVIDERS {
            let label = format!("binding-{provider:?}").to_lowercase();
            let destination = destination(provider, &label).await;
            let mut rival = destination.authority.clone();
            rival.logical_scope.push_str("-rival");
            let interposed = Arc::new(Interposed::new(destination.inner.clone(), Offer::Override));
            let inner = destination.inner.clone();
            let winner = rival.clone();
            interposed.arm(Box::pin(async move {
                EventlogBindingProvisioner::new(inner, LIMITS)
                    .provision_binding(winner, context("rival"))
                    .await
                    .expect("the rival binds first");
            }));
            let erased: Arc<dyn EventlogBackend> = interposed.clone();
            let lost = EventlogBindingProvisioner::new(erased, LIMITS)
                .provision_binding(destination.authority.clone(), context(&label))
                .await
                .expect_err("the tenant is already bound to the rival");
            assert_eq!(
                lost,
                ProvisionBindingFailure::Conflict {
                    requested: destination.authority.clone(),
                    found: rival,
                },
                "{provider:?}: the lost binding names the winner"
            );
            assert_eq!(
                bound(&destination.inner, &destination.tenant).await,
                interposed.after_rival(),
                "{provider:?}: a lost binding binds no blob of its own"
            );
        }
    });
}

/// All three writes, each committing on a provider that overrides the method, send no blob on its
/// own path: every blob travels with the group, under its one durability barrier.
#[test]
fn every_guarded_write_binds_its_blobs_with_its_group_on_sqlite_and_file() {
    block_on(async {
        for provider in PROVIDERS {
            let label = format!("single-call-{provider:?}").to_lowercase();
            let destination = destination(provider, &label).await;
            let interposed = Arc::new(Interposed::new(destination.inner.clone(), Offer::Override));
            let erased: Arc<dyn EventlogBackend> = interposed.clone();
            EventlogBindingProvisioner::new(erased.clone(), LIMITS)
                .provision_binding(destination.authority.clone(), context(&label))
                .await
                .expect("binding provisioned");
            let store = EventlogRecordedStore::open(erased, destination.authority.clone(), LIMITS)
                .await
                .expect("bound store");
            let operation = store.operation(context(&label));
            let registry = registry();
            Executor::new(&registry, &operation)
                .create(create("created", "created-create"))
                .await
                .expect("the append commits");
            operation
                .import_anchor(imported("imported", "imported-create"))
                .await
                .expect("the import commits");
            assert_eq!(
                interposed.puts.load(Ordering::SeqCst),
                0,
                "{provider:?}: no blob was written outside its group"
            );
            assert_eq!(
                interposed.submissions.lock().expect("submissions").len(),
                3,
                "{provider:?}: the binding, the append and the import each sent one \
                 blob-bearing group"
            );
            assert!(
                store
                    .load(&Subject::new("ticket", "created").expect("subject"))
                    .await
                    .expect("state read")
                    .is_some(),
                "{provider:?}: the appended subject reads back"
            );
        }
    });
}

/// A blob-bearing retry is admitted again, so each write has to tolerate the commit it already
/// made. The append and import guards admit it and Eventlog answers from the command it recorded;
/// the binding guard refuses it and the provisioner recovers the binding. None of the three turns
/// its own replay into a conflict.
#[test]
fn a_re_admitted_retry_of_each_guarded_write_is_answered_as_its_commit() {
    block_on(async {
        for provider in PROVIDERS {
            let label = format!("retry-{provider:?}").to_lowercase();
            let destination = destination(provider, &label).await;
            let replaying = Arc::new(Interposed::replaying(destination.inner.clone()));
            let erased: Arc<dyn EventlogBackend> = replaying.clone();
            EventlogBindingProvisioner::new(erased.clone(), LIMITS)
                .provision_binding(destination.authority.clone(), context(&label))
                .await
                .expect("a re-admitted binding retry settles");
            // `BindingGuard` refuses its own retry and the provisioner recovers the binding it
            // made, so the binding records its commit and not the refused second submission.
            assert_eq!(
                std::mem::take(&mut *replaying.submissions.lock().expect("submissions")).len(),
                1,
                "{provider:?}: the binding committed once"
            );
            let store = EventlogRecordedStore::open(erased, destination.authority.clone(), LIMITS)
                .await
                .expect("bound store");
            let operation = store.operation(context(&label));
            let registry = registry();
            let appended = Executor::new(&registry, &operation)
                .create(create("retried", "retried-create"))
                .await
                .expect("a re-admitted append retry settles");
            assert!(
                matches!(appended, AppendOutcome::Committed { .. }),
                "{provider:?}: {appended:?}"
            );
            let named = Executor::new(&registry, &operation)
                .batch(
                    BatchKey::Named(format!("{label}-named")),
                    vec![BatchAction::Create(create("named", "named-create"))],
                )
                .await
                .expect("a re-admitted named-batch retry settles");
            assert!(
                matches!(named, AppendOutcome::Committed { .. }),
                "{provider:?}: {named:?}"
            );
            operation
                .import_anchor(imported("reimported", "reimported-create"))
                .await
                .expect("a re-admitted import retry settles");
            let submissions = replaying.submissions.lock().expect("submissions").clone();
            assert_eq!(
                submissions.len(),
                6,
                "{provider:?}: every append and import group went twice and was answered twice"
            );
            for pair in submissions.chunks(2) {
                assert_eq!(
                    pair[1], pair[0],
                    "{provider:?}: a retry answers with the events its commit made"
                );
            }
        }
    });
}

/// A provider without the override takes the adapter's fallback — every blob on its own path, then
/// the guarded group — and each write still commits.
#[test]
fn every_guarded_write_commits_through_the_fallback() {
    block_on(async {
        for provider in PROVIDERS {
            let label = format!("fallback-{provider:?}").to_lowercase();
            let destination = destination(provider, &label).await;
            let interposed = Arc::new(Interposed::new(
                destination.inner.clone(),
                Offer::DefaultOnly,
            ));
            let erased: Arc<dyn EventlogBackend> = interposed.clone();
            EventlogBindingProvisioner::new(erased.clone(), LIMITS)
                .provision_binding(destination.authority.clone(), context(&label))
                .await
                .expect("binding provisioned through the fallback");
            let store = EventlogRecordedStore::open(erased, destination.authority.clone(), LIMITS)
                .await
                .expect("bound store");
            let operation = store.operation(context(&label));
            let registry = registry();
            Executor::new(&registry, &operation)
                .create(create("created", "created-create"))
                .await
                .expect("the append commits through the fallback");
            operation
                .import_anchor(imported("imported", "imported-create"))
                .await
                .expect("the import commits through the fallback");
            // One binding blob, four append blobs, one record and one anchor blob.
            assert_eq!(
                interposed.puts.load(Ordering::SeqCst),
                7,
                "{provider:?}: the fallback uploads every blob on its own path"
            );
            assert!(
                store
                    .load(&Subject::new("ticket", "created").expect("subject"))
                    .await
                    .expect("state read")
                    .is_some(),
                "{provider:?}: the appended subject reads back"
            );
        }
    });
}

/// A blob upload that fails on the fallback path is reported through each write's own blob
/// mapping. `Overloaded` is a variant the blob and append mappings translate differently, so this
/// holds that the fallback did not start reporting its uploads as appends.
#[test]
fn a_failed_fallback_upload_keeps_each_writes_blob_error() {
    block_on(async {
        let expected = AsyncStoreError::ProviderIntegrity {
            provider: "eventlog".into(),
            detail: EventLogError::Overloaded.to_string(),
        };
        for provider in PROVIDERS {
            let label = format!("fallback-fault-{provider:?}").to_lowercase();
            let destination = destination(provider, &label).await;
            let interposed = Arc::new(Interposed::new(
                destination.inner.clone(),
                Offer::DefaultOnly,
            ));
            let erased: Arc<dyn EventlogBackend> = interposed.clone();
            let provisioner = EventlogBindingProvisioner::new(erased.clone(), LIMITS);
            *interposed.fail_next_put.lock().expect("fault") = Some(EventLogError::Overloaded);
            assert_eq!(
                provisioner
                    .provision_binding(destination.authority.clone(), context(&label))
                    .await,
                Err(ProvisionBindingFailure::NotCommitted(expected.clone())),
                "{provider:?}: binding"
            );
            provisioner
                .provision_binding(destination.authority.clone(), context(&label))
                .await
                .expect("binding provisioned");
            let store = EventlogRecordedStore::open(erased, destination.authority.clone(), LIMITS)
                .await
                .expect("bound store");
            let operation = store.operation(context(&label));
            let registry = registry();
            *interposed.fail_next_put.lock().expect("fault") = Some(EventLogError::Overloaded);
            let appended = Executor::new(&registry, &operation)
                .create(create("created", "created-create"))
                .await
                .expect_err("the first upload fails");
            assert!(
                matches!(
                    &appended,
                    entity_executor::ExecutionError::Write(WriteFailure::NotCommitted(error))
                        if *error == expected
                ),
                "{provider:?}: append: {appended:?}"
            );
            *interposed.fail_next_put.lock().expect("fault") = Some(EventLogError::Overloaded);
            assert_eq!(
                operation
                    .import_anchor(imported("imported", "imported-create"))
                    .await,
                Err(ImportAnchorFailure::NotCommitted(expected.clone())),
                "{provider:?}: import"
            );
            *interposed.fail_next_put.lock().expect("fault") = Some(EventLogError::Overloaded);
            assert_eq!(
                operation
                    .import_anchors(vec![imported("batched", "batched-create")])
                    .await,
                Err(ImportAnchorFailure::NotCommitted(expected.clone())),
                "{provider:?}: batch import"
            );
        }
    });
}
