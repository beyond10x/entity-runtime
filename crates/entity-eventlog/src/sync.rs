//! Explicit synchronous facade over the asynchronous recorded adapter and executor.

#[cfg(feature = "file")]
use std::path::PathBuf;
use std::{
    num::NonZeroU16,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU8, AtomicUsize, Ordering},
        mpsc::{Receiver, SyncSender, TrySendError, sync_channel},
    },
    thread::{JoinHandle, ThreadId},
    time::Instant,
};

use entity_core::{EntityInstance, Registry};
use entity_executor::{BatchAction, CreateRequest, ExecuteRequest, ExecutionError, Executor};
use entity_store::{
    RecordedObservation,
    asynchronous::{
        AppendOutcome, AppendRequest, AsyncRecordedReader, AsyncRecordedWriter, AsyncStateReader,
        AsyncStoreError, BatchKey, BoxFuture, CompleteStoreSnapshot, RecordLookup, StoredBatch,
        Subject, SubjectHistory, WriteFailure,
    },
};
use eventlog_core::InlineProjectionAdmin;

use crate::{
    AsyncBindingProvisioner, AsyncImportedAnchorWriter, Authority, ErRecordedProjector,
    EventlogBackend, EventlogBindingProvisioner, EventlogOperationContext, EventlogRecordedStore,
    ImportAnchorFailure, ImportAnchorOutcome, ImportAnchorUncertainty, ProvisionBindingFailure,
};

/// Finite bridge queue configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BridgeConfig {
    /// Maximum accepted requests waiting for provider dispatch.
    pub queue_capacity: NonZeroU16,
}

/// How long a synchronous caller waits.
#[derive(Debug, Clone, Copy)]
pub enum CallWait {
    /// Wait until the operation reaches a terminal result.
    Forever,
    /// Stop waiting at this monotonic deadline.
    Until(Instant),
}

/// Caller-selected logical authority for explicit provisioning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvisionAuthority {
    /// Entity Runtime logical scope, preserved byte for byte.
    pub logical_scope: String,
    /// Eventlog tenant identity.
    pub tenant: String,
    /// Optional exact existing provider generation; `None` accepts the generation minted now.
    pub expected_stream_identity: Option<String>,
}

/// Provider construction owned by the worker thread.
pub enum EventlogRecordedStoreOwner {
    #[cfg(feature = "file")]
    /// Durable file provider rooted at the selected directory.
    File {
        /// Existing provider root.
        path: PathBuf,
        /// Exact immutable ER/Eventlog binding.
        authority: Authority,
        /// Bounds for every authoritative capture.
        limits: eventlog_core::CaptureLimits,
    },
    #[cfg(feature = "sqlite")]
    /// SQLite provider at a path and owner prefix.
    Sqlite {
        /// Existing SQLite database path.
        path: String,
        /// Admitted Eventlog owner prefix.
        prefix: String,
        /// Exact immutable ER/Eventlog binding.
        authority: Authority,
        /// Bounds for every authoritative capture.
        limits: eventlog_core::CaptureLimits,
    },
    #[cfg(feature = "sqlite")]
    /// Process-local SQLite provider for tests.
    SqliteMemory {
        /// Admitted Eventlog owner prefix.
        prefix: String,
        /// Exact immutable ER/Eventlog binding.
        authority: Authority,
        /// Bounds for every authoritative capture.
        limits: eventlog_core::CaptureLimits,
    },
    #[cfg(feature = "postgres")]
    /// Isolated loopback PostgreSQL provider for explicit test fixtures.
    PostgresLocal {
        /// Explicit loopback fixture URL.
        url: String,
        /// Isolated Eventlog owner prefix.
        prefix: String,
        /// Exact immutable ER/Eventlog binding.
        authority: Authority,
        /// Bounds for every authoritative capture.
        limits: eventlog_core::CaptureLimits,
    },
    #[cfg(feature = "postgres")]
    /// Hosted PostgreSQL provider using the caller's exact transport authority and pool bounds.
    Postgres {
        /// Caller-selected Eventlog connection authority.
        config: eventlog_postgres::PostgresConfig,
        /// Finite process-local pool bounds.
        options: eventlog_postgres::PoolOptions,
        /// Observed database-wide connection capacity.
        database_connections: usize,
        /// Admitted application replica count.
        replicas: usize,
        /// Connections retained for administration and recovery.
        reserved_connections: usize,
        /// Exact immutable ER/Eventlog binding.
        authority: Authority,
        /// Bounds for every authoritative capture.
        limits: eventlog_core::CaptureLimits,
    },
}

/// Explicit native preparation plus immutable binding establishment before ordinary open.
pub enum EventlogRecordedStoreProvisioner {
    #[cfg(feature = "file")]
    /// Prepare one durable File authority.
    File {
        /// Destination provider root.
        path: PathBuf,
        /// Exact immutable ER/Eventlog binding.
        authority: ProvisionAuthority,
        /// Bounds for every authoritative capture.
        limits: eventlog_core::CaptureLimits,
    },
    #[cfg(feature = "sqlite")]
    /// Prepare one file-backed SQLite authority.
    Sqlite {
        /// Destination SQLite path.
        path: String,
        /// Admitted Eventlog owner prefix.
        prefix: String,
        /// Exact immutable ER/Eventlog binding.
        authority: ProvisionAuthority,
        /// Bounds for every authoritative capture.
        limits: eventlog_core::CaptureLimits,
    },
    #[cfg(feature = "sqlite")]
    /// Prepare one process-local SQLite authority.
    SqliteMemory {
        /// Admitted Eventlog owner prefix.
        prefix: String,
        /// Exact immutable ER/Eventlog binding.
        authority: ProvisionAuthority,
        /// Bounds for every authoritative capture.
        limits: eventlog_core::CaptureLimits,
    },
    #[cfg(feature = "postgres")]
    /// Prepare an isolated loopback PostgreSQL fixture.
    PostgresLocal {
        /// Explicit loopback fixture URL.
        url: String,
        /// Isolated Eventlog owner prefix.
        prefix: String,
        /// Exact immutable ER/Eventlog binding.
        authority: ProvisionAuthority,
        /// Bounds for every authoritative capture.
        limits: eventlog_core::CaptureLimits,
    },
    #[cfg(feature = "postgres")]
    /// Migrate with an administrative authority, then open with a separate application authority.
    Postgres {
        /// Caller-owned migration-role transport authority.
        migration: eventlog_postgres::PostgresConfig,
        /// Caller-owned DML-only application transport authority.
        application: eventlog_postgres::PostgresConfig,
        /// Finite process-local pool bounds used by both phases.
        options: Box<eventlog_postgres::PoolOptions>,
        /// Observed database-wide connection capacity.
        database_connections: usize,
        /// Admitted application replica count.
        replicas: usize,
        /// Connections retained for administration and recovery.
        reserved_connections: usize,
        /// Exact immutable ER/Eventlog binding.
        authority: ProvisionAuthority,
        /// Bounds for every authoritative capture.
        limits: eventlog_core::CaptureLimits,
    },
}

impl std::fmt::Debug for EventlogRecordedStoreOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EventlogRecordedStoreOwner(..)")
    }
}

impl std::fmt::Debug for EventlogRecordedStoreProvisioner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EventlogRecordedStoreProvisioner(..)")
    }
}

enum OwnedBackend {
    #[cfg(feature = "file")]
    File(Arc<eventlog_file::FileEventStore>),
    #[cfg(feature = "sqlite")]
    Sqlite(Arc<eventlog_sqlite::SqliteEventStore>),
    #[cfg(feature = "postgres")]
    Postgres(Arc<eventlog_postgres::PostgresEventStore>),
}

impl EventlogRecordedStoreOwner {
    /// Exact logical and physical authority this worker owner will open.
    #[must_use]
    pub fn authority(&self) -> &Authority {
        match self {
            #[cfg(feature = "file")]
            Self::File { authority, .. } => authority,
            #[cfg(feature = "sqlite")]
            Self::Sqlite { authority, .. } | Self::SqliteMemory { authority, .. } => authority,
            #[cfg(feature = "postgres")]
            Self::PostgresLocal { authority, .. } | Self::Postgres { authority, .. } => authority,
        }
    }

    async fn open(self) -> Result<(EventlogRecordedStore, OwnedBackend), AsyncStoreError> {
        match self {
            #[cfg(feature = "file")]
            Self::File {
                path,
                authority,
                limits,
            } => {
                let concrete = Arc::new(
                    eventlog_file::FileEventStore::open_existing(path)
                        .await
                        .map_err(store_open)?,
                );
                concrete
                    .attach_inline_existing(Arc::new(ErRecordedProjector::new()))
                    .await
                    .map_err(store_open)?;
                let backend: Arc<dyn EventlogBackend> = concrete.clone();
                Ok((
                    EventlogRecordedStore::open(backend, authority, limits).await?,
                    OwnedBackend::File(concrete),
                ))
            }
            #[cfg(feature = "sqlite")]
            Self::Sqlite {
                path,
                prefix,
                authority,
                limits,
            } => {
                let concrete = Arc::new(
                    eventlog_sqlite::SqliteEventStore::open_existing(&path, &prefix)
                        .await
                        .map_err(store_open)?,
                );
                concrete
                    .attach_inline_existing(Arc::new(ErRecordedProjector::new()))
                    .await
                    .map_err(store_open)?;
                let backend: Arc<dyn EventlogBackend> = concrete.clone();
                Ok((
                    EventlogRecordedStore::open(backend, authority, limits).await?,
                    OwnedBackend::Sqlite(concrete),
                ))
            }
            #[cfg(feature = "sqlite")]
            Self::SqliteMemory {
                prefix,
                authority,
                limits,
            } => {
                let concrete = Arc::new(
                    eventlog_sqlite::SqliteEventStore::in_memory(&prefix)
                        .await
                        .map_err(store_open)?,
                );
                concrete
                    .attach_inline_existing(Arc::new(ErRecordedProjector::new()))
                    .await
                    .map_err(store_open)?;
                let backend: Arc<dyn EventlogBackend> = concrete.clone();
                Ok((
                    EventlogRecordedStore::open(backend, authority, limits).await?,
                    OwnedBackend::Sqlite(concrete),
                ))
            }
            #[cfg(feature = "postgres")]
            Self::PostgresLocal {
                url,
                prefix,
                authority,
                limits,
            } => {
                let concrete = Arc::new(
                    eventlog_postgres::PostgresEventStore::connect_local(
                        &url,
                        &prefix,
                        eventlog_postgres::PoolOptions::default(),
                    )
                    .await
                    .map_err(store_open)?,
                );
                concrete
                    .attach_inline_existing(Arc::new(ErRecordedProjector::new()))
                    .await
                    .map_err(store_open)?;
                let backend: Arc<dyn EventlogBackend> = concrete.clone();
                Ok((
                    EventlogRecordedStore::open(backend, authority, limits).await?,
                    OwnedBackend::Postgres(concrete),
                ))
            }
            #[cfg(feature = "postgres")]
            Self::Postgres {
                config,
                options,
                database_connections,
                replicas,
                reserved_connections,
                authority,
                limits,
            } => {
                let concrete = Arc::new(
                    eventlog_postgres::PostgresEventStore::open(
                        config,
                        options,
                        database_connections,
                        replicas,
                        reserved_connections,
                    )
                    .await
                    .map_err(store_open)?,
                );
                concrete
                    .attach_inline_existing(Arc::new(ErRecordedProjector::new()))
                    .await
                    .map_err(store_open)?;
                let backend: Arc<dyn EventlogBackend> = concrete.clone();
                Ok((
                    EventlogRecordedStore::open(backend, authority, limits).await?,
                    OwnedBackend::Postgres(concrete),
                ))
            }
        }
    }
}

impl EventlogRecordedStoreProvisioner {
    /// Exact logical scope this preparation will establish.
    #[must_use]
    pub fn logical_scope(&self) -> &str {
        match self {
            #[cfg(feature = "file")]
            Self::File { authority, .. } => &authority.logical_scope,
            #[cfg(feature = "sqlite")]
            Self::Sqlite { authority, .. } | Self::SqliteMemory { authority, .. } => {
                &authority.logical_scope
            }
            #[cfg(feature = "postgres")]
            Self::PostgresLocal { authority, .. } | Self::Postgres { authority, .. } => {
                &authority.logical_scope
            }
        }
    }

    async fn provision(
        self,
        context: EventlogOperationContext,
    ) -> Result<(EventlogRecordedStore, OwnedBackend), BridgeStartError> {
        match self {
            #[cfg(feature = "file")]
            Self::File {
                path,
                authority,
                limits,
            } => {
                let concrete = Arc::new(
                    eventlog_file::FileEventStore::open(path)
                        .await
                        .map_err(|error| BridgeStartError::Open(store_open(error)))?,
                );
                let store = provision_backend(concrete.clone(), authority, limits, context).await?;
                Ok((store, OwnedBackend::File(concrete)))
            }
            #[cfg(feature = "sqlite")]
            Self::Sqlite {
                path,
                prefix,
                authority,
                limits,
            } => {
                let concrete = Arc::new(
                    eventlog_sqlite::SqliteEventStore::open(&path, &prefix)
                        .await
                        .map_err(|error| BridgeStartError::Open(store_open(error)))?,
                );
                let store = provision_backend(concrete.clone(), authority, limits, context).await?;
                Ok((store, OwnedBackend::Sqlite(concrete)))
            }
            #[cfg(feature = "sqlite")]
            Self::SqliteMemory {
                prefix,
                authority,
                limits,
            } => {
                let concrete = Arc::new(
                    eventlog_sqlite::SqliteEventStore::in_memory(&prefix)
                        .await
                        .map_err(|error| BridgeStartError::Open(store_open(error)))?,
                );
                let store = provision_backend(concrete.clone(), authority, limits, context).await?;
                Ok((store, OwnedBackend::Sqlite(concrete)))
            }
            #[cfg(feature = "postgres")]
            Self::PostgresLocal {
                url,
                prefix,
                authority,
                limits,
            } => {
                let concrete = Arc::new(
                    eventlog_postgres::PostgresEventStore::connect_local(
                        &url,
                        &prefix,
                        eventlog_postgres::PoolOptions::default(),
                    )
                    .await
                    .map_err(|error| BridgeStartError::Open(store_open(error)))?,
                );
                let store = provision_backend(concrete.clone(), authority, limits, context).await?;
                Ok((store, OwnedBackend::Postgres(concrete)))
            }
            #[cfg(feature = "postgres")]
            Self::Postgres {
                migration,
                application,
                options,
                database_connections,
                replicas,
                reserved_connections,
                authority,
                limits,
            } => {
                let options = *options;
                eventlog_postgres::PostgresEventStore::migrate(
                    migration,
                    options.clone(),
                    crate::projection_specs(),
                )
                .await
                .map_err(|error| BridgeStartError::Open(store_open(error)))?;
                let concrete = Arc::new(
                    eventlog_postgres::PostgresEventStore::open(
                        application,
                        options,
                        database_connections,
                        replicas,
                        reserved_connections,
                    )
                    .await
                    .map_err(|error| BridgeStartError::Open(store_open(error)))?,
                );
                concrete
                    .attach_inline_existing(Arc::new(ErRecordedProjector::new()))
                    .await
                    .map_err(|error| BridgeStartError::Open(store_open(error)))?;
                let backend: Arc<dyn EventlogBackend> = concrete.clone();
                let store = provision_binding(backend, authority, limits, context).await?;
                Ok((store, OwnedBackend::Postgres(concrete)))
            }
        }
    }
}

async fn provision_backend<B: EventlogBackend>(
    concrete: Arc<B>,
    authority: ProvisionAuthority,
    limits: eventlog_core::CaptureLimits,
    context: EventlogOperationContext,
) -> Result<EventlogRecordedStore, BridgeStartError> {
    let projector = Arc::new(ErRecordedProjector::new());
    concrete
        .create_projections(projector.clone())
        .await
        .map_err(|error| BridgeStartError::Open(store_open(error)))?;
    concrete
        .attach_inline_existing(projector)
        .await
        .map_err(|error| BridgeStartError::Open(store_open(error)))?;
    let backend: Arc<dyn EventlogBackend> = concrete;
    provision_binding(backend, authority, limits, context).await
}

async fn provision_binding(
    backend: Arc<dyn EventlogBackend>,
    selection: ProvisionAuthority,
    limits: eventlog_core::CaptureLimits,
    context: EventlogOperationContext,
) -> Result<EventlogRecordedStore, BridgeStartError> {
    let tenant = eventlog_core::TenantId::new(selection.tenant.clone())
        .map_err(|error| BridgeStartError::Open(store_open(error)))?;
    let stream_identity = backend
        .stream_identity(&tenant)
        .await
        .map_err(|error| BridgeStartError::Open(store_open(error)))?;
    if selection
        .expected_stream_identity
        .as_ref()
        .is_some_and(|expected| expected != &stream_identity)
    {
        return Err(BridgeStartError::Open(AsyncStoreError::ProviderIntegrity {
            provider: "eventlog".to_owned(),
            detail: "provisioned provider generation differs from the caller's expected generation"
                .to_owned(),
        }));
    }
    let authority = Authority {
        logical_scope: selection.logical_scope,
        tenant: selection.tenant,
        stream_identity,
    };
    EventlogBindingProvisioner::new(backend.clone(), limits)
        .provision_binding(authority.clone(), context)
        .await
        .map_err(|error| BridgeStartError::Provision(Box::new(error)))?;
    EventlogRecordedStore::open(backend, authority, limits)
        .await
        .map_err(BridgeStartError::Open)
}

impl OwnedBackend {
    async fn retire(&self) -> Result<(), AsyncStoreError> {
        match self {
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.shutdown().await.map_err(store_open),
            #[cfg(feature = "file")]
            Self::File(store) => {
                let _ = Arc::strong_count(store);
                Ok(())
            }
            #[cfg(feature = "sqlite")]
            Self::Sqlite(store) => {
                let _ = Arc::strong_count(store);
                Ok(())
            }
        }
    }
}

trait WorkerDriver: Send {
    fn load<'a>(
        &'a self,
        subject: &'a Subject,
    ) -> BoxFuture<'a, Result<Option<EntityInstance>, AsyncStoreError>>;
    fn lookup_record<'a>(
        &'a self,
        id: &'a str,
    ) -> BoxFuture<'a, Result<Option<RecordLookup>, AsyncStoreError>>;
    fn lookup_batch<'a>(
        &'a self,
        key: &'a BatchKey,
    ) -> BoxFuture<'a, Result<Option<StoredBatch>, AsyncStoreError>>;
    fn history<'a>(
        &'a self,
        subject: &'a Subject,
    ) -> BoxFuture<'a, Result<SubjectHistory, AsyncStoreError>>;
    fn complete_snapshot<'a>(
        &'a self,
        scope: &'a str,
    ) -> BoxFuture<'a, Result<CompleteStoreSnapshot, AsyncStoreError>>;
    fn append<'a>(
        &'a self,
        context: EventlogOperationContext,
        request: AppendRequest,
    ) -> BoxFuture<'a, Result<AppendOutcome, WriteFailure>>;
    fn create<'a>(
        &'a self,
        context: EventlogOperationContext,
        request: CreateRequest,
    ) -> BoxFuture<'a, Result<AppendOutcome, ExecutionError>>;
    fn execute<'a>(
        &'a self,
        context: EventlogOperationContext,
        request: ExecuteRequest,
    ) -> BoxFuture<'a, Result<AppendOutcome, ExecutionError>>;
    fn observe<'a>(
        &'a self,
        context: EventlogOperationContext,
        request: RecordedObservation,
    ) -> BoxFuture<'a, Result<AppendOutcome, ExecutionError>>;
    fn batch<'a>(
        &'a self,
        context: EventlogOperationContext,
        key: BatchKey,
        actions: Vec<BatchAction>,
    ) -> BoxFuture<'a, Result<AppendOutcome, ExecutionError>>;
    fn import_anchor<'a>(
        &'a self,
        context: EventlogOperationContext,
        history: SubjectHistory,
        source_id: Option<String>,
    ) -> BoxFuture<'a, Result<ImportAnchorOutcome, ImportAnchorFailure>>;
    fn retire(&self) -> BoxFuture<'_, Result<(), AsyncStoreError>>;
}

struct ProductionDriver {
    registry: Registry,
    store: EventlogRecordedStore,
    backend: OwnedBackend,
}

impl WorkerDriver for ProductionDriver {
    fn load<'a>(
        &'a self,
        subject: &'a Subject,
    ) -> BoxFuture<'a, Result<Option<EntityInstance>, AsyncStoreError>> {
        self.store.load(subject)
    }

    fn lookup_record<'a>(
        &'a self,
        id: &'a str,
    ) -> BoxFuture<'a, Result<Option<RecordLookup>, AsyncStoreError>> {
        self.store.lookup_record(id)
    }

    fn lookup_batch<'a>(
        &'a self,
        key: &'a BatchKey,
    ) -> BoxFuture<'a, Result<Option<StoredBatch>, AsyncStoreError>> {
        self.store.lookup_batch(key)
    }

    fn history<'a>(
        &'a self,
        subject: &'a Subject,
    ) -> BoxFuture<'a, Result<SubjectHistory, AsyncStoreError>> {
        self.store.history(subject)
    }

    fn complete_snapshot<'a>(
        &'a self,
        scope: &'a str,
    ) -> BoxFuture<'a, Result<CompleteStoreSnapshot, AsyncStoreError>> {
        self.store.complete_snapshot(scope)
    }

    fn append<'a>(
        &'a self,
        context: EventlogOperationContext,
        request: AppendRequest,
    ) -> BoxFuture<'a, Result<AppendOutcome, WriteFailure>> {
        Box::pin(async move { self.store.operation(context).append(request).await })
    }

    fn create<'a>(
        &'a self,
        context: EventlogOperationContext,
        request: CreateRequest,
    ) -> BoxFuture<'a, Result<AppendOutcome, ExecutionError>> {
        Box::pin(async move {
            let operation = self.store.operation(context);
            Executor::new(&self.registry, &operation)
                .create(request)
                .await
        })
    }

    fn execute<'a>(
        &'a self,
        context: EventlogOperationContext,
        request: ExecuteRequest,
    ) -> BoxFuture<'a, Result<AppendOutcome, ExecutionError>> {
        Box::pin(async move {
            let operation = self.store.operation(context);
            Executor::new(&self.registry, &operation)
                .execute(request)
                .await
        })
    }

    fn observe<'a>(
        &'a self,
        context: EventlogOperationContext,
        request: RecordedObservation,
    ) -> BoxFuture<'a, Result<AppendOutcome, ExecutionError>> {
        Box::pin(async move {
            let operation = self.store.operation(context);
            Executor::new(&self.registry, &operation)
                .observe(request)
                .await
        })
    }

    fn batch<'a>(
        &'a self,
        context: EventlogOperationContext,
        key: BatchKey,
        actions: Vec<BatchAction>,
    ) -> BoxFuture<'a, Result<AppendOutcome, ExecutionError>> {
        Box::pin(async move {
            let operation = self.store.operation(context);
            Executor::new(&self.registry, &operation)
                .batch(key, actions)
                .await
        })
    }

    fn import_anchor<'a>(
        &'a self,
        context: EventlogOperationContext,
        history: SubjectHistory,
        source_id: Option<String>,
    ) -> BoxFuture<'a, Result<ImportAnchorOutcome, ImportAnchorFailure>> {
        Box::pin(async move {
            let operation = self.store.operation(context);
            match source_id {
                Some(source_id) => operation.import_source_anchor(source_id, history).await,
                None => operation.import_anchor(history).await,
            }
        })
    }

    fn retire(&self) -> BoxFuture<'_, Result<(), AsyncStoreError>> {
        Box::pin(async move { self.backend.retire().await })
    }
}

/// Startup refusal before a usable owner is returned.
#[derive(Debug)]
pub enum BridgeStartError {
    /// The operating system refused the worker thread.
    ThreadSpawn(std::io::Error),
    /// Tokio could not construct the owned runtime.
    RuntimeBuild(std::io::Error),
    /// The worker could not open and verify its provider.
    Open(AsyncStoreError),
    /// Explicit provisioning failed before a usable owner existed.
    Provision(Box<ProvisionBindingFailure>),
    /// The worker panicked before completing the ownership handshake.
    WorkerPanicked,
}

/// Definite bridge-local rejection before provider dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeRejection {
    /// The supplied deadline had already elapsed before admission.
    DeadlineBeforeAcceptance,
    /// A queued call was cancelled before provider dispatch.
    DeadlineBeforeDispatch,
    /// The bounded queue had no available slot.
    QueueFull {
        /// Configured request capacity.
        capacity: u16,
    },
    /// The bridge no longer accepts work.
    Closed,
    /// The provider-owning worker tried to call its own facade.
    Reentrant,
    /// The worker stopped while this request was still queued.
    WorkerStoppedBeforeDispatch,
}

/// Lost read result after provider dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeAfterDispatch {
    /// The caller's deadline elapsed after provider dispatch.
    Deadline,
    /// The worker panicked while polling the operation.
    WorkerPanicked,
    /// The worker stopped without returning the dispatched read result.
    WorkerStopped,
}

/// Synchronous read failure.
#[derive(Debug)]
pub enum SyncReadError {
    /// The bridge rejected the read before provider dispatch.
    Rejected(BridgeRejection),
    /// The provider completed the read with a store error.
    Store(AsyncStoreError),
    /// A dispatched read lost its result.
    AfterDispatch(BridgeAfterDispatch),
}

/// Synchronous direct-write failure.
#[derive(Debug)]
pub enum SyncWriteError {
    /// The bridge rejected the write before provider dispatch.
    Rejected(BridgeRejection),
    /// The provider or deadline produced the recorded-write result.
    Write(WriteFailure),
}

/// Synchronous executor failure.
#[derive(Debug)]
pub enum SyncExecutionError {
    /// The bridge rejected execution before provider dispatch.
    Rejected(BridgeRejection),
    /// The existing executor produced the result.
    Execution(ExecutionError),
}

/// Synchronous explicit-import failure.
#[derive(Debug)]
pub enum SyncImportError {
    /// The bridge rejected import before provider dispatch.
    Rejected(BridgeRejection),
    /// The imported-anchor writer produced the settled or uncertain result.
    Import(ImportAnchorFailure),
}

/// Queue handling on shutdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownMode {
    /// Finish every already accepted request before retirement.
    Drain,
    /// Cancel requests that have not reached provider dispatch.
    CancelQueued,
}

/// Identity of the one currently dispatched operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeOperationIdentity {
    /// One dispatched read kind.
    Read(BridgeReadKind),
    /// One dispatched write's semantic batch key.
    Write(BatchKey),
    /// One imported subject boundary.
    Import(Subject),
}

/// Closed read operation names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeReadKind {
    /// Current-state lookup.
    Load,
    /// Global record lookup.
    LookupRecord,
    /// Batch lookup.
    LookupBatch,
    /// Subject history lookup.
    History,
    /// Complete scoped snapshot.
    CompleteSnapshot,
}

/// Actual shutdown state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShutdownOutcome {
    /// Worker joined after provider retirement completed.
    Joined {
        /// Actual provider retirement result.
        provider: Result<(), AsyncStoreError>,
    },
    /// The wait expired while the bridge retained join ownership.
    TimedOut {
        /// Requests still queued before dispatch.
        queued: usize,
        /// Currently dispatched operation, when one exists.
        dispatched: Option<BridgeOperationIdentity>,
    },
    /// Shutdown was invoked from the provider-owning worker.
    Reentrant,
}

const RUNNING: u8 = 0;
const CLOSING_DRAIN: u8 = 1;
const CLOSING_CANCEL: u8 = 2;
const JOINED: u8 = 3;
const QUEUED: u8 = 0;
const CANCELLED: u8 = 1;
const DISPATCHED: u8 = 2;
const COMPLETED: u8 = 3;

struct Shared {
    lifecycle: AtomicU8,
    admission: Mutex<()>,
    queued: AtomicUsize,
    dispatched: Mutex<Option<BridgeOperationIdentity>>,
    finished: (Mutex<bool>, Condvar),
    provider: Mutex<Option<Result<(), AsyncStoreError>>>,
    #[cfg(test)]
    before_dispatch: Mutex<std::collections::VecDeque<Box<dyn FnOnce() + Send>>>,
    #[cfg(test)]
    after_dispatch: Mutex<std::collections::VecDeque<Box<dyn FnOnce() + Send>>>,
    #[cfg(test)]
    after_admission_check: Mutex<std::collections::VecDeque<Box<dyn FnOnce() + Send>>>,
    #[cfg(test)]
    after_enqueue: Mutex<std::collections::VecDeque<Box<dyn FnOnce() + Send>>>,
    #[cfg(test)]
    before_terminal_admission: Mutex<std::collections::VecDeque<Box<dyn FnOnce() + Send>>>,
    #[cfg(test)]
    after_terminal_drain: Mutex<std::collections::VecDeque<Box<dyn FnOnce() + Send>>>,
}

struct Cell<T> {
    phase: AtomicU8,
    result: Mutex<Option<T>>,
    wake: Condvar,
}
impl<T> Cell<T> {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            phase: AtomicU8::new(QUEUED),
            result: Mutex::new(None),
            wake: Condvar::new(),
        })
    }
    fn complete(&self, value: T) {
        *self.result.lock().expect("result cell") = Some(value);
        self.phase.store(COMPLETED, Ordering::Release);
        self.wake.notify_all();
    }
}

enum Request {
    Load(
        Subject,
        Arc<Cell<Result<Option<EntityInstance>, SyncReadError>>>,
    ),
    LookupRecord(
        String,
        Arc<Cell<Result<Option<RecordLookup>, SyncReadError>>>,
    ),
    LookupBatch(
        BatchKey,
        Arc<Cell<Result<Option<StoredBatch>, SyncReadError>>>,
    ),
    History(Subject, Arc<Cell<Result<SubjectHistory, SyncReadError>>>),
    Snapshot(
        String,
        Arc<Cell<Result<CompleteStoreSnapshot, SyncReadError>>>,
    ),
    Append(
        EventlogOperationContext,
        AppendRequest,
        BatchKey,
        Arc<Cell<Result<AppendOutcome, SyncWriteError>>>,
    ),
    Create(
        EventlogOperationContext,
        CreateRequest,
        BatchKey,
        Arc<Cell<Result<AppendOutcome, SyncExecutionError>>>,
    ),
    Execute(
        EventlogOperationContext,
        ExecuteRequest,
        BatchKey,
        Arc<Cell<Result<AppendOutcome, SyncExecutionError>>>,
    ),
    Observe(
        EventlogOperationContext,
        RecordedObservation,
        BatchKey,
        Arc<Cell<Result<AppendOutcome, SyncExecutionError>>>,
    ),
    Batch(
        EventlogOperationContext,
        BatchKey,
        Vec<BatchAction>,
        Arc<Cell<Result<AppendOutcome, SyncExecutionError>>>,
    ),
    Import(
        EventlogOperationContext,
        SubjectHistory,
        Subject,
        Arc<Cell<Result<ImportAnchorOutcome, SyncImportError>>>,
        Option<String>,
    ),
    Wake,
}

impl Request {
    fn phase(&self) -> Option<&AtomicU8> {
        match self {
            Self::Load(_, c) => Some(&c.phase),
            Self::LookupRecord(_, c) => Some(&c.phase),
            Self::LookupBatch(_, c) => Some(&c.phase),
            Self::History(_, c) => Some(&c.phase),
            Self::Snapshot(_, c) => Some(&c.phase),
            Self::Append(_, _, _, c) => Some(&c.phase),
            Self::Create(_, _, _, c) => Some(&c.phase),
            Self::Execute(_, _, _, c) => Some(&c.phase),
            Self::Observe(_, _, _, c) => Some(&c.phase),
            Self::Batch(_, _, _, c) => Some(&c.phase),
            Self::Import(_, _, _, c, _) => Some(&c.phase),
            Self::Wake => None,
        }
    }
    fn identity(&self) -> Option<BridgeOperationIdentity> {
        match self {
            Self::Load(..) => Some(BridgeOperationIdentity::Read(BridgeReadKind::Load)),
            Self::LookupRecord(..) => {
                Some(BridgeOperationIdentity::Read(BridgeReadKind::LookupRecord))
            }
            Self::LookupBatch(..) => {
                Some(BridgeOperationIdentity::Read(BridgeReadKind::LookupBatch))
            }
            Self::History(..) => Some(BridgeOperationIdentity::Read(BridgeReadKind::History)),
            Self::Snapshot(..) => Some(BridgeOperationIdentity::Read(
                BridgeReadKind::CompleteSnapshot,
            )),
            Self::Append(_, _, k, _)
            | Self::Create(_, _, k, _)
            | Self::Execute(_, _, k, _)
            | Self::Observe(_, _, k, _)
            | Self::Batch(_, k, _, _) => Some(BridgeOperationIdentity::Write(k.clone())),
            Self::Import(_, _, subject, _, _) => {
                Some(BridgeOperationIdentity::Import(subject.clone()))
            }
            Self::Wake => None,
        }
    }
    fn cancel_closed(self) {
        match self {
            Self::Load(_, c) => c.complete(Err(SyncReadError::Rejected(BridgeRejection::Closed))),
            Self::LookupRecord(_, c) => {
                c.complete(Err(SyncReadError::Rejected(BridgeRejection::Closed)))
            }
            Self::LookupBatch(_, c) => {
                c.complete(Err(SyncReadError::Rejected(BridgeRejection::Closed)))
            }
            Self::History(_, c) => {
                c.complete(Err(SyncReadError::Rejected(BridgeRejection::Closed)))
            }
            Self::Snapshot(_, c) => {
                c.complete(Err(SyncReadError::Rejected(BridgeRejection::Closed)))
            }
            Self::Append(_, _, _, c) => {
                c.complete(Err(SyncWriteError::Rejected(BridgeRejection::Closed)))
            }
            Self::Create(_, _, _, c) | Self::Execute(_, _, _, c) | Self::Observe(_, _, _, c) => {
                c.complete(Err(SyncExecutionError::Rejected(BridgeRejection::Closed)))
            }
            Self::Batch(_, _, _, c) => {
                c.complete(Err(SyncExecutionError::Rejected(BridgeRejection::Closed)))
            }
            Self::Import(_, _, _, c, _) => {
                c.complete(Err(SyncImportError::Rejected(BridgeRejection::Closed)))
            }
            Self::Wake => {}
        }
    }

    fn cancel_stopped(self) {
        let rejection = BridgeRejection::WorkerStoppedBeforeDispatch;
        match self {
            Self::Load(_, c) => c.complete(Err(SyncReadError::Rejected(rejection))),
            Self::LookupRecord(_, c) => c.complete(Err(SyncReadError::Rejected(rejection))),
            Self::LookupBatch(_, c) => c.complete(Err(SyncReadError::Rejected(rejection))),
            Self::History(_, c) => c.complete(Err(SyncReadError::Rejected(rejection))),
            Self::Snapshot(_, c) => c.complete(Err(SyncReadError::Rejected(rejection))),
            Self::Append(_, _, _, c) => c.complete(Err(SyncWriteError::Rejected(rejection))),
            Self::Create(_, _, _, c) | Self::Execute(_, _, _, c) | Self::Observe(_, _, _, c) => {
                c.complete(Err(SyncExecutionError::Rejected(rejection)))
            }
            Self::Batch(_, _, _, c) => c.complete(Err(SyncExecutionError::Rejected(rejection))),
            Self::Import(_, _, _, c, _) => c.complete(Err(SyncImportError::Rejected(rejection))),
            Self::Wake => {}
        }
    }

    #[cfg(test)]
    fn cancel_after_dispatch_stopped(self) {
        match self {
            Self::Load(_, c) => c.complete(Err(SyncReadError::AfterDispatch(
                BridgeAfterDispatch::WorkerStopped,
            ))),
            Self::LookupRecord(_, c) => c.complete(Err(SyncReadError::AfterDispatch(
                BridgeAfterDispatch::WorkerStopped,
            ))),
            Self::LookupBatch(_, c) => c.complete(Err(SyncReadError::AfterDispatch(
                BridgeAfterDispatch::WorkerStopped,
            ))),
            Self::History(_, c) => c.complete(Err(SyncReadError::AfterDispatch(
                BridgeAfterDispatch::WorkerStopped,
            ))),
            Self::Snapshot(_, c) => c.complete(Err(SyncReadError::AfterDispatch(
                BridgeAfterDispatch::WorkerStopped,
            ))),
            Self::Append(_, _, key, c) => {
                c.complete(Err(SyncWriteError::Write(WriteFailure::Uncertain {
                    key,
                    cause: "sync bridge worker stopped after dispatch".into(),
                })))
            }
            Self::Create(_, _, key, c)
            | Self::Execute(_, _, key, c)
            | Self::Observe(_, _, key, c)
            | Self::Batch(_, key, _, c) => c.complete(Err(SyncExecutionError::Execution(
                ExecutionError::Write(WriteFailure::Uncertain {
                    key,
                    cause: "sync bridge worker stopped after dispatch".into(),
                }),
            ))),
            Self::Import(_, _, subject, c, _) => c.complete(Err(SyncImportError::Import(
                ImportAnchorFailure::Uncertain {
                    subject,
                    cause: ImportAnchorUncertainty::RecoveryUnavailable,
                },
            ))),
            Self::Wake => {}
        }
    }
}

/// Non-clone bridge owner.
pub struct RecordedEventlogBridge {
    sender: SyncSender<Request>,
    worker_id: ThreadId,
    join: Option<JoinHandle<()>>,
    shared: Arc<Shared>,
    capacity: u16,
    joined: Option<Result<(), AsyncStoreError>>,
}

/// Operation-scoped synchronous facade.
pub struct SyncEventlogOperation<'a> {
    bridge: &'a RecordedEventlogBridge,
    context: EventlogOperationContext,
}

impl RecordedEventlogBridge {
    /// Starts and conclusively opens the worker-owned provider.
    pub fn start(
        registry: Registry,
        owner: EventlogRecordedStoreOwner,
        config: BridgeConfig,
    ) -> Result<Self, BridgeStartError> {
        Self::start_with(config, move |runtime| {
            let (store, backend) = runtime
                .block_on(owner.open())
                .map_err(BridgeStartError::Open)?;
            Ok(Box::new(ProductionDriver {
                registry,
                store,
                backend,
            }))
        })
    }

    /// Explicitly provisions projections/binding, then opens the worker-owned provider.
    ///
    /// # Errors
    ///
    /// Worker/runtime construction, native preparation, binding conflict/uncertainty, or open
    /// verification failure. Runtime [`Self::start`] never performs these mutations.
    pub fn provision_and_start(
        registry: Registry,
        owner: EventlogRecordedStoreProvisioner,
        context: EventlogOperationContext,
        config: BridgeConfig,
    ) -> Result<(Self, Authority), BridgeStartError> {
        let (authority_tx, authority_rx) = std::sync::mpsc::channel();
        let bridge = Self::start_with(config, move |runtime| {
            let (store, backend) = runtime.block_on(owner.provision(context))?;
            authority_tx
                .send(store.authority().clone())
                .map_err(|_| BridgeStartError::WorkerPanicked)?;
            Ok(Box::new(ProductionDriver {
                registry,
                store,
                backend,
            }))
        })?;
        let authority = authority_rx
            .recv()
            .map_err(|_| BridgeStartError::WorkerPanicked)?;
        Ok((bridge, authority))
    }

    fn start_with(
        config: BridgeConfig,
        startup: impl FnOnce(
            &tokio::runtime::Runtime,
        ) -> Result<Box<dyn WorkerDriver>, BridgeStartError>
        + Send
        + 'static,
    ) -> Result<Self, BridgeStartError> {
        Self::start_with_parts(
            config,
            startup,
            || {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
            },
            |work| {
                std::thread::Builder::new()
                    .name("entity-eventlog".into())
                    .spawn(work)
            },
        )
    }

    fn start_with_parts(
        config: BridgeConfig,
        startup: impl FnOnce(
            &tokio::runtime::Runtime,
        ) -> Result<Box<dyn WorkerDriver>, BridgeStartError>
        + Send
        + 'static,
        runtime_builder: impl FnOnce() -> std::io::Result<tokio::runtime::Runtime> + Send + 'static,
        spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> std::io::Result<std::thread::JoinHandle<()>>,
    ) -> Result<Self, BridgeStartError> {
        let (sender, receiver) = sync_channel(usize::from(config.queue_capacity.get()));
        let shared = Arc::new(Shared {
            lifecycle: AtomicU8::new(RUNNING),
            admission: Mutex::new(()),
            queued: AtomicUsize::new(0),
            dispatched: Mutex::new(None),
            finished: (Mutex::new(false), Condvar::new()),
            provider: Mutex::new(None),
            #[cfg(test)]
            before_dispatch: Mutex::new(std::collections::VecDeque::new()),
            #[cfg(test)]
            after_dispatch: Mutex::new(std::collections::VecDeque::new()),
            #[cfg(test)]
            after_admission_check: Mutex::new(std::collections::VecDeque::new()),
            #[cfg(test)]
            after_enqueue: Mutex::new(std::collections::VecDeque::new()),
            #[cfg(test)]
            before_terminal_admission: Mutex::new(std::collections::VecDeque::new()),
            #[cfg(test)]
            after_terminal_drain: Mutex::new(std::collections::VecDeque::new()),
        });
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let worker_shared = shared.clone();
        let work = Box::new(move || {
            contain_worker(worker_shared.clone(), &receiver, || {
                worker_main(startup, runtime_builder, &receiver, worker_shared, ready_tx);
            });
        });
        let join = spawn(work).map_err(BridgeStartError::ThreadSpawn)?;
        match ready_rx.recv() {
            Ok(Ok(worker_id)) => Ok(Self {
                sender,
                worker_id,
                join: Some(join),
                shared,
                capacity: config.queue_capacity.get(),
                joined: None,
            }),
            Ok(Err(error)) => {
                let _ = join.join();
                Err(error)
            }
            Err(_) => {
                let _ = join.join();
                Err(BridgeStartError::WorkerPanicked)
            }
        }
    }
    /// Supplies operational facts for subsequent write calls.
    #[must_use]
    pub fn operation(&self, context: EventlogOperationContext) -> SyncEventlogOperation<'_> {
        SyncEventlogOperation {
            bridge: self,
            context,
        }
    }
    /// Loads current state.
    pub fn load(
        &self,
        subject: &Subject,
        wait: CallWait,
    ) -> Result<Option<EntityInstance>, SyncReadError> {
        let cell = Cell::new();
        self.submit(
            Request::Load(subject.clone(), cell.clone()),
            &cell,
            wait,
            BridgeOperationIdentity::Read(BridgeReadKind::Load),
        )
    }
    /// Looks up a global record.
    pub fn lookup_record(
        &self,
        id: &str,
        wait: CallWait,
    ) -> Result<Option<RecordLookup>, SyncReadError> {
        let cell = Cell::new();
        self.submit(
            Request::LookupRecord(id.into(), cell.clone()),
            &cell,
            wait,
            BridgeOperationIdentity::Read(BridgeReadKind::LookupRecord),
        )
    }
    /// Looks up an original batch.
    pub fn lookup_batch(
        &self,
        key: &BatchKey,
        wait: CallWait,
    ) -> Result<Option<StoredBatch>, SyncReadError> {
        let cell = Cell::new();
        self.submit(
            Request::LookupBatch(key.clone(), cell.clone()),
            &cell,
            wait,
            BridgeOperationIdentity::Read(BridgeReadKind::LookupBatch),
        )
    }
    /// Loads mixed subject history.
    pub fn history(
        &self,
        subject: &Subject,
        wait: CallWait,
    ) -> Result<SubjectHistory, SyncReadError> {
        let cell = Cell::new();
        self.submit(
            Request::History(subject.clone(), cell.clone()),
            &cell,
            wait,
            BridgeOperationIdentity::Read(BridgeReadKind::History),
        )
    }
    /// Captures a complete logical scope.
    pub fn complete_snapshot(
        &self,
        scope: &str,
        wait: CallWait,
    ) -> Result<CompleteStoreSnapshot, SyncReadError> {
        let cell = Cell::new();
        self.submit(
            Request::Snapshot(scope.into(), cell.clone()),
            &cell,
            wait,
            BridgeOperationIdentity::Read(BridgeReadKind::CompleteSnapshot),
        )
    }

    fn submit<T, E>(
        &self,
        request: Request,
        cell: &Arc<Cell<Result<T, E>>>,
        wait: CallWait,
        identity: BridgeOperationIdentity,
    ) -> Result<T, E>
    where
        E: LocalFailure,
    {
        submit_request(
            &self.sender,
            self.worker_id,
            &self.shared,
            self.capacity,
            request,
            cell,
            wait,
            identity,
        )
    }

    /// Closes admission and waits for actual provider retirement and thread join.
    pub fn shutdown(&mut self, mode: ShutdownMode, wait: CallWait) -> ShutdownOutcome {
        if std::thread::current().id() == self.worker_id {
            return ShutdownOutcome::Reentrant;
        }
        let desired = match mode {
            ShutdownMode::Drain => CLOSING_DRAIN,
            ShutdownMode::CancelQueued => CLOSING_CANCEL,
        };
        loop {
            let current = self.shared.lifecycle.load(Ordering::Acquire);
            if current == JOINED {
                break;
            }
            let next = if current == RUNNING {
                desired
            } else if current == CLOSING_DRAIN && desired == CLOSING_CANCEL {
                CLOSING_CANCEL
            } else {
                current
            };
            if next == current
                || self
                    .shared
                    .lifecycle
                    .compare_exchange(current, next, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            {
                break;
            }
        }
        let _ = self.sender.try_send(Request::Wake);
        let (lock, wake) = &self.shared.finished;
        let mut finished = lock.lock().expect("finish lock");
        while !*finished {
            match wait {
                CallWait::Forever => finished = wake.wait(finished).expect("finish wait"),
                CallWait::Until(deadline) => {
                    let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                        return self.timed_out();
                    };
                    let (next, result) = wake.wait_timeout(finished, left).expect("finish wait");
                    finished = next;
                    if result.timed_out() && !*finished {
                        return self.timed_out();
                    }
                }
            }
        }
        drop(finished);
        if let Some(join) = self.join.take()
            && join.join().is_err()
        {
            self.joined = Some(Err(AsyncStoreError::Backend(
                "sync bridge worker panicked".into(),
            )));
        }
        let result = self
            .joined
            .get_or_insert_with(|| {
                self.shared
                    .provider
                    .lock()
                    .expect("provider result")
                    .clone()
                    .unwrap_or_else(|| {
                        Err(AsyncStoreError::Backend(
                            "worker stopped without retirement result".into(),
                        ))
                    })
            })
            .clone();
        self.shared.lifecycle.store(JOINED, Ordering::Release);
        ShutdownOutcome::Joined { provider: result }
    }
    fn timed_out(&self) -> ShutdownOutcome {
        ShutdownOutcome::TimedOut {
            queued: self.shared.queued.load(Ordering::Acquire),
            dispatched: self
                .shared
                .dispatched
                .lock()
                .expect("dispatch state")
                .clone(),
        }
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the private seam mirrors one complete admission"
)]
fn submit_request<T, E>(
    sender: &SyncSender<Request>,
    worker_id: ThreadId,
    shared: &Shared,
    capacity: u16,
    request: Request,
    cell: &Arc<Cell<Result<T, E>>>,
    wait: CallWait,
    identity: BridgeOperationIdentity,
) -> Result<T, E>
where
    E: LocalFailure,
{
    if std::thread::current().id() == worker_id {
        return Err(E::rejected(BridgeRejection::Reentrant));
    }
    if matches!(wait,CallWait::Until(deadline) if deadline<=Instant::now()) {
        return Err(E::rejected(BridgeRejection::DeadlineBeforeAcceptance));
    }
    let admission = shared.admission.lock().expect("admission");
    if shared.lifecycle.load(Ordering::Acquire) != RUNNING {
        return Err(E::rejected(BridgeRejection::Closed));
    }
    #[cfg(test)]
    if let Some(hook) = shared
        .after_admission_check
        .lock()
        .expect("admission hook")
        .pop_front()
    {
        hook();
    }
    shared.queued.fetch_add(1, Ordering::AcqRel);
    match sender.try_send(request) {
        Ok(()) => {
            #[cfg(test)]
            if let Some(hook) = shared
                .after_enqueue
                .lock()
                .expect("enqueue hook")
                .pop_front()
            {
                hook();
            }
            drop(admission);
            wait_cell(cell, wait, identity)
        }
        Err(TrySendError::Full(_)) => {
            shared.queued.fetch_sub(1, Ordering::AcqRel);
            Err(E::rejected(BridgeRejection::QueueFull { capacity }))
        }
        Err(TrySendError::Disconnected(_)) => {
            shared.queued.fetch_sub(1, Ordering::AcqRel);
            Err(E::rejected(BridgeRejection::Closed))
        }
    }
}

impl Drop for RecordedEventlogBridge {
    fn drop(&mut self) {
        if self.shared.lifecycle.load(Ordering::Acquire) == RUNNING {
            self.shared
                .lifecycle
                .store(CLOSING_CANCEL, Ordering::Release);
        } else {
            let _ = self.shared.lifecycle.compare_exchange(
                CLOSING_DRAIN,
                CLOSING_CANCEL,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
        let _ = self.sender.try_send(Request::Wake);
        let _ = self.join.take();
    }
}

impl SyncEventlogOperation<'_> {
    /// Directly appends an already decided request.
    pub fn append(
        &self,
        request: AppendRequest,
        wait: CallWait,
    ) -> Result<AppendOutcome, SyncWriteError> {
        if let Err(e) = request.validate() {
            return Err(SyncWriteError::Write(WriteFailure::NotCommitted(e)));
        }
        let Some(key) = request.key.clone() else {
            return Ok(AppendOutcome::Empty);
        };
        let cell = Cell::new();
        self.bridge.submit(
            Request::Append(self.context.clone(), request, key.clone(), cell.clone()),
            &cell,
            wait,
            BridgeOperationIdentity::Write(key),
        )
    }
    /// Executes creation through the existing executor.
    pub fn create(
        &self,
        request: CreateRequest,
        wait: CallWait,
    ) -> Result<AppendOutcome, SyncExecutionError> {
        let key = BatchKey::SingleRecord(request.recording.record_id.clone());
        let cell = Cell::new();
        self.bridge.submit(
            Request::Create(self.context.clone(), request, key.clone(), cell.clone()),
            &cell,
            wait,
            BridgeOperationIdentity::Write(key),
        )
    }
    /// Executes an operation through the existing executor.
    pub fn execute(
        &self,
        request: ExecuteRequest,
        wait: CallWait,
    ) -> Result<AppendOutcome, SyncExecutionError> {
        let key = BatchKey::SingleRecord(request.recording.record_id.clone());
        let cell = Cell::new();
        self.bridge.submit(
            Request::Execute(self.context.clone(), request, key.clone(), cell.clone()),
            &cell,
            wait,
            BridgeOperationIdentity::Write(key),
        )
    }
    /// Records an observation through the existing executor.
    pub fn observe(
        &self,
        request: RecordedObservation,
        wait: CallWait,
    ) -> Result<AppendOutcome, SyncExecutionError> {
        let key = BatchKey::SingleRecord(request.envelope.record_id.clone());
        let cell = Cell::new();
        self.bridge.submit(
            Request::Observe(self.context.clone(), request, key.clone(), cell.clone()),
            &cell,
            wait,
            BridgeOperationIdentity::Write(key),
        )
    }
    /// Executes an ordered atomic batch.
    pub fn batch(
        &self,
        key: BatchKey,
        actions: Vec<BatchAction>,
        wait: CallWait,
    ) -> Result<AppendOutcome, SyncExecutionError> {
        if actions.is_empty() {
            return Ok(AppendOutcome::Empty);
        }
        let cell = Cell::new();
        self.bridge.submit(
            Request::Batch(self.context.clone(), key.clone(), actions, cell.clone()),
            &cell,
            wait,
            BridgeOperationIdentity::Write(key),
        )
    }

    /// Establishes one explicit legacy boundary through the existing adapter import path.
    pub fn import_anchor(
        &self,
        history: SubjectHistory,
        wait: CallWait,
    ) -> Result<ImportAnchorOutcome, SyncImportError> {
        self.submit_import(history, None, wait)
    }

    /// Establishes a legacy boundary whose durable replay identity includes its acquisition source.
    pub fn import_source_anchor(
        &self,
        source_id: String,
        history: SubjectHistory,
        wait: CallWait,
    ) -> Result<ImportAnchorOutcome, SyncImportError> {
        self.submit_import(history, Some(source_id), wait)
    }

    fn submit_import(
        &self,
        history: SubjectHistory,
        source_id: Option<String>,
        wait: CallWait,
    ) -> Result<ImportAnchorOutcome, SyncImportError> {
        let subject = history.subject.clone();
        let cell = Cell::new();
        self.bridge.submit(
            Request::Import(
                self.context.clone(),
                history,
                subject.clone(),
                cell.clone(),
                source_id,
            ),
            &cell,
            wait,
            BridgeOperationIdentity::Import(subject),
        )
    }
}

trait LocalFailure: Sized {
    fn rejected(value: BridgeRejection) -> Self;
    fn deadline(identity: &BridgeOperationIdentity) -> Self;
}
impl LocalFailure for SyncReadError {
    fn rejected(value: BridgeRejection) -> Self {
        Self::Rejected(value)
    }
    fn deadline(_: &BridgeOperationIdentity) -> Self {
        Self::AfterDispatch(BridgeAfterDispatch::Deadline)
    }
}
impl LocalFailure for SyncWriteError {
    fn rejected(value: BridgeRejection) -> Self {
        Self::Rejected(value)
    }
    fn deadline(identity: &BridgeOperationIdentity) -> Self {
        let BridgeOperationIdentity::Write(key) = identity else {
            return Self::Rejected(BridgeRejection::WorkerStoppedBeforeDispatch);
        };
        Self::Write(WriteFailure::Uncertain {
            key: key.clone(),
            cause: "sync bridge deadline after dispatch".into(),
        })
    }
}
impl LocalFailure for SyncExecutionError {
    fn rejected(value: BridgeRejection) -> Self {
        Self::Rejected(value)
    }
    fn deadline(identity: &BridgeOperationIdentity) -> Self {
        let BridgeOperationIdentity::Write(key) = identity else {
            return Self::Rejected(BridgeRejection::WorkerStoppedBeforeDispatch);
        };
        Self::Execution(ExecutionError::Write(WriteFailure::Uncertain {
            key: key.clone(),
            cause: "sync bridge deadline after dispatch".into(),
        }))
    }
}
impl LocalFailure for SyncImportError {
    fn rejected(value: BridgeRejection) -> Self {
        Self::Rejected(value)
    }
    fn deadline(identity: &BridgeOperationIdentity) -> Self {
        let BridgeOperationIdentity::Import(subject) = identity else {
            return Self::Rejected(BridgeRejection::WorkerStoppedBeforeDispatch);
        };
        Self::Import(ImportAnchorFailure::Uncertain {
            subject: subject.clone(),
            cause: ImportAnchorUncertainty::RecoveryUnavailable,
        })
    }
}

fn wait_cell<T, E>(
    cell: &Arc<Cell<Result<T, E>>>,
    wait: CallWait,
    identity: BridgeOperationIdentity,
) -> Result<T, E>
where
    E: LocalFailure,
{
    let mut result = cell.result.lock().expect("result cell");
    loop {
        if cell.phase.load(Ordering::Acquire) == COMPLETED {
            return result.take().expect("completed result");
        }
        match wait {
            CallWait::Forever => result = cell.wake.wait(result).expect("result wait"),
            CallWait::Until(deadline) => {
                let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                    if cell
                        .phase
                        .compare_exchange(QUEUED, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                    {
                        return Err(E::rejected(BridgeRejection::DeadlineBeforeDispatch));
                    }
                    return Err(E::deadline(&identity));
                };
                let (next, timed) = cell.wake.wait_timeout(result, left).expect("result wait");
                result = next;
                if timed.timed_out() && cell.phase.load(Ordering::Acquire) != COMPLETED {
                    if cell
                        .phase
                        .compare_exchange(QUEUED, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                    {
                        return Err(E::rejected(BridgeRejection::DeadlineBeforeDispatch));
                    }
                    return Err(E::deadline(&identity));
                }
            }
        }
    }
}

fn worker_main(
    startup: impl FnOnce(&tokio::runtime::Runtime) -> Result<Box<dyn WorkerDriver>, BridgeStartError>,
    runtime_builder: impl FnOnce() -> std::io::Result<tokio::runtime::Runtime>,
    receiver: &Receiver<Request>,
    shared: Arc<Shared>,
    ready: std::sync::mpsc::Sender<Result<ThreadId, BridgeStartError>>,
) {
    let runtime = match runtime_builder() {
        Ok(v) => v,
        Err(e) => {
            let _ = ready.send(Err(BridgeStartError::RuntimeBuild(e)));
            finish(
                &shared,
                Err(AsyncStoreError::Backend("runtime build failed".into())),
            );
            return;
        }
    };
    let driver = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| startup(&runtime)))
    {
        Ok(Ok(v)) => v,
        Ok(Err(error)) => {
            let _ = ready.send(Err(error));
            finish(&shared, Ok(()));
            return;
        }
        Err(_) => {
            let _ = ready.send(Err(BridgeStartError::WorkerPanicked));
            finish(
                &shared,
                Err(AsyncStoreError::Backend(
                    "worker panicked during open".into(),
                )),
            );
            return;
        }
    };
    if ready.send(Ok(std::thread::current().id())).is_err() {
        finish(&shared, runtime.block_on(driver.retire()));
        return;
    }
    let mut worker_panicked = false;
    while let Ok(request) = receiver.recv() {
        if matches!(request, Request::Wake) {
            if shared.lifecycle.load(Ordering::Acquire) != RUNNING
                && shared.queued.load(Ordering::Acquire) == 0
            {
                break;
            }
            continue;
        }
        #[cfg(test)]
        if let Some(hook) = shared
            .before_dispatch
            .lock()
            .expect("dispatch hook")
            .pop_front()
            && std::panic::catch_unwind(std::panic::AssertUnwindSafe(hook)).is_err()
        {
            shared.queued.fetch_sub(1, Ordering::AcqRel);
            request.cancel_stopped();
            worker_panicked = true;
            break;
        }
        let phase = request.phase().expect("work has phase");
        if phase.load(Ordering::Acquire) == CANCELLED {
            shared.queued.fetch_sub(1, Ordering::AcqRel);
            continue;
        }
        if shared.lifecycle.load(Ordering::Acquire) == CLOSING_CANCEL {
            shared.queued.fetch_sub(1, Ordering::AcqRel);
            request.cancel_closed();
            continue;
        }
        if phase
            .compare_exchange(QUEUED, DISPATCHED, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            shared.queued.fetch_sub(1, Ordering::AcqRel);
            continue;
        }
        shared.queued.fetch_sub(1, Ordering::AcqRel);
        *shared.dispatched.lock().expect("dispatch state") = request.identity();
        #[cfg(test)]
        if let Some(hook) = shared
            .after_dispatch
            .lock()
            .expect("dispatch hook")
            .pop_front()
            && std::panic::catch_unwind(std::panic::AssertUnwindSafe(hook)).is_err()
        {
            request.cancel_after_dispatch_stopped();
            *shared.dispatched.lock().expect("dispatch state") = None;
            worker_panicked = true;
            break;
        }
        let panicked = drive_request(&runtime, driver.as_ref(), request);
        *shared.dispatched.lock().expect("dispatch state") = None;
        if panicked {
            worker_panicked = true;
            break;
        }
        if shared.lifecycle.load(Ordering::Acquire) != RUNNING
            && shared.queued.load(Ordering::Acquire) == 0
        {
            break;
        }
    }
    terminal_drain(&shared, receiver, worker_panicked);
    let retired = runtime.block_on(driver.retire());
    finish(&shared, retired);
}

fn terminal_drain(shared: &Shared, receiver: &Receiver<Request>, worker_panicked: bool) {
    #[cfg(test)]
    if let Some(hook) = shared
        .before_terminal_admission
        .lock()
        .expect("terminal admission hook")
        .pop_front()
    {
        hook();
    }
    let admission = shared.admission.lock().expect("admission");
    if worker_panicked || shared.lifecycle.load(Ordering::Acquire) == RUNNING {
        shared.lifecycle.store(CLOSING_CANCEL, Ordering::Release);
    }
    let mut pending = Vec::new();
    while let Ok(request) = receiver.try_recv() {
        if request.phase().is_some() {
            shared.queued.fetch_sub(1, Ordering::AcqRel);
        }
        pending.push(request);
    }
    #[cfg(test)]
    if let Some(hook) = shared
        .after_terminal_drain
        .lock()
        .expect("terminal drain hook")
        .pop_front()
    {
        hook();
    }
    drop(admission);
    for request in pending {
        if worker_panicked {
            request.cancel_stopped();
        } else {
            request.cancel_closed();
        }
    }
}

fn contain_worker(shared: Arc<Shared>, receiver: &Receiver<Request>, work: impl FnOnce()) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).is_err() {
        terminal_drain(&shared, receiver, true);
        let already_finished = *shared.finished.0.lock().expect("finish lock");
        if !already_finished {
            finish(
                &shared,
                Err(AsyncStoreError::Backend(
                    "sync bridge worker disappeared after panic".into(),
                )),
            );
        }
    }
}

fn drive_request(
    runtime: &tokio::runtime::Runtime,
    driver: &dyn WorkerDriver,
    request: Request,
) -> bool {
    match request {
        Request::Load(v, c) => poll_read(c, || runtime.block_on(driver.load(&v))),
        Request::LookupRecord(v, c) => poll_read(c, || runtime.block_on(driver.lookup_record(&v))),
        Request::LookupBatch(v, c) => poll_read(c, || runtime.block_on(driver.lookup_batch(&v))),
        Request::History(v, c) => poll_read(c, || runtime.block_on(driver.history(&v))),
        Request::Snapshot(v, c) => poll_read(c, || runtime.block_on(driver.complete_snapshot(&v))),
        Request::Append(context, v, key, c) => {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                runtime.block_on(driver.append(context, v))
            })) {
                Ok(result) => {
                    c.complete(result.map_err(SyncWriteError::Write));
                    false
                }
                Err(_) => {
                    c.complete(Err(SyncWriteError::Write(WriteFailure::Uncertain {
                        key,
                        cause: "sync bridge worker panicked after dispatch".into(),
                    })));
                    true
                }
            }
        }
        Request::Create(context, v, key, c) => {
            poll_execution(c, key, || runtime.block_on(driver.create(context, v)))
        }
        Request::Execute(context, v, key, c) => {
            poll_execution(c, key, || runtime.block_on(driver.execute(context, v)))
        }
        Request::Observe(context, v, key, c) => {
            poll_execution(c, key, || runtime.block_on(driver.observe(context, v)))
        }
        Request::Batch(context, key, v, c) => {
            let panic_key = key.clone();
            poll_execution(c, panic_key, || {
                runtime.block_on(driver.batch(context, key, v))
            })
        }
        Request::Import(context, history, subject, c, source_id) => poll_import(c, subject, || {
            runtime.block_on(driver.import_anchor(context, history, source_id))
        }),
        Request::Wake => false,
    }
}

fn poll_read<T>(
    cell: Arc<Cell<Result<T, SyncReadError>>>,
    work: impl FnOnce() -> Result<T, AsyncStoreError>,
) -> bool {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)) {
        Ok(result) => {
            cell.complete(result.map_err(SyncReadError::Store));
            false
        }
        Err(_) => {
            cell.complete(Err(SyncReadError::AfterDispatch(
                BridgeAfterDispatch::WorkerPanicked,
            )));
            true
        }
    }
}
fn poll_execution(
    cell: Arc<Cell<Result<AppendOutcome, SyncExecutionError>>>,
    key: BatchKey,
    work: impl FnOnce() -> Result<AppendOutcome, ExecutionError>,
) -> bool {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)) {
        Ok(result) => {
            cell.complete(result.map_err(SyncExecutionError::Execution));
            false
        }
        Err(_) => {
            cell.complete(Err(SyncExecutionError::Execution(ExecutionError::Write(
                WriteFailure::Uncertain {
                    key,
                    cause: "sync bridge worker panicked after dispatch".into(),
                },
            ))));
            true
        }
    }
}

fn poll_import(
    cell: Arc<Cell<Result<ImportAnchorOutcome, SyncImportError>>>,
    subject: Subject,
    work: impl FnOnce() -> Result<ImportAnchorOutcome, ImportAnchorFailure>,
) -> bool {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)) {
        Ok(result) => {
            cell.complete(result.map_err(SyncImportError::Import));
            false
        }
        Err(_) => {
            cell.complete(Err(SyncImportError::Import(
                ImportAnchorFailure::Uncertain {
                    subject,
                    cause: ImportAnchorUncertainty::RecoveryUnavailable,
                },
            )));
            true
        }
    }
}

fn finish(shared: &Shared, result: Result<(), AsyncStoreError>) {
    *shared.provider.lock().expect("provider result") = Some(result);
    let (lock, wake) = &shared.finished;
    *lock.lock().expect("finish lock") = true;
    wake.notify_all();
}
fn store_open(error: eventlog_core::EventLogError) -> AsyncStoreError {
    AsyncStoreError::Backend(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        future::poll_fn,
        sync::atomic::AtomicBool,
        task::{Poll, Waker},
        time::Duration,
    };

    use entity_core::{CoreError, Runtime};
    use entity_store::{
        Expect, RecordedCommit, Recording,
        asynchronous::{
            AppendMember, BatchReceipt, CommitReceipt, HistoryOrigin, RecordPosition,
            RecordReceipt, StoreCoverage, original_request_comparison_bytes,
        },
    };

    #[derive(Default)]
    struct Barrier {
        entered: AtomicUsize,
        released: AtomicBool,
        state: Mutex<()>,
        wake: Condvar,
        task: Mutex<Option<Waker>>,
    }

    impl Barrier {
        fn arrive(&self) {
            self.entered.fetch_add(1, Ordering::AcqRel);
            self.wake.notify_all();
        }

        async fn arrive_and_wait(&self) {
            self.arrive();
            poll_fn(|context| {
                if self.released.load(Ordering::Acquire) {
                    Poll::Ready(())
                } else {
                    *self.task.lock().expect("barrier task") = Some(context.waker().clone());
                    if self.released.load(Ordering::Acquire) {
                        Poll::Ready(())
                    } else {
                        Poll::Pending
                    }
                }
            })
            .await;
        }

        fn arrive_and_wait_blocking(&self) {
            self.arrive();
            let mut state = self.state.lock().expect("barrier state");
            while !self.released.load(Ordering::Acquire) {
                state = self.wake.wait(state).expect("barrier wait");
            }
        }

        fn wait_until_entered(&self) {
            let mut state = self.state.lock().expect("barrier state");
            while self.entered.load(Ordering::Acquire) == 0 {
                state = self.wake.wait(state).expect("barrier entered");
            }
        }

        fn wait_until_entered_for(&self, timeout: Duration) -> bool {
            let deadline = Instant::now() + timeout;
            let mut state = self.state.lock().expect("barrier state");
            while self.entered.load(Ordering::Acquire) == 0 {
                let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                    return false;
                };
                let (next, result) = self
                    .wake
                    .wait_timeout(state, left)
                    .expect("barrier entered");
                state = next;
                if result.timed_out() && self.entered.load(Ordering::Acquire) == 0 {
                    return false;
                }
            }
            true
        }

        fn release(&self) {
            self.released.store(true, Ordering::Release);
            self.wake.notify_all();
            if let Some(task) = self.task.lock().expect("barrier task").take() {
                task.wake();
            }
        }
    }

    enum ExecutionFailure {
        Core,
        Store(AsyncStoreError),
        Write(WriteFailure),
    }

    struct FakeState {
        calls: Mutex<Vec<String>>,
        contexts: Mutex<Vec<EventlogOperationContext>>,
        next_gate: Mutex<Option<Arc<Barrier>>>,
        next_read_error: Mutex<Option<AsyncStoreError>>,
        next_write_error: Mutex<Option<WriteFailure>>,
        next_execution_error: Mutex<Option<ExecutionFailure>>,
        committed: Mutex<Option<(AppendRequest, CommitReceipt)>>,
        panic_next: AtomicBool,
        panic_after_gate: AtomicBool,
        retire_gate: Mutex<Option<Arc<Barrier>>>,
        retire_result: Mutex<Result<(), AsyncStoreError>>,
        retire_calls: AtomicUsize,
        constructed_on: Mutex<Option<ThreadId>>,
        dropped_on: Mutex<Option<ThreadId>>,
    }

    impl Default for FakeState {
        fn default() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                contexts: Mutex::new(Vec::new()),
                next_gate: Mutex::new(None),
                next_read_error: Mutex::new(None),
                next_write_error: Mutex::new(None),
                next_execution_error: Mutex::new(None),
                committed: Mutex::new(None),
                panic_next: AtomicBool::new(false),
                panic_after_gate: AtomicBool::new(false),
                retire_gate: Mutex::new(None),
                retire_result: Mutex::new(Ok(())),
                retire_calls: AtomicUsize::new(0),
                constructed_on: Mutex::new(None),
                dropped_on: Mutex::new(None),
            }
        }
    }

    impl FakeState {
        fn arm_call(&self) -> Arc<Barrier> {
            let gate = Arc::new(Barrier::default());
            let replaced = self
                .next_gate
                .lock()
                .expect("next gate")
                .replace(gate.clone());
            assert!(replaced.is_none(), "a call gate is already armed");
            gate
        }

        async fn enter(&self, kind: &str) {
            self.calls.lock().expect("calls").push(kind.to_owned());
            if self.panic_next.swap(false, Ordering::AcqRel) {
                panic!("controlled worker-driver panic");
            }
            let gate = self.next_gate.lock().expect("next gate").take();
            if let Some(gate) = gate {
                gate.arrive_and_wait().await;
            }
            if self.panic_after_gate.swap(false, Ordering::AcqRel) {
                panic!("controlled worker-driver panic after gate");
            }
        }

        fn call_count(&self) -> usize {
            self.calls.lock().expect("calls").len()
        }
    }

    struct FakeDriver {
        state: Arc<FakeState>,
    }

    impl Drop for FakeDriver {
        fn drop(&mut self) {
            *self.state.dropped_on.lock().expect("dropped thread") =
                Some(std::thread::current().id());
        }
    }

    impl WorkerDriver for FakeDriver {
        fn load<'a>(
            &'a self,
            subject: &'a Subject,
        ) -> BoxFuture<'a, Result<Option<EntityInstance>, AsyncStoreError>> {
            Box::pin(async move {
                self.state.enter(&format!("load:{}", subject.id)).await;
                if let Some(error) = self
                    .state
                    .next_read_error
                    .lock()
                    .expect("read error")
                    .take()
                {
                    Err(error)
                } else {
                    Ok(None)
                }
            })
        }

        fn lookup_record<'a>(
            &'a self,
            _: &'a str,
        ) -> BoxFuture<'a, Result<Option<RecordLookup>, AsyncStoreError>> {
            Box::pin(async move {
                self.state.enter("lookup-record").await;
                Ok(None)
            })
        }

        fn lookup_batch<'a>(
            &'a self,
            _: &'a BatchKey,
        ) -> BoxFuture<'a, Result<Option<StoredBatch>, AsyncStoreError>> {
            Box::pin(async move {
                self.state.enter("lookup-batch").await;
                Ok(None)
            })
        }

        fn history<'a>(
            &'a self,
            subject: &'a Subject,
        ) -> BoxFuture<'a, Result<SubjectHistory, AsyncStoreError>> {
            Box::pin(async move {
                self.state.enter("history").await;
                Ok(SubjectHistory {
                    subject: subject.clone(),
                    origin: HistoryOrigin::Genesis,
                    records: Vec::new(),
                })
            })
        }

        fn complete_snapshot<'a>(
            &'a self,
            scope: &'a str,
        ) -> BoxFuture<'a, Result<CompleteStoreSnapshot, AsyncStoreError>> {
            Box::pin(async move {
                self.state.enter("snapshot").await;
                Ok(CompleteStoreSnapshot {
                    scope: scope.to_owned(),
                    coverage: StoreCoverage::CompleteSnapshot,
                    histories: Vec::new(),
                })
            })
        }

        fn append<'a>(
            &'a self,
            context: EventlogOperationContext,
            request: AppendRequest,
        ) -> BoxFuture<'a, Result<AppendOutcome, WriteFailure>> {
            Box::pin(async move {
                self.state.contexts.lock().expect("contexts").push(context);
                self.state.enter("append").await;
                if let Some(error) = self
                    .state
                    .next_write_error
                    .lock()
                    .expect("write error")
                    .take()
                {
                    return Err(error);
                }
                let mut committed = self.state.committed.lock().expect("committed request");
                if let Some((original, receipt)) = committed.as_ref() {
                    assert_eq!(original, &request, "retry bytes and key changed");
                    return Ok(AppendOutcome::Committed {
                        receipt: receipt.clone(),
                        replayed: true,
                    });
                }
                let receipt = receipt_for(&request);
                *committed = Some((request, receipt.clone()));
                Ok(AppendOutcome::Committed {
                    receipt,
                    replayed: false,
                })
            })
        }

        fn create<'a>(
            &'a self,
            context: EventlogOperationContext,
            _: CreateRequest,
        ) -> BoxFuture<'a, Result<AppendOutcome, ExecutionError>> {
            self.execution("create", context)
        }

        fn execute<'a>(
            &'a self,
            context: EventlogOperationContext,
            _: ExecuteRequest,
        ) -> BoxFuture<'a, Result<AppendOutcome, ExecutionError>> {
            self.execution("execute", context)
        }

        fn observe<'a>(
            &'a self,
            context: EventlogOperationContext,
            _: RecordedObservation,
        ) -> BoxFuture<'a, Result<AppendOutcome, ExecutionError>> {
            self.execution("observe", context)
        }

        fn batch<'a>(
            &'a self,
            context: EventlogOperationContext,
            _: BatchKey,
            _: Vec<BatchAction>,
        ) -> BoxFuture<'a, Result<AppendOutcome, ExecutionError>> {
            self.execution("batch", context)
        }

        fn import_anchor<'a>(
            &'a self,
            context: EventlogOperationContext,
            history: SubjectHistory,
            _source_id: Option<String>,
        ) -> BoxFuture<'a, Result<ImportAnchorOutcome, ImportAnchorFailure>> {
            Box::pin(async move {
                self.state.contexts.lock().expect("contexts").push(context);
                self.state.enter("import").await;
                let HistoryOrigin::Imported(anchor) = history.origin else {
                    return Err(ImportAnchorFailure::NotCommitted(
                        AsyncStoreError::InvalidInput(
                            "fake import requires an explicit imported boundary".to_owned(),
                        ),
                    ));
                };
                Ok(ImportAnchorOutcome {
                    assurance:
                        entity_store::asynchronous::SubjectAssurance::VerifiedAfterBoundary {
                            subject: history.subject,
                            anchor_revision: anchor.instance.revision,
                        },
                    replayed: false,
                })
            })
        }

        fn retire(&self) -> BoxFuture<'_, Result<(), AsyncStoreError>> {
            Box::pin(async move {
                self.state.retire_calls.fetch_add(1, Ordering::AcqRel);
                let gate = self.state.retire_gate.lock().expect("retire gate").take();
                if let Some(gate) = gate {
                    gate.arrive_and_wait().await;
                }
                self.state
                    .retire_result
                    .lock()
                    .expect("retire result")
                    .clone()
            })
        }
    }

    impl FakeDriver {
        fn execution<'a>(
            &'a self,
            kind: &'static str,
            context: EventlogOperationContext,
        ) -> BoxFuture<'a, Result<AppendOutcome, ExecutionError>> {
            Box::pin(async move {
                self.state.contexts.lock().expect("contexts").push(context);
                self.state.enter(kind).await;
                match self
                    .state
                    .next_execution_error
                    .lock()
                    .expect("execution error")
                    .take()
                {
                    Some(ExecutionFailure::Core) => {
                        Err(ExecutionError::Core(CoreError::EntityNotRegistered {
                            entity: "test".into(),
                            version: 1,
                        }))
                    }
                    Some(ExecutionFailure::Store(error)) => Err(ExecutionError::Store(error)),
                    Some(ExecutionFailure::Write(error)) => Err(ExecutionError::Write(error)),
                    None => Ok(AppendOutcome::Empty),
                }
            })
        }
    }

    fn fake_bridge(state: Arc<FakeState>, capacity: u16) -> RecordedEventlogBridge {
        RecordedEventlogBridge::start_with(
            BridgeConfig {
                queue_capacity: NonZeroU16::new(capacity).expect("nonzero capacity"),
            },
            move |_| {
                *state.constructed_on.lock().expect("constructed thread") =
                    Some(std::thread::current().id());
                Ok(Box::new(FakeDriver { state }))
            },
        )
        .expect("fake worker started")
    }

    fn context(label: &str) -> EventlogOperationContext {
        EventlogOperationContext {
            subject: "bridge-test".into(),
            actor: "entity-eventlog-test".into(),
            request_id: format!("request-{label}"),
            trace_id: format!("trace-{label}"),
            causation_id: None,
            causation_depth: 0,
            occurred_at: time::OffsetDateTime::UNIX_EPOCH,
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

    fn append_request(label: &str) -> AppendRequest {
        let decision = Runtime::new(&registry())
            .create("ticket", 1, label, serde_json::json!({"title": label}))
            .expect("decision");
        let commit = RecordedCommit::new(
            decision,
            &Recording {
                record_id: format!("record-{label}"),
                recorded_at: "2026-09-16T00:00:00Z".into(),
                correlation: None,
                causation: None,
                actor: None,
            },
        )
        .expect("commit");
        let entry = entity_store::asynchronous::RecordedEntry::Decision(commit);
        let bytes = original_request_comparison_bytes(&entry).expect("request bytes");
        AppendRequest::new(
            BatchKey::SingleRecord(entry.record_id().to_owned()),
            vec![AppendMember::new(Expect::Absent, entry, bytes)],
        )
        .expect("append request")
    }

    fn receipt_for(request: &AppendRequest) -> CommitReceipt {
        let key = request.key.clone().expect("nonempty key");
        let members: Vec<_> = request
            .members
            .iter()
            .enumerate()
            .map(|(index, member)| RecordReceipt {
                record_id: member.entry.record_id().to_owned(),
                subject: member.entry.subject(),
                kind: member.entry.kind(),
                revision: member.entry.revision(),
                position: RecordPosition {
                    subject: u64::try_from(index + 1).expect("position"),
                    store: u64::try_from(index + 1).expect("position"),
                },
                batch_key: key.clone(),
                member_index: u64::try_from(index).expect("member index"),
            })
            .collect();
        match key {
            BatchKey::SingleRecord(_) => CommitReceipt::Single(members[0].clone()),
            BatchKey::Named(_) => CommitReceipt::Batch(BatchReceipt { key, members }),
        }
    }

    fn wait_until(predicate: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !predicate() {
            assert!(Instant::now() < deadline, "condition did not become true");
            std::thread::yield_now();
        }
    }

    fn execution_deadline_case(
        bridge: &RecordedEventlogBridge,
        state: &Arc<FakeState>,
        key: BatchKey,
        call: impl FnOnce(CallWait) -> Result<AppendOutcome, SyncExecutionError> + Send,
    ) {
        let gate = state.arm_call();
        std::thread::scope(|scope| {
            let caller = scope
                .spawn(move || call(CallWait::Until(Instant::now() + Duration::from_millis(100))));
            gate.wait_until_entered();
            let result = caller.join().expect("execution caller");
            assert!(matches!(
                result,
                Err(SyncExecutionError::Execution(ExecutionError::Write(
                    WriteFailure::Uncertain { key: found, .. }
                ))) if found == key
            ));
            gate.release();
        });
        wait_until(|| {
            bridge
                .shared
                .dispatched
                .lock()
                .expect("dispatch state")
                .is_none()
        });
    }

    fn enqueue_load(
        bridge: &RecordedEventlogBridge,
        label: &str,
    ) -> Arc<Cell<Result<Option<EntityInstance>, SyncReadError>>> {
        let cell = Cell::new();
        bridge.shared.queued.fetch_add(1, Ordering::AcqRel);
        bridge
            .sender
            .try_send(Request::Load(
                Subject::new("ticket", label).expect("subject"),
                cell.clone(),
            ))
            .expect("queue load");
        cell
    }

    fn shared(lifecycle: u8) -> Arc<Shared> {
        Arc::new(Shared {
            lifecycle: AtomicU8::new(lifecycle),
            admission: Mutex::new(()),
            queued: AtomicUsize::new(0),
            dispatched: Mutex::new(None),
            finished: (Mutex::new(false), Condvar::new()),
            provider: Mutex::new(None),
            before_dispatch: Mutex::new(std::collections::VecDeque::new()),
            after_dispatch: Mutex::new(std::collections::VecDeque::new()),
            after_admission_check: Mutex::new(std::collections::VecDeque::new()),
            after_enqueue: Mutex::new(std::collections::VecDeque::new()),
            before_terminal_admission: Mutex::new(std::collections::VecDeque::new()),
            after_terminal_drain: Mutex::new(std::collections::VecDeque::new()),
        })
    }

    fn foreign_thread_id() -> ThreadId {
        std::thread::spawn(|| std::thread::current().id())
            .join()
            .expect("thread id")
    }

    fn inert_bridge(lifecycle: u8, capacity: u16) -> (RecordedEventlogBridge, Receiver<Request>) {
        let (sender, receiver) = sync_channel(usize::from(capacity));
        (
            RecordedEventlogBridge {
                sender,
                worker_id: foreign_thread_id(),
                join: None,
                shared: shared(lifecycle),
                capacity,
                joined: None,
            },
            receiver,
        )
    }

    #[test]
    fn production_worker_startup_owns_and_drops_the_driver_on_its_thread() {
        let config = || BridgeConfig {
            queue_capacity: NonZeroU16::new(1).expect("capacity"),
        };
        let thread_spawn = RecordedEventlogBridge::start_with_parts(
            config(),
            |_| unreachable!("a failed spawn never runs startup"),
            || {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
            },
            |_work| Err(std::io::Error::other("controlled thread spawn failure")),
        );
        assert!(matches!(
            thread_spawn,
            Err(BridgeStartError::ThreadSpawn(error))
                if error.to_string() == "controlled thread spawn failure"
        ));

        let runtime_build = RecordedEventlogBridge::start_with_parts(
            config(),
            |_| unreachable!("a failed runtime build never runs startup"),
            || Err(std::io::Error::other("controlled runtime build failure")),
            |work| std::thread::Builder::new().spawn(work),
        );
        assert!(matches!(
            runtime_build,
            Err(BridgeStartError::RuntimeBuild(error))
                if error.to_string() == "controlled runtime build failure"
        ));

        let open = RecordedEventlogBridge::start_with(config(), |_| {
            Err(BridgeStartError::Open(AsyncStoreError::Backend(
                "controlled open refusal".into(),
            )))
        });
        assert!(matches!(
            open,
            Err(BridgeStartError::Open(AsyncStoreError::Backend(detail)))
                if detail == "controlled open refusal"
        ));

        let panicked = RecordedEventlogBridge::start_with(
            config(),
            |_| -> Result<Box<dyn WorkerDriver>, BridgeStartError> {
                panic!("controlled startup panic")
            },
        );
        assert!(matches!(panicked, Err(BridgeStartError::WorkerPanicked)));

        let state = Arc::new(FakeState::default());
        let mut bridge = fake_bridge(state.clone(), 1);
        let worker = bridge.worker_id;
        assert_ne!(worker, std::thread::current().id());
        assert_eq!(
            *state.constructed_on.lock().expect("constructed thread"),
            Some(worker)
        );
        assert_eq!(
            bridge.shutdown(ShutdownMode::Drain, CallWait::Forever),
            ShutdownOutcome::Joined { provider: Ok(()) }
        );
        assert_eq!(state.retire_calls.load(Ordering::Acquire), 1);
        assert_eq!(
            *state.dropped_on.lock().expect("dropped thread"),
            Some(worker)
        );
    }

    #[test]
    fn production_worker_calls_are_runtime_independent_and_reentrant_safe() {
        let state = Arc::new(FakeState::default());
        let mut bridge = fake_bridge(state.clone(), 2);
        let subject = Subject::new("ticket", "runtime-independent").expect("subject");
        assert!(
            bridge
                .load(&subject, CallWait::Forever)
                .expect("outside")
                .is_none()
        );

        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("current-thread runtime")
            .block_on(async {
                assert!(
                    bridge
                        .load(&subject, CallWait::Forever)
                        .expect("current")
                        .is_none()
                );
            });
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("multi-thread runtime")
            .block_on(async {
                assert!(
                    bridge
                        .load(&subject, CallWait::Forever)
                        .expect("multi")
                        .is_none()
                );
            });
        assert!(
            bridge
                .lookup_record("missing", CallWait::Forever)
                .expect("record lookup")
                .is_none()
        );
        assert!(
            bridge
                .lookup_batch(&BatchKey::Named("missing".into()), CallWait::Forever)
                .expect("batch lookup")
                .is_none()
        );
        assert_eq!(
            bridge
                .history(&subject, CallWait::Forever)
                .expect("history")
                .subject,
            subject
        );
        assert_eq!(
            bridge
                .complete_snapshot("scope", CallWait::Forever)
                .expect("snapshot")
                .scope,
            "scope"
        );

        let sender = bridge.sender.clone();
        let worker_id = bridge.worker_id;
        let shared = bridge.shared.clone();
        let hook_shared = shared.clone();
        let capacity = bridge.capacity;
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        shared
            .before_dispatch
            .lock()
            .expect("dispatch hook")
            .push_back(Box::new(move || {
                let cell = Cell::new();
                let before = hook_shared.queued.load(Ordering::Acquire);
                let result = submit_request(
                    &sender,
                    worker_id,
                    &hook_shared,
                    capacity,
                    Request::Load(
                        Subject::new("ticket", "reentrant").expect("subject"),
                        cell.clone(),
                    ),
                    &cell,
                    CallWait::Forever,
                    BridgeOperationIdentity::Read(BridgeReadKind::Load),
                );
                result_tx
                    .send((before, hook_shared.queued.load(Ordering::Acquire), result))
                    .expect("reentrant result");
            }));
        assert!(
            bridge
                .load(
                    &Subject::new("ticket", "hook-trigger").expect("subject"),
                    CallWait::Forever,
                )
                .expect("outer load")
                .is_none()
        );
        let (before, after, result) = result_rx.recv().expect("hook result");
        assert_eq!(before, after);
        assert!(matches!(
            result,
            Err(SyncReadError::Rejected(BridgeRejection::Reentrant))
        ));
        assert_eq!(
            state.call_count(),
            8,
            "the rejected inner call did not dispatch"
        );
        assert_eq!(
            bridge.shutdown(ShutdownMode::Drain, CallWait::Forever),
            ShutdownOutcome::Joined { provider: Ok(()) }
        );
    }

    #[test]
    fn production_worker_enforces_capacity_and_discards_cancelled_queue_cells() {
        let state = Arc::new(FakeState::default());
        let first_gate = state.arm_call();
        let mut bridge = fake_bridge(state.clone(), 1);
        assert!(matches!(
            bridge.load(
                &Subject::new("ticket", "expired-before-send").expect("subject"),
                CallWait::Until(Instant::now()),
            ),
            Err(SyncReadError::Rejected(
                BridgeRejection::DeadlineBeforeAcceptance
            ))
        ));
        assert_eq!(state.call_count(), 0, "expired calls never enter the queue");
        std::thread::scope(|scope| {
            let first = scope.spawn(|| {
                bridge.load(
                    &Subject::new("ticket", "active").expect("subject"),
                    CallWait::Forever,
                )
            });
            first_gate.wait_until_entered();
            let second = scope.spawn(|| {
                bridge.load(
                    &Subject::new("ticket", "queued").expect("subject"),
                    CallWait::Forever,
                )
            });
            wait_until(|| bridge.shared.queued.load(Ordering::Acquire) == 1);
            assert!(matches!(
                bridge
                    .operation(context("inert-append"))
                    .append(AppendRequest::empty(), CallWait::Forever),
                Ok(AppendOutcome::Empty)
            ));
            assert!(matches!(
                bridge.operation(context("invalid-empty")).append(
                    AppendRequest {
                        key: Some(BatchKey::Named("invalid-empty".into())),
                        members: Vec::new(),
                    },
                    CallWait::Forever,
                ),
                Err(SyncWriteError::Write(WriteFailure::NotCommitted(
                    AsyncStoreError::InvalidInput(_)
                )))
            ));
            assert!(matches!(
                bridge.operation(context("inert-batch")).batch(
                    BatchKey::Named(String::new()),
                    Vec::new(),
                    CallWait::Forever,
                ),
                Ok(AppendOutcome::Empty)
            ));
            assert!(matches!(
                bridge.load(
                    &Subject::new("ticket", "full").expect("subject"),
                    CallWait::Forever,
                ),
                Err(SyncReadError::Rejected(BridgeRejection::QueueFull {
                    capacity: 1
                }))
            ));
            first_gate.release();
            assert!(
                first
                    .join()
                    .expect("first caller")
                    .expect("first load")
                    .is_none()
            );
            assert!(
                second
                    .join()
                    .expect("second caller")
                    .expect("second load")
                    .is_none()
            );
        });
        assert_eq!(state.call_count(), 2, "QueueFull never reached the driver");

        let active_gate = state.arm_call();
        std::thread::scope(|scope| {
            let active = scope.spawn(|| {
                bridge.load(
                    &Subject::new("ticket", "cancel-active").expect("subject"),
                    CallWait::Forever,
                )
            });
            active_gate.wait_until_entered();
            let cancelled = Cell::new();
            bridge.shared.queued.fetch_add(1, Ordering::AcqRel);
            bridge
                .sender
                .try_send(Request::Load(
                    Subject::new("ticket", "cancelled").expect("subject"),
                    cancelled.clone(),
                ))
                .expect("queue cancelled request");
            assert!(matches!(
                wait_cell(
                    &cancelled,
                    CallWait::Until(Instant::now()),
                    BridgeOperationIdentity::Read(BridgeReadKind::Load)
                ),
                Err(SyncReadError::Rejected(
                    BridgeRejection::DeadlineBeforeDispatch
                ))
            ));
            active_gate.release();
            assert!(
                active
                    .join()
                    .expect("active caller")
                    .expect("active load")
                    .is_none()
            );
        });
        wait_until(|| bridge.shared.queued.load(Ordering::Acquire) == 0);
        assert_eq!(
            state.call_count(),
            3,
            "cancelled queued cell never dispatched"
        );
        assert_eq!(
            bridge.shutdown(ShutdownMode::Drain, CallWait::Forever),
            ShutdownOutcome::Joined { provider: Ok(()) }
        );
    }

    #[test]
    fn dispatch_deadline_cas_has_one_winner_and_cancelled_work_never_calls_the_driver() {
        let state = Arc::new(FakeState::default());
        let mut bridge = fake_bridge(state.clone(), 1);
        let hook_gate = Arc::new(Barrier::default());
        let worker_gate = hook_gate.clone();
        bridge
            .shared
            .before_dispatch
            .lock()
            .expect("dispatch hook")
            .push_back(Box::new(move || worker_gate.arrive_and_wait_blocking()));
        let cell = Cell::new();
        bridge.shared.queued.fetch_add(1, Ordering::AcqRel);
        bridge
            .sender
            .try_send(Request::Load(
                Subject::new("ticket", "dispatch-race-cancelled").expect("subject"),
                cell.clone(),
            ))
            .expect("queue race request");
        hook_gate.wait_until_entered();
        assert!(matches!(
            wait_cell(
                &cell,
                CallWait::Until(Instant::now()),
                BridgeOperationIdentity::Read(BridgeReadKind::Load)
            ),
            Err(SyncReadError::Rejected(
                BridgeRejection::DeadlineBeforeDispatch
            ))
        ));
        hook_gate.release();
        wait_until(|| bridge.shared.queued.load(Ordering::Acquire) == 0);
        assert_eq!(state.call_count(), 0, "cancel won before driver dispatch");

        let dispatch_gate = state.arm_call();
        let dispatched = Cell::new();
        bridge.shared.queued.fetch_add(1, Ordering::AcqRel);
        bridge
            .sender
            .try_send(Request::Load(
                Subject::new("ticket", "dispatch-race-dispatched").expect("subject"),
                dispatched.clone(),
            ))
            .expect("queue dispatched request");
        dispatch_gate.wait_until_entered();
        assert!(matches!(
            wait_cell(
                &dispatched,
                CallWait::Until(Instant::now()),
                BridgeOperationIdentity::Read(BridgeReadKind::Load)
            ),
            Err(SyncReadError::AfterDispatch(BridgeAfterDispatch::Deadline))
        ));
        dispatch_gate.release();
        wait_until(|| dispatched.phase.load(Ordering::Acquire) == COMPLETED);
        assert_eq!(state.call_count(), 1, "dispatch won exactly once");
        assert_eq!(
            bridge.shutdown(ShutdownMode::Drain, CallWait::Forever),
            ShutdownOutcome::Joined { provider: Ok(()) }
        );
    }

    #[test]
    fn timed_out_caller_does_not_cancel_commit_and_same_key_retry_recovers_it() {
        let state = Arc::new(FakeState::default());
        let commit_gate = state.arm_call();
        let mut bridge = fake_bridge(state.clone(), 1);
        let request = append_request("lost-reply");
        let key = request.key.clone().expect("request key");
        let first_context = context("first-attempt");
        std::thread::scope(|scope| {
            let caller = scope.spawn(|| {
                bridge.operation(first_context).append(
                    request.clone(),
                    CallWait::Until(Instant::now() + Duration::from_millis(100)),
                )
            });
            commit_gate.wait_until_entered();
            let first = caller.join().expect("first caller");
            assert!(matches!(
                first,
                Err(SyncWriteError::Write(WriteFailure::Uncertain {
                    key: found,
                    ..
                })) if found == key
            ));
            assert_eq!(
                state.call_count(),
                1,
                "the bridge did not retry automatically"
            );
            commit_gate.release();
        });
        wait_until(|| {
            bridge
                .shared
                .dispatched
                .lock()
                .expect("dispatch state")
                .is_none()
        });
        assert_eq!(
            state.call_count(),
            1,
            "completion after caller return stayed one call"
        );
        let retry = bridge
            .operation(context("fresh-retry"))
            .append(request, CallWait::Forever)
            .expect("same-key semantic retry");
        assert!(retry.replayed());
        assert_eq!(state.call_count(), 2);
        let contexts = state.contexts.lock().expect("contexts");
        assert_eq!(contexts[0].request_id, "request-first-attempt");
        assert_eq!(contexts[1].request_id, "request-fresh-retry");
        drop(contexts);
        assert_eq!(
            bridge.shutdown(ShutdownMode::Drain, CallWait::Forever),
            ShutdownOutcome::Joined { provider: Ok(()) }
        );
    }

    #[test]
    fn every_executor_write_keeps_its_original_key_after_dispatch_deadline() {
        let state = Arc::new(FakeState::default());
        let mut bridge = fake_bridge(state.clone(), 1);
        let recording = |label: &str| Recording {
            record_id: format!("record-{label}"),
            recorded_at: "2026-09-16T00:00:00Z".into(),
            correlation: None,
            causation: None,
            actor: None,
        };
        let subject = Subject::new("ticket", "executor-deadline").expect("subject");

        let create = CreateRequest {
            subject: subject.clone(),
            definition_version: 1,
            fields: serde_json::json!({"title": "create"}),
            recording: recording("create"),
        };
        execution_deadline_case(
            &bridge,
            &state,
            BatchKey::SingleRecord("record-create".into()),
            |wait| bridge.operation(context("create")).create(create, wait),
        );

        let execute = ExecuteRequest {
            subject: subject.clone(),
            expected_revision: 1,
            operation: "close".into(),
            arguments: serde_json::json!({}),
            fulfillments: Default::default(),
            recording: recording("execute"),
        };
        execution_deadline_case(
            &bridge,
            &state,
            BatchKey::SingleRecord("record-execute".into()),
            |wait| bridge.operation(context("execute")).execute(execute, wait),
        );

        let observation = RecordedObservation {
            entity: "ticket".into(),
            id: "executor-deadline".into(),
            revision: 1,
            envelope: entity_store::Envelope::new(
                serde_json::json!({"observed": true}),
                "record-observe",
                "2026-09-16T00:00:00Z",
                None,
                None,
                None,
            )
            .expect("observation envelope"),
        };
        execution_deadline_case(
            &bridge,
            &state,
            BatchKey::SingleRecord("record-observe".into()),
            |wait| {
                bridge
                    .operation(context("observe"))
                    .observe(observation, wait)
            },
        );

        let batch_key = BatchKey::Named("executor-batch".into());
        let batch_create = CreateRequest {
            subject,
            definition_version: 1,
            fields: serde_json::json!({"title": "batch"}),
            recording: recording("batch"),
        };
        execution_deadline_case(&bridge, &state, batch_key.clone(), |wait| {
            bridge.operation(context("batch")).batch(
                batch_key,
                vec![BatchAction::Create(batch_create)],
                wait,
            )
        });

        assert!(
            bridge
                .load(
                    &Subject::new("ticket", "worker-continued").expect("subject"),
                    CallWait::Forever,
                )
                .expect("worker continued")
                .is_none()
        );
        assert_eq!(
            bridge.shutdown(ShutdownMode::Drain, CallWait::Forever),
            ShutdownOutcome::Joined { provider: Ok(()) }
        );
    }

    #[test]
    fn drain_finishes_in_order_and_escalation_cancels_only_queued_work() {
        let state = Arc::new(FakeState::default());
        let active_gate = state.arm_call();
        let mut bridge = fake_bridge(state.clone(), 2);
        let active = enqueue_load(&bridge, "drain-active");
        active_gate.wait_until_entered();
        let queued = enqueue_load(&bridge, "drain-queued");
        assert_eq!(
            bridge.shutdown(ShutdownMode::Drain, CallWait::Until(Instant::now())),
            ShutdownOutcome::TimedOut {
                queued: 1,
                dispatched: Some(BridgeOperationIdentity::Read(BridgeReadKind::Load)),
            }
        );
        assert!(matches!(
            bridge.load(
                &Subject::new("ticket", "no-revival").expect("subject"),
                CallWait::Forever,
            ),
            Err(SyncReadError::Rejected(BridgeRejection::Closed))
        ));
        assert_eq!(
            bridge.shutdown(ShutdownMode::CancelQueued, CallWait::Until(Instant::now())),
            ShutdownOutcome::TimedOut {
                queued: 1,
                dispatched: Some(BridgeOperationIdentity::Read(BridgeReadKind::Load)),
            }
        );
        active_gate.release();
        assert_eq!(
            bridge.shutdown(ShutdownMode::CancelQueued, CallWait::Forever),
            ShutdownOutcome::Joined { provider: Ok(()) }
        );
        assert!(matches!(
            wait_cell(
                &active,
                CallWait::Forever,
                BridgeOperationIdentity::Read(BridgeReadKind::Load)
            ),
            Ok(None)
        ));
        assert!(matches!(
            wait_cell(
                &queued,
                CallWait::Forever,
                BridgeOperationIdentity::Read(BridgeReadKind::Load)
            ),
            Err(SyncReadError::Rejected(BridgeRejection::Closed))
        ));
        assert_eq!(
            state.call_count(),
            1,
            "queued work did not dispatch after escalation"
        );
        assert_eq!(state.retire_calls.load(Ordering::Acquire), 1);

        let drain_state = Arc::new(FakeState::default());
        let drain_gate = drain_state.arm_call();
        let mut drain = fake_bridge(drain_state.clone(), 2);
        let first = enqueue_load(&drain, "ordered-first");
        drain_gate.wait_until_entered();
        let second = enqueue_load(&drain, "ordered-second");
        assert!(matches!(
            drain.shutdown(ShutdownMode::Drain, CallWait::Until(Instant::now())),
            ShutdownOutcome::TimedOut { queued: 1, .. }
        ));
        drain_gate.release();
        assert_eq!(
            drain.shutdown(ShutdownMode::Drain, CallWait::Forever),
            ShutdownOutcome::Joined { provider: Ok(()) }
        );
        assert!(matches!(
            wait_cell(
                &first,
                CallWait::Forever,
                BridgeOperationIdentity::Read(BridgeReadKind::Load)
            ),
            Ok(None)
        ));
        assert!(matches!(
            wait_cell(
                &second,
                CallWait::Forever,
                BridgeOperationIdentity::Read(BridgeReadKind::Load)
            ),
            Ok(None)
        ));
        assert_eq!(
            *drain_state.calls.lock().expect("calls"),
            vec!["load:ordered-first", "load:ordered-second"]
        );
    }

    #[test]
    fn retirement_timeout_later_joins_once_and_preserves_provider_failure() {
        let state = Arc::new(FakeState::default());
        let retirement = Arc::new(Barrier::default());
        *state.retire_gate.lock().expect("retire gate") = Some(retirement.clone());
        *state.retire_result.lock().expect("retire result") = Err(AsyncStoreError::Backend(
            "controlled retirement failure".into(),
        ));
        let mut bridge = fake_bridge(state.clone(), 1);
        assert_eq!(
            bridge.shutdown(ShutdownMode::Drain, CallWait::Until(Instant::now())),
            ShutdownOutcome::TimedOut {
                queued: 0,
                dispatched: None,
            }
        );
        retirement.wait_until_entered();
        retirement.release();
        let expected = ShutdownOutcome::Joined {
            provider: Err(AsyncStoreError::Backend(
                "controlled retirement failure".into(),
            )),
        };
        assert_eq!(
            bridge.shutdown(ShutdownMode::Drain, CallWait::Forever),
            expected
        );
        assert_eq!(
            bridge.shutdown(ShutdownMode::CancelQueued, CallWait::Forever),
            expected,
            "later shutdown returns the cached retirement result"
        );
        assert_eq!(state.retire_calls.load(Ordering::Acquire), 1);
        assert!(bridge.join.is_none());
    }

    #[test]
    fn worker_panics_cancel_real_queued_cells_without_stranding_waiters() {
        let before_state = Arc::new(FakeState::default());
        let mut before = fake_bridge(before_state.clone(), 2);
        let hook_gate = Arc::new(Barrier::default());
        let worker_gate = hook_gate.clone();
        before
            .shared
            .before_dispatch
            .lock()
            .expect("dispatch hook")
            .push_back(Box::new(move || {
                worker_gate.arrive_and_wait_blocking();
                panic!("controlled panic before dispatch");
            }));
        let current = enqueue_load(&before, "panic-before-current");
        hook_gate.wait_until_entered();
        let queued = enqueue_load(&before, "panic-before-queued");
        hook_gate.release();
        assert!(matches!(
            wait_cell(
                &current,
                CallWait::Forever,
                BridgeOperationIdentity::Read(BridgeReadKind::Load)
            ),
            Err(SyncReadError::Rejected(
                BridgeRejection::WorkerStoppedBeforeDispatch
            ))
        ));
        assert!(matches!(
            wait_cell(
                &queued,
                CallWait::Forever,
                BridgeOperationIdentity::Read(BridgeReadKind::Load)
            ),
            Err(SyncReadError::Rejected(
                BridgeRejection::WorkerStoppedBeforeDispatch
            ))
        ));
        assert_eq!(before_state.call_count(), 0);
        assert_eq!(
            before.shutdown(ShutdownMode::CancelQueued, CallWait::Forever),
            ShutdownOutcome::Joined { provider: Ok(()) }
        );

        let after_state = Arc::new(FakeState::default());
        after_state.panic_next.store(true, Ordering::Release);
        let mut after = fake_bridge(after_state.clone(), 2);
        let after_hook = Arc::new(Barrier::default());
        let worker_hook = after_hook.clone();
        after
            .shared
            .before_dispatch
            .lock()
            .expect("dispatch hook")
            .push_back(Box::new(move || worker_hook.arrive_and_wait_blocking()));
        let active = enqueue_load(&after, "panic-after-active");
        after_hook.wait_until_entered();
        let queued = enqueue_load(&after, "panic-after-queued");
        after_hook.release();
        assert!(matches!(
            wait_cell(
                &active,
                CallWait::Forever,
                BridgeOperationIdentity::Read(BridgeReadKind::Load)
            ),
            Err(SyncReadError::AfterDispatch(
                BridgeAfterDispatch::WorkerPanicked
            ))
        ));
        assert!(matches!(
            wait_cell(
                &queued,
                CallWait::Forever,
                BridgeOperationIdentity::Read(BridgeReadKind::Load)
            ),
            Err(SyncReadError::Rejected(
                BridgeRejection::WorkerStoppedBeforeDispatch
            ))
        ));
        assert_eq!(after_state.call_count(), 1);
        assert_eq!(
            after.shutdown(ShutdownMode::CancelQueued, CallWait::Forever),
            ShutdownOutcome::Joined { provider: Ok(()) }
        );

        let stopped_state = Arc::new(FakeState::default());
        let mut stopped = fake_bridge(stopped_state.clone(), 2);
        let stopped_hook = Arc::new(Barrier::default());
        let worker_hook = stopped_hook.clone();
        stopped
            .shared
            .after_dispatch
            .lock()
            .expect("dispatch hook")
            .push_back(Box::new(move || {
                worker_hook.arrive_and_wait_blocking();
                panic!("controlled disappearance after dispatch");
            }));
        let active = enqueue_load(&stopped, "stopped-after-active");
        stopped_hook.wait_until_entered();
        let queued = enqueue_load(&stopped, "stopped-after-queued");
        stopped_hook.release();
        assert!(matches!(
            wait_cell(
                &active,
                CallWait::Forever,
                BridgeOperationIdentity::Read(BridgeReadKind::Load)
            ),
            Err(SyncReadError::AfterDispatch(
                BridgeAfterDispatch::WorkerStopped
            ))
        ));
        assert!(matches!(
            wait_cell(
                &queued,
                CallWait::Forever,
                BridgeOperationIdentity::Read(BridgeReadKind::Load)
            ),
            Err(SyncReadError::Rejected(
                BridgeRejection::WorkerStoppedBeforeDispatch
            ))
        ));
        assert_eq!(stopped_state.call_count(), 0);
        assert_eq!(
            stopped.shutdown(ShutdownMode::CancelQueued, CallWait::Forever),
            ShutdownOutcome::Joined { provider: Ok(()) }
        );
    }

    #[test]
    fn worker_panic_closes_admission_before_terminal_drain_and_wakes_late_accepted_caller() {
        let state = Arc::new(FakeState::default());
        let retirement = Arc::new(Barrier::default());
        *state.retire_gate.lock().expect("retire gate") = Some(retirement.clone());
        let mut bridge = fake_bridge(state.clone(), 2);

        let active_gate = state.arm_call();
        state.panic_after_gate.store(true, Ordering::Release);
        let active = enqueue_load(&bridge, "terminal-active");
        let active_entered = active_gate.wait_until_entered_for(Duration::from_secs(2));

        let admission_gate = Arc::new(Barrier::default());
        let caller_gate = admission_gate.clone();
        bridge
            .shared
            .after_admission_check
            .lock()
            .expect("admission hook")
            .push_back(Box::new(move || caller_gate.arrive_and_wait_blocking()));
        let (enqueued_tx, enqueued_rx) = std::sync::mpsc::channel();
        let (drained_tx, drained_rx) = std::sync::mpsc::channel();
        let drained_while_admitted = Arc::new(AtomicBool::new(false));
        let observed_drain = drained_while_admitted.clone();
        bridge
            .shared
            .after_enqueue
            .lock()
            .expect("enqueue hook")
            .push_back(Box::new(move || {
                let _ = enqueued_tx.send(());
                if drained_rx.recv_timeout(Duration::from_millis(250)).is_ok() {
                    observed_drain.store(true, Ordering::Release);
                }
            }));
        let (terminal_tx, terminal_rx) = std::sync::mpsc::channel();
        bridge
            .shared
            .before_terminal_admission
            .lock()
            .expect("terminal admission hook")
            .push_back(Box::new(move || {
                let _ = terminal_tx.send(());
            }));
        bridge
            .shared
            .after_terminal_drain
            .lock()
            .expect("terminal drain hook")
            .push_back(Box::new(move || {
                let _ = drained_tx.send(());
            }));

        let late = Cell::new();
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        let mut admission_entered = false;
        let mut terminal_entered = false;
        let mut late_enqueued = false;
        let mut retirement_entered = false;
        let mut completed_on_time = false;
        let mut late_result = None;
        std::thread::scope(|scope| {
            let caller_cell = late.clone();
            let sender = &bridge.sender;
            let worker_id = bridge.worker_id;
            let shared = &bridge.shared;
            let capacity = bridge.capacity;
            let caller = scope.spawn(move || {
                let result = submit_request(
                    sender,
                    worker_id,
                    shared,
                    capacity,
                    Request::Load(
                        Subject::new("ticket", "terminal-late").expect("subject"),
                        caller_cell.clone(),
                    ),
                    &caller_cell,
                    CallWait::Forever,
                    BridgeOperationIdentity::Read(BridgeReadKind::Load),
                );
                let _ = result_tx.send(result);
            });

            admission_entered = admission_gate.wait_until_entered_for(Duration::from_secs(2));
            active_gate.release();
            terminal_entered = terminal_rx.recv_timeout(Duration::from_secs(2)).is_ok();
            admission_gate.release();
            late_enqueued = enqueued_rx.recv_timeout(Duration::from_secs(2)).is_ok();
            retirement_entered = retirement.wait_until_entered_for(Duration::from_secs(2));
            if let Ok(result) = result_rx.recv_timeout(Duration::from_secs(2)) {
                completed_on_time = true;
                late_result = Some(result);
            }
            retirement.release();
            if !completed_on_time {
                late.complete(Err(SyncReadError::Rejected(BridgeRejection::Closed)));
                late_result = result_rx.recv_timeout(Duration::from_secs(2)).ok();
            }
            caller.join().expect("caller");
        });

        assert!(active_entered, "active driver call did not reach its gate");
        assert!(admission_entered, "late caller did not enter admission");
        assert!(terminal_entered, "worker did not begin terminal admission");
        assert!(late_enqueued, "late caller did not enqueue after admission");
        assert!(
            !drained_while_admitted.load(Ordering::Acquire),
            "worker drained before the admitted caller finished enqueueing"
        );
        assert!(
            retirement_entered,
            "worker did not begin provider retirement"
        );
        assert!(
            completed_on_time,
            "late accepted Forever caller was not completed by terminal drain"
        );
        assert!(matches!(
            late_result,
            Some(Err(SyncReadError::Rejected(
                BridgeRejection::WorkerStoppedBeforeDispatch
            )))
        ));
        assert!(matches!(
            wait_cell(
                &active,
                CallWait::Forever,
                BridgeOperationIdentity::Read(BridgeReadKind::Load)
            ),
            Err(SyncReadError::AfterDispatch(
                BridgeAfterDispatch::WorkerPanicked
            ))
        ));
        assert_eq!(bridge.shared.queued.load(Ordering::Acquire), 0);
        assert_eq!(state.call_count(), 1);
        assert_eq!(
            bridge.shutdown(ShutdownMode::CancelQueued, CallWait::Forever),
            ShutdownOutcome::Joined { provider: Ok(()) }
        );
    }

    #[test]
    fn drop_requests_closure_without_waiting_for_an_active_future() {
        let state = Arc::new(FakeState::default());
        let active_gate = state.arm_call();
        let bridge = fake_bridge(state.clone(), 1);
        let active = enqueue_load(&bridge, "drop-active");
        active_gate.wait_until_entered();
        let shared = bridge.shared.clone();
        drop(bridge);
        assert_eq!(shared.lifecycle.load(Ordering::Acquire), CLOSING_CANCEL);
        assert!(state.dropped_on.lock().expect("dropped thread").is_none());
        active_gate.release();
        wait_until(|| state.dropped_on.lock().expect("dropped thread").is_some());
        assert!(matches!(
            wait_cell(
                &active,
                CallWait::Forever,
                BridgeOperationIdentity::Read(BridgeReadKind::Load)
            ),
            Ok(None)
        ));
        assert_eq!(state.retire_calls.load(Ordering::Acquire), 1);
    }

    #[test]
    fn panicked_or_stopped_worker_keeps_dispatched_write_uncertainty() {
        let panic_state = Arc::new(FakeState::default());
        panic_state.panic_next.store(true, Ordering::Release);
        let mut panicked = fake_bridge(panic_state, 1);
        let panic_request = append_request("panic-write-loop");
        let panic_key = panic_request.key.clone().expect("key");
        assert!(matches!(
            panicked
                .operation(context("panic-write-loop"))
                .append(panic_request, CallWait::Forever),
            Err(SyncWriteError::Write(WriteFailure::Uncertain {
                key: found,
                ..
            })) if found == panic_key
        ));
        assert_eq!(
            panicked.shutdown(ShutdownMode::CancelQueued, CallWait::Forever),
            ShutdownOutcome::Joined { provider: Ok(()) }
        );

        let stopped_state = Arc::new(FakeState::default());
        let mut stopped = fake_bridge(stopped_state, 1);
        stopped
            .shared
            .after_dispatch
            .lock()
            .expect("dispatch hook")
            .push_back(Box::new(|| panic!("controlled stopped write")));
        let stopped_request = append_request("stopped-write-loop");
        let stopped_key = stopped_request.key.clone().expect("key");
        assert!(matches!(
            stopped
                .operation(context("stopped-write-loop"))
                .append(stopped_request, CallWait::Forever),
            Err(SyncWriteError::Write(WriteFailure::Uncertain {
                key: found,
                ..
            })) if found == stopped_key
        ));
        assert_eq!(
            stopped.shutdown(ShutdownMode::CancelQueued, CallWait::Forever),
            ShutdownOutcome::Joined { provider: Ok(()) }
        );
    }

    #[test]
    fn production_worker_preserves_every_driver_error_classification() {
        let state = Arc::new(FakeState::default());
        let mut bridge = fake_bridge(state.clone(), 1);
        let subject = Subject::new("ticket", "errors").expect("subject");
        let errors = vec![
            AsyncStoreError::InvalidInput("invalid".into()),
            AsyncStoreError::Encoding("encoding".into()),
            AsyncStoreError::RevisionConflict {
                subject: subject.clone(),
                expected: Expect::Absent,
                found: Some(1),
            },
            AsyncStoreError::RecordConflict {
                record_id: "record".into(),
            },
            AsyncStoreError::DuplicateRecordId {
                record_id: "record".into(),
            },
            AsyncStoreError::BatchConflict {
                key: BatchKey::Named("batch".into()),
            },
            AsyncStoreError::PreviouslyRecordedBatchEntries { indices: vec![0] },
            AsyncStoreError::CorruptHistory {
                subject: subject.clone(),
                detail: "corrupt".into(),
            },
            AsyncStoreError::ProviderIntegrity {
                provider: "fake".into(),
                detail: "integrity".into(),
            },
            AsyncStoreError::HistoricalRetryUnverifiable {
                record_id: "legacy".into(),
                detail: "missing".into(),
            },
            AsyncStoreError::PositionExhausted {
                domain: "position".into(),
            },
            AsyncStoreError::Unreachable {
                provider: "fake".into(),
                detail: "offline".into(),
            },
            AsyncStoreError::Backend("backend".into()),
        ];
        for error in errors {
            *state.next_read_error.lock().expect("read error") = Some(error.clone());
            assert!(matches!(
                bridge.load(&subject, CallWait::Forever),
                Err(SyncReadError::Store(found)) if found == error
            ));
        }

        let not_committed = AsyncStoreError::Backend("write refused".into());
        *state.next_write_error.lock().expect("write error") =
            Some(WriteFailure::NotCommitted(not_committed.clone()));
        assert!(matches!(
            bridge
                .operation(context("not-committed"))
                .append(append_request("not-committed"), CallWait::Forever),
            Err(SyncWriteError::Write(WriteFailure::NotCommitted(found)))
                if found == not_committed
        ));
        let uncertain_request = append_request("provider-uncertain");
        let uncertain_key = uncertain_request.key.clone().expect("key");
        *state.next_write_error.lock().expect("write error") = Some(WriteFailure::Uncertain {
            key: uncertain_key.clone(),
            cause: "provider uncertainty".into(),
        });
        assert!(matches!(
            bridge
                .operation(context("provider-uncertain"))
                .append(uncertain_request, CallWait::Forever),
            Err(SyncWriteError::Write(WriteFailure::Uncertain {
                key: found,
                cause,
            })) if found == uncertain_key && cause == "provider uncertainty"
        ));

        let create = |label: &str| CreateRequest {
            subject: Subject::new("ticket", label).expect("subject"),
            definition_version: 1,
            fields: serde_json::json!({"title": label}),
            recording: Recording {
                record_id: format!("record-{label}"),
                recorded_at: "2026-09-16T00:00:00Z".into(),
                correlation: None,
                causation: None,
                actor: None,
            },
        };
        *state.next_execution_error.lock().expect("execution error") = Some(ExecutionFailure::Core);
        assert!(matches!(
            bridge
                .operation(context("core"))
                .create(create("core"), CallWait::Forever),
            Err(SyncExecutionError::Execution(ExecutionError::Core(
                CoreError::EntityNotRegistered { .. }
            )))
        ));
        let store_error = AsyncStoreError::Backend("execution store".into());
        *state.next_execution_error.lock().expect("execution error") =
            Some(ExecutionFailure::Store(store_error.clone()));
        assert!(matches!(
            bridge
                .operation(context("store"))
                .create(create("store"), CallWait::Forever),
            Err(SyncExecutionError::Execution(ExecutionError::Store(found)))
                if found == store_error
        ));
        let execution_key = BatchKey::SingleRecord("record-execution-write".into());
        *state.next_execution_error.lock().expect("execution error") =
            Some(ExecutionFailure::Write(WriteFailure::Uncertain {
                key: execution_key.clone(),
                cause: "execution provider".into(),
            }));
        assert!(matches!(
            bridge
                .operation(context("execution-write"))
                .create(create("execution-write"), CallWait::Forever),
            Err(SyncExecutionError::Execution(ExecutionError::Write(
                WriteFailure::Uncertain { key: found, cause }
            ))) if found == execution_key && cause == "execution provider"
        ));

        assert_eq!(
            bridge.shutdown(ShutdownMode::Drain, CallWait::Forever),
            ShutdownOutcome::Joined { provider: Ok(()) }
        );
    }

    #[test]
    fn an_expired_queued_call_is_cancelled_before_dispatch() {
        let cell = Cell::<Result<Option<EntityInstance>, SyncReadError>>::new();
        let result = wait_cell(
            &cell,
            CallWait::Until(Instant::now()),
            BridgeOperationIdentity::Read(BridgeReadKind::Load),
        );
        assert!(matches!(
            result,
            Err(SyncReadError::Rejected(
                BridgeRejection::DeadlineBeforeDispatch
            ))
        ));
        assert_eq!(cell.phase.load(Ordering::Acquire), CANCELLED);
    }

    #[test]
    fn an_expired_dispatched_write_retains_its_original_key_as_uncertain() {
        let cell = Cell::<Result<AppendOutcome, SyncWriteError>>::new();
        cell.phase.store(DISPATCHED, Ordering::Release);
        let key = BatchKey::Named("opaque-key".into());
        let result = wait_cell(
            &cell,
            CallWait::Until(Instant::now()),
            BridgeOperationIdentity::Write(key.clone()),
        );
        assert!(matches!(
            result,
            Err(SyncWriteError::Write(WriteFailure::Uncertain {
                key: found,
                ..
            })) if found == key
        ));
        assert_eq!(cell.phase.load(Ordering::Acquire), DISPATCHED);
    }

    #[test]
    fn completion_wins_a_deadline_race_without_reclassification() {
        let cell = Cell::<Result<Option<EntityInstance>, SyncReadError>>::new();
        cell.complete(Ok(None));
        assert!(matches!(
            wait_cell(
                &cell,
                CallWait::Until(Instant::now()),
                BridgeOperationIdentity::Read(BridgeReadKind::Load),
            ),
            Ok(None)
        ));
    }

    #[test]
    fn queue_capacity_and_closed_admission_are_deterministic() {
        let (bridge, _receiver) = inert_bridge(RUNNING, 1);
        bridge.sender.try_send(Request::Wake).expect("fill queue");
        assert!(matches!(
            bridge.load(
                &Subject::new("ticket", "full").expect("subject"),
                CallWait::Forever
            ),
            Err(SyncReadError::Rejected(BridgeRejection::QueueFull {
                capacity: 1
            }))
        ));
        assert_eq!(bridge.shared.queued.load(Ordering::Acquire), 0);

        let (closed, _receiver) = inert_bridge(CLOSING_CANCEL, 1);
        assert!(matches!(
            closed.load(
                &Subject::new("ticket", "closed").expect("subject"),
                CallWait::Forever
            ),
            Err(SyncReadError::Rejected(BridgeRejection::Closed))
        ));
    }

    #[test]
    fn reentrant_admission_is_rejected_before_queue_accounting() {
        let (sender, _receiver) = sync_channel(1);
        let bridge = RecordedEventlogBridge {
            sender,
            worker_id: std::thread::current().id(),
            join: None,
            shared: shared(RUNNING),
            capacity: 1,
            joined: None,
        };
        assert!(matches!(
            bridge.load(
                &Subject::new("ticket", "reentrant").expect("subject"),
                CallWait::Forever
            ),
            Err(SyncReadError::Rejected(BridgeRejection::Reentrant))
        ));
        assert_eq!(bridge.shared.queued.load(Ordering::Acquire), 0);
    }

    #[test]
    fn queued_shutdown_cancellation_returns_closed_for_reads_and_writes() {
        let read = Cell::<Result<Option<EntityInstance>, SyncReadError>>::new();
        Request::Load(
            Subject::new("ticket", "queued-read").expect("subject"),
            read.clone(),
        )
        .cancel_closed();
        assert!(matches!(
            wait_cell(
                &read,
                CallWait::Forever,
                BridgeOperationIdentity::Read(BridgeReadKind::Load)
            ),
            Err(SyncReadError::Rejected(BridgeRejection::Closed))
        ));

        let key = BatchKey::Named("queued-write".into());
        let write = Cell::<Result<AppendOutcome, SyncWriteError>>::new();
        Request::Append(
            EventlogOperationContext {
                subject: "subject".into(),
                actor: "actor".into(),
                request_id: "request".into(),
                trace_id: "trace".into(),
                causation_id: None,
                causation_depth: 0,
                occurred_at: time::OffsetDateTime::UNIX_EPOCH,
            },
            AppendRequest::empty(),
            key.clone(),
            write.clone(),
        )
        .cancel_closed();
        assert!(matches!(
            wait_cell(
                &write,
                CallWait::Forever,
                BridgeOperationIdentity::Write(key)
            ),
            Err(SyncWriteError::Rejected(BridgeRejection::Closed))
        ));
    }

    #[test]
    fn panics_after_dispatch_keep_read_and_write_classification() {
        let read = Cell::<Result<Option<EntityInstance>, SyncReadError>>::new();
        assert!(poll_read(read.clone(), || -> Result<_, AsyncStoreError> {
            panic!("read panic")
        }));
        assert!(matches!(
            wait_cell(
                &read,
                CallWait::Forever,
                BridgeOperationIdentity::Read(BridgeReadKind::Load)
            ),
            Err(SyncReadError::AfterDispatch(
                BridgeAfterDispatch::WorkerPanicked
            ))
        ));

        let key = BatchKey::Named("panic-write".into());
        let write = Cell::<Result<AppendOutcome, SyncExecutionError>>::new();
        assert!(poll_execution(write.clone(), key.clone(), || {
            panic!("write panic")
        }));
        assert!(matches!(
            wait_cell(
                &write,
                CallWait::Forever,
                BridgeOperationIdentity::Write(key.clone())
            ),
            Err(SyncExecutionError::Execution(ExecutionError::Write(
                WriteFailure::Uncertain { key: found, .. }
            ))) if found == key
        ));
    }

    #[test]
    fn provider_errors_pass_through_without_bridge_reclassification() {
        let read = Cell::<Result<Option<EntityInstance>, SyncReadError>>::new();
        assert!(!poll_read(read.clone(), || {
            Err(AsyncStoreError::Backend("read backend".into()))
        }));
        assert!(matches!(
            wait_cell(
                &read,
                CallWait::Forever,
                BridgeOperationIdentity::Read(BridgeReadKind::Load)
            ),
            Err(SyncReadError::Store(AsyncStoreError::Backend(detail)))
                if detail == "read backend"
        ));

        let key = BatchKey::Named("execution-backend".into());
        let execution = Cell::<Result<AppendOutcome, SyncExecutionError>>::new();
        assert!(!poll_execution(execution.clone(), key.clone(), || {
            Err(ExecutionError::Write(WriteFailure::Uncertain {
                key: key.clone(),
                cause: "provider result".into(),
            }))
        }));
        assert!(matches!(
            wait_cell(
                &execution,
                CallWait::Forever,
                BridgeOperationIdentity::Write(key.clone())
            ),
            Err(SyncExecutionError::Execution(ExecutionError::Write(
                WriteFailure::Uncertain { key: found, cause }
            ))) if found == key && cause == "provider result"
        ));
    }

    #[test]
    fn completion_after_a_dispatched_waiter_times_out_is_safe() {
        let cell = Cell::<Result<Option<EntityInstance>, SyncReadError>>::new();
        cell.phase.store(DISPATCHED, Ordering::Release);
        assert!(matches!(
            wait_cell(
                &cell,
                CallWait::Until(Instant::now()),
                BridgeOperationIdentity::Read(BridgeReadKind::Load)
            ),
            Err(SyncReadError::AfterDispatch(BridgeAfterDispatch::Deadline))
        ));
        cell.complete(Ok(None));
        assert_eq!(cell.phase.load(Ordering::Acquire), COMPLETED);
    }

    #[test]
    fn shutdown_joins_once_and_caches_provider_retirement_failure() {
        let (sender, receiver) = sync_channel(1);
        let shared = shared(RUNNING);
        let worker_shared = shared.clone();
        let (id_tx, id_rx) = std::sync::mpsc::channel();
        let join = std::thread::spawn(move || {
            id_tx.send(std::thread::current().id()).expect("worker id");
            assert!(matches!(receiver.recv(), Ok(Request::Wake)));
            finish(
                &worker_shared,
                Err(AsyncStoreError::Backend("retirement failed".into())),
            );
        });
        let mut bridge = RecordedEventlogBridge {
            sender,
            worker_id: id_rx.recv().expect("worker id"),
            join: Some(join),
            shared,
            capacity: 1,
            joined: None,
        };
        let expected = ShutdownOutcome::Joined {
            provider: Err(AsyncStoreError::Backend("retirement failed".into())),
        };
        assert_eq!(
            bridge.shutdown(ShutdownMode::Drain, CallWait::Forever),
            expected
        );
        assert_eq!(
            bridge.shutdown(ShutdownMode::CancelQueued, CallWait::Forever),
            expected
        );
        assert!(bridge.join.is_none());
    }

    #[test]
    fn timed_out_shutdown_reports_in_flight_identity_and_can_escalate() {
        let (sender, receiver) = sync_channel(2);
        let shared = shared(RUNNING);
        shared.queued.store(1, Ordering::Release);
        let key = BatchKey::Named("in-flight".into());
        *shared.dispatched.lock().expect("dispatch") =
            Some(BridgeOperationIdentity::Write(key.clone()));
        let worker_shared = shared.clone();
        let (id_tx, id_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let join = std::thread::spawn(move || {
            id_tx.send(std::thread::current().id()).expect("worker id");
            assert!(matches!(receiver.recv(), Ok(Request::Wake)));
            release_rx.recv().expect("release");
            worker_shared.queued.store(0, Ordering::Release);
            *worker_shared.dispatched.lock().expect("dispatch") = None;
            finish(&worker_shared, Ok(()));
        });
        let mut bridge = RecordedEventlogBridge {
            sender,
            worker_id: id_rx.recv().expect("worker id"),
            join: Some(join),
            shared,
            capacity: 2,
            joined: None,
        };
        assert_eq!(
            bridge.shutdown(ShutdownMode::Drain, CallWait::Until(Instant::now())),
            ShutdownOutcome::TimedOut {
                queued: 1,
                dispatched: Some(BridgeOperationIdentity::Write(key)),
            }
        );
        release_tx.send(()).expect("release");
        assert_eq!(
            bridge.shutdown(ShutdownMode::CancelQueued, CallWait::Forever),
            ShutdownOutcome::Joined { provider: Ok(()) }
        );
    }

    #[test]
    fn outer_worker_panic_marks_finished_and_preserves_a_retirement_failure() {
        let shared = shared(RUNNING);
        let (_sender, receiver) = sync_channel(1);
        contain_worker(shared.clone(), &receiver, || panic!("worker disappeared"));
        assert!(*shared.finished.0.lock().expect("finish lock"));
        assert_eq!(shared.lifecycle.load(Ordering::Acquire), CLOSING_CANCEL);
        assert!(matches!(
            shared.provider.lock().expect("provider result").as_ref(),
            Some(Err(AsyncStoreError::Backend(detail)))
                if detail == "sync bridge worker disappeared after panic"
        ));
    }

    #[test]
    fn drop_after_shutdown_timeout_escalates_and_never_waits_for_the_worker() {
        let (sender, receiver) = sync_channel(2);
        let shared = shared(RUNNING);
        let worker_shared = shared.clone();
        let (id_tx, id_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let join = std::thread::spawn(move || {
            id_tx.send(std::thread::current().id()).expect("worker id");
            assert!(matches!(receiver.recv(), Ok(Request::Wake)));
            release_rx.recv().expect("release");
            finish(&worker_shared, Ok(()));
            done_tx.send(()).expect("done");
        });
        let mut bridge = RecordedEventlogBridge {
            sender,
            worker_id: id_rx.recv().expect("worker id"),
            join: Some(join),
            shared: shared.clone(),
            capacity: 2,
            joined: None,
        };
        assert!(matches!(
            bridge.shutdown(ShutdownMode::Drain, CallWait::Until(Instant::now())),
            ShutdownOutcome::TimedOut { .. }
        ));
        drop(bridge);
        assert_eq!(shared.lifecycle.load(Ordering::Acquire), CLOSING_CANCEL);
        release_tx.send(()).expect("release");
        done_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("detached worker completed");
    }
}
