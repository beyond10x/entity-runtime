//! A command reads only the streams of the entities it names.
//!
//! Every provider call the adapter makes goes through a counting backend, so what a command read
//! is observed at the provider boundary rather than inferred from the adapter's own counters: a
//! command on one subject of a store that holds several takes no complete tenant capture, reads
//! no tenant feed and scans no projection, and the only subject stream it reads is its own.
#![cfg(all(
    feature = "sync-bridge",
    any(feature = "tree", feature = "sqlite", feature = "file")
))]

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

use entity_core::Registry;
use entity_eventlog::{
    AsyncBindingProvisioner, Authority, ErRecordedProjector, EventlogBackend,
    EventlogBindingProvisioner, EventlogOperationContext, EventlogRecordedStore,
};
use entity_executor::{BatchAction, CreateRequest, ExecuteRequest, ExecutionError, Executor};
use entity_store::{
    Recording,
    asynchronous::{AppendOutcome, AsyncStoreError, BatchKey, Subject},
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
}

impl<B> CountingBackend<B> {
    fn new(inner: Arc<B>) -> Self {
        Self {
            inner,
            observed: Mutex::new(Observed::default()),
            substituted_identity: Mutex::new(None),
            forged_projection: Mutex::new(None),
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
            // A forged row is forged in storage, so a complete capture holds it too.
            let forged = *self.forged_projection.lock().expect("forged");
            for projection in &mut capture.projections {
                if Some(projection.specification.name) == forged {
                    for (_, body) in &mut projection.rows {
                        *body = json!(["er.eventlog.forged-row/1", { "forged": true }]);
                    }
                }
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

const SUBJECTS: [&str; 4] = ["alpha", "beta", "gamma", "delta"];

/// A bound store holding one created ticket per name in [`SUBJECTS`], each by its own command,
/// and the subject stream each was created on.
async fn populated<B: EventlogBackend>(
    inner: Arc<B>,
    label: &str,
) -> (
    Arc<CountingBackend<B>>,
    EventlogRecordedStore,
    BTreeMap<&'static str, String>,
) {
    let tenant = TenantId::new(format!("per-entity-{label}")).expect("tenant");
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
    let counting = Arc::new(CountingBackend::new(inner));
    let backend: Arc<dyn EventlogBackend> = counting.clone();
    EventlogBindingProvisioner::new(backend.clone(), LIMITS)
        .provision_binding(authority.clone(), context("binding"))
        .await
        .expect("binding");
    let store = EventlogRecordedStore::open(backend, authority, LIMITS)
        .await
        .expect("open");
    let registry = registry();
    for id in SUBJECTS {
        let operation = store.operation(context(id));
        Executor::new(&registry, &operation)
            .batch(
                BatchKey::SingleRecord(format!("{id}-create")),
                vec![create(id)],
            )
            .await
            .expect("a create commits");
    }
    let streams = {
        let observed = counting.observed.lock().expect("observed");
        SUBJECTS
            .into_iter()
            .map(|id| {
                let appended = &observed.appended[&format!("request-{id}")];
                assert_eq!(appended.len(), 1, "{id} was created on one stream");
                (id, appended[0].clone())
            })
            .collect()
    };
    (counting, store, streams)
}

/// The acceptance case: one command on one subject reads that subject's stream and no other.
async fn a_command_on_one_subject_reads_only_its_stream<B: EventlogBackend>(
    inner: Arc<B>,
    label: &str,
) {
    let (counting, store, streams) = populated(inner, label).await;
    let registry = registry();
    counting.reset();
    let operation = store.operation(context("alpha-touch"));
    let outcome = Executor::new(&registry, &operation)
        .batch(
            BatchKey::SingleRecord("alpha-touch".into()),
            vec![touch("alpha", "alpha-touch")],
        )
        .await
        .expect("the touch commits");
    assert!(
        matches!(
            outcome,
            AppendOutcome::Committed {
                replayed: false,
                ..
            }
        ),
        "{label}: {outcome:?}"
    );

    {
        let observed = counting.observed.lock().expect("observed");
        assert_eq!(
            (observed.captures, observed.feeds, observed.scans),
            (0, 0, 0),
            "{label}: a command on one subject took a complete tenant capture, read the tenant feed \
         or scanned an index"
        );
        let subject_streams: BTreeSet<&str> = observed
            .streams
            .iter()
            .filter(|(stream_type, _)| stream_type == "er.subject")
            .map(|(_, stream_id)| stream_id.as_str())
            .collect();
        assert_eq!(
            subject_streams,
            BTreeSet::from([streams["alpha"].as_str()]),
            "{label}: the command read subject streams other than its own"
        );
        let foreign: Vec<_> = observed
            .streams
            .iter()
            .filter(|(stream_type, stream_id)| {
                !(stream_type == "er.subject"
                    || (stream_type == "er.binding" && stream_id == "singleton"))
            })
            .collect();
        assert!(
            foreign.is_empty(),
            "{label}: the command read streams outside its subject and the binding: {foreign:?}"
        );
    }

    // What the command decided on is what a complete read of the store holds.
    let alpha = Subject::new("ticket", "alpha").expect("subject");
    let state = entity_store::asynchronous::AsyncStateReader::load(&store, &alpha)
        .await
        .expect("complete read")
        .expect("alpha exists");
    assert_eq!(
        state.revision, 2,
        "{label}: the touch moved alpha to revision 2"
    );
}

/// The stream identity the adapter is bound to is still checked before a command reads anything:
/// a provider that reports another identity is refused, and nothing is appended.
async fn a_command_refuses_a_provider_reporting_another_stream_identity<B: EventlogBackend>(
    inner: Arc<B>,
    label: &str,
) {
    let (counting, store, _) = populated(inner, label).await;
    let registry = registry();
    counting.substitute_identity("a-substituted-stream-identity");
    counting.reset();
    let operation = store.operation(context("alpha-substituted"));
    let refused = Executor::new(&registry, &operation)
        .batch(
            BatchKey::SingleRecord("alpha-substituted".into()),
            vec![touch("alpha", "alpha-substituted")],
        )
        .await
        .expect_err("a substituted stream identity is refused");
    match refused {
        ExecutionError::Store(AsyncStoreError::ProviderIntegrity { ref detail, .. })
            if detail.contains("tenant generation") => {}
        other => panic!("{label}: refused for another reason: {other:?}"),
    }
    assert_eq!(
        counting.observed.lock().expect("observed").appends,
        0,
        "{label}: a command refused for its identity appended"
    );
}

/// Reading only its own subject does not mean trusting what it reads: the index row the command
/// would be guarded on is still held to the subject's own events, and a forged one is refused.
async fn a_command_refuses_a_forged_index_row_of_its_own_subject<B: EventlogBackend>(
    inner: Arc<B>,
    label: &str,
) {
    let (counting, store, _) = populated(inner, label).await;
    let registry = registry();
    counting.forge_projection("er_subjects_v1");
    counting.reset();
    let operation = store.operation(context("alpha-forged"));
    let refused = Executor::new(&registry, &operation)
        .batch(
            BatchKey::SingleRecord("alpha-forged".into()),
            vec![touch("alpha", "alpha-forged")],
        )
        .await
        .expect_err("a forged subject row is refused");
    match refused {
        ExecutionError::Store(AsyncStoreError::ProviderIntegrity { ref detail, .. })
            if detail == "projection er_subjects_v1 differs from authoritative events" => {}
        other => panic!("{label}: refused for another reason: {other:?}"),
    }
    assert_eq!(
        counting.observed.lock().expect("observed").appends,
        0,
        "{label}: a command refused for a forged row appended"
    );
}

#[cfg(feature = "tree")]
async fn tree(directory: &std::path::Path) -> Arc<eventlog_tree::TreeEventStore> {
    Arc::new(
        eventlog_tree::TreeEventStore::open(directory)
            .await
            .expect("tree provider"),
    )
}

#[cfg(feature = "sqlite")]
async fn sqlite(prefix: &str) -> Arc<eventlog_sqlite::SqliteEventStore> {
    Arc::new(
        eventlog_sqlite::SqliteEventStore::in_memory(prefix)
            .await
            .expect("SQLite provider"),
    )
}

#[cfg(feature = "file")]
async fn file(directory: &std::path::Path) -> Arc<eventlog_file::FileEventStore> {
    Arc::new(
        eventlog_file::FileEventStore::open(directory)
            .await
            .expect("file provider"),
    )
}

#[cfg(feature = "tree")]
#[test]
fn a_command_on_one_subject_reads_only_its_own_stream_on_the_tree_provider() {
    block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        a_command_on_one_subject_reads_only_its_stream(tree(directory.path()).await, "tree").await;
    });
}

#[cfg(feature = "sqlite")]
#[test]
fn a_command_on_one_subject_reads_only_its_own_stream_on_the_sqlite_provider() {
    block_on(async {
        a_command_on_one_subject_reads_only_its_stream(sqlite("per_entity").await, "sqlite").await;
    });
}

#[cfg(feature = "file")]
#[test]
fn a_command_on_one_subject_reads_only_its_own_stream_on_the_file_provider() {
    block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        a_command_on_one_subject_reads_only_its_stream(file(directory.path()).await, "file").await;
    });
}

#[cfg(feature = "tree")]
#[test]
fn a_command_refuses_a_substituted_stream_identity_on_the_tree_provider() {
    block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        a_command_refuses_a_provider_reporting_another_stream_identity(
            tree(directory.path()).await,
            "tree",
        )
        .await;
    });
}

#[cfg(feature = "sqlite")]
#[test]
fn a_command_refuses_a_substituted_stream_identity_on_the_sqlite_provider() {
    block_on(async {
        a_command_refuses_a_provider_reporting_another_stream_identity(
            sqlite("per_entity_identity").await,
            "sqlite",
        )
        .await;
    });
}

#[cfg(feature = "tree")]
#[test]
fn a_command_refuses_a_forged_index_row_of_its_own_subject_on_the_tree_provider() {
    block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        a_command_refuses_a_forged_index_row_of_its_own_subject(
            tree(directory.path()).await,
            "tree",
        )
        .await;
    });
}

#[cfg(feature = "sqlite")]
#[test]
fn a_command_refuses_a_forged_index_row_of_its_own_subject_on_the_sqlite_provider() {
    block_on(async {
        a_command_refuses_a_forged_index_row_of_its_own_subject(
            sqlite("per_entity_forged").await,
            "sqlite",
        )
        .await;
    });
}
