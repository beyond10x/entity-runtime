//! Adversary pass on R2 (per-entity reads): what a command decides when it no longer takes a
//! complete tenant capture.
//!
//! The counting, forging backend is copied from `per_entity_reads.rs` so this file owns its own
//! harness; the one addition is an honest rival writer that commits between the separate reads a
//! per-entity read is made of.
#![cfg(all(feature = "sync-bridge", feature = "sqlite"))]
// The harness is copied whole from `per_entity_reads.rs`; not every helper is used here.
#![allow(dead_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

use entity_core::Registry;
use entity_eventlog::{
    AsyncBindingProvisioner, Authority, ErRecordedProjector, EventlogBackend,
    EventlogBindingProvisioner, EventlogOperationContext, EventlogRecordedStore,
};
use entity_executor::{BatchAction, CreateRequest, ExecuteRequest, Executor};
use entity_store::{
    Recording,
    asynchronous::{BatchKey, Subject},
};
use eventlog_core::{
    AppendGroup, AppendGroupResult, AppendResult, AtomicEventStore, BoxFuture, Capabilities,
    CaptureError, CaptureLimits, CatchUpProgress, Claim, ClaimedCommand, CommandMeta,
    ConsistentTenantCapture, EventLogError, EventStore, Expected, FeedPage, Guard,
    InlineProjectionAdmin, InlineRebuildResult, NewEvent, ProjectionPage, ProjectionSpec,
    Projector, Read, ReadResult, RecordedEvent, Snapshot, SnapshotGeneration, StreamId,
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

/// What the adapter asked the provider for.
#[derive(Default)]
struct Observed {
    /// Complete tenant captures.
    captures: usize,
    /// Tenant feed pages: a read of every stream at once.
    feeds: usize,
    /// Projection pages and lists: a scan of every row of an index.
    scans: usize,
    /// Every stream read, as `(stream type, stream id)`.
    streams: BTreeSet<(String, String)>,
    /// The subject streams each command's group appended to, by request id.
    appended: BTreeMap<String, Vec<String>>,
    /// Appends reaching the provider.
    appends: usize,
}

/// Passes every call through to a real provider and records what was read.
struct CountingBackend<B> {
    inner: Arc<B>,
    observed: Mutex<Observed>,
    /// When set, the provider reports this stream identity instead of its own.
    substituted_identity: Mutex<Option<String>>,
    /// When set, every row of the projection of this name is answered with a forged body.
    forged_projection: Mutex<Option<&'static str>>,
    /// An honest second writer that commits a touch on `alpha` just before a subject index row
    /// is read, standing in for a concurrent process on the same provider.
    rival: Mutex<Option<Rival>>,
}

/// A second, honest store handle on the same provider, and how many more commits it makes.
struct Rival {
    store: Arc<EventlogRecordedStore>,
    remaining: usize,
    next_revision: u64,
}

impl<B> CountingBackend<B> {
    fn new(inner: Arc<B>) -> Self {
        Self {
            inner,
            observed: Mutex::new(Observed::default()),
            substituted_identity: Mutex::new(None),
            forged_projection: Mutex::new(None),
            rival: Mutex::new(None),
        }
    }

    fn arm_rival(&self, store: Arc<EventlogRecordedStore>, commits: usize, next_revision: u64) {
        *self.rival.lock().expect("rival") = Some(Rival {
            store,
            remaining: commits,
            next_revision,
        });
    }

    /// Commits one rival touch on `alpha` if the rival has any left, through its own handle on
    /// the unwrapped provider, so none of its reads reach this wrapper.
    async fn rival_commit(&self) {
        let armed = {
            let mut rival = self.rival.lock().expect("rival");
            match rival.as_mut() {
                Some(rival) if rival.remaining > 0 => {
                    rival.remaining -= 1;
                    let revision = rival.next_revision;
                    rival.next_revision += 1;
                    Some((Arc::clone(&rival.store), revision))
                }
                _ => None,
            }
        };
        if let Some((store, revision)) = armed {
            let record = format!("rival-touch-{revision}");
            let registry = registry();
            let operation = store.operation(context(&record));
            Executor::new(&registry, &operation)
                .batch(
                    BatchKey::SingleRecord(record.clone()),
                    vec![touch_at("alpha", &record, revision)],
                )
                .await
                .expect("the rival's honest touch commits");
        }
    }

    fn forge_projection(&self, name: &'static str) {
        *self.forged_projection.lock().expect("forged") = Some(name);
    }

    fn reset(&self) {
        let mut observed = self.observed.lock().expect("observed");
        observed.captures = 0;
        observed.feeds = 0;
        observed.scans = 0;
        observed.streams.clear();
        observed.appends = 0;
    }

    fn read_stream_ids(&self, stream: &StreamId) {
        self.observed.lock().expect("observed").streams.insert((
            stream.stream_type().to_owned(),
            stream.stream_id().to_owned(),
        ));
    }

    fn substitute_identity(&self, identity: &str) {
        *self.substituted_identity.lock().expect("identity") = Some(identity.to_owned());
    }

    fn substituted(&self) -> Option<String> {
        self.substituted_identity.lock().expect("identity").clone()
    }
}

impl<B: EventlogBackend> EventStore for CountingBackend<B> {
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
        self.observed.lock().expect("observed").appends += 1;
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
        self.read_stream_ids(stream);
        self.inner.read_stream(stream, after, limit)
    }

    fn stream_version<'a>(
        &'a self,
        stream: &'a StreamId,
    ) -> BoxFuture<'a, Result<Option<u64>, EventLogError>> {
        self.read_stream_ids(stream);
        self.inner.stream_version(stream)
    }

    fn read_feed<'a>(
        &'a self,
        tenant: &'a TenantId,
        after: u64,
        limit: usize,
    ) -> BoxFuture<'a, Result<FeedPage, EventLogError>> {
        self.observed.lock().expect("observed").feeds += 1;
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
        self.inner.snapshot_generation(stream)
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
        self.observed.lock().expect("observed").appends += 1;
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
            if projection.name == "er_subjects_v1" {
                self.rival_commit().await;
            }
            let row = self.inner.projection_get(projection, tenant, key).await?;
            let forged = *self.forged_projection.lock().expect("forged");
            Ok(match (row, forged) {
                (Some(_), Some(name)) if name == projection.name => {
                    Some(json!(["er.eventlog.forged-row/1", { "forged": true }]))
                }
                (row, _) => row,
            })
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
        self.observed.lock().expect("observed").scans += 1;
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
        self.observed.lock().expect("observed").scans += 1;
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
        self.observed.lock().expect("observed").scans += 1;
        self.inner
            .projection_page(projection, tenant, prefix, cursor, limit)
    }

    fn stream_identity<'a>(
        &'a self,
        tenant: &'a TenantId,
    ) -> BoxFuture<'a, Result<String, EventLogError>> {
        Box::pin(async move {
            let own = self.inner.stream_identity(tenant).await?;
            Ok(self.substituted().unwrap_or(own))
        })
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
        for read in reads {
            if let Read::Stream { stream, .. } = read {
                self.read_stream_ids(stream);
            }
        }
        // The provider's own batch, so a provider that answers a batch from one observation still
        // does.
        self.inner.read_many(reads)
    }
}

impl<B: EventlogBackend> AtomicEventStore for CountingBackend<B> {
    fn append_group_guarded<'a>(
        &'a self,
        group: &'a AppendGroup,
        admission: Arc<dyn Guard>,
    ) -> BoxFuture<'a, Result<AppendGroupResult, EventLogError>> {
        self.observe_group(group);
        self.inner.append_group_guarded(group, admission)
    }

    fn append_group_guarded_with_blobs<'a>(
        &'a self,
        group: &'a AppendGroup,
        admission: Arc<dyn Guard>,
        blobs: &'a [(String, Vec<u8>)],
    ) -> BoxFuture<'a, Result<AppendGroupResult, EventLogError>> {
        self.observe_group(group);
        self.inner
            .append_group_guarded_with_blobs(group, admission, blobs)
    }
}

impl<B: EventlogBackend> CountingBackend<B> {
    /// Both group methods are observed alike: a command's group reaches the provider through
    /// whichever one the provider implements.
    fn observe_group(&self, group: &AppendGroup) {
        let mut observed = self.observed.lock().expect("observed");
        observed.appends += 1;
        observed
            .appended
            .entry(group.meta.request_id.clone())
            .or_default()
            .extend(
                group
                    .appends
                    .iter()
                    .filter(|append| append.stream.stream_type() == "er.subject")
                    .map(|append| append.stream.stream_id().to_owned()),
            );
    }
}

impl<B: EventlogBackend> ConsistentTenantCapture for CountingBackend<B> {
    fn capture_tenant<'a>(
        &'a self,
        tenant: &'a TenantId,
        projections: &'a [ProjectionSpec],
        limits: CaptureLimits,
    ) -> BoxFuture<'a, Result<TenantCapture, CaptureError>> {
        Box::pin(async move {
            self.observed.lock().expect("observed").captures += 1;
            let mut capture = self
                .inner
                .capture_tenant(tenant, projections, limits)
                .await?;
            if let Some(identity) = self.substituted() {
                capture.stream_identity = identity;
            }
            Ok(capture)
        })
    }
}

impl<B: EventlogBackend> InlineProjectionAdmin for CountingBackend<B> {
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

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

fn context(label: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "per-entity-read-test".into(),
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
        recorded_at: "2026-09-24T00:00:00Z".into(),
        correlation: None,
        causation: None,
        actor: None,
    }
}

fn create(id: &str) -> BatchAction {
    BatchAction::Create(CreateRequest {
        subject: Subject::new("ticket", id).expect("subject"),
        definition_version: 1,
        fields: json!({ "title": id }),
        recording: recording(&format!("{id}-create")),
    })
}

fn touch(id: &str, record_id: &str) -> BatchAction {
    BatchAction::Execute(ExecuteRequest {
        subject: Subject::new("ticket", id).expect("subject"),
        expected_revision: 1,
        operation: "touch".into(),
        arguments: json!({}),
        fulfillments: BTreeMap::new(),
        recording: recording(record_id),
    })
}

fn touch_at(id: &str, record_id: &str, expected_revision: u64) -> BatchAction {
    BatchAction::Execute(ExecuteRequest {
        subject: Subject::new("ticket", id).expect("subject"),
        expected_revision,
        operation: "touch".into(),
        arguments: json!({}),
        fulfillments: BTreeMap::new(),
        recording: recording(record_id),
    })
}

/// A provisioned, opened store on a fresh in-memory SQLite provider, read with `limits`.
async fn bound(
    label: &str,
    limits: CaptureLimits,
) -> (
    Arc<eventlog_sqlite::SqliteEventStore>,
    Arc<CountingBackend<eventlog_sqlite::SqliteEventStore>>,
    EventlogRecordedStore,
    Authority,
) {
    let inner = Arc::new(
        eventlog_sqlite::SqliteEventStore::in_memory(label)
            .await
            .expect("SQLite provider"),
    );
    let tenant = TenantId::new(format!("adversary-{label}")).expect("tenant");
    let stream_identity = inner.stream_identity(&tenant).await.expect("identity");
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
        logical_scope: format!("scope-{label}"),
        tenant: tenant.as_str().to_owned(),
        stream_identity,
    };
    let counting = Arc::new(CountingBackend::new(Arc::clone(&inner)));
    let backend: Arc<dyn EventlogBackend> = counting.clone();
    EventlogBindingProvisioner::new(backend.clone(), limits)
        .provision_binding(authority.clone(), context("binding"))
        .await
        .expect("binding");
    let store = EventlogRecordedStore::open(backend, authority.clone(), limits)
        .await
        .expect("open");
    (inner, counting, store, authority)
}

async fn create_all(store: &EventlogRecordedStore, ids: &[&str]) {
    let registry = registry();
    for id in ids {
        let operation = store.operation(context(id));
        Executor::new(&registry, &operation)
            .batch(
                BatchKey::SingleRecord(format!("{id}-create")),
                vec![create(id)],
            )
            .await
            .expect("a create commits");
    }
}

/// Concurrency. An honest second writer that keeps committing to the subject a command names —
/// one commit between the stream read and the subject-row read of each of the per-entity read's
/// three attempts — is contention, and contention is a revision conflict. The complete capture it
/// replaced was one consistent observation and could never call a live writer tampering.
#[test]
fn an_honest_concurrent_writer_is_a_revision_conflict_not_provider_integrity() {
    block_on(async {
        let (inner, counting, store, authority) = bound("rival", LIMITS).await;
        create_all(&store, &["alpha"]).await;
        let rival_backend: Arc<dyn EventlogBackend> = inner.clone();
        let rival = Arc::new(
            EventlogRecordedStore::open(rival_backend, authority, LIMITS)
                .await
                .expect("the rival opens on the same provider"),
        );
        counting.arm_rival(rival, 3, 1);
        let registry = registry();
        let operation = store.operation(context("victim"));
        let outcome = Executor::new(&registry, &operation)
            .batch(
                BatchKey::SingleRecord("victim-touch".into()),
                vec![touch_at("alpha", "victim-touch", 1)],
            )
            .await;
        match outcome {
            Err(ref error) if error.is_revision_conflict() => {}
            other => panic!(
                "a command racing an honest writer was not refused as a revision conflict: \
                 {other:?}"
            ),
        }
    });
}

/// The handle's own read bound. `docs/design/eventlog-recorded-errors-import-v0.1.md` step 4b:
/// "A writer that commits past its own reader leaves a destination it cannot read … The singular
/// path overshoots its reader by at most the one event it wrote and the next call refuses." Every
/// command this handle reports committed must still be readable by this handle's complete reader.
#[test]
fn a_handle_does_not_commit_commands_it_can_no_longer_read_back() {
    // The binding and three created subjects fill the bound exactly.
    const TIGHT: CaptureLimits = CaptureLimits {
        max_events: 4,
        max_blobs: 512,
        max_projection_rows: 512,
        max_payload_bytes: 4 * 1024 * 1024,
    };
    block_on(async {
        let (_inner, _counting, store, _) = bound("tight", TIGHT).await;
        let registry = registry();
        let mut committed_unreadable = Vec::new();
        for index in 0..8 {
            let id = format!("ticket-{index}");
            let operation = store.operation(context(&id));
            let outcome = Executor::new(&registry, &operation)
                .batch(
                    BatchKey::SingleRecord(format!("{id}-create")),
                    vec![create(&id)],
                )
                .await;
            if outcome.is_ok() {
                let subject = Subject::new("ticket", &id).expect("subject");
                if let Err(error) =
                    entity_store::asynchronous::AsyncStateReader::load(&store, &subject).await
                {
                    committed_unreadable.push(format!("{id}: {error:?}"));
                }
            }
        }
        assert!(
            committed_unreadable.len() <= 1,
            "the handle kept committing past the bound it reads the store back with; {} commits \
             reported committed that the same handle cannot read: {committed_unreadable:#?}",
            committed_unreadable.len()
        );
    });
}

/// The unit's register row, driven as amended. R-151 (`docs/requirements.md`): "A command's
/// reads … are per-entity … and take no complete tenant capture … The store handle's own readers
/// … still use one native complete tenant capture per read."
#[test]
fn r151_a_command_read_is_per_entity_and_a_handle_read_is_one_complete_capture() {
    block_on(async {
        let (_inner, counting, store, _) = bound("register", LIMITS).await;
        create_all(&store, &["alpha"]).await;
        counting.reset();
        let operation = store.operation(context("read"));
        let alpha = Subject::new("ticket", "alpha").expect("subject");
        entity_store::asynchronous::AsyncStateReader::load(&operation, &alpha)
            .await
            .expect("a command's read")
            .expect("alpha exists");
        assert_eq!(
            counting.observed.lock().expect("observed").captures,
            0,
            "R-151: a command's read took a complete tenant capture"
        );
        entity_store::asynchronous::AsyncStateReader::load(&store, &alpha)
            .await
            .expect("the handle's own read")
            .expect("alpha exists");
        assert_eq!(
            counting.observed.lock().expect("observed").captures,
            1,
            "R-151: the handle's own read did not use one native complete tenant capture"
        );
    });
}

/// R-151 again: ordinary reads "do not initialize storage". `stream_identity` is Eventlog's
/// write-on-miss identity call, which the adapter design names as the one not to call from a read.
/// A handle whose tenant has since been forgotten must refuse without minting the tenant a new
/// identity.
#[test]
fn a_read_through_a_forgotten_tenant_does_not_mint_it_a_new_identity() {
    block_on(async {
        let (inner, _counting, store, authority) = bound("forgotten", LIMITS).await;
        create_all(&store, &["alpha"]).await;
        let tenant = TenantId::new(authority.tenant.clone()).expect("tenant");
        inner.forget_tenant(&tenant).await.expect("forget");
        let absent = inner.capture_tenant(&tenant, &[], LIMITS).await;
        assert!(
            matches!(absent, Err(CaptureError::TenantIdentityMissing)),
            "precondition: the forgotten tenant has no identity: {:?}",
            absent.map(|capture| capture.stream_identity)
        );
        let operation = store.operation(context("after-forget"));
        let alpha = Subject::new("ticket", "alpha").expect("subject");
        let read = entity_store::asynchronous::AsyncStateReader::load(&operation, &alpha).await;
        assert!(read.is_err(), "a forgotten tenant was served: {read:?}");
        let after = inner.capture_tenant(&tenant, &[], LIMITS).await;
        assert!(
            matches!(after, Err(CaptureError::TenantIdentityMissing)),
            "a read re-created the forgotten tenant's identity: {:?}",
            after.map(|capture| capture.stream_identity)
        );
    });
}
