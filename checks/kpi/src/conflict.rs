//! E19: a provider that answers every physical append with `Conflict`, so the adapter's retry
//! path runs as often as it is willing to. It forwards every other call to a real File provider,
//! the way `crates/entity-eventlog/tests/import_batch_retry_admission.rs` wraps one.

use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
};

use entity_eventlog::{
    AsyncBindingProvisioner, Authority, ErRecordedProjector, EventlogBackend,
    EventlogBindingProvisioner, EventlogRecordedStore,
};
use entity_executor::Executor;
use eventlog_core::{
    AppendGroup, AppendGroupResult, AppendResult, AtomicEventStore, BoxFuture, CaptureError,
    CaptureLimits, CatchUpProgress, Claim, ClaimedCommand, CommandMeta, ConsistentTenantCapture,
    EventLogError, EventStore, Expected, FeedPage, Guard, InlineProjectionAdmin,
    InlineRebuildResult, NewEvent, ProjectionSpec, Projector, RecordedEvent, Snapshot, StreamId,
    StreamSlice, TenantCapture, TenantId,
};
use serde_json::Value;

use crate::fixture::{block_on, context, registry, COUNTER, LIMITS};
use crate::stores::create_request;

/// Attempts after which the injecting provider stops answering `Conflict` and fails instead, so a
/// retry loop without its own bound ends. A bounded adapter reads below it.
pub const CAP: u64 = 64;

struct Conflicting {
    inner: Arc<dyn EventlogBackend>,
    armed: AtomicBool,
    attempts: AtomicU64,
}

impl Conflicting {
    fn refuse(&self) -> Option<EventLogError> {
        if !self.armed.load(Ordering::Relaxed) {
            return None;
        }
        let attempt = self.attempts.fetch_add(1, Ordering::Relaxed) + 1;
        Some(if attempt >= CAP {
            EventLogError::Backend("er-kpi: conflict injection cap reached".into())
        } else {
            EventLogError::Conflict {
                expected: 0,
                actual: 1,
            }
        })
    }
}

/// Runs one fresh recorded create against the injecting provider; returns the append attempts.
pub fn attempts_under_conflict(directory: &Path) -> u64 {
    block_on(async {
        let tenant = TenantId::new("er-kpi-conflict").expect("tenant");
        let file = Arc::new(
            eventlog_file::FileEventStore::open(directory)
                .await
                .expect("file provider"),
        );
        let stream_identity = file.stream_identity(&tenant).await.expect("identity");
        let projector = Arc::new(ErRecordedProjector::new());
        file.create_projections(projector.clone())
            .await
            .expect("projections");
        file.attach_inline_existing(projector)
            .await
            .expect("attached");
        let authority = Authority {
            logical_scope: "er-kpi-conflict-scope".into(),
            tenant: tenant.as_str().to_owned(),
            stream_identity,
        };
        let inner: Arc<dyn EventlogBackend> = file;
        EventlogBindingProvisioner::new(inner.clone(), LIMITS)
            .provision_binding(authority.clone(), context("conflict"))
            .await
            .expect("binding");
        let conflicting = Arc::new(Conflicting {
            inner,
            armed: AtomicBool::new(false),
            attempts: AtomicU64::new(0),
        });
        let erased: Arc<dyn EventlogBackend> = conflicting.clone();
        let store = EventlogRecordedStore::open(erased, authority, LIMITS)
            .await
            .expect("bound store");
        let registry = registry(&[COUNTER]);
        let operation = store.operation(context("conflict-run"));
        conflicting.armed.store(true, Ordering::Relaxed);
        let outcome = Executor::new(&registry, &operation)
            .create(create_request("c000", "c000-create"))
            .await;
        assert!(
            outcome.is_err(),
            "every append conflicted, so the create cannot commit: {outcome:?}"
        );
        conflicting.attempts.load(Ordering::Relaxed)
    })
}

impl AtomicEventStore for Conflicting {
    fn append_group<'a>(
        &'a self,
        group: &'a AppendGroup,
    ) -> BoxFuture<'a, Result<AppendGroupResult, EventLogError>> {
        match self.refuse() {
            Some(error) => Box::pin(async move { Err(error) }),
            None => self.inner.append_group(group),
        }
    }
    fn append_group_guarded<'a>(
        &'a self,
        group: &'a AppendGroup,
        admission: Arc<dyn Guard>,
    ) -> BoxFuture<'a, Result<AppendGroupResult, EventLogError>> {
        match self.refuse() {
            Some(error) => Box::pin(async move { Err(error) }),
            None => self.inner.append_group_guarded(group, admission),
        }
    }
    fn append_group_guarded_with_blobs<'a>(
        &'a self,
        group: &'a AppendGroup,
        admission: Arc<dyn Guard>,
        blobs: &'a [(String, Vec<u8>)],
    ) -> BoxFuture<'a, Result<AppendGroupResult, EventLogError>> {
        match self.refuse() {
            Some(error) => Box::pin(async move { Err(error) }),
            None => self
                .inner
                .append_group_guarded_with_blobs(group, admission, blobs),
        }
    }
}

impl ConsistentTenantCapture for Conflicting {
    fn capture_tenant<'a>(
        &'a self,
        tenant: &'a TenantId,
        projections: &'a [ProjectionSpec],
        limits: CaptureLimits,
    ) -> BoxFuture<'a, Result<TenantCapture, CaptureError>> {
        self.inner.capture_tenant(tenant, projections, limits)
    }
}

impl InlineProjectionAdmin for Conflicting {
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

impl EventStore for Conflicting {
    fn append<'a>(
        &'a self,
        stream: &'a StreamId,
        expected: Expected,
        events: &'a [NewEvent],
        meta: &'a CommandMeta,
    ) -> BoxFuture<'a, Result<AppendResult, EventLogError>> {
        match self.refuse() {
            Some(error) => Box::pin(async move { Err(error) }),
            None => self.inner.append(stream, expected, events, meta),
        }
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
        match self.refuse() {
            Some(error) => Box::pin(async move { Err(error) }),
            None => self
                .inner
                .append_guarded(stream, expected, events, meta, guard),
        }
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
}
