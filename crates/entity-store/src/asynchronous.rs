//! Runtime-neutral recorded storage for asynchronous applications.
//!
//! These ports require complete history. There is deliberately no fallback from a recorded
//! commit to an event-only write. Providers own durable atomicity and global record-id equality.

use std::{future::Future, pin::Pin, sync::Arc};

use entity_core::{DecisionRecord, DomainEvent, EntityInstance};

use crate::{
    Envelope, Expect, MemoryStore, RecordedCommit, RecordedObservation, RecordedStore, Store,
    StoreError,
};

/// A provider operation that may suspend on its caller's executor.
pub type StoreFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, StoreError>> + Send + 'a>>;

/// Complete recorded storage without choosing an async runtime or transport.
///
/// Implementations preserve the synchronous recorded contract: exact global record-id retries
/// succeed before checking the old expectation; changed content conflicts. A successful write
/// durably commits the complete envelope, resulting state and events together. Cancellation or a
/// transport failure after submission may have an unknown outcome; retry the same record, never
/// manufacture a new id. Read methods return append order, and `ids` returns sorted identities.
/// Record reads and writes are required; the verified-history helper may reuse those reads.
/// Lack of history can never silently become event-only success.
pub trait AsyncRecordedStore: Send {
    /// A verified decision prefix shared with the executor without a second replay.
    ///
    /// Providers may override this with an immutable cached proof after checking that its
    /// underlying history generation and prefix still hold. The default verifies all records.
    fn verified_history<'a>(
        &'a mut self,
        entity: &'a str,
        id: &'a str,
    ) -> StoreFuture<'a, Arc<crate::VerifiedHistory>> {
        Box::pin(async move {
            let mut history = crate::VerifiedHistory::new(entity, id);
            for envelope in self.records(entity, id).await? {
                history.append(envelope)?;
            }
            Ok(Arc::new(history))
        })
    }
    /// Reads the materialized state; provider failure is distinct from absence.
    fn load<'a>(
        &'a mut self,
        entity: &'a str,
        id: &'a str,
    ) -> StoreFuture<'a, Option<EntityInstance>>;
    /// Lists sorted subject identities for an entity type.
    fn ids<'a>(&'a mut self, entity: &'a str) -> StoreFuture<'a, Vec<String>>;
    /// Reads events in revision and emission order, including retained legacy events.
    fn events<'a>(&'a mut self, entity: &'a str, id: &'a str) -> StoreFuture<'a, Vec<DomainEvent>>;
    /// Reads a consistent prefix of complete decision envelopes in append order.
    fn records<'a>(
        &'a mut self,
        entity: &'a str,
        id: &'a str,
    ) -> StoreFuture<'a, Vec<Envelope<DecisionRecord>>>;
    /// Reads non-state-changing observations in append order.
    fn observations<'a>(
        &'a mut self,
        entity: &'a str,
        id: &'a str,
    ) -> StoreFuture<'a, Vec<RecordedObservation>>;
    /// Atomically persists a complete decision against its optimistic expectation.
    fn commit_recorded<'a>(
        &'a mut self,
        commit: &'a RecordedCommit,
        expect: Expect,
    ) -> StoreFuture<'a, ()>;
    /// Appends an observation at its subject revision without changing state or events.
    fn observe<'a>(&'a mut self, observation: &'a RecordedObservation) -> StoreFuture<'a, ()>;
}

/// A complete recorded decision paired with its transaction-local expectation.
#[derive(Debug, Clone, PartialEq)]
pub struct AtomicRecordedCommit {
    /// Complete provenance and resulting state, including zero-event decisions.
    pub commit: RecordedCommit,
    /// The revision expected when this entry is reached in request order.
    pub expect: Expect,
}

/// Optional synchronous capability for atomic batches that retain complete recorded provenance.
///
/// `AtomicBatchStore` accepts unrecorded decisions and cannot supply this stronger promise.
pub trait AtomicRecordedStore: RecordedStore {
    /// Commits all entries in request order or none; an empty batch changes nothing.
    ///
    /// # Errors
    ///
    /// Any validation, record-id or revision conflict rolls back the whole batch.
    fn commit_recorded_batch(&mut self, commits: &[AtomicRecordedCommit])
        -> Result<(), StoreError>;
}

/// Optional async transaction capability; single-write providers must not emulate it in a loop.
pub trait AsyncAtomicRecordedStore: AsyncRecordedStore {
    /// Commits all entries atomically, checking expectations against earlier entries in the batch.
    /// A refused batch changes no state, record, event, observation or record-id bookkeeping.
    fn commit_recorded_batch<'a>(
        &'a mut self,
        commits: &'a [AtomicRecordedCommit],
    ) -> StoreFuture<'a, ()>;
}

/// Explicit compatibility with synchronous providers, executing on the polling thread.
///
/// This adapter does **not** make filesystem or database IO nonblocking. An application must
/// place it on a suitable worker when needed. No executor is created, and no nested `block_on`
/// is used. The provider must implement the complete recorded contract, not merely `Store`.
pub struct BlockingRecordedStore<S>(pub S);

impl<S: RecordedStore + Send> AsyncRecordedStore for BlockingRecordedStore<S> {
    fn load<'a>(
        &'a mut self,
        entity: &'a str,
        id: &'a str,
    ) -> StoreFuture<'a, Option<EntityInstance>> {
        Box::pin(async move { self.0.load(entity, id) })
    }
    fn ids<'a>(&'a mut self, entity: &'a str) -> StoreFuture<'a, Vec<String>> {
        Box::pin(async move { self.0.ids(entity) })
    }
    fn events<'a>(&'a mut self, entity: &'a str, id: &'a str) -> StoreFuture<'a, Vec<DomainEvent>> {
        Box::pin(async move { self.0.events(entity, id) })
    }
    fn records<'a>(
        &'a mut self,
        entity: &'a str,
        id: &'a str,
    ) -> StoreFuture<'a, Vec<Envelope<DecisionRecord>>> {
        Box::pin(async move { self.0.records(entity, id) })
    }
    fn observations<'a>(
        &'a mut self,
        entity: &'a str,
        id: &'a str,
    ) -> StoreFuture<'a, Vec<RecordedObservation>> {
        Box::pin(async move { self.0.observations(entity, id) })
    }
    fn commit_recorded<'a>(
        &'a mut self,
        commit: &'a RecordedCommit,
        expect: Expect,
    ) -> StoreFuture<'a, ()> {
        Box::pin(async move { self.0.commit_recorded(commit, expect) })
    }
    fn observe<'a>(&'a mut self, observation: &'a RecordedObservation) -> StoreFuture<'a, ()> {
        Box::pin(async move { self.0.observe(observation) })
    }
}

impl<S: AtomicRecordedStore + Send> AsyncAtomicRecordedStore for BlockingRecordedStore<S> {
    fn commit_recorded_batch<'a>(
        &'a mut self,
        commits: &'a [AtomicRecordedCommit],
    ) -> StoreFuture<'a, ()> {
        Box::pin(async move { self.0.commit_recorded_batch(commits) })
    }
}

impl AtomicRecordedStore for MemoryStore {
    fn commit_recorded_batch(
        &mut self,
        commits: &[AtomicRecordedCommit],
    ) -> Result<(), StoreError> {
        // Stage all maps, including the global retry index, before exposing any prefix.
        let mut candidate = self.clone();
        for entry in commits {
            candidate.commit_recorded(&entry.commit, entry.expect)?;
        }
        *self = candidate;
        Ok(())
    }
}
