//! Adversary cases: a rival command under the *same command key* commits between this handle's
//! read and its write, so the provider already holds a receipt with a different fingerprint.
//!
//! On a provider that overrides `append_group_guarded_with_blobs`, a batch-bearing group with a
//! recorded receipt is admitted again before the fingerprint is compared; on the fallback (the
//! path every write took before this change) it is not. The interposer is a copy of the one in
//! `guarded_write_blob_binding.rs`.
#![cfg(all(feature = "sqlite", feature = "file", feature = "sync-bridge"))]
#![allow(dead_code, unused_imports)]

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

/// An imported boundary for `subject_id` whose title (and so whose anchor bytes) is `title`.
fn imported_titled(subject_id: &str, title: &str, record_id: &str) -> SubjectHistory {
    let registry = registry();
    let decision = Runtime::new(&registry)
        .create("ticket", 1, subject_id, json!({ "title": title }))
        .expect("legacy decision");
    let commit =
        entity_store::RecordedCommit::new(decision, &recording(record_id)).expect("legacy record");
    SubjectHistory {
        subject: Subject::new("ticket", subject_id).expect("legacy subject"),
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

/// A named batch whose key a rival took with different content after this handle read the store.
async fn raced_named_batch(provider: Provider, offer: Offer) -> entity_executor::ExecutionError {
    let label = format!("race-batch-{provider:?}-{offer:?}").to_lowercase();
    let destination = destination(provider, &label).await;
    destination.bind().await;
    let interposed = Arc::new(Interposed::new(destination.inner.clone(), offer));
    interposed.arm(destination.rival_append("shared", create("rival", "rival-create")));
    let erased: Arc<dyn EventlogBackend> = interposed.clone();
    let store = EventlogRecordedStore::open(erased, destination.authority.clone(), LIMITS)
        .await
        .expect("bound store");
    let operation = store.operation(context(&label));
    let registry = registry();
    Executor::new(&registry, &operation)
        .batch(
            BatchKey::Named("shared".into()),
            vec![BatchAction::Create(create("late", "late-create"))],
        )
        .await
        .expect_err("the key already names another batch")
}

/// The same key reused by a different command, raced in after this handle read the store. The
/// deferral in `AppendGuard` admits the re-run on the override; the caller must still see what
/// the fallback (the base's only path) reports.
#[test]
fn adversary_a_batch_key_raced_by_another_command_is_refused_alike_through_override_and_fallback() {
    block_on(async {
        let mut differ = Vec::new();
        for provider in PROVIDERS {
            let through_override =
                format!("{:?}", raced_named_batch(provider, Offer::Override).await);
            let through_fallback = format!(
                "{:?}",
                raced_named_batch(provider, Offer::DefaultOnly).await
            );
            if through_override != through_fallback {
                differ.push(format!(
                    "{provider:?}: override {through_override} / fallback {through_fallback}"
                ));
            }
        }
        assert!(differ.is_empty(), "{differ:#?}");
    });
}

/// A singular import raced by a rival importing a different anchor for the same subject, sharing
/// its record identity. Both writes derive one command key from the subject, so the provider holds
/// the rival's receipt when this group arrives.
async fn raced_import(provider: Provider, offer: Offer) -> ImportAnchorFailure {
    let label = format!("race-import-{provider:?}-{offer:?}").to_lowercase();
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
            .import_anchor(imported_titled("shared", "first", "shared-create"))
            .await
            .expect("the rival imports first");
    }));
    let erased: Arc<dyn EventlogBackend> = interposed.clone();
    let store = EventlogRecordedStore::open(erased, destination.authority.clone(), LIMITS)
        .await
        .expect("bound store");
    store
        .operation(context(&label))
        .import_anchor(imported_titled("shared", "second", "shared-create"))
        .await
        .expect_err("the subject is already imported by the rival")
}

/// The fallback is exactly the sequence every singular import ran before this change (each blob,
/// then `append_group_guarded`, which answers a recorded key without admission), so it is what
/// the caller observed at the base. The override now re-runs `ImportGuard` on the rival's receipt
/// before the fingerprint is compared, and the guard answers first.
#[test]
fn adversary_a_raced_import_reports_the_same_refusal_through_override_and_fallback() {
    block_on(async {
        let mut differ = Vec::new();
        for provider in PROVIDERS {
            let through_fallback = raced_import(provider, Offer::DefaultOnly).await;
            let through_override = raced_import(provider, Offer::Override).await;
            if through_override != through_fallback {
                differ.push(format!(
                    "{provider:?}: override {through_override:?} / fallback {through_fallback:?}"
                ));
            }
        }
        assert!(
            differ.is_empty(),
            "the override reports a raced import differently from the fallback (and the base): \
             {differ:#?}"
        );
    });
}

/// Without the race the adapter's own pre-read (`settled_against`) answers the same import; the
/// raced answer through the override is held to it.
#[test]
fn adversary_a_raced_import_through_the_override_reports_what_the_unraced_import_reports() {
    block_on(async {
        let mut differ = Vec::new();
        for provider in PROVIDERS {
            let label = format!("unraced-import-{provider:?}").to_lowercase();
            let destination = destination(provider, &label).await;
            destination.bind().await;
            let store = EventlogRecordedStore::open(
                destination.inner.clone(),
                destination.authority.clone(),
                LIMITS,
            )
            .await
            .expect("bound store");
            store
                .operation(context("rival"))
                .import_anchor(imported_titled("shared", "first", "shared-create"))
                .await
                .expect("the rival imports first");
            let unraced = store
                .operation(context(&label))
                .import_anchor(imported_titled("shared", "second", "shared-create"))
                .await
                .expect_err("the subject is already imported");
            let raced = raced_import(provider, Offer::Override).await;
            if raced != unraced {
                differ.push(format!(
                    "{provider:?}: raced {raced:?} / unraced {unraced:?}"
                ));
            }
        }
        assert!(differ.is_empty(), "{differ:#?}");
    });
}

/// The mirror of `a_refused_append_does_not_count_its_unbound_blobs_against_the_read_bound`, on
/// the fallback: there the refused append *did* bind its four blobs (orphans a capture reads), so
/// the handle must count them, and a later write that would take the tenant past its read bound is
/// refused. Kills the mutant `blobs_bound: false` on the fallback arm, which the suite misses.
#[test]
fn adversary_a_fallback_refused_append_counts_its_orphaned_blobs_against_the_read_bound() {
    const TIGHT: CaptureLimits = CaptureLimits {
        max_blobs: 9,
        ..LIMITS
    };
    block_on(async {
        for provider in PROVIDERS {
            let label = format!("fallback-bound-{provider:?}").to_lowercase();
            let destination = destination(provider, &label).await;
            destination.bind().await;
            let interposed = Arc::new(Interposed::new(
                destination.inner.clone(),
                Offer::DefaultOnly,
            ));
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
            Executor::new(&registry, &operation)
                .batch(
                    BatchKey::Named("late".into()),
                    vec![BatchAction::Create(create("late", "contested"))],
                )
                .await
                .expect_err("the destination guard refuses a record identity the rival holds");
            assert_eq!(
                bound(&destination.inner, &destination.tenant).await.len(),
                9,
                "{provider:?}: the fallback left the refused append's four blobs bound"
            );
            let fresh = Executor::new(&registry, &operation)
                .create(create("fresh", "fresh-create"))
                .await;
            assert!(
                matches!(
                    fresh.as_ref().err().and_then(|e| e.store_error()),
                    Some(AsyncStoreError::BatchExceedsReadBounds { bound, .. }) if bound == "max_blobs"
                ),
                "{provider:?}: a write past the read bound must be refused, got {fresh:?}"
            );
        }
    });
}
