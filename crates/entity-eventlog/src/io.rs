//! The recorded mapping needs the same narrow IO surface inside and outside a transaction.
use std::sync::Arc;

use eventlog_core::{
    AppendGroup, AppendGroupResult, AtomicEventStore, BoxFuture, EventLogError, ProjectionPage,
    ProjectionQuery, ProjectionSpec, SnapshotGeneration, StreamId, StreamSlice, TenantId,
    TransactionSession,
};
use serde_json::Value;

pub(crate) trait StoreIo: Send {
    fn read_stream<'a>(
        &'a mut self,
        stream: &'a StreamId,
        after: u64,
        limit: usize,
    ) -> BoxFuture<'a, Result<StreamSlice, EventLogError>>;
    fn stream_version<'a>(
        &'a mut self,
        stream: &'a StreamId,
    ) -> BoxFuture<'a, Result<Option<u64>, EventLogError>>;
    fn snapshot_generation<'a>(
        &'a mut self,
        stream: &'a StreamId,
    ) -> BoxFuture<'a, Result<Option<SnapshotGeneration>, EventLogError>>;
    fn list_streams<'a>(
        &'a mut self,
        tenant: &'a TenantId,
        kind: &'a str,
        after: Option<&'a str>,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<StreamId>, EventLogError>>;
    fn append_group<'a>(
        &'a mut self,
        group: &'a AppendGroup,
    ) -> BoxFuture<'a, Result<AppendGroupResult, EventLogError>>;
    fn projection_get<'a>(
        &'a mut self,
        spec: &'a ProjectionSpec,
        tenant: &'a TenantId,
        key: &'a str,
    ) -> BoxFuture<'a, Result<Option<Value>, EventLogError>>;
    fn projection_query<'a>(
        &'a mut self,
        spec: &'a ProjectionSpec,
        tenant: &'a TenantId,
        query: &'a ProjectionQuery,
    ) -> BoxFuture<'a, Result<ProjectionPage, EventLogError>>;
}

impl<S: AtomicEventStore + ?Sized> StoreIo for Arc<S> {
    fn read_stream<'a>(
        &'a mut self,
        stream: &'a StreamId,
        after: u64,
        limit: usize,
    ) -> BoxFuture<'a, Result<StreamSlice, EventLogError>> {
        Arc::as_ref(self).read_stream(stream, after, limit)
    }
    fn stream_version<'a>(
        &'a mut self,
        stream: &'a StreamId,
    ) -> BoxFuture<'a, Result<Option<u64>, EventLogError>> {
        Arc::as_ref(self).stream_version(stream)
    }
    fn snapshot_generation<'a>(
        &'a mut self,
        stream: &'a StreamId,
    ) -> BoxFuture<'a, Result<Option<SnapshotGeneration>, EventLogError>> {
        Arc::as_ref(self).snapshot_generation(stream)
    }
    fn list_streams<'a>(
        &'a mut self,
        tenant: &'a TenantId,
        kind: &'a str,
        after: Option<&'a str>,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<StreamId>, EventLogError>> {
        Arc::as_ref(self).list_streams(tenant, kind, after, limit)
    }
    fn append_group<'a>(
        &'a mut self,
        group: &'a AppendGroup,
    ) -> BoxFuture<'a, Result<AppendGroupResult, EventLogError>> {
        Arc::as_ref(self).append_group(group)
    }
    fn projection_get<'a>(
        &'a mut self,
        spec: &'a ProjectionSpec,
        tenant: &'a TenantId,
        key: &'a str,
    ) -> BoxFuture<'a, Result<Option<Value>, EventLogError>> {
        Arc::as_ref(self).projection_get(spec, tenant, key)
    }
    fn projection_query<'a>(
        &'a mut self,
        spec: &'a ProjectionSpec,
        tenant: &'a TenantId,
        query: &'a ProjectionQuery,
    ) -> BoxFuture<'a, Result<ProjectionPage, EventLogError>> {
        Arc::as_ref(self).projection_query(spec, tenant, query)
    }
}

pub(crate) struct ScopedIo<'a>(pub(crate) &'a mut dyn TransactionSession);

impl StoreIo for ScopedIo<'_> {
    fn read_stream<'a>(
        &'a mut self,
        stream: &'a StreamId,
        after: u64,
        limit: usize,
    ) -> BoxFuture<'a, Result<StreamSlice, EventLogError>> {
        self.0.read_stream(stream, after, limit)
    }
    fn stream_version<'a>(
        &'a mut self,
        stream: &'a StreamId,
    ) -> BoxFuture<'a, Result<Option<u64>, EventLogError>> {
        self.0.stream_version(stream)
    }
    fn snapshot_generation<'a>(
        &'a mut self,
        stream: &'a StreamId,
    ) -> BoxFuture<'a, Result<Option<SnapshotGeneration>, EventLogError>> {
        self.0.snapshot_generation(stream)
    }
    fn list_streams<'a>(
        &'a mut self,
        tenant: &'a TenantId,
        kind: &'a str,
        after: Option<&'a str>,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<StreamId>, EventLogError>> {
        Box::pin(async move {
            if tenant != self.0.tenant() {
                return Err(EventLogError::Invalid(
                    "session inventory cannot cross tenant".into(),
                ));
            }
            self.0.list_streams(kind, after, limit).await
        })
    }
    fn append_group<'a>(
        &'a mut self,
        group: &'a AppendGroup,
    ) -> BoxFuture<'a, Result<AppendGroupResult, EventLogError>> {
        self.0.append_group(group)
    }
    fn projection_get<'a>(
        &'a mut self,
        spec: &'a ProjectionSpec,
        tenant: &'a TenantId,
        key: &'a str,
    ) -> BoxFuture<'a, Result<Option<Value>, EventLogError>> {
        self.0.projections().get(spec, tenant, key)
    }
    fn projection_query<'a>(
        &'a mut self,
        spec: &'a ProjectionSpec,
        tenant: &'a TenantId,
        query: &'a ProjectionQuery,
    ) -> BoxFuture<'a, Result<ProjectionPage, EventLogError>> {
        self.0.projections().query_documents(spec, tenant, query)
    }
}
