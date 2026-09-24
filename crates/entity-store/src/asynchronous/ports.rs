use std::{future::Future, pin::Pin};

use entity_core::EntityInstance;

use super::{
    AppendOutcome, AppendRequest, AsyncStoreError, BatchKey, CompleteStoreSnapshot, RecordLookup,
    RecordedRefusal, StoredBatch, Subject, SubjectHistory, WriteFailure,
};

/// A store that keeps the commands it refused.
pub trait AsyncRefusalRecorder: Send + Sync {
    /// Records one refusal. `true` when an identical refusal was already recorded.
    ///
    /// # Errors
    ///
    /// The refusal could not be encoded or the store refused the append.
    fn record_refusal<'a>(
        &'a self,
        refusal: &'a RecordedRefusal,
    ) -> BoxFuture<'a, Result<bool, AsyncStoreError>>;

    /// Every recorded refusal, in store order.
    ///
    /// # Errors
    ///
    /// The store's history could not be read or verified.
    fn refusals<'a>(&'a self) -> BoxFuture<'a, Result<Vec<RecordedRefusal>, AsyncStoreError>>;
}

/// An object-safe boxed future returned by asynchronous recorded-store ports.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Asynchronous materialized-state reads.
pub trait AsyncStateReader: Send + Sync {
    /// Loads one current instance, distinguishing absence from authority failure.
    fn load<'a>(
        &'a self,
        subject: &'a Subject,
    ) -> BoxFuture<'a, Result<Option<EntityInstance>, AsyncStoreError>>;
}

/// Asynchronous immutable identity, batch, and mixed-history reads.
pub trait AsyncRecordedReader: Send + Sync {
    /// Looks up one global record identity across committed and imported evidence.
    fn lookup_record<'a>(
        &'a self,
        record_id: &'a str,
    ) -> BoxFuture<'a, Result<Option<RecordLookup>, AsyncStoreError>>;

    /// Looks up one original batch claim in its disjoint namespace.
    fn lookup_batch<'a>(
        &'a self,
        key: &'a BatchKey,
    ) -> BoxFuture<'a, Result<Option<StoredBatch>, AsyncStoreError>>;

    /// Loads one immutable ordered mixed subject history.
    fn history<'a>(
        &'a self,
        subject: &'a Subject,
    ) -> BoxFuture<'a, Result<SubjectHistory, AsyncStoreError>>;

    /// Obtains a provider-owned consistent complete snapshot.
    ///
    /// Implementors must capture every subject in `scope`, their histories, and terminal states
    /// under one authority-consistency boundary. The returned editable data is not by itself proof
    /// of completeness; callers obtain that assurance through `verify_complete_store`, which
    /// invokes this port directly. A deliberately lying provider remains outside this contract.
    fn complete_snapshot<'a>(
        &'a self,
        scope: &'a str,
    ) -> BoxFuture<'a, Result<CompleteStoreSnapshot, AsyncStoreError>>;
}

/// Mandatory complete-record atomic append; there is no bare-decision fallback.
pub trait AsyncRecordedWriter: Send + Sync {
    /// Appends every ordered member or none.
    ///
    /// Implementors must invoke [`AppendRequest::validate`] at the public writer entry before
    /// consuming provider behavior or consulting or changing any authority. Public request fields
    /// may be directly constructed or mutated after a successful constructor call.
    fn append(&self, request: AppendRequest) -> BoxFuture<'_, Result<AppendOutcome, WriteFailure>>;
}

/// The complete asynchronous recorded storage surface.
pub trait AsyncRecordedStore:
    AsyncStateReader + AsyncRecordedReader + AsyncRecordedWriter + Send + Sync
{
}

impl<T> AsyncRecordedStore for T where
    T: AsyncStateReader + AsyncRecordedReader + AsyncRecordedWriter + Send + Sync + ?Sized
{
}
