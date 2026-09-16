//! Independent regression for binding recovery after an ambiguous append result.
#![cfg(feature = "sqlite")]

use std::sync::Arc;

use entity_eventlog::{
    AsyncBindingProvisioner, Authority, ErRecordedProjector, EventlogBackend,
    EventlogBindingProvisioner, EventlogOperationContext, ProvisionBindingFailure,
};
use eventlog_core::{
    AppendGroup, AppendGroupResult, AppendResult, AtomicEventStore, BoxFuture, CaptureError,
    CaptureLimits, CatchUpProgress, Claim, ClaimedCommand, CommandMeta, ConsistentTenantCapture,
    EventLogError, EventStore, Expected, FeedPage, Guard, InlineProjectionAdmin,
    InlineRebuildResult, NewEvent, ProjectionPage, ProjectionSpec, Projector, RecordedEvent,
    Snapshot, SnapshotGeneration, StreamId, StreamSlice, TenantCapture, TenantId,
};
use serde_json::Value;
use time::OffsetDateTime;

const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 8,
    max_blobs: 16,
    max_projection_rows: 16,
    max_payload_bytes: 64 * 1024,
};

struct ForeignWinnerOnAppend<B> {
    inner: Arc<B>,
    foreign: Authority,
    drop_binding_row: bool,
}

impl<B: EventlogBackend> EventStore for ForeignWinnerOnAppend<B> {
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

impl<B: EventlogBackend> AtomicEventStore for ForeignWinnerOnAppend<B> {
    fn append_group_guarded<'a>(
        &'a self,
        _group: &'a AppendGroup,
        _admission: Arc<dyn Guard>,
    ) -> BoxFuture<'a, Result<AppendGroupResult, EventLogError>> {
        Box::pin(async move {
            let inner: Arc<dyn EventlogBackend> = self.inner.clone();
            EventlogBindingProvisioner::new(inner, LIMITS)
                .provision_binding(self.foreign.clone(), context("foreign"))
                .await
                .expect("the competing authority wins conclusively");
            Err(EventLogError::UnknownCommit)
        })
    }
}

impl<B: EventlogBackend> ConsistentTenantCapture for ForeignWinnerOnAppend<B> {
    fn capture_tenant<'a>(
        &'a self,
        tenant: &'a TenantId,
        projections: &'a [ProjectionSpec],
        limits: CaptureLimits,
    ) -> BoxFuture<'a, Result<TenantCapture, CaptureError>> {
        Box::pin(async move {
            let mut capture = self
                .inner
                .capture_tenant(tenant, projections, limits)
                .await?;
            if self.drop_binding_row {
                capture.projections[0].rows.clear();
            }
            Ok(capture)
        })
    }
}

impl<B: EventlogBackend> InlineProjectionAdmin for ForeignWinnerOnAppend<B> {
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
        self.inner.rebuild_inline_projection(name, tenant)
    }
}

fn context(label: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "reviewer".into(),
        actor: "entity-eventlog-review".into(),
        request_id: format!("request-{label}"),
        trace_id: format!("trace-{label}"),
        causation_id: None,
        causation_depth: 0,
        occurred_at: OffsetDateTime::UNIX_EPOCH,
    }
}

#[test]
fn unknown_commit_recovery_preserves_a_conclusive_foreign_binding_conflict() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(async {
            let concrete = Arc::new(
                eventlog_sqlite::SqliteEventStore::in_memory("reviewer_binding_conflict")
                    .await
                    .expect("SQLite provider"),
            );
            let tenant = TenantId::new("reviewer-binding-conflict").expect("tenant");
            let stream_identity = concrete
                .stream_identity(&tenant)
                .await
                .expect("stream identity");
            let projector = Arc::new(ErRecordedProjector::new());
            concrete
                .create_projections(projector.clone())
                .await
                .expect("projection admission");
            concrete
                .attach_inline_existing(projector)
                .await
                .expect("projection attachment");

            let requested = Authority {
                logical_scope: "requested-scope".into(),
                tenant: tenant.as_str().into(),
                stream_identity: stream_identity.clone(),
            };
            let foreign = Authority {
                logical_scope: "foreign-winner-scope".into(),
                tenant: tenant.as_str().into(),
                stream_identity,
            };
            let backend: Arc<dyn EventlogBackend> = Arc::new(ForeignWinnerOnAppend {
                inner: concrete,
                foreign: foreign.clone(),
                drop_binding_row: false,
            });

            let result = EventlogBindingProvisioner::new(backend, LIMITS)
                .provision_binding(requested.clone(), context("requested"))
                .await;

            assert_eq!(
                result,
                Err(ProvisionBindingFailure::Conflict {
                    requested,
                    found: foreign,
                })
            );
        });
}

#[test]
fn a_foreign_binding_is_not_a_conflict_until_its_derived_row_is_validated() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(async {
            let concrete = Arc::new(
                eventlog_sqlite::SqliteEventStore::in_memory("reviewer_binding_row")
                    .await
                    .expect("SQLite provider"),
            );
            let tenant = TenantId::new("reviewer-binding-row").expect("tenant");
            let stream_identity = concrete
                .stream_identity(&tenant)
                .await
                .expect("stream identity");
            let projector = Arc::new(ErRecordedProjector::new());
            concrete
                .create_projections(projector.clone())
                .await
                .expect("projection admission");
            concrete
                .attach_inline_existing(projector)
                .await
                .expect("projection attachment");

            let foreign = Authority {
                logical_scope: "foreign-scope".into(),
                tenant: tenant.as_str().into(),
                stream_identity: stream_identity.clone(),
            };
            let concrete_backend: Arc<dyn EventlogBackend> = concrete.clone();
            EventlogBindingProvisioner::new(concrete_backend, LIMITS)
                .provision_binding(foreign.clone(), context("foreign-row"))
                .await
                .expect("foreign binding provisioned");

            let requested = Authority {
                logical_scope: "requested-scope".into(),
                tenant: tenant.as_str().into(),
                stream_identity,
            };
            let corrupted: Arc<dyn EventlogBackend> = Arc::new(ForeignWinnerOnAppend {
                inner: concrete,
                foreign,
                drop_binding_row: true,
            });
            let result = EventlogBindingProvisioner::new(corrupted, LIMITS)
                .recover_binding(requested)
                .await;

            assert!(
                matches!(
                    result,
                    Err(ProvisionBindingFailure::NotCommitted(
                        entity_store::asynchronous::AsyncStoreError::ProviderIntegrity { .. }
                    ))
                ),
                "a foreign event without its derived binding row is corrupt, not a valid conflict: {result:?}"
            );
        });
}
