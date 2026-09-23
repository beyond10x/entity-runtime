//! Fault-boundary acceptance tests over a real transactional provider.
#![cfg(feature = "sqlite")]

use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use entity_core::{Registry, Runtime};
use entity_eventlog::{
    AsyncBindingProvisioner, AsyncImportedAnchorWriter, Authority, ErRecordedProjector,
    EventlogBackend, EventlogBindingProvisioner, EventlogOperationContext, EventlogRecordedStore,
    ImportAnchorFailure, ImportAnchorUncertainty, ProvisionBindingFailure,
    ProvisionBindingUncertainty,
};
use entity_store::{
    Expect, RecordedCommit, Recording,
    asynchronous::{
        AppendMember, AppendOutcome, AppendRequest, AsyncRecordedReader, AsyncRecordedWriter,
        AsyncStateReader, BatchKey, HistoryOrigin, ImportedRecordEvidence, KnownLegacyOrder,
        LegacyAnchor, LegacyCompleteness, LegacyEvidence, LegacyOrderDeclaration, RecordedEntry,
        Subject, SubjectHistory, WriteFailure, original_request_comparison_bytes,
    },
};
use eventlog_core::{
    AppendGroup, AppendGroupResult, AppendResult, AtomicEventStore, BoxFuture, CaptureError,
    CaptureLimits, CatchUpProgress, Claim, ClaimedCommand, CommandMeta, ConsistentTenantCapture,
    EventLogError, EventStore, Expected, FeedPage, Guard, InlineProjectionAdmin,
    InlineRebuildResult, NewEvent, ProjectionPage, ProjectionSpec, ProjectionStore, Projector,
    RecordedEvent, Snapshot, SnapshotGeneration, StreamId, StreamSlice, TenantCapture, TenantId,
};
use serde_json::Value;
use time::OffsetDateTime;

const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 64,
    max_blobs: 128,
    max_projection_rows: 128,
    max_payload_bytes: 1024 * 1024,
};

#[derive(Debug)]
enum AppendFault {
    Return(EventLogError),
    ReturnAndFailRecovery(EventLogError, CaptureError),
    CommitThen(EventLogError),
    CommitThenCorruptRecovery(EventLogError, CaptureFault),
    CommitThenMalformedResult,
}

#[derive(Debug)]
enum CaptureFault {
    Error(CaptureError),
    MissingBindingRow,
    MalformedBindingRow,
    SubstituteGeneration,
    SubstituteTenant,
    MissingBlob,
    UnknownEvent,
    RedactedEvent,
    MissingProjection,
    DuplicateBindingRow,
    DuplicateBindingEvent,
    TrailingUnknownEvent,
    WrongBindingBlob,
    WrongBindingPhysical,
    MissingRecordRow,
    TamperedBatchRow,
    DuplicateSubjectRow,
    TamperedBlob,
}

struct FaultBackend<B> {
    inner: Arc<B>,
    appends: Mutex<VecDeque<AppendFault>>,
    capture: Mutex<Option<CaptureFault>>,
    put_calls: AtomicUsize,
    fail_put_at: Mutex<Option<(usize, EventLogError)>>,
    rebuild_error: Mutex<Option<EventLogError>>,
    append_gate: Mutex<Option<Arc<AppendGate>>>,
    guard_refused_then: Mutex<Option<(String, EventLogError)>>,
}

impl<B> FaultBackend<B> {
    fn new(inner: Arc<B>) -> Self {
        Self {
            inner,
            appends: Mutex::new(VecDeque::new()),
            capture: Mutex::new(None),
            put_calls: AtomicUsize::new(0),
            fail_put_at: Mutex::new(None),
            rebuild_error: Mutex::new(None),
            append_gate: Mutex::new(None),
            guard_refused_then: Mutex::new(None),
        }
    }

    fn append_fault(&self, fault: AppendFault) {
        self.appends.lock().expect("append faults").push_back(fault);
    }

    fn capture_fault(&self, fault: CaptureFault) {
        *self.capture.lock().expect("capture fault") = Some(fault);
    }

    fn fail_after_put(&self, relative_call: usize, error: EventLogError) {
        let target = self.put_calls.load(Ordering::Acquire) + relative_call;
        *self.fail_put_at.lock().expect("put fault") = Some((target, error));
    }

    fn fail_rebuild(&self, error: EventLogError) {
        *self.rebuild_error.lock().expect("rebuild fault") = Some(error);
    }

    fn order_next_two_appends(&self, first_request: &str) -> Arc<AppendGate> {
        let gate = Arc::new(AppendGate::new(first_request));
        *self.append_gate.lock().expect("append gate") = Some(gate.clone());
        gate
    }

    fn substitute_after_guard_refusal(&self, request_id: &str, error: EventLogError) {
        *self.guard_refused_then.lock().expect("guard substitution") =
            Some((request_id.into(), error));
    }
}

struct AppendGate {
    first_request: String,
    arrived: AtomicUsize,
    completed_first: AtomicUsize,
    release_first: tokio::sync::Notify,
    release_second: tokio::sync::Notify,
}

impl AppendGate {
    fn new(first_request: &str) -> Self {
        Self {
            first_request: first_request.into(),
            arrived: AtomicUsize::new(0),
            completed_first: AtomicUsize::new(0),
            release_first: tokio::sync::Notify::new(),
            release_second: tokio::sync::Notify::new(),
        }
    }

    async fn wait(&self, request_id: &str) -> bool {
        self.arrived.fetch_add(1, Ordering::AcqRel);
        if request_id == self.first_request {
            self.release_first.notified().await;
            true
        } else {
            self.release_second.notified().await;
            false
        }
    }

    fn complete(&self, first: bool) {
        if first {
            self.completed_first.store(1, Ordering::Release);
        }
    }

    async fn drive(&self) {
        while self.arrived.load(Ordering::Acquire) != 2 {
            tokio::task::yield_now().await;
        }
        self.release_first.notify_one();
        while self.completed_first.load(Ordering::Acquire) == 0 {
            tokio::task::yield_now().await;
        }
        self.release_second.notify_one();
    }

    async fn wait_for_arrivals(&self, count: usize) {
        while self.arrived.load(Ordering::Acquire) != count {
            tokio::task::yield_now().await;
        }
    }

    fn release_first(&self) {
        self.release_first.notify_one();
    }
}

impl<B: EventlogBackend> EventStore for FaultBackend<B> {
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
        Box::pin(async move {
            self.inner.put_blob(tenant, digest, bytes).await?;
            let call = self.put_calls.fetch_add(1, Ordering::AcqRel) + 1;
            let mut fault = self.fail_put_at.lock().expect("put fault");
            if fault.as_ref().is_some_and(|(target, _)| *target == call) {
                return Err(fault.take().expect("matching put fault").1);
            }
            Ok(())
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

impl<B: EventlogBackend> AtomicEventStore for FaultBackend<B> {
    fn append_group_guarded<'a>(
        &'a self,
        group: &'a AppendGroup,
        admission: Arc<dyn Guard>,
    ) -> BoxFuture<'a, Result<AppendGroupResult, EventLogError>> {
        Box::pin(async move {
            let fault = self.appends.lock().expect("append faults").pop_front();
            let guard_substitution = {
                let mut substitution = self.guard_refused_then.lock().expect("guard substitution");
                if substitution
                    .as_ref()
                    .is_some_and(|(request_id, _)| request_id == &group.meta.request_id)
                {
                    substitution.take().map(|(_, error)| error)
                } else {
                    None
                }
            };
            let gate = self.append_gate.lock().expect("append gate").clone();
            let first = if let Some(gate) = &gate {
                Some(gate.wait(&group.meta.request_id).await)
            } else {
                None
            };
            if let Some(error) = guard_substitution {
                let result = self.inner.append_group_guarded(group, admission).await;
                assert!(
                    matches!(&result, Err(EventLogError::GuardRefused { .. })),
                    "the real provider must first roll back a refusing guard: {result:?}"
                );
                return Err(error);
            }
            match fault {
                None => {
                    let result = self.inner.append_group_guarded(group, admission).await;
                    if let (Some(gate), Some(first)) = (gate, first) {
                        gate.complete(first);
                    }
                    result
                }
                Some(AppendFault::Return(error)) => Err(error),
                Some(AppendFault::ReturnAndFailRecovery(error, capture)) => {
                    self.capture_fault(CaptureFault::Error(capture));
                    Err(error)
                }
                Some(AppendFault::CommitThen(error)) => {
                    self.inner.append_group_guarded(group, admission).await?;
                    Err(error)
                }
                Some(AppendFault::CommitThenCorruptRecovery(error, capture)) => {
                    self.inner.append_group_guarded(group, admission).await?;
                    self.capture_fault(capture);
                    Err(error)
                }
                Some(AppendFault::CommitThenMalformedResult) => {
                    let mut result = self.inner.append_group_guarded(group, admission).await?;
                    result.appends.clear();
                    Ok(result)
                }
            }
        })
    }
}

impl<B: EventlogBackend> ConsistentTenantCapture for FaultBackend<B> {
    fn capture_tenant<'a>(
        &'a self,
        tenant: &'a TenantId,
        projections: &'a [ProjectionSpec],
        limits: CaptureLimits,
    ) -> BoxFuture<'a, Result<TenantCapture, CaptureError>> {
        Box::pin(async move {
            let fault = match self.capture.lock().expect("capture fault").take() {
                Some(CaptureFault::Error(error)) => return Err(error),
                other => other,
            };
            let mut capture = self
                .inner
                .capture_tenant(tenant, projections, limits)
                .await?;
            match fault {
                None => {}
                Some(CaptureFault::MissingBindingRow) => {
                    capture.projections[0].rows.clear();
                }
                Some(CaptureFault::MalformedBindingRow) => {
                    capture.projections[0].rows[0].1 = serde_json::json!(["wrong", {}]);
                }
                Some(CaptureFault::SubstituteGeneration) => {
                    capture.stream_identity.push_str("-substituted");
                }
                Some(CaptureFault::SubstituteTenant) => {
                    capture.tenant = TenantId::new("substituted").expect("valid tenant");
                }
                Some(CaptureFault::MissingBlob) => {
                    capture.blobs.clear();
                }
                Some(CaptureFault::UnknownEvent) => {
                    capture.events[0].name = "foreign.event".into();
                }
                Some(CaptureFault::RedactedEvent) => {
                    capture.events[0].redacted_at = Some(OffsetDateTime::UNIX_EPOCH);
                }
                Some(CaptureFault::MissingProjection) => {
                    capture.projections.pop();
                }
                Some(CaptureFault::DuplicateBindingRow) => {
                    let row = capture.projections[0].rows[0].clone();
                    capture.projections[0].rows.push(row);
                }
                Some(CaptureFault::DuplicateBindingEvent) => {
                    let event = capture.events[0].clone();
                    capture.events.push(event);
                }
                Some(CaptureFault::TrailingUnknownEvent) => {
                    let mut event = capture.events.last().expect("captured event").clone();
                    event.name = "foreign.event".into();
                    capture.events.push(event);
                }
                Some(CaptureFault::WrongBindingBlob) => {
                    capture.projections[0].rows[0].1[1]["binding_blob"] =
                        serde_json::json!(format!("sha256:{}", "0".repeat(64)));
                }
                Some(CaptureFault::WrongBindingPhysical) => {
                    capture.projections[0].rows[0].1[1]["physical"]["global_seq"] =
                        serde_json::json!(u64::MAX);
                }
                Some(CaptureFault::MissingRecordRow) => {
                    capture.projections[1].rows.clear();
                }
                Some(CaptureFault::TamperedBatchRow) => {
                    capture.projections[2].rows[0].1 = serde_json::json!(["wrong", {}]);
                }
                Some(CaptureFault::DuplicateSubjectRow) => {
                    let row = capture.projections[3].rows[0].clone();
                    capture.projections[3].rows.push(row);
                }
                Some(CaptureFault::TamperedBlob) => {
                    let blob = capture.blobs.last_mut().expect("a bound blob");
                    blob.bytes.push(b' ');
                }
                Some(CaptureFault::Error(_)) => unreachable!("returned before capture"),
            }
            Ok(capture)
        })
    }
}

impl<B: EventlogBackend> InlineProjectionAdmin for FaultBackend<B> {
    fn attach_inline_existing(
        &self,
        projector: Arc<dyn Projector>,
    ) -> BoxFuture<'_, Result<(), EventLogError>> {
        self.inner.attach_inline_existing(projector)
    }

    fn rebuild_inline_projection<'a>(
        &'a self,
        name: &'a str,
        tenant: &'a TenantId,
    ) -> BoxFuture<'a, Result<InlineRebuildResult, EventLogError>> {
        Box::pin(async move {
            let rebuilt = self.inner.rebuild_inline_projection(name, tenant).await?;
            if let Some(error) = self.rebuild_error.lock().expect("rebuild fault").take() {
                return Err(error);
            }
            Ok(rebuilt)
        })
    }
}

#[derive(Clone, Copy)]
enum ProjectorFaultTiming {
    Before,
    After,
}

struct FaultProjector {
    inner: ErRecordedProjector,
    fault: Mutex<Option<ProjectorFaultTiming>>,
}

impl FaultProjector {
    fn new() -> Self {
        Self {
            inner: ErRecordedProjector::new(),
            fault: Mutex::new(None),
        }
    }

    fn fail_next(&self, timing: ProjectorFaultTiming) {
        let replaced = self.fault.lock().expect("projector fault").replace(timing);
        assert!(replaced.is_none(), "projector fault was already armed");
    }
}

impl Projector for FaultProjector {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn projections(&self) -> &'static [ProjectionSpec] {
        self.inner.projections()
    }

    fn apply<'a>(
        &'a self,
        event: &'a RecordedEvent,
        store: &'a mut dyn ProjectionStore,
    ) -> BoxFuture<'a, Result<(), EventLogError>> {
        Box::pin(async move {
            let fault = self.fault.lock().expect("projector fault").take();
            if matches!(fault, Some(ProjectorFaultTiming::Before)) {
                return Err(EventLogError::Backend(
                    "controlled failure before real ER projector".into(),
                ));
            }
            self.inner.apply(event, store).await?;
            if matches!(fault, Some(ProjectorFaultTiming::After)) {
                return Err(EventLogError::Backend(
                    "controlled failure after real ER projector".into(),
                ));
            }
            Ok(())
        })
    }
}

struct Fixture<B> {
    backend: Arc<FaultBackend<B>>,
    projector: Arc<FaultProjector>,
    authority: Authority,
}

async fn fixture(label: &str) -> Fixture<eventlog_sqlite::SqliteEventStore> {
    let concrete = Arc::new(
        eventlog_sqlite::SqliteEventStore::in_memory("fault")
            .await
            .expect("SQLite provider"),
    );
    prepare(concrete, label).await
}

async fn prepare<B: EventlogBackend>(concrete: Arc<B>, label: &str) -> Fixture<B> {
    let tenant = TenantId::new(format!("fault-{label}")).expect("tenant");
    let stream_identity = concrete
        .stream_identity(&tenant)
        .await
        .expect("stream identity");
    let projector = Arc::new(FaultProjector::new());
    concrete
        .create_projections(projector.clone())
        .await
        .expect("projection admission");
    concrete
        .attach_inline_existing(projector.clone())
        .await
        .expect("projection attachment");
    Fixture {
        backend: Arc::new(FaultBackend::new(concrete)),
        projector,
        authority: Authority {
            logical_scope: format!("scope-{label}"),
            tenant: tenant.as_str().to_owned(),
            stream_identity,
        },
    }
}

fn context(label: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "fault-test".into(),
        actor: "entity-eventlog-test".into(),
        request_id: format!("request-{label}"),
        trace_id: format!("trace-{label}"),
        causation_id: None,
        causation_depth: 0,
        occurred_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn registry() -> Registry {
    let definition = serde_json::from_value(serde_json::json!({
        "entity": "ticket",
        "version": 1,
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "open", "states": ["open"] },
        "operations": {}
    }))
    .expect("definition");
    let mut registry = Registry::new();
    registry.register(definition).expect("valid definition");
    registry
}

fn request(label: &str) -> AppendRequest {
    request_with_record(label, &format!("record-{label}"))
}

fn request_with_record(label: &str, record_id: &str) -> AppendRequest {
    let decision = Runtime::new(&registry())
        .create("ticket", 1, label, serde_json::json!({"title":label}))
        .expect("decision");
    let commit = RecordedCommit::new(
        decision,
        &Recording {
            record_id: record_id.into(),
            recorded_at: "2026-09-16T00:00:00Z".into(),
            correlation: None,
            causation: None,
            actor: None,
        },
    )
    .expect("commit");
    let entry = RecordedEntry::Decision(commit);
    let bytes = original_request_comparison_bytes(&entry).expect("request bytes");
    AppendRequest::new(
        BatchKey::SingleRecord(entry.record_id().to_owned()),
        vec![AppendMember::new(Expect::Absent, entry, bytes)],
    )
    .expect("append request")
}

fn named_request_with_record(label: &str, record_id: &str) -> AppendRequest {
    let mut request = request_with_record(label, record_id);
    request.key = Some(BatchKey::Named(format!("batch-{label}")));
    request.validate().expect("named append request");
    request
}

fn imported_history(label: &str) -> SubjectHistory {
    imported_history_with_record(label, &format!("legacy-record-{label}"))
}

fn imported_history_with_record(label: &str, record_id: &str) -> SubjectHistory {
    let decision = Runtime::new(&registry())
        .create("ticket", 1, label, serde_json::json!({"title":label}))
        .expect("legacy decision");
    let commit = RecordedCommit::new(
        decision,
        &Recording {
            record_id: record_id.into(),
            recorded_at: "2026-09-16T00:00:00Z".into(),
            correlation: None,
            causation: None,
            actor: None,
        },
    )
    .expect("legacy commit");
    SubjectHistory {
        subject: Subject::new("ticket", label).expect("legacy subject"),
        origin: HistoryOrigin::Imported(LegacyAnchor {
            instance: commit.instance.clone(),
            completeness: LegacyCompleteness::AvailableEvidenceOnly,
            order: LegacyOrderDeclaration::PerKindOnly,
            evidence: vec![LegacyEvidence::Envelope(
                ImportedRecordEvidence::new(
                    RecordedEntry::Decision(commit),
                    "legacy-source",
                    "records/0",
                    KnownLegacyOrder::PerKind(0),
                )
                .expect("legacy evidence"),
            )],
        }),
        records: Vec::new(),
    }
}

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

async fn provision<B: EventlogBackend>(fixture: &Fixture<B>, label: &str) {
    let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
    EventlogBindingProvisioner::new(backend, LIMITS)
        .provision_binding(fixture.authority.clone(), context(label))
        .await
        .expect("binding");
}

#[test]
fn provision_recovers_commit_before_lost_response_with_original_coordinates() {
    block_on(async {
        let fixture = fixture("provision-lost").await;
        fixture
            .backend
            .append_fault(AppendFault::CommitThen(EventLogError::UnknownCommit));
        let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
        let provisioner = EventlogBindingProvisioner::new(backend, LIMITS);
        let recovered = provisioner
            .provision_binding(fixture.authority.clone(), context("first"))
            .await
            .expect("semantic recovery");
        assert!(recovered.replayed);
        let fresh_context = provisioner
            .provision_binding(fixture.authority.clone(), context("fresh"))
            .await
            .expect("restart-style recovery");
        assert!(fresh_context.replayed);
        assert_eq!(fresh_context.physical, recovered.physical);
    });
}

#[cfg(feature = "file")]
#[test]
fn file_provider_recovers_commit_before_lost_response() {
    block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        let concrete = Arc::new(
            eventlog_file::FileEventStore::open(directory.path())
                .await
                .expect("file provider"),
        );
        let fixture = prepare(concrete, "file-lost").await;
        fixture
            .backend
            .append_fault(AppendFault::CommitThen(EventLogError::UnknownCommit));
        let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
        let outcome = EventlogBindingProvisioner::new(backend, LIMITS)
            .provision_binding(fixture.authority, context("file-lost"))
            .await
            .expect("semantic recovery");
        assert!(outcome.replayed);
    });
}

#[cfg(feature = "postgres")]
#[test]
fn postgres_provider_recovers_commit_before_lost_response_when_assigned() {
    block_on(async {
        let Ok(url) = std::env::var("ENTITY_POSTGRES_URL") else {
            eprintln!("PostgreSQL fault test skipped: ENTITY_POSTGRES_URL is not assigned");
            return;
        };
        let suffix: String = OffsetDateTime::now_utc()
            .unix_timestamp_nanos()
            .unsigned_abs()
            .to_string()
            .bytes()
            .map(|digit| char::from(b'a' + digit - b'0'))
            .collect();
        let concrete = Arc::new(
            eventlog_postgres::PostgresEventStore::connect_local(
                &url,
                &format!("fault_{suffix}"),
                eventlog_postgres::PoolOptions::default(),
            )
            .await
            .expect("PostgreSQL provider"),
        );
        let fixture = prepare(concrete.clone(), &format!("postgres-{suffix}")).await;
        fixture
            .backend
            .append_fault(AppendFault::CommitThen(EventLogError::UnknownCommit));
        let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
        let outcome = EventlogBindingProvisioner::new(backend, LIMITS)
            .provision_binding(fixture.authority, context("postgres-lost"))
            .await
            .expect("semantic recovery");
        assert!(outcome.replayed);
        concrete.shutdown().await.expect("PostgreSQL shutdown");
    });
}

#[test]
fn provision_unknown_rollback_and_unavailable_recovery_remain_uncertain() {
    block_on(async {
        let fixture = fixture("provision-rollback").await;
        fixture
            .backend
            .append_fault(AppendFault::ReturnAndFailRecovery(
                EventLogError::UnknownCommit,
                CaptureError::Store(EventLogError::Backend("capture unavailable".into())),
            ));
        let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
        let error = EventlogBindingProvisioner::new(backend, LIMITS)
            .provision_binding(fixture.authority.clone(), context("rollback"))
            .await
            .expect_err("unknown outcome");
        assert!(matches!(
            error,
            ProvisionBindingFailure::Uncertain {
                cause: ProvisionBindingUncertainty::UnknownCommit,
                ..
            }
        ));
    });
}

#[test]
fn provision_recovery_is_absent_then_unavailable_while_the_append_is_in_flight() {
    block_on(async {
        let fixture = fixture("provision-in-flight").await;
        let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
        let provisioner = EventlogBindingProvisioner::new(backend, LIMITS);
        let gate = fixture.backend.order_next_two_appends("request-inflight");
        let provision =
            provisioner.provision_binding(fixture.authority.clone(), context("inflight"));
        let observe = async {
            gate.wait_for_arrivals(1).await;
            assert_eq!(
                provisioner
                    .recover_binding(fixture.authority.clone())
                    .await
                    .expect("available recovery"),
                None
            );
            fixture
                .backend
                .capture_fault(CaptureFault::Error(CaptureError::Store(
                    EventLogError::Backend("capture unavailable".into()),
                )));
            assert!(matches!(
                provisioner.recover_binding(fixture.authority.clone()).await,
                Err(ProvisionBindingFailure::Uncertain {
                    cause: ProvisionBindingUncertainty::RecoveryUnavailable,
                    ..
                })
            ));
            gate.release_first();
        };
        let (outcome, ()) = tokio::join!(provision, observe);
        assert!(!outcome.expect("in-flight winner").replayed);
    });
}

#[test]
fn competing_provisioners_are_atomic_in_both_winner_orders() {
    block_on(async {
        for first in ["request-left", "request-right"] {
            let fixture = fixture(&format!("provision-race-{first}")).await;
            let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
            let provisioner = EventlogBindingProvisioner::new(backend, LIMITS);
            let left_authority = fixture.authority.clone();
            let mut right_authority = fixture.authority.clone();
            right_authority.logical_scope.push_str("-right");
            let gate = fixture.backend.order_next_two_appends(first);
            let (left, right, ()) = tokio::join!(
                provisioner.provision_binding(left_authority, context("left")),
                provisioner.provision_binding(right_authority, context("right")),
                gate.drive()
            );
            if first == "request-left" {
                assert!(left.is_ok(), "left should win: {left:?}");
                assert!(matches!(
                    right,
                    Err(ProvisionBindingFailure::Conflict { .. })
                ));
            } else {
                assert!(right.is_ok(), "right should win: {right:?}");
                assert!(matches!(
                    left,
                    Err(ProvisionBindingFailure::Conflict { .. })
                ));
            }
        }
    });
}

#[test]
fn binding_recovery_rejects_missing_malformed_and_substituted_capture_authority() {
    block_on(async {
        for (label, fault) in [
            ("missing-row", CaptureFault::MissingBindingRow),
            ("malformed-row", CaptureFault::MalformedBindingRow),
            ("generation", CaptureFault::SubstituteGeneration),
            ("tenant", CaptureFault::SubstituteTenant),
            ("missing-blob", CaptureFault::MissingBlob),
            ("unknown-event", CaptureFault::UnknownEvent),
            ("redacted-event", CaptureFault::RedactedEvent),
            ("missing-projection", CaptureFault::MissingProjection),
            ("duplicate-row", CaptureFault::DuplicateBindingRow),
        ] {
            let fixture = fixture(label).await;
            provision(&fixture, label).await;
            fixture.backend.capture_fault(fault);
            let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
            let result = EventlogBindingProvisioner::new(backend, LIMITS)
                .recover_binding(fixture.authority.clone())
                .await;
            assert!(
                matches!(result, Err(ProvisionBindingFailure::NotCommitted(_))),
                "{label}: {result:?}"
            );
        }
    });
}

#[test]
fn binding_recovery_validates_the_complete_foreign_model_before_conflict() {
    block_on(async {
        let fixture = fixture("foreign-complete-model").await;
        provision(&fixture, "foreign-complete-model").await;
        let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
        let store = EventlogRecordedStore::open(backend.clone(), fixture.authority.clone(), LIMITS)
            .await
            .expect("foreign winner opens");
        store
            .operation(context("foreign-record"))
            .append(request("foreign-record"))
            .await
            .expect("foreign winner records a complete subject");

        let provisioner = EventlogBindingProvisioner::new(backend, LIMITS);
        let matching = provisioner
            .recover_binding(fixture.authority.clone())
            .await
            .expect("complete matching authority validates")
            .expect("binding exists after later records");
        assert!(matching.replayed);

        let mut requested = fixture.authority.clone();
        requested.logical_scope.push_str("-requested");
        assert_eq!(
            provisioner.recover_binding(requested.clone()).await,
            Err(ProvisionBindingFailure::Conflict {
                requested: requested.clone(),
                found: fixture.authority.clone(),
            })
        );

        for (label, fault) in [
            ("missing-binding-row", CaptureFault::MissingBindingRow),
            ("malformed-binding-row", CaptureFault::MalformedBindingRow),
            ("wrong-generation", CaptureFault::SubstituteGeneration),
            ("wrong-tenant", CaptureFault::SubstituteTenant),
            ("missing-blob", CaptureFault::MissingBlob),
            ("unknown-binding-event", CaptureFault::UnknownEvent),
            ("redacted-binding-event", CaptureFault::RedactedEvent),
            ("missing-projection", CaptureFault::MissingProjection),
            ("duplicate-binding-row", CaptureFault::DuplicateBindingRow),
            (
                "duplicate-binding-event",
                CaptureFault::DuplicateBindingEvent,
            ),
            ("trailing-unknown-event", CaptureFault::TrailingUnknownEvent),
            ("wrong-binding-blob", CaptureFault::WrongBindingBlob),
            ("wrong-binding-physical", CaptureFault::WrongBindingPhysical),
            ("missing-record-row", CaptureFault::MissingRecordRow),
            ("tampered-batch-row", CaptureFault::TamperedBatchRow),
            ("duplicate-subject-row", CaptureFault::DuplicateSubjectRow),
        ] {
            fixture.backend.capture_fault(fault);
            let result = provisioner.recover_binding(requested.clone()).await;
            assert!(
                matches!(
                    result,
                    Err(ProvisionBindingFailure::NotCommitted(
                        entity_store::asynchronous::AsyncStoreError::ProviderIntegrity { .. }
                    ))
                ),
                "{label}: incomplete foreign authority must not become Conflict: {result:?}"
            );
        }
    });
}

#[test]
fn unknown_commit_with_corrupt_recovery_keeps_the_original_uncertainty() {
    block_on(async {
        let fixture = fixture("provision-corrupt-recovery").await;
        fixture
            .backend
            .append_fault(AppendFault::CommitThenCorruptRecovery(
                EventLogError::UnknownCommit,
                CaptureFault::MissingBindingRow,
            ));
        let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
        let provisioner = EventlogBindingProvisioner::new(backend, LIMITS);
        let result = provisioner
            .provision_binding(fixture.authority.clone(), context("corrupt-recovery"))
            .await;
        assert!(matches!(
            result,
            Err(ProvisionBindingFailure::Uncertain {
                authority,
                cause: ProvisionBindingUncertainty::UnknownCommit,
            }) if authority == fixture.authority
        ));
        assert!(
            provisioner
                .recover_binding(fixture.authority.clone())
                .await
                .expect("the one-shot corrupt capture is gone")
                .is_some(),
            "the ambiguous append really committed; corruption did not prove rollback"
        );
    });
}

#[test]
fn provision_preserves_exact_tuple_conflict() {
    block_on(async {
        let fixture = fixture("tuple-conflict").await;
        provision(&fixture, "winner").await;
        let mut requested = fixture.authority.clone();
        requested.logical_scope.push_str("-loser");
        let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
        assert!(matches!(
            EventlogBindingProvisioner::new(backend, LIMITS)
                .provision_binding(requested, context("loser"))
                .await,
            Err(ProvisionBindingFailure::Conflict { .. })
        ));
    });
}

#[test]
fn open_rejects_every_mutated_authoritative_capture_shape() {
    block_on(async {
        for (label, fault) in [
            ("open-missing-row", CaptureFault::MissingBindingRow),
            ("open-malformed-row", CaptureFault::MalformedBindingRow),
            ("open-generation", CaptureFault::SubstituteGeneration),
            ("open-tenant", CaptureFault::SubstituteTenant),
            ("open-missing-blob", CaptureFault::MissingBlob),
            ("open-unknown-event", CaptureFault::UnknownEvent),
            ("open-redacted-event", CaptureFault::RedactedEvent),
            ("open-missing-projection", CaptureFault::MissingProjection),
            ("open-duplicate-row", CaptureFault::DuplicateBindingRow),
        ] {
            let fixture = fixture(label).await;
            provision(&fixture, label).await;
            fixture.backend.capture_fault(fault);
            let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
            assert!(
                EventlogRecordedStore::open(backend, fixture.authority.clone(), LIMITS)
                    .await
                    .is_err(),
                "{label}"
            );
        }
    });
}

#[test]
fn rebuild_propagates_provider_failure_and_revalidates_the_fresh_capture() {
    block_on(async {
        let fixture = fixture("rebuild-fault").await;
        provision(&fixture, "rebuild-fault").await;
        let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
        let store = EventlogRecordedStore::open(backend, fixture.authority.clone(), LIMITS)
            .await
            .expect("open");
        fixture
            .backend
            .fail_rebuild(EventLogError::Backend("rebuild unavailable".into()));
        assert!(matches!(
            store.rebuild_indexes().await,
            Err(entity_store::asynchronous::AsyncStoreError::Backend(_))
        ));

        for fault in [
            CaptureFault::MissingBindingRow,
            CaptureFault::MalformedBindingRow,
            CaptureFault::MissingBlob,
            CaptureFault::UnknownEvent,
            CaptureFault::RedactedEvent,
            CaptureFault::MissingProjection,
            CaptureFault::DuplicateBindingRow,
        ] {
            fixture.backend.capture_fault(fault);
            assert!(matches!(
                store.rebuild_indexes().await,
                Err(entity_store::asynchronous::AsyncStoreError::ProviderIntegrity { .. })
            ));
        }
    });
}

#[test]
fn reads_reject_missing_tampered_and_colliding_record_indexes() {
    block_on(async {
        let fixture = fixture("record-index-corruption").await;
        provision(&fixture, "record-index-corruption").await;
        let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
        let store = EventlogRecordedStore::open(backend, fixture.authority.clone(), LIMITS)
            .await
            .expect("open");
        let subject = Subject::new("ticket", "indexed").expect("subject");
        store
            .operation(context("indexed"))
            .append(request("indexed"))
            .await
            .expect("indexed append");
        for fault in [
            CaptureFault::MissingRecordRow,
            CaptureFault::TamperedBatchRow,
            CaptureFault::DuplicateSubjectRow,
        ] {
            fixture.backend.capture_fault(fault);
            assert!(matches!(
                store.load(&subject).await,
                Err(entity_store::asynchronous::AsyncStoreError::ProviderIntegrity { .. })
            ));
        }
    });
}

/// A handle that has already verified its head still refuses every capture that is not that
/// head: the verified model is reused only for a capture equal to the one it was built from, or
/// advanced only past events this handle appended over byte-identical blobs. A read after a
/// verified read and after the handle's own commit is refused exactly as a first read would be.
#[test]
fn a_verified_head_is_never_reused_for_a_capture_that_differs_from_it() {
    block_on(async {
        let fixture = fixture("verified-head-reuse").await;
        provision(&fixture, "verified-head-reuse").await;
        let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
        let store = EventlogRecordedStore::open(backend, fixture.authority.clone(), LIMITS)
            .await
            .expect("open");
        let subject = Subject::new("ticket", "verified-head").expect("subject");
        store
            .operation(context("verified-head"))
            .append(request("verified-head"))
            .await
            .expect("append");
        assert!(store.load(&subject).await.expect("verified read").is_some());
        for (label, fault) in [
            ("missing-blob", CaptureFault::MissingBlob),
            ("tampered-blob", CaptureFault::TamperedBlob),
            ("unknown-event", CaptureFault::UnknownEvent),
            ("redacted-event", CaptureFault::RedactedEvent),
            ("trailing-unknown-event", CaptureFault::TrailingUnknownEvent),
            ("missing-record-row", CaptureFault::MissingRecordRow),
            ("tampered-batch-row", CaptureFault::TamperedBatchRow),
            ("missing-projection", CaptureFault::MissingProjection),
        ] {
            let before = store.calls();
            fixture.backend.capture_fault(fault);
            assert!(
                matches!(
                    store.load(&subject).await,
                    Err(entity_store::asynchronous::AsyncStoreError::ProviderIntegrity { .. })
                ),
                "{label}: a capture differing from the verified head was answered"
            );
            assert!(
                store
                    .load(&subject)
                    .await
                    .expect("the unfaulted head")
                    .is_some(),
                "{label}: the unfaulted head is readable again"
            );
            assert_eq!(
                (
                    store.calls().model_builds - before.model_builds,
                    store.calls().model_advances - before.model_advances,
                ),
                (1, 0),
                "{label}: a head once refused is verified whole again, never reused"
            );
        }
    });
}

#[test]
fn append_recovers_committed_lost_reply_and_preserves_the_original_receipt() {
    block_on(async {
        let fixture = fixture("append-lost").await;
        provision(&fixture, "append-lost").await;
        let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
        let store = EventlogRecordedStore::open(backend, fixture.authority.clone(), LIMITS)
            .await
            .expect("open");
        fixture
            .backend
            .append_fault(AppendFault::CommitThen(EventLogError::UnknownCommit));
        let append = request("append-lost");
        let recovered = store
            .operation(context("old"))
            .append(append.clone())
            .await
            .expect("semantic recovery");
        assert!(recovered.replayed());
        let replay = store
            .operation(context("fresh"))
            .append(append)
            .await
            .expect("fresh-context replay");
        assert!(replay.replayed());
        assert_eq!(replay.receipt(), recovered.receipt());
    });
}

#[test]
fn append_unknown_rollback_with_unavailable_recovery_retains_the_batch_key() {
    block_on(async {
        let fixture = fixture("append-rollback").await;
        provision(&fixture, "append-rollback").await;
        let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
        let store = EventlogRecordedStore::open(backend, fixture.authority.clone(), LIMITS)
            .await
            .expect("open");
        fixture
            .backend
            .append_fault(AppendFault::ReturnAndFailRecovery(
                EventLogError::UnknownCommit,
                CaptureError::Store(EventLogError::Backend("capture unavailable".into())),
            ));
        let append = request("append-rollback");
        let key = append.key.clone().expect("key");
        assert!(matches!(
            store.operation(context("rollback")).append(append).await,
            Err(WriteFailure::Uncertain { key: found, .. }) if found == key
        ));
    });
}

#[test]
fn append_retries_a_false_physical_conflict_but_refuses_a_malformed_commit_reply() {
    block_on(async {
        let fixture = fixture("append-boundaries").await;
        provision(&fixture, "append-boundaries").await;
        let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
        let store = EventlogRecordedStore::open(backend, fixture.authority.clone(), LIMITS)
            .await
            .expect("open");
        fixture
            .backend
            .append_fault(AppendFault::Return(EventLogError::Conflict {
                expected: 0,
                actual: 1,
            }));
        assert!(matches!(
            store
                .operation(context("conflict"))
                .append(request("false-conflict"))
                .await,
            Ok(AppendOutcome::Committed {
                replayed: false,
                ..
            })
        ));

        fixture
            .backend
            .append_fault(AppendFault::CommitThenMalformedResult);
        let malformed = request("malformed-result");
        let key = malformed.key.clone().expect("key");
        assert!(matches!(
            store
                .operation(context("malformed"))
                .append(malformed)
                .await,
            Err(WriteFailure::Uncertain { key: found, .. }) if found == key
        ));
    });
}

#[test]
fn append_never_trusts_a_guard_code_without_the_typed_transaction_slot() {
    block_on(async {
        let fixture = fixture("forged-guard").await;
        provision(&fixture, "forged-guard").await;
        let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
        let store = EventlogRecordedStore::open(backend, fixture.authority.clone(), LIMITS)
            .await
            .expect("open");
        for (index, code) in [
            "er_revision_conflict",
            "er_record_conflict",
            "er_batch_conflict",
            "er_previously_recorded_batch_entries",
            "er_corrupt_history",
            "er_provider_integrity",
            "foreign_guard_code",
        ]
        .into_iter()
        .enumerate()
        {
            fixture
                .backend
                .append_fault(AppendFault::Return(EventLogError::GuardRefused {
                    code: code.into(),
                }));
            assert!(matches!(
                store
                    .operation(context(code))
                    .append(request(&format!("forged-{index}")))
                    .await,
                Err(WriteFailure::NotCommitted(
                    entity_store::asynchronous::AsyncStoreError::ProviderIntegrity { .. }
                ))
            ));
        }
    });
}

#[test]
fn returned_backend_or_unknown_commit_wins_after_a_real_guard_refusal_rollback() {
    block_on(async {
        for (label, substitute) in [
            (
                "backend",
                EventLogError::Backend("substituted after guard rollback".into()),
            ),
            ("unknown", EventLogError::UnknownCommit),
        ] {
            let fixture = fixture(&format!("guard-return-{label}")).await;
            provision(&fixture, "guard-return").await;
            let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
            let store = EventlogRecordedStore::open(backend, fixture.authority.clone(), LIMITS)
                .await
                .expect("open");
            let gate = fixture.backend.order_next_two_appends("request-winner");
            fixture
                .backend
                .substitute_after_guard_refusal("request-loser", substitute);
            let winner = store.operation(context("winner"));
            let loser = store.operation(context("loser"));
            let losing_request = named_request_with_record("loser", "guard-shared-record");
            let losing_key = losing_request.key.clone().expect("losing key");
            let (winner, loser, ()) = tokio::join!(
                winner.append(named_request_with_record("winner", "guard-shared-record")),
                loser.append(losing_request),
                gate.drive()
            );
            assert!(winner.is_ok(), "real guard winner: {winner:?}");
            match label {
                "backend" => assert!(matches!(
                    loser,
                    Err(WriteFailure::NotCommitted(
                        entity_store::asynchronous::AsyncStoreError::Backend(detail)
                    )) if detail == "substituted after guard rollback"
                )),
                "unknown" => assert!(matches!(
                    loser,
                    Err(WriteFailure::Uncertain { key, .. }) if key == losing_key
                )),
                _ => unreachable!("closed test table"),
            }
        }
    });
}

#[test]
fn real_projector_failures_before_and_after_apply_roll_back_append_and_import() {
    block_on(async {
        for (label, timing, detail) in [
            (
                "before",
                ProjectorFaultTiming::Before,
                "controlled failure before real ER projector",
            ),
            (
                "after",
                ProjectorFaultTiming::After,
                "controlled failure after real ER projector",
            ),
        ] {
            let fixture = fixture(&format!("projector-{label}")).await;
            provision(&fixture, "projector-boundary").await;
            let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
            let store = EventlogRecordedStore::open(backend, fixture.authority.clone(), LIMITS)
                .await
                .expect("open");

            let append_label = format!("projector-append-{label}");
            let append = request(&append_label);
            let append_key = append.key.clone().expect("append key");
            let append_record_id = format!("record-{append_label}");
            let append_subject = Subject::new("ticket", &append_label).expect("append subject");
            fixture.projector.fail_next(timing);
            assert!(matches!(
                store
                    .operation(context(&format!("append-{label}")))
                    .append(append.clone())
                    .await,
                Err(WriteFailure::NotCommitted(
                    entity_store::asynchronous::AsyncStoreError::Backend(found)
                )) if found == detail
            ));
            assert_eq!(
                store
                    .lookup_record(&append_record_id)
                    .await
                    .expect("record lookup after projector rollback"),
                None
            );
            assert_eq!(
                store
                    .lookup_batch(&append_key)
                    .await
                    .expect("batch lookup after projector rollback"),
                None
            );
            assert_eq!(
                store
                    .load(&append_subject)
                    .await
                    .expect("subject lookup after projector rollback"),
                None
            );
            assert!(matches!(
                store
                    .operation(context(&format!("append-retry-{label}")))
                    .append(append)
                    .await,
                Ok(AppendOutcome::Committed {
                    replayed: false,
                    ..
                })
            ));

            let import_label = format!("projector-import-{label}");
            let history = imported_history(&import_label);
            let import_record_id = format!("legacy-record-{import_label}");
            fixture.projector.fail_next(timing);
            assert!(matches!(
                store
                    .operation(context(&format!("import-{label}")))
                    .import_anchor(history.clone())
                    .await,
                Err(ImportAnchorFailure::NotCommitted(
                    entity_store::asynchronous::AsyncStoreError::Backend(found)
                )) if found == detail
            ));
            assert_eq!(
                store
                    .lookup_record(&import_record_id)
                    .await
                    .expect("import lookup after projector rollback"),
                None
            );
            assert_eq!(
                store
                    .load(&history.subject)
                    .await
                    .expect("import subject lookup after projector rollback"),
                None
            );
            assert!(
                !store
                    .operation(context(&format!("import-retry-{label}")))
                    .import_anchor(history)
                    .await
                    .expect("fresh import after projector rollback")
                    .replayed
            );
        }
    });
}

#[test]
fn append_refuses_after_each_individual_blob_upload_boundary() {
    block_on(async {
        for ordinal in 1..=4 {
            let fixture = fixture(&format!("append-blob-{ordinal}")).await;
            provision(&fixture, "append-blob").await;
            let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
            let store = EventlogRecordedStore::open(backend, fixture.authority.clone(), LIMITS)
                .await
                .expect("open");
            fixture.backend.fail_after_put(
                ordinal,
                EventLogError::Backend(format!("lost after blob {ordinal}")),
            );
            assert!(matches!(
                store
                    .operation(context("blob"))
                    .append(request(&format!("blob-{ordinal}")))
                    .await,
                Err(WriteFailure::NotCommitted(
                    entity_store::asynchronous::AsyncStoreError::Backend(_)
                ))
            ));
        }
    });
}

#[test]
fn absent_global_record_race_is_atomic_in_both_winner_orders() {
    block_on(async {
        for first in ["request-left", "request-right"] {
            let fixture = fixture(&format!("record-race-{first}")).await;
            provision(&fixture, "record-race").await;
            let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
            let store = EventlogRecordedStore::open(backend, fixture.authority.clone(), LIMITS)
                .await
                .expect("open");
            let gate = fixture.backend.order_next_two_appends(first);
            let left = store.operation(context("left"));
            let right = store.operation(context("right"));
            let (left, right, ()) = tokio::join!(
                left.append(named_request_with_record("left", "shared-record")),
                right.append(named_request_with_record("right", "shared-record")),
                gate.drive()
            );
            if first == "request-left" {
                assert!(left.is_ok(), "left should win: {left:?}");
                assert!(
                    matches!(
                        &right,
                        Err(WriteFailure::NotCommitted(
                            entity_store::asynchronous::AsyncStoreError::PreviouslyRecordedBatchEntries {
                                indices
                            }
                        )) if indices == &vec![0]
                    ),
                    "right loser: {right:?}"
                );
            } else {
                assert!(right.is_ok(), "right should win: {right:?}");
                assert!(
                    matches!(
                        &left,
                        Err(WriteFailure::NotCommitted(
                            entity_store::asynchronous::AsyncStoreError::PreviouslyRecordedBatchEntries {
                                indices
                            }
                        )) if indices == &vec![0]
                    ),
                    "left loser: {left:?}"
                );
            }
        }
    });
}

#[test]
fn import_recovers_committed_lost_reply_and_restart_with_fresh_context() {
    block_on(async {
        let fixture = fixture("import-lost").await;
        provision(&fixture, "import-lost").await;
        let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
        let store = EventlogRecordedStore::open(backend, fixture.authority.clone(), LIMITS)
            .await
            .expect("open");
        fixture
            .backend
            .append_fault(AppendFault::CommitThen(EventLogError::UnknownCommit));
        let history = imported_history("import-lost");
        let recovered = store
            .operation(context("old"))
            .import_anchor(history.clone())
            .await
            .expect("semantic recovery");
        assert!(recovered.replayed);
        assert!(
            store
                .operation(context("fresh"))
                .import_anchor(history)
                .await
                .expect("fresh context recovery")
                .replayed
        );
    });
}

#[test]
fn import_unknown_rollback_and_malformed_reply_keep_subject_uncertainty() {
    block_on(async {
        let fixture = fixture("import-uncertain").await;
        provision(&fixture, "import-uncertain").await;
        let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
        let store = EventlogRecordedStore::open(backend, fixture.authority.clone(), LIMITS)
            .await
            .expect("open");
        let rollback = imported_history("import-rollback");
        fixture
            .backend
            .append_fault(AppendFault::ReturnAndFailRecovery(
                EventLogError::UnknownCommit,
                CaptureError::Store(EventLogError::Backend("capture unavailable".into())),
            ));
        assert!(matches!(
            store
                .operation(context("rollback"))
                .import_anchor(rollback.clone())
                .await,
            Err(ImportAnchorFailure::Uncertain {
                subject,
                cause: ImportAnchorUncertainty::UnknownCommit,
            }) if subject == rollback.subject
        ));

        let malformed = imported_history("import-malformed");
        fixture
            .backend
            .append_fault(AppendFault::CommitThenMalformedResult);
        assert!(matches!(
            store
                .operation(context("malformed"))
                .import_anchor(malformed.clone())
                .await,
            Err(ImportAnchorFailure::Uncertain {
                subject,
                cause: ImportAnchorUncertainty::RecoveryUnavailable,
            }) if subject == malformed.subject
        ));
    });
}

#[test]
fn import_refuses_after_each_individual_blob_upload_boundary() {
    block_on(async {
        for ordinal in 1..=2 {
            let fixture = fixture(&format!("import-blob-{ordinal}")).await;
            provision(&fixture, "import-blob").await;
            let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
            let store = EventlogRecordedStore::open(backend, fixture.authority.clone(), LIMITS)
                .await
                .expect("open");
            fixture.backend.fail_after_put(
                ordinal,
                EventLogError::Backend(format!("lost after import blob {ordinal}")),
            );
            assert!(matches!(
                store
                    .operation(context("import-blob"))
                    .import_anchor(imported_history(&format!("import-blob-{ordinal}")))
                    .await,
                Err(ImportAnchorFailure::NotCommitted(
                    entity_store::asynchronous::AsyncStoreError::Backend(_)
                ))
            ));
        }
    });
}

#[test]
fn ordinary_and_import_record_identity_race_is_atomic_in_both_winner_orders() {
    block_on(async {
        for first in ["request-ordinary", "request-import"] {
            let fixture = fixture(&format!("mixed-race-{first}")).await;
            provision(&fixture, "mixed-race").await;
            let backend: Arc<dyn EventlogBackend> = fixture.backend.clone();
            let store = EventlogRecordedStore::open(backend, fixture.authority.clone(), LIMITS)
                .await
                .expect("open");
            let gate = fixture.backend.order_next_two_appends(first);
            let ordinary = store.operation(context("ordinary"));
            let import = store.operation(context("import"));
            let (ordinary, import, ()) = tokio::join!(
                ordinary.append(request_with_record("ordinary", "mixed-record")),
                import.import_anchor(imported_history_with_record("imported", "mixed-record")),
                gate.drive()
            );
            if first == "request-ordinary" {
                assert!(ordinary.is_ok(), "ordinary should win: {ordinary:?}");
                assert!(matches!(
                    import,
                    Err(ImportAnchorFailure::NotCommitted(
                        entity_store::asynchronous::AsyncStoreError::RecordConflict { .. }
                    ))
                ));
            } else {
                assert!(import.is_ok(), "import should win: {import:?}");
                assert!(matches!(
                    ordinary,
                    Ok(AppendOutcome::Historical { .. })
                        | Err(WriteFailure::NotCommitted(
                            entity_store::asynchronous::AsyncStoreError::RecordConflict { .. }
                        ))
                ));
            }
        }
    });
}

// Individual group-entry persistence, command bookkeeping, and transaction commit/rollback remain
// provider-private boundaries. The public guard and projector callbacks above exercise their real
// provider transaction slots; the outer append wrapper separately proves pre-call failure,
// committed then lost reply, malformed reply, and recovery-capture failure.
