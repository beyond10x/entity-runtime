//! Explicit synchronous facade over the asynchronous recorded adapter and executor.

use std::{
    num::NonZeroU16,
    path::PathBuf,
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
        AsyncStoreError, BatchKey, CompleteStoreSnapshot, RecordLookup, StoredBatch, Subject,
        SubjectHistory, WriteFailure,
    },
};
use eventlog_core::InlineProjectionAdmin;

use crate::{
    Authority, ErRecordedProjector, EventlogBackend, EventlogOperationContext,
    EventlogRecordedStore,
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
}

impl std::fmt::Debug for EventlogRecordedStoreOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EventlogRecordedStoreOwner(..)")
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
    async fn open(self) -> Result<(EventlogRecordedStore, OwnedBackend), AsyncStoreError> {
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
                    eventlog_sqlite::SqliteEventStore::open(&path, &prefix)
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
        }
    }
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

/// Startup refusal before a usable owner is returned.
#[derive(Debug)]
pub enum BridgeStartError {
    /// The operating system refused the worker thread.
    ThreadSpawn(std::io::Error),
    /// Tokio could not construct the owned runtime.
    RuntimeBuild(std::io::Error),
    /// The worker could not open and verify its provider.
    Open(AsyncStoreError),
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
    queued: AtomicUsize,
    dispatched: Mutex<Option<BridgeOperationIdentity>>,
    finished: (Mutex<bool>, Condvar),
    provider: Mutex<Option<Result<(), AsyncStoreError>>>,
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
        let (sender, receiver) = sync_channel(usize::from(config.queue_capacity.get()));
        let shared = Arc::new(Shared {
            lifecycle: AtomicU8::new(RUNNING),
            queued: AtomicUsize::new(0),
            dispatched: Mutex::new(None),
            finished: (Mutex::new(false), Condvar::new()),
            provider: Mutex::new(None),
        });
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let worker_shared = shared.clone();
        let join = std::thread::Builder::new()
            .name("entity-eventlog".into())
            .spawn(move || {
                contain_worker(worker_shared.clone(), || {
                    worker_main(registry, owner, receiver, worker_shared, ready_tx);
                });
            })
            .map_err(BridgeStartError::ThreadSpawn)?;
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
        if std::thread::current().id() == self.worker_id {
            return Err(E::rejected(BridgeRejection::Reentrant));
        }
        if matches!(wait,CallWait::Until(deadline) if deadline<=Instant::now()) {
            return Err(E::rejected(BridgeRejection::DeadlineBeforeAcceptance));
        }
        if self.shared.lifecycle.load(Ordering::Acquire) != RUNNING {
            return Err(E::rejected(BridgeRejection::Closed));
        }
        self.shared.queued.fetch_add(1, Ordering::AcqRel);
        match self.sender.try_send(request) {
            Ok(()) => wait_cell(cell, wait, identity),
            Err(TrySendError::Full(_)) => {
                self.shared.queued.fetch_sub(1, Ordering::AcqRel);
                Err(E::rejected(BridgeRejection::QueueFull {
                    capacity: self.capacity,
                }))
            }
            Err(TrySendError::Disconnected(_)) => {
                self.shared.queued.fetch_sub(1, Ordering::AcqRel);
                Err(E::rejected(BridgeRejection::Closed))
            }
        }
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
    registry: Registry,
    owner: EventlogRecordedStoreOwner,
    receiver: Receiver<Request>,
    shared: Arc<Shared>,
    ready: std::sync::mpsc::Sender<Result<ThreadId, BridgeStartError>>,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
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
    let (store, backend) = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime.block_on(owner.open())
    })) {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            let _ = ready.send(Err(BridgeStartError::Open(e)));
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
        finish(&shared, runtime.block_on(backend.retire()));
        return;
    }
    while let Ok(request) = receiver.recv() {
        if matches!(request, Request::Wake) {
            if shared.lifecycle.load(Ordering::Acquire) != RUNNING
                && shared.queued.load(Ordering::Acquire) == 0
            {
                break;
            }
            continue;
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
        let panicked = drive_request(&runtime, &registry, &store, request);
        *shared.dispatched.lock().expect("dispatch state") = None;
        if panicked {
            shared.lifecycle.store(CLOSING_CANCEL, Ordering::Release);
            break;
        }
        if shared.lifecycle.load(Ordering::Acquire) != RUNNING
            && shared.queued.load(Ordering::Acquire) == 0
        {
            break;
        }
    }
    while let Ok(request) = receiver.try_recv() {
        request.cancel_closed();
    }
    let retired = runtime.block_on(backend.retire());
    finish(&shared, retired);
}

fn contain_worker(shared: Arc<Shared>, work: impl FnOnce()) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).is_err() {
        shared.lifecycle.store(CLOSING_CANCEL, Ordering::Release);
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
    registry: &Registry,
    store: &EventlogRecordedStore,
    request: Request,
) -> bool {
    match request {
        Request::Load(v, c) => poll_read(c, || runtime.block_on(store.load(&v))),
        Request::LookupRecord(v, c) => poll_read(c, || runtime.block_on(store.lookup_record(&v))),
        Request::LookupBatch(v, c) => poll_read(c, || runtime.block_on(store.lookup_batch(&v))),
        Request::History(v, c) => poll_read(c, || runtime.block_on(store.history(&v))),
        Request::Snapshot(v, c) => poll_read(c, || runtime.block_on(store.complete_snapshot(&v))),
        Request::Append(context, v, key, c) => {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                runtime.block_on(store.operation(context).append(v))
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
        Request::Create(context, v, key, c) => poll_execution(c, key, || {
            let operation = store.operation(context);
            runtime.block_on(Executor::new(registry, &operation).create(v))
        }),
        Request::Execute(context, v, key, c) => poll_execution(c, key, || {
            let operation = store.operation(context);
            runtime.block_on(Executor::new(registry, &operation).execute(v))
        }),
        Request::Observe(context, v, key, c) => poll_execution(c, key, || {
            let operation = store.operation(context);
            runtime.block_on(Executor::new(registry, &operation).observe(v))
        }),
        Request::Batch(context, key, v, c) => {
            let panic_key = key.clone();
            poll_execution(c, panic_key, || {
                let operation = store.operation(context);
                runtime.block_on(Executor::new(registry, &operation).batch(key, v))
            })
        }
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

    fn shared(lifecycle: u8) -> Arc<Shared> {
        Arc::new(Shared {
            lifecycle: AtomicU8::new(lifecycle),
            queued: AtomicUsize::new(0),
            dispatched: Mutex::new(None),
            finished: (Mutex::new(false), Condvar::new()),
            provider: Mutex::new(None),
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
        contain_worker(shared.clone(), || panic!("worker disappeared"));
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
