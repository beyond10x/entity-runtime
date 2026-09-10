//! Complete entity history over caller-selected Eventlog storage.
//!
//! The outer handle and native transaction sessions share the same replay, identity, batch and
//! document-query implementation. Sessions never re-enter the outer provider or retain staged
//! history after the callback ends. Existing provider layouts remain readable by their adapters.
use std::sync::{Arc, Mutex};

use entity_core::{DecisionRecord, DomainEvent, EntityInstance};
use entity_query::{AsyncDocumentQueryProvider, DocumentQuery, QueryError, QueryFuture};
use entity_store::{
    Envelope, Expect, RecordedCommit, RecordedObservation, StoreError,
    asynchronous::{
        AsyncAtomicRecordedStore, AsyncRecordedStore, AtomicRecordedCommit, StoreFuture,
    },
};
use eventlog_core::{AtomicEventStore, EventLogError, TenantId, TransactionalEventStore};

mod io;
mod recorded;
mod wire;
use io::ScopedIo;
pub use recorded::{DOCUMENT_PROJECTION, EntityDocumentProjector};
use recorded::{RecordedStore, backend, digest};

/// A tenant and namespace scoped ER store over a caller-owned Eventlog provider.
///
/// Record ids are global within this tenant and namespace. The host supplies opaque Eventlog
/// subject/actor attribution; original ER recording metadata remains unchanged in every record.
pub struct EventlogStore<S: ?Sized> {
    inner: RecordedStore<Arc<S>>,
}

impl<S: AtomicEventStore + ?Sized> EventlogStore<S> {
    /// Selects storage and authority without opening a path or choosing credentials or time.
    /// # Errors
    /// Blank namespaces or invalid opaque attribution identifiers.
    pub fn new(
        store: Arc<S>,
        tenant: TenantId,
        namespace: &str,
        subject: &str,
        actor: &str,
    ) -> Result<Self, StoreError> {
        Ok(Self {
            inner: RecordedStore::new(store, tenant, namespace, subject, actor)?,
        })
    }

    /// Bounds cached prefixes by subject count and encoded record-body bytes.
    /// Either zero disables retention. Defaults are 64 subjects and 16 MiB; this is not an exact
    /// heap limit. Changing the budget discards cached prefixes, never durable history.
    #[must_use]
    pub fn with_cache_budget(mut self, subjects: usize, encoded_bytes: usize) -> Self {
        self.inner = self.inner.with_cache_budget(subjects, encoded_bytes);
        self
    }

    /// Verifies complete inline document coverage at startup, before native queries are enabled.
    /// The host registers every writer's projector and fences any required rebuild first.
    /// # Errors
    /// Unsupported or unregistered native queries, incomplete coverage or invalid history.
    pub async fn enable_document_queries(&mut self) -> Result<(), QueryError> {
        self.inner.documents_ready = false;
        if !self.inner.store.is_inline(DOCUMENT_PROJECTION.name).await {
            return Err(QueryError::Invalid(
                "register the entity document projector inline before enabling queries".into(),
            ));
        }
        self.inner.enable_document_queries().await
    }
}

/// A callback-scoped ER view of one native Eventlog transaction.
///
/// Recorded writes and queries see earlier staged work. All history verification is shared with
/// the outer adapter. Its separate cache is discarded on both commit and rollback. The caller
/// owns lock ordering and must keep external IO outside this transaction.
pub struct EventlogSession<'a> {
    inner: RecordedStore<ScopedIo<'a>>,
}

impl EventlogSession<'_> {
    /// Locks the writer's stream coordinate, including absent identities, then verifies its state.
    pub async fn load_for_update(
        &mut self,
        entity: &str,
        id: &str,
    ) -> Result<Option<EntityInstance>, StoreError> {
        let stream = self.inner.history_stream(entity, id)?;
        self.inner
            .store
            .0
            .lock_stream(&stream)
            .await
            .map_err(backend)?;
        self.inner.load(entity, id).await
    }

    /// Serializes a logical identity within this tenant and ER namespace, even before a row exists.
    pub async fn lock_identity(
        &mut self,
        namespace: &str,
        identity: &str,
    ) -> Result<(), StoreError> {
        let namespace = self.coordinate(namespace)?;
        // Arbitrary application identity text stays in the adapter's hashed storage coordinate.
        let identity = digest(&identity)?;
        self.inner
            .store
            .0
            .lock_identity(&namespace, &identity)
            .await
            .map_err(backend)
    }

    /// Reserves consecutive values and returns the value before the range, scoped to this ER namespace.
    /// Reservations roll back with the callback. An unknown outcome never authorizes a fresh retry.
    pub async fn reserve_sequence(
        &mut self,
        namespace: &str,
        count: u64,
    ) -> Result<u64, StoreError> {
        let namespace = self.coordinate(namespace)?;
        self.inner
            .store
            .0
            .reserve_sequence(&namespace, count)
            .await
            .map_err(backend)
    }

    fn coordinate(&self, namespace: &str) -> Result<String, StoreError> {
        if namespace.trim().is_empty() {
            return Err(wire::invalid("session namespace must not be empty"));
        }
        digest(&(&self.inner.history_type, namespace))
    }
}

impl<S: TransactionalEventStore> EventlogStore<S> {
    /// Runs one callback inside the provider's native transaction and returns only after commit.
    ///
    /// Capture owned inputs and return an owned value. Callback errors preserve their exact ER variant after successful
    /// rollback. Interrupted or failed settlement reports the native storage error instead. There
    /// is no callback replay or durable receipt for a sequence-only transaction. Query readiness
    /// must be enabled on this handle before using queries inside its session.
    pub fn with_transaction<'a, T, F>(&'a mut self, work: F) -> StoreFuture<'a, T>
    where
        T: Send + 'static,
        F: for<'transaction> FnOnce(EventlogSession<'transaction>) -> StoreFuture<'transaction, T>
            + Send
            + 'a,
    {
        let store = self.inner.store.clone();
        let tenant = self.inner.tenant.clone();
        let template = self.inner.with_io(());
        Box::pin(async move {
            // Preserve a typed caller refusal across Eventlog's error boundary without serializing
            // it into the log, changing provider errors or retaining a callback's staged cache.
            let refusal = Arc::new(Mutex::new(None));
            let callback_refusal = refusal.clone();
            let result = store
                .with_transaction(&tenant, move |transaction| {
                    let session = EventlogSession {
                        inner: template.with_io(ScopedIo(transaction)),
                    };
                    let pending = work(session);
                    Box::pin(async move {
                        match pending.await {
                            Ok(value) => Ok(value),
                            Err(error) => {
                                *callback_refusal
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner) =
                                    Some(error);
                                Err(EventLogError::GuardRefused {
                                    code: "entity_session_callback_refused".into(),
                                })
                            }
                        }
                    })
                })
                .await;
            match result {
                Err(EventLogError::GuardRefused { code })
                    if code == "entity_session_callback_refused" =>
                {
                    let error = refusal
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .take()
                        .unwrap_or_else(|| {
                            wire::invalid("native transaction returned an unowned caller refusal")
                        });
                    Err(error)
                }
                other => other.map_err(backend),
            }
        })
    }
}

// Forward public ports to one private implementation, for both storage contexts.
macro_rules! recorded_ports {
    ($target:ty, [$($generics:tt)*], [$($bounds:tt)*]) => {
        impl<$($generics)*> AsyncRecordedStore for $target where $($bounds)* {
            fn verified_history<'a>(&'a mut self, entity: &'a str, id: &'a str)
                -> StoreFuture<'a, Arc<entity_store::VerifiedHistory>> { self.inner.verified_history(entity, id) }
            fn load<'a>(&'a mut self, entity: &'a str, id: &'a str)
                -> StoreFuture<'a, Option<EntityInstance>> { self.inner.load(entity, id) }
            fn ids<'a>(&'a mut self, entity: &'a str) -> StoreFuture<'a, Vec<String>> { self.inner.ids(entity) }
            fn events<'a>(&'a mut self, entity: &'a str, id: &'a str)
                -> StoreFuture<'a, Vec<DomainEvent>> { self.inner.events(entity, id) }
            fn records<'a>(&'a mut self, entity: &'a str, id: &'a str)
                -> StoreFuture<'a, Vec<Envelope<DecisionRecord>>> { self.inner.records(entity, id) }
            fn observations<'a>(&'a mut self, entity: &'a str, id: &'a str)
                -> StoreFuture<'a, Vec<RecordedObservation>> { self.inner.observations(entity, id) }
            fn commit_recorded<'a>(&'a mut self, commit: &'a RecordedCommit, expect: Expect)
                -> StoreFuture<'a, ()> { self.inner.commit_recorded(commit, expect) }
            fn observe<'a>(&'a mut self, observation: &'a RecordedObservation)
                -> StoreFuture<'a, ()> { self.inner.observe(observation) }
        }
        impl<$($generics)*> AsyncAtomicRecordedStore for $target where $($bounds)* {
            fn commit_recorded_batch<'a>(&'a mut self, commits: &'a [AtomicRecordedCommit])
                -> StoreFuture<'a, ()> { self.inner.commit_recorded_batch(commits) }
        }
        impl<$($generics)*> AsyncDocumentQueryProvider for $target where $($bounds)* {
            fn query_documents<'a>(&'a mut self, query: &'a DocumentQuery) -> QueryFuture<'a> {
                self.inner.query_documents(query)
            }
        }
    };
}
recorded_ports!(EventlogStore<S>, [S: ?Sized], [S: AtomicEventStore]);
recorded_ports!(EventlogSession<'s>, ['s], [Self: Send]);
