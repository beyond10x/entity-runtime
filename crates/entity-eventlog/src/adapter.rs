use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use entity_core::EntityInstance;
use entity_store::{
    Expect,
    asynchronous::{
        AppendMember, AppendOutcome, AppendRequest, AsyncRecordedReader, AsyncRecordedWriter,
        AsyncStateReader, AsyncStoreError, BatchKey, BatchReceipt, BoxFuture, CommitReceipt,
        CompleteStoreSnapshot, HistoryOrigin, RecordLookup, RecordPosition, RecordReceipt,
        StoreCoverage, StoredBatch, StoredRecord, Subject, SubjectAssurance, SubjectHistory,
        SubjectSnapshot, WriteFailure, batch_comparison_bytes, original_request_comparison_bytes,
        record_comparison_bytes, validate_entry_against_state, verify_subject_history,
    },
};
use eventlog_core::{
    AppendGroup, AtomicEventStore, CaptureError, CaptureLimits, CommandMeta,
    ConsistentTenantCapture, EventLogError, EventStore, Expected, Guard, InlineProjectionAdmin,
    NewEvent, ProjectionStore, RecordedEvent, StreamAppend, StreamId, TenantCapture, TenantId,
};
use serde_json::{Value, json};
use time::OffsetDateTime;

use crate::{
    encoding::{
        ANCHOR_BLOB_DOMAIN, Authority, BATCH_BLOB_DOMAIN, BINDING_BLOB_DOMAIN, ENTRY_BLOB_DOMAIN,
        EvidenceWire, PhysicalRef, RECORD_BLOB_DOMAIN, REQUEST_BLOB_DOMAIN, RecordedEntryWrapper,
        SubjectWire, anchor_from_history, decode_anchor, decode_batch, decode_binding,
        decode_entry, decode_record, encode_anchor, encode_binding, encode_entry, framed_key,
        history_from_anchor, key_for_value,
    },
    projection::{
        PROJECTOR_NAME, batch_key as physical_batch_key, batch_spec, binding_spec, physical,
        projection_specs, record_key, record_spec, subject_key, subject_spec, subject_stream_id,
        tagged_body,
    },
};

/// Eventlog capabilities required by the adapter, available as one object-safe backend.
pub trait EventlogBackend:
    EventStore + AtomicEventStore + ConsistentTenantCapture + InlineProjectionAdmin
{
}

impl<T> EventlogBackend for T where
    T: EventStore + AtomicEventStore + ConsistentTenantCapture + InlineProjectionAdmin
{
}

/// Caller-owned operational facts for one possible Eventlog command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventlogOperationContext {
    /// Opaque principal for whom the operation runs.
    pub subject: String,
    /// Opaque agent or service issuing it.
    pub actor: String,
    /// Caller-stable request identity.
    pub request_id: String,
    /// Caller-stable trace identity.
    pub trace_id: String,
    /// Optional causing event identity.
    pub causation_id: Option<String>,
    /// Bounded automation depth.
    pub causation_depth: u32,
    /// Caller-understood occurrence time.
    pub occurred_at: OffsetDateTime,
}

impl EventlogOperationContext {
    fn meta(
        &self,
        idempotency_key: String,
        request_hash: String,
    ) -> Result<CommandMeta, AsyncStoreError> {
        let meta = CommandMeta {
            idempotency_key,
            request_hash,
            subject: self.subject.clone(),
            actor: self.actor.clone(),
            request_id: self.request_id.clone(),
            trace_id: self.trace_id.clone(),
            causation_id: self.causation_id.clone(),
            causation_depth: self.causation_depth,
            occurred_at: self.occurred_at,
            claim: None,
        };
        meta.validate().map_err(input_eventlog)?;
        Ok(meta)
    }
}

/// Bound, non-initializing Eventlog implementation of complete recorded reads.
pub struct EventlogRecordedStore {
    backend: Arc<dyn EventlogBackend>,
    authority: Authority,
    tenant: TenantId,
    limits: CaptureLimits,
}

impl std::fmt::Debug for EventlogRecordedStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EventlogRecordedStore")
            .field("authority", &self.authority)
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

impl EventlogRecordedStore {
    /// Exact immutable logical/physical authority verified by this handle.
    #[must_use]
    pub const fn authority(&self) -> &Authority {
        &self.authority
    }

    /// Opens an already provisioned, already attached store without mutating provider state.
    ///
    /// # Errors
    /// Refuses missing attachment, binding, generation, references, or exact index equality.
    pub async fn open(
        backend: Arc<dyn EventlogBackend>,
        authority: Authority,
        limits: CaptureLimits,
    ) -> Result<Self, AsyncStoreError> {
        authority.validate()?;
        let tenant = TenantId::new(authority.tenant.clone()).map_err(input_eventlog)?;
        if !backend.is_inline(PROJECTOR_NAME).await {
            return Err(integrity("the fixed inline projector is not attached"));
        }
        let store = Self {
            backend,
            authority,
            tenant,
            limits,
        };
        let model = store.capture_model().await?;
        if model.binding.is_none() {
            return Err(integrity("the tenant has no authoritative binding"));
        }
        Ok(store)
    }

    /// Returns a per-operation facade supplying facts only when a new physical write is needed.
    #[must_use]
    pub fn operation(&self, context: EventlogOperationContext) -> EventlogOperationStore<'_> {
        EventlogOperationStore {
            store: self,
            context,
        }
    }

    /// Rebuilds the fixed registered inline projector and verifies a fresh complete capture.
    ///
    /// # Errors
    /// Provider failure or any post-rebuild authority/index mismatch.
    pub async fn rebuild_indexes(
        &self,
    ) -> Result<eventlog_core::InlineRebuildResult, AsyncStoreError> {
        let result = self
            .backend
            .rebuild_inline_projection(PROJECTOR_NAME, &self.tenant)
            .await
            .map_err(map_read_error)?;
        self.capture_model().await?;
        Ok(result)
    }

    async fn capture(&self) -> Result<TenantCapture, AsyncStoreError> {
        let capture = self
            .backend
            .capture_tenant(&self.tenant, projection_specs(), self.limits)
            .await
            .map_err(map_capture)?;
        if capture.tenant != self.tenant
            || capture.stream_identity != self.authority.stream_identity
        {
            return Err(integrity("native capture substituted tenant generation"));
        }
        Ok(capture)
    }

    async fn capture_model(&self) -> Result<CapturedModel, AsyncStoreError> {
        build_model(&self.authority, self.capture().await?)
    }
}

/// One operation-scoped writer and reader facade.
#[derive(Debug)]
pub struct EventlogOperationStore<'a> {
    store: &'a EventlogRecordedStore,
    context: EventlogOperationContext,
}

impl AsyncStateReader for EventlogRecordedStore {
    fn load<'a>(
        &'a self,
        subject: &'a Subject,
    ) -> BoxFuture<'a, Result<Option<EntityInstance>, AsyncStoreError>> {
        Box::pin(async move { Ok(self.capture_model().await?.terminals.get(subject).cloned()) })
    }
}

impl AsyncRecordedReader for EventlogRecordedStore {
    fn lookup_record<'a>(
        &'a self,
        record_id: &'a str,
    ) -> BoxFuture<'a, Result<Option<RecordLookup>, AsyncStoreError>> {
        Box::pin(async move { Ok(self.capture_model().await?.records.get(record_id).cloned()) })
    }
    fn lookup_batch<'a>(
        &'a self,
        key: &'a BatchKey,
    ) -> BoxFuture<'a, Result<Option<StoredBatch>, AsyncStoreError>> {
        Box::pin(async move { Ok(self.capture_model().await?.batches.get(key).cloned()) })
    }
    fn history<'a>(
        &'a self,
        subject: &'a Subject,
    ) -> BoxFuture<'a, Result<SubjectHistory, AsyncStoreError>> {
        Box::pin(async move {
            Ok(self
                .capture_model()
                .await?
                .histories
                .get(subject)
                .cloned()
                .unwrap_or_else(|| SubjectHistory {
                    subject: subject.clone(),
                    origin: HistoryOrigin::Genesis,
                    records: Vec::new(),
                }))
        })
    }
    fn complete_snapshot<'a>(
        &'a self,
        scope: &'a str,
    ) -> BoxFuture<'a, Result<CompleteStoreSnapshot, AsyncStoreError>> {
        Box::pin(async move {
            if scope != self.authority.logical_scope {
                return Err(AsyncStoreError::InvalidInput(
                    "requested scope differs from bound authority".into(),
                ));
            }
            let model = self.capture_model().await?;
            let histories = model
                .histories
                .into_iter()
                .map(|(subject, history)| {
                    let terminal = model
                        .terminals
                        .get(&subject)
                        .cloned()
                        .ok_or_else(|| integrity("subject history has no terminal state"))?;
                    Ok(SubjectSnapshot { history, terminal })
                })
                .collect::<Result<Vec<_>, AsyncStoreError>>()?;
            Ok(CompleteStoreSnapshot {
                scope: scope.to_owned(),
                coverage: StoreCoverage::CompleteSnapshot,
                histories,
            })
        })
    }
}

impl AsyncStateReader for EventlogOperationStore<'_> {
    fn load<'a>(
        &'a self,
        subject: &'a Subject,
    ) -> BoxFuture<'a, Result<Option<EntityInstance>, AsyncStoreError>> {
        self.store.load(subject)
    }
}
impl AsyncRecordedReader for EventlogOperationStore<'_> {
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
}

impl AsyncRecordedWriter for EventlogOperationStore<'_> {
    fn append<'a>(
        &'a self,
        request: AppendRequest,
    ) -> BoxFuture<'a, Result<AppendOutcome, WriteFailure>> {
        Box::pin(async move { self.append_inner(request).await })
    }
}

impl EventlogOperationStore<'_> {
    async fn append_inner(&self, request: AppendRequest) -> Result<AppendOutcome, WriteFailure> {
        request.validate().map_err(WriteFailure::NotCommitted)?;
        let Some(key) = request.key.clone() else {
            return Ok(AppendOutcome::Empty);
        };
        if let Some(outcome) = recover_append(self.store, &key, &request)
            .await
            .map_err(WriteFailure::NotCommitted)?
        {
            return Ok(outcome);
        }
        let batch_bytes =
            batch_comparison_bytes(&key, &request.members).map_err(WriteFailure::NotCommitted)?;
        let batch_digest =
            framed_key(BATCH_BLOB_DOMAIN, &batch_bytes).map_err(WriteFailure::NotCommitted)?;
        let mut wrappers = Vec::with_capacity(request.members.len());
        for (index, member) in request.members.iter().enumerate() {
            let record_bytes =
                record_comparison_bytes(&member.entry).map_err(WriteFailure::NotCommitted)?;
            let record_digest = framed_key(RECORD_BLOB_DOMAIN, &record_bytes)
                .map_err(WriteFailure::NotCommitted)?;
            let request_digest = framed_key(REQUEST_BLOB_DOMAIN, &member.request_bytes)
                .map_err(WriteFailure::NotCommitted)?;
            let wrapper = RecordedEntryWrapper {
                authority: self.store.authority.clone(),
                batch_blob: batch_digest.clone(),
                batch_key: crate::encoding::BatchKeyWire::from(&key),
                member_index: u64::try_from(index).map_err(|_| {
                    WriteFailure::NotCommitted(AsyncStoreError::PositionExhausted {
                        domain: "batch member".into(),
                    })
                })?,
                record_blob: record_digest.clone(),
                request_blob: request_digest.clone(),
                subject: SubjectWire::from(&member.entry.subject()),
            };
            let wrapper_bytes = encode_entry(&wrapper).map_err(WriteFailure::NotCommitted)?;
            let wrapper_digest = framed_key(ENTRY_BLOB_DOMAIN, &wrapper_bytes)
                .map_err(WriteFailure::NotCommitted)?;
            wrappers.push((
                wrapper,
                wrapper_bytes,
                wrapper_digest,
                record_bytes,
                record_digest,
            ));
        }
        let command_key = key_for_value("er.eventlog.batch-command-key/1", json!({"authority":self.store.authority,"batch_key":crate::encoding::BatchKeyWire::from(&key)})).map_err(WriteFailure::NotCommitted)?;
        let meta = self
            .context
            .meta(command_key, batch_digest.clone())
            .map_err(WriteFailure::NotCommitted)?;
        self.store
            .backend
            .put_blob(&self.store.tenant, &batch_digest, &batch_bytes)
            .await
            .map_err(|e| WriteFailure::NotCommitted(map_put_error(e)))?;
        for (wrapper, wrapper_bytes, wrapper_digest, record_bytes, record_digest) in &wrappers {
            self.store
                .backend
                .put_blob(&self.store.tenant, record_digest, record_bytes)
                .await
                .map_err(|e| WriteFailure::NotCommitted(map_put_error(e)))?;
            let member = &request.members
                [usize::try_from(wrapper.member_index).expect("checked from usize")];
            self.store
                .backend
                .put_blob(
                    &self.store.tenant,
                    &wrapper.request_blob,
                    &member.request_bytes,
                )
                .await
                .map_err(|e| WriteFailure::NotCommitted(map_put_error(e)))?;
            self.store
                .backend
                .put_blob(&self.store.tenant, wrapper_digest, wrapper_bytes)
                .await
                .map_err(|e| WriteFailure::NotCommitted(map_put_error(e)))?;
        }
        let model = self
            .store
            .capture_model()
            .await
            .map_err(WriteFailure::NotCommitted)?;
        let mut heads: BTreeMap<Subject, Option<u64>> = BTreeMap::new();
        for member in &request.members {
            heads.entry(member.entry.subject()).or_insert_with(|| {
                model
                    .histories
                    .get(&member.entry.subject())
                    .and_then(|h| h.records.last().map(|r| r.position.subject))
                    .or_else(|| {
                        model.histories.get(&member.entry.subject()).and_then(|h| {
                            matches!(h.origin, HistoryOrigin::Imported(_)).then_some(1)
                        })
                    })
            });
        }
        let mut appends = Vec::with_capacity(wrappers.len());
        for ((_, _, wrapper_digest, _, _), member) in wrappers.iter().zip(&request.members) {
            let subject = member.entry.subject();
            let head = heads.get_mut(&subject).expect("subject head was collected");
            let expected = head.map_or(Expected::NoStream, Expected::Exact);
            *head = Some(head.unwrap_or(0).checked_add(1).ok_or_else(|| {
                WriteFailure::NotCommitted(AsyncStoreError::PositionExhausted {
                    domain: "subject event stream".into(),
                })
            })?);
            appends.push(StreamAppend {
                stream: StreamId::new(
                    self.store.tenant.clone(),
                    "er.subject",
                    subject_stream_id(&self.store.authority, &subject)
                        .map_err(WriteFailure::NotCommitted)?,
                )
                .map_err(|e| WriteFailure::NotCommitted(input_eventlog(e)))?,
                expected,
                events: vec![
                    NewEvent::new("er.recorded_entry", 1, json!({"blob":wrapper_digest}))
                        .map_err(|e| WriteFailure::NotCommitted(input_eventlog(e)))?,
                ],
            });
        }
        let group = AppendGroup {
            tenant: self.store.tenant.clone(),
            appends,
            meta,
        };
        group
            .fingerprint()
            .map_err(|e| WriteFailure::NotCommitted(input_eventlog(e)))?;
        let slot = Arc::new(Mutex::new(None));
        let guard = Arc::new(AppendGuard {
            authority: self.store.authority.clone(),
            tenant: self.store.tenant.clone(),
            key: key.clone(),
            request: request.clone(),
            slot: slot.clone(),
        });
        match self.store.backend.append_group_guarded(&group, guard).await {
            Ok(result) => {
                if validate_group_result(&result, wrappers.len()).is_err() {
                    return Err(WriteFailure::Uncertain {
                        key,
                        cause: "Eventlog reported a commit with an invalid result shape".into(),
                    });
                }
                committed_outcome(self.store, &key, false, &result)
                    .await
                    .map_err(|_| {
                    WriteFailure::Uncertain {
                        key,
                        cause: "Eventlog reported a commit but authoritative verification is unavailable"
                            .into(),
                    }
                })
            }
            Err(error) => {
                self.resolve_append_error(error, &key, &request, &slot)
                    .await
            }
        }
    }

    async fn resolve_append_error(
        &self,
        error: EventLogError,
        key: &BatchKey,
        request: &AppendRequest,
        slot: &Arc<Mutex<Option<GuardRefusal>>>,
    ) -> Result<AppendOutcome, WriteFailure> {
        match error {
            EventLogError::GuardRefused { code } => {
                let refusal = slot
                    .lock()
                    .map_err(|_| {
                        WriteFailure::NotCommitted(integrity("guard refusal slot was poisoned"))
                    })?
                    .take();
                match refusal {
                    Some(refusal)
                        if refusal.code.as_str() == code
                            && refusal.code.matches(&refusal.error) =>
                    {
                        Err(WriteFailure::NotCommitted(refusal.error))
                    }
                    _ => Err(WriteFailure::NotCommitted(integrity(
                        "guard refusal code and typed slot disagree",
                    ))),
                }
            }
            EventLogError::UnknownCommit => match recover_append(self.store, key, request).await {
                Ok(Some(outcome)) => Ok(outcome),
                Ok(None) | Err(_) => Err(WriteFailure::Uncertain {
                    key: key.clone(),
                    cause:
                        "Eventlog commit outcome is unknown and semantic recovery is unavailable"
                            .into(),
                }),
            },
            EventLogError::Conflict { .. } => match recover_append(self.store, key, request).await {
                Ok(Some(outcome)) => Ok(outcome),
                Ok(None) => Box::pin(self.append_inner(request.clone())).await,
                Err(_) => Err(WriteFailure::Uncertain {
                    key: key.clone(),
                    cause:
                        "Eventlog physical conflict cannot be resolved against semantic authority"
                            .into(),
                }),
            },
            EventLogError::IdempotencyMismatch { .. } => {
                match recover_append(self.store, key, request).await {
                    Ok(Some(outcome)) => Ok(outcome),
                    Ok(None) => Err(WriteFailure::NotCommitted(integrity(
                        "Eventlog command identity exists without matching ER authority",
                    ))),
                    Err(_) => Err(WriteFailure::Uncertain {
                        key: key.clone(),
                        cause: "Eventlog command identity cannot be resolved against semantic authority"
                            .into(),
                    }),
                }
            }
            other => Err(WriteFailure::NotCommitted(map_append_error(other))),
        }
    }
}

async fn recover_append(
    store: &EventlogRecordedStore,
    key: &BatchKey,
    request: &AppendRequest,
) -> Result<Option<AppendOutcome>, AsyncStoreError> {
    let model = store.capture_model().await?;
    if let Some(batch) = model.batches.get(key) {
        let expected = batch_comparison_bytes(key, &request.members)?;
        if batch.comparison_bytes == expected {
            return Ok(Some(AppendOutcome::Committed {
                receipt: batch.receipt.clone(),
                replayed: true,
            }));
        }
        return Err(AsyncStoreError::BatchConflict { key: key.clone() });
    }
    let mut occupied = Vec::new();
    for (index, member) in request.members.iter().enumerate() {
        if let Some(found) = model.records.get(member.entry.record_id()) {
            if let (BatchKey::SingleRecord(_), RecordLookup::Committed(record)) = (key, found)
                && record.entry == member.entry
                && record.request_bytes == member.request_bytes
            {
                return Ok(Some(AppendOutcome::Committed {
                    receipt: CommitReceipt::Single(record.receipt.clone()),
                    replayed: true,
                }));
            }
            if let (BatchKey::SingleRecord(_), RecordLookup::Imported(evidence)) = (key, found)
                && evidence.entry == member.entry
            {
                let history = model
                    .histories
                    .get(&member.entry.subject())
                    .ok_or_else(|| {
                        corrupt(
                            &member.entry.subject(),
                            "imported lookup has no subject history",
                        )
                    })?;
                let assurance = verify_subject_history(
                    history,
                    model
                        .terminals
                        .get(&member.entry.subject())
                        .ok_or_else(|| {
                            corrupt(&member.entry.subject(), "imported history has no terminal")
                        })?,
                )?;
                return Ok(Some(AppendOutcome::Historical {
                    evidence: evidence.clone(),
                    assurance,
                }));
            }
            occupied.push(u64::try_from(index).map_err(|_| {
                AsyncStoreError::PositionExhausted {
                    domain: "batch member".into(),
                }
            })?);
        }
    }
    if let BatchKey::SingleRecord(record_id) = key {
        if !occupied.is_empty() {
            return Err(AsyncStoreError::RecordConflict {
                record_id: record_id.clone(),
            });
        }
    } else if !occupied.is_empty() {
        return Err(AsyncStoreError::PreviouslyRecordedBatchEntries { indices: occupied });
    }
    Ok(None)
}

async fn committed_outcome(
    store: &EventlogRecordedStore,
    key: &BatchKey,
    replayed: bool,
    result: &eventlog_core::AppendGroupResult,
) -> Result<AppendOutcome, AsyncStoreError> {
    let model = store.capture_model().await?;
    let batch = model
        .batches
        .get(key)
        .ok_or_else(|| integrity("committed group is absent from authoritative capture"))?;
    for (saved, append) in batch.records.iter().zip(&result.appends) {
        let returned = append
            .events
            .first()
            .ok_or_else(|| integrity("committed group result has no event"))?;
        if model.record_physical.get(saved.entry.record_id()) != Some(&physical(returned)) {
            return Err(integrity(
                "committed group result differs from authoritative physical coordinates",
            ));
        }
    }
    Ok(AppendOutcome::Committed {
        receipt: batch.receipt.clone(),
        replayed,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GuardRefusalCode {
    RevisionConflict,
    RecordConflict,
    BatchConflict,
    PreviouslyRecordedBatchEntries,
    CorruptHistory,
    ProviderIntegrity,
}
impl GuardRefusalCode {
    fn as_str(self) -> &'static str {
        match self {
            Self::RevisionConflict => "er_revision_conflict",
            Self::RecordConflict => "er_record_conflict",
            Self::BatchConflict => "er_batch_conflict",
            Self::PreviouslyRecordedBatchEntries => "er_previously_recorded_batch_entries",
            Self::CorruptHistory => "er_corrupt_history",
            Self::ProviderIntegrity => "er_provider_integrity",
        }
    }
    fn for_error(error: &AsyncStoreError) -> Self {
        match error {
            AsyncStoreError::RevisionConflict { .. } => Self::RevisionConflict,
            AsyncStoreError::RecordConflict { .. } => Self::RecordConflict,
            AsyncStoreError::BatchConflict { .. } => Self::BatchConflict,
            AsyncStoreError::PreviouslyRecordedBatchEntries { .. } => {
                Self::PreviouslyRecordedBatchEntries
            }
            AsyncStoreError::CorruptHistory { .. } => Self::CorruptHistory,
            _ => Self::ProviderIntegrity,
        }
    }
    fn matches(self, error: &AsyncStoreError) -> bool {
        Self::for_error(error) == self
    }
}
struct GuardRefusal {
    code: GuardRefusalCode,
    error: AsyncStoreError,
}

struct AppendGuard {
    authority: Authority,
    tenant: TenantId,
    key: BatchKey,
    request: AppendRequest,
    slot: Arc<Mutex<Option<GuardRefusal>>>,
}

impl Guard for AppendGuard {
    fn check<'a>(
        &'a self,
        store: &'a mut dyn ProjectionStore,
    ) -> eventlog_core::BoxFuture<'a, Result<(), EventLogError>> {
        Box::pin(async move {
            let result = self.check_inner(store).await;
            match result {
                Ok(()) => Ok(()),
                Err(GuardCheckError::Store(error)) => Err(error),
                Err(GuardCheckError::Domain(error)) => {
                    let code = GuardRefusalCode::for_error(&error);
                    let mut slot = self
                        .slot
                        .lock()
                        .map_err(|_| EventLogError::Invalid("guard slot poisoned".into()))?;
                    if slot.is_some() {
                        return Err(EventLogError::Invalid("guard slot written twice".into()));
                    }
                    *slot = Some(GuardRefusal { code, error });
                    Err(EventLogError::GuardRefused {
                        code: code.as_str().into(),
                    })
                }
            }
        })
    }
}

enum GuardCheckError {
    Store(EventLogError),
    Domain(AsyncStoreError),
}
impl From<EventLogError> for GuardCheckError {
    fn from(value: EventLogError) -> Self {
        Self::Store(value)
    }
}
impl AppendGuard {
    async fn check_inner(&self, store: &mut dyn ProjectionStore) -> Result<(), GuardCheckError> {
        let mut locks: Vec<(u8, String, &'static eventlog_core::ProjectionSpec)> =
            vec![(0, "singleton".into(), binding_spec())];
        locks.push((
            1,
            physical_batch_key(&self.authority, &self.key).map_err(GuardCheckError::Domain)?,
            batch_spec(),
        ));
        for member in &self.request.members {
            locks.push((
                2,
                record_key(&self.authority, member.entry.record_id())
                    .map_err(GuardCheckError::Domain)?,
                record_spec(),
            ));
            locks.push((
                3,
                subject_key(&self.authority, &member.entry.subject())
                    .map_err(GuardCheckError::Domain)?,
                subject_spec(),
            ));
        }
        locks.sort_by(|a, b| (a.0, a.1.as_bytes()).cmp(&(b.0, b.1.as_bytes())));
        locks.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
        let mut rows = BTreeMap::new();
        for (rank, key, spec) in locks {
            let row = store.get_for_update(spec, &self.tenant, &key).await?;
            rows.insert((rank, key), row);
        }
        let binding = rows
            .get(&(0, "singleton".into()))
            .and_then(Option::as_ref)
            .ok_or_else(|| GuardCheckError::Domain(integrity("binding row is absent")))?;
        let body =
            tagged_body(binding, "er.eventlog.binding-index/1").map_err(GuardCheckError::Store)?;
        let found: Authority = serde_json::from_value(body["authority"].clone())
            .map_err(|e| GuardCheckError::Domain(integrity(e.to_string())))?;
        if found != self.authority {
            return Err(GuardCheckError::Domain(integrity(
                "binding row authority changed",
            )));
        }
        let batch_key =
            physical_batch_key(&self.authority, &self.key).map_err(GuardCheckError::Domain)?;
        if rows.get(&(1, batch_key)).is_some_and(Option::is_some) {
            return Err(GuardCheckError::Domain(AsyncStoreError::BatchConflict {
                key: self.key.clone(),
            }));
        }
        let mut occupied = Vec::new();
        for (index, member) in self.request.members.iter().enumerate() {
            let key = record_key(&self.authority, member.entry.record_id())
                .map_err(GuardCheckError::Domain)?;
            if rows.get(&(2, key)).is_some_and(Option::is_some) {
                occupied.push(u64::try_from(index).map_err(|_| {
                    GuardCheckError::Domain(AsyncStoreError::PositionExhausted {
                        domain: "batch member".into(),
                    })
                })?);
            }
        }
        if !occupied.is_empty() {
            let error = match &self.key {
                BatchKey::SingleRecord(id) => AsyncStoreError::RecordConflict {
                    record_id: id.clone(),
                },
                BatchKey::Named(_) => {
                    AsyncStoreError::PreviouslyRecordedBatchEntries { indices: occupied }
                }
            };
            return Err(GuardCheckError::Domain(error));
        }
        let mut overlay: BTreeMap<Subject, Option<EntityInstance>> = BTreeMap::new();
        for member in &self.request.members {
            let subject = member.entry.subject();
            if !overlay.contains_key(&subject) {
                let key =
                    subject_key(&self.authority, &subject).map_err(GuardCheckError::Domain)?;
                let state = match rows.get(&(3, key)).and_then(Option::as_ref) {
                    Some(row) => Some(resolve_subject_state(row, store).await?),
                    None => None,
                };
                overlay.insert(subject.clone(), state);
            }
            let next = validate_entry_against_state(
                &member.entry,
                member.expect,
                overlay.get(&subject).and_then(Option::as_ref),
            )
            .map_err(GuardCheckError::Domain)?;
            overlay.insert(subject, next);
        }
        Ok(())
    }
}

async fn resolve_subject_state(
    row: &Value,
    store: &mut dyn ProjectionStore,
) -> Result<EntityInstance, GuardCheckError> {
    let body = tagged_body(row, "er.eventlog.subject-index/1").map_err(GuardCheckError::Store)?;
    let source = body
        .get("state_source")
        .and_then(Value::as_object)
        .ok_or_else(|| GuardCheckError::Domain(integrity("subject row lacks state source")))?;
    let digest = source
        .get("record_blob")
        .or_else(|| source.get("anchor_blob"))
        .and_then(Value::as_str)
        .ok_or_else(|| GuardCheckError::Domain(integrity("state source has no blob")))?;
    let bytes = store
        .get_blob(digest)
        .await?
        .ok_or_else(|| GuardCheckError::Domain(integrity("state source blob is missing")))?;
    match source.get("kind").and_then(Value::as_str) {
        Some("decision") => match decode_record(&bytes).map_err(GuardCheckError::Domain)? {
            entity_store::asynchronous::RecordedEntry::Decision(commit) => Ok(commit.instance),
            _ => Err(GuardCheckError::Domain(integrity(
                "state source is not a decision",
            ))),
        },
        Some("anchor") => Ok(decode_anchor(&bytes)
            .map_err(GuardCheckError::Domain)?
            .instance),
        _ => Err(GuardCheckError::Domain(integrity("unknown state source"))),
    }
}

fn validate_group_result(
    result: &eventlog_core::AppendGroupResult,
    members: usize,
) -> Result<(), AsyncStoreError> {
    if result.appends.len() != members || result.appends.iter().any(|a| a.events.len() != 1) {
        return Err(integrity(
            "provider group result does not reproduce member events",
        ));
    }
    Ok(())
}

fn map_put_error(error: EventLogError) -> AsyncStoreError {
    match error {
        EventLogError::Invalid(v) => AsyncStoreError::ProviderIntegrity {
            provider: "eventlog".into(),
            detail: v,
        },
        other => map_read_error(other),
    }
}
fn map_append_error(error: EventLogError) -> AsyncStoreError {
    match error {
        EventLogError::Invalid(v) => integrity(v),
        EventLogError::Overloaded => AsyncStoreError::Unreachable {
            provider: "eventlog".into(),
            detail: "resource budget exhausted".into(),
        },
        EventLogError::Closed => AsyncStoreError::Unreachable {
            provider: "eventlog".into(),
            detail: "closed".into(),
        },
        EventLogError::Deadline { operation } => AsyncStoreError::Unreachable {
            provider: "eventlog".into(),
            detail: format!("deadline during {operation}"),
        },
        EventLogError::CausationDepthExceeded { .. } => {
            integrity("provider rejected locally validated causation depth")
        }
        EventLogError::NotFound => integrity("provider lost required append authority"),
        EventLogError::Backend(v) => AsyncStoreError::Backend(v),
        other => integrity(other.to_string()),
    }
}

/// Administrative binding capability, separate from runtime open.
pub trait AsyncBindingProvisioner: Send + Sync {
    /// Establishes one immutable binding or recovers its exact winner.
    fn provision_binding<'a>(
        &'a self,
        authority: Authority,
        context: EventlogOperationContext,
    ) -> BoxFuture<'a, Result<ProvisionBindingOutcome, ProvisionBindingFailure>>;
    /// Reads and verifies one binding without mutation.
    fn recover_binding<'a>(
        &'a self,
        authority: Authority,
    ) -> BoxFuture<'a, Result<Option<ProvisionBindingOutcome>, ProvisionBindingFailure>>;
}

/// Unbound administrative owner used under caller-established maintenance exclusion.
pub struct EventlogBindingProvisioner {
    backend: Arc<dyn EventlogBackend>,
    limits: CaptureLimits,
}

impl EventlogBindingProvisioner {
    /// Constructs a non-initializing provisioner over a prepared native handle.
    #[must_use]
    pub fn new(backend: Arc<dyn EventlogBackend>, limits: CaptureLimits) -> Self {
        Self { backend, limits }
    }
}

/// Settled binding coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvisionBindingOutcome {
    /// Exact immutable authority.
    pub authority: Authority,
    /// Actual provider-minted coordinates.
    pub physical: PhysicalRef,
    /// Whether an already committed binding was recovered.
    pub replayed: bool,
}

/// Binding failure with conflict and uncertainty preserved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvisionBindingFailure {
    /// This invocation definitely did not publish a binding.
    NotCommitted(AsyncStoreError),
    /// Another immutable authority won.
    Conflict {
        /// Authority the caller attempted to bind.
        requested: Authority,
        /// Already committed immutable authority.
        found: Authority,
    },
    /// Publication may have happened and recovery could not settle it.
    Uncertain {
        /// Authority whose publication is unresolved.
        authority: Authority,
        /// Recovery condition that prevented a conclusion.
        cause: ProvisionBindingUncertainty,
    },
}

/// Why a binding result is unresolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvisionBindingUncertainty {
    /// The provider may have committed before losing its reply.
    UnknownCommit,
    /// A complete authoritative recovery observation was unavailable.
    RecoveryUnavailable,
}

impl AsyncBindingProvisioner for EventlogBindingProvisioner {
    fn recover_binding<'a>(
        &'a self,
        authority: Authority,
    ) -> BoxFuture<'a, Result<Option<ProvisionBindingOutcome>, ProvisionBindingFailure>> {
        Box::pin(async move { self.recover_inner(authority).await })
    }
    fn provision_binding<'a>(
        &'a self,
        authority: Authority,
        context: EventlogOperationContext,
    ) -> BoxFuture<'a, Result<ProvisionBindingOutcome, ProvisionBindingFailure>> {
        Box::pin(async move {
            authority
                .validate()
                .map_err(ProvisionBindingFailure::NotCommitted)?;
            if let Some(outcome) = self.recover_inner(authority.clone()).await? {
                return Ok(outcome);
            }
            let tenant = TenantId::new(authority.tenant.clone())
                .map_err(|e| ProvisionBindingFailure::NotCommitted(input_eventlog(e)))?;
            let bytes =
                encode_binding(&authority).map_err(ProvisionBindingFailure::NotCommitted)?;
            let digest = framed_key(BINDING_BLOB_DOMAIN, &bytes)
                .map_err(ProvisionBindingFailure::NotCommitted)?;
            let meta = context
                .meta("er.binding/1".into(), digest.clone())
                .map_err(ProvisionBindingFailure::NotCommitted)?;
            self.backend
                .put_blob(&tenant, &digest, &bytes)
                .await
                .map_err(|e| ProvisionBindingFailure::NotCommitted(map_put_error(e)))?;
            let group = AppendGroup {
                tenant: tenant.clone(),
                appends: vec![StreamAppend {
                    stream: StreamId::new(tenant.clone(), "er.binding", "singleton")
                        .map_err(|e| ProvisionBindingFailure::NotCommitted(input_eventlog(e)))?,
                    expected: Expected::NoStream,
                    events: vec![
                        NewEvent::new("er.binding", 1, json!({"blob":digest})).map_err(|e| {
                            ProvisionBindingFailure::NotCommitted(input_eventlog(e))
                        })?,
                    ],
                }],
                meta,
            };
            let slot = Arc::new(Mutex::new(None));
            let guard = Arc::new(BindingGuard {
                authority: authority.clone(),
                tenant: tenant.clone(),
                slot: slot.clone(),
            });
            match self.backend.append_group_guarded(&group, guard).await {
                Ok(result) => {
                    if validate_group_result(&result, 1).is_err() {
                        return Err(ProvisionBindingFailure::Uncertain {
                            authority,
                            cause: ProvisionBindingUncertainty::RecoveryUnavailable,
                        });
                    }
                    let mut settled = match self.recover_inner(authority.clone()).await {
                        Ok(Some(settled)) => settled,
                        Ok(None) | Err(_) => {
                            return Err(ProvisionBindingFailure::Uncertain {
                                authority,
                                cause: ProvisionBindingUncertainty::RecoveryUnavailable,
                            });
                        }
                    };
                    if settled.physical != physical(&result.appends[0].events[0]) {
                        return Err(ProvisionBindingFailure::Uncertain {
                            authority,
                            cause: ProvisionBindingUncertainty::RecoveryUnavailable,
                        });
                    }
                    settled.replayed = result.deduplicated;
                    Ok(settled)
                }
                Err(EventLogError::UnknownCommit) => {
                    match self.recover_inner(authority.clone()).await {
                        Ok(Some(mut outcome)) => {
                            outcome.replayed = true;
                            Ok(outcome)
                        }
                        Err(conflict @ ProvisionBindingFailure::Conflict { .. }) => Err(conflict),
                        Ok(None) | Err(_) => Err(ProvisionBindingFailure::Uncertain {
                            authority,
                            cause: ProvisionBindingUncertainty::UnknownCommit,
                        }),
                    }
                }
                Err(
                    EventLogError::Conflict { .. }
                    | EventLogError::IdempotencyMismatch { .. }
                    | EventLogError::GuardRefused { .. },
                ) => match self.recover_inner(authority.clone()).await {
                    Ok(Some(outcome)) => Ok(outcome),
                    Ok(None) => Err(ProvisionBindingFailure::NotCommitted(integrity(
                        "binding command conflicted without binding authority",
                    ))),
                    Err(error) => Err(error),
                },
                Err(error) => Err(ProvisionBindingFailure::NotCommitted(map_append_error(
                    error,
                ))),
            }
        })
    }
}

impl EventlogBindingProvisioner {
    #[allow(clippy::result_large_err)]
    async fn recover_inner(
        &self,
        authority: Authority,
    ) -> Result<Option<ProvisionBindingOutcome>, ProvisionBindingFailure> {
        authority
            .validate()
            .map_err(ProvisionBindingFailure::NotCommitted)?;
        let tenant = TenantId::new(authority.tenant.clone())
            .map_err(|e| ProvisionBindingFailure::NotCommitted(input_eventlog(e)))?;
        if !self.backend.is_inline(PROJECTOR_NAME).await {
            return Err(ProvisionBindingFailure::NotCommitted(integrity(
                "the fixed inline projector is not attached",
            )));
        }
        let capture = self
            .backend
            .capture_tenant(&tenant, projection_specs(), self.limits)
            .await
            .map_err(|error| ProvisionBindingFailure::Uncertain {
                authority: authority.clone(),
                cause: match error {
                    CaptureError::TenantIdentityMissing
                    | CaptureError::Store(EventLogError::NotFound) => {
                        ProvisionBindingUncertainty::RecoveryUnavailable
                    }
                    _ => ProvisionBindingUncertainty::RecoveryUnavailable,
                },
            })?;
        if capture.tenant != tenant || capture.stream_identity != authority.stream_identity {
            return Err(ProvisionBindingFailure::NotCommitted(integrity(
                "native capture substituted tenant or generation",
            )));
        }
        let observed =
            captured_binding_authority(&capture).map_err(ProvisionBindingFailure::NotCommitted)?;
        let Some(observed) = observed else {
            let model =
                build_model(&authority, capture).map_err(ProvisionBindingFailure::NotCommitted)?;
            debug_assert!(model.binding.is_none());
            return Ok(None);
        };
        if observed.tenant != authority.tenant
            || observed.stream_identity != authority.stream_identity
        {
            return Err(ProvisionBindingFailure::NotCommitted(integrity(
                "binding authority differs from the captured tenant or generation",
            )));
        }
        let model =
            build_model(&observed, capture).map_err(ProvisionBindingFailure::NotCommitted)?;
        let physical = model.binding.ok_or_else(|| {
            ProvisionBindingFailure::NotCommitted(integrity(
                "binding authority has no physical event",
            ))
        })?;
        if observed != authority {
            return Err(ProvisionBindingFailure::Conflict {
                requested: authority,
                found: observed,
            });
        }
        Ok(Some(ProvisionBindingOutcome {
            authority,
            physical,
            replayed: true,
        }))
    }
}

fn captured_binding_authority(
    capture: &TenantCapture,
) -> Result<Option<Authority>, AsyncStoreError> {
    let blobs: BTreeMap<String, Vec<u8>> = capture
        .blobs
        .iter()
        .map(|blob| (blob.digest.clone(), blob.bytes.clone()))
        .collect();
    let mut found = None;
    for event in &capture.events {
        if event.name != "er.binding" {
            continue;
        }
        if event.is_redacted() || event.schema_version != 1 {
            return Err(integrity(
                "binding recovery found redacted or unknown-version authority",
            ));
        }
        if event.stream_type != "er.binding" || event.stream_id != "singleton" || event.version != 1
        {
            return Err(integrity(
                "binding event has inconsistent physical identity",
            ));
        }
        let digest = reference_digest(event)?;
        let bytes = get_bound_blob(&blobs, digest, BINDING_BLOB_DOMAIN)?;
        let authority = decode_binding(bytes)?;
        if found.replace(authority).is_some() {
            return Err(integrity("more than one binding event exists"));
        }
    }
    Ok(found)
}

struct BindingGuard {
    authority: Authority,
    tenant: TenantId,
    slot: Arc<Mutex<Option<GuardRefusal>>>,
}
impl Guard for BindingGuard {
    fn check<'a>(
        &'a self,
        store: &'a mut dyn ProjectionStore,
    ) -> eventlog_core::BoxFuture<'a, Result<(), EventLogError>> {
        Box::pin(async move {
            match store
                .get_for_update(binding_spec(), &self.tenant, "singleton")
                .await?
            {
                None => Ok(()),
                Some(row) => {
                    let body = tagged_body(&row, "er.eventlog.binding-index/1")?;
                    let found: Authority = serde_json::from_value(body["authority"].clone())
                        .map_err(|_| EventLogError::Invalid("binding row is malformed".into()))?;
                    let error = if found == self.authority {
                        integrity("binding retry reached guard instead of Eventlog deduplication")
                    } else {
                        integrity("binding row already names different authority")
                    };
                    let code = GuardRefusalCode::ProviderIntegrity;
                    *self
                        .slot
                        .lock()
                        .map_err(|_| EventLogError::Invalid("guard slot poisoned".into()))? =
                        Some(GuardRefusal { code, error });
                    Err(EventLogError::GuardRefused {
                        code: code.as_str().into(),
                    })
                }
            }
        })
    }
}

/// Separate administrative imported-boundary writer.
pub trait AsyncImportedAnchorWriter: Send + Sync {
    /// Atomically establishes one verified imported boundary.
    fn import_anchor<'a>(
        &'a self,
        history: SubjectHistory,
    ) -> BoxFuture<'a, Result<ImportAnchorOutcome, ImportAnchorFailure>>;
}

/// Result of a settled imported boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportAnchorOutcome {
    /// Verification assurance established for the imported boundary.
    pub assurance: SubjectAssurance,
    /// Whether the exact anchor was already committed.
    pub replayed: bool,
}

/// Imported-boundary failure preserving subject-keyed uncertainty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportAnchorFailure {
    /// The invocation definitely did not publish the anchor.
    NotCommitted(AsyncStoreError),
    /// The anchor may have committed and recovery could not settle it.
    Uncertain {
        /// Subject whose import is unresolved.
        subject: Subject,
        /// Recovery condition that prevented a conclusion.
        cause: ImportAnchorUncertainty,
    },
}

/// Why an import result is unresolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportAnchorUncertainty {
    /// The provider may have committed before losing its reply.
    UnknownCommit,
    /// A complete authoritative recovery observation was unavailable.
    RecoveryUnavailable,
}

impl AsyncImportedAnchorWriter for EventlogOperationStore<'_> {
    fn import_anchor<'a>(
        &'a self,
        history: SubjectHistory,
    ) -> BoxFuture<'a, Result<ImportAnchorOutcome, ImportAnchorFailure>> {
        Box::pin(async move { self.import_inner(history).await })
    }
}

impl EventlogOperationStore<'_> {
    async fn import_inner(
        &self,
        history: SubjectHistory,
    ) -> Result<ImportAnchorOutcome, ImportAnchorFailure> {
        let subject = history.subject.clone();
        let HistoryOrigin::Imported(anchor) = &history.origin else {
            return Err(ImportAnchorFailure::NotCommitted(
                AsyncStoreError::InvalidInput("import requires an Imported origin".into()),
            ));
        };
        if !history.records.is_empty() {
            return Err(ImportAnchorFailure::NotCommitted(
                AsyncStoreError::InvalidInput(
                    "import anchor input must have an empty suffix".into(),
                ),
            ));
        }
        let assurance = verify_subject_history(&history, &anchor.instance)
            .map_err(ImportAnchorFailure::NotCommitted)?;
        let model = self
            .store
            .capture_model()
            .await
            .map_err(ImportAnchorFailure::NotCommitted)?;
        let mut record_blobs = Vec::new();
        let mut uploads = Vec::new();
        for evidence in &anchor.evidence {
            if let entity_store::asynchronous::LegacyEvidence::Envelope(saved) = evidence {
                let bytes = record_comparison_bytes(&saved.entry)
                    .map_err(ImportAnchorFailure::NotCommitted)?;
                let digest = framed_key(RECORD_BLOB_DOMAIN, &bytes)
                    .map_err(ImportAnchorFailure::NotCommitted)?;
                record_blobs.push(digest.clone());
                uploads.push((digest, bytes));
            }
        }
        let wrapper = anchor_from_history(self.store.authority.clone(), &history, &record_blobs)
            .map_err(ImportAnchorFailure::NotCommitted)?;
        let bytes = encode_anchor(&wrapper).map_err(ImportAnchorFailure::NotCommitted)?;
        if let Some(existing) = model.anchors.get(&subject) {
            if *existing == bytes {
                return Ok(ImportAnchorOutcome {
                    assurance,
                    replayed: true,
                });
            }
            return Err(ImportAnchorFailure::NotCommitted(
                AsyncStoreError::RevisionConflict {
                    subject,
                    expected: Expect::Absent,
                    found: model.terminals.get(&history.subject).map(|i| i.revision),
                },
            ));
        }
        if let Some(existing) = model.histories.get(&subject) {
            return Err(ImportAnchorFailure::NotCommitted(
                AsyncStoreError::RevisionConflict {
                    subject,
                    expected: Expect::Absent,
                    found: model.terminals.get(&existing.subject).map(|i| i.revision),
                },
            ));
        }
        let digest =
            framed_key(ANCHOR_BLOB_DOMAIN, &bytes).map_err(ImportAnchorFailure::NotCommitted)?;
        let command_key = key_for_value(
            "er.eventlog.import-command-key/1",
            json!({"authority":self.store.authority,"subject":SubjectWire::from(&history.subject)}),
        )
        .map_err(ImportAnchorFailure::NotCommitted)?;
        let meta = self
            .context
            .meta(command_key, digest.clone())
            .map_err(ImportAnchorFailure::NotCommitted)?;
        for (key, value) in uploads {
            self.store
                .backend
                .put_blob(&self.store.tenant, &key, &value)
                .await
                .map_err(|e| ImportAnchorFailure::NotCommitted(map_put_error(e)))?;
        }
        self.store
            .backend
            .put_blob(&self.store.tenant, &digest, &bytes)
            .await
            .map_err(|e| ImportAnchorFailure::NotCommitted(map_put_error(e)))?;
        let group = AppendGroup {
            tenant: self.store.tenant.clone(),
            appends: vec![StreamAppend {
                stream: StreamId::new(
                    self.store.tenant.clone(),
                    "er.subject",
                    subject_stream_id(&self.store.authority, &history.subject)
                        .map_err(ImportAnchorFailure::NotCommitted)?,
                )
                .map_err(|e| ImportAnchorFailure::NotCommitted(input_eventlog(e)))?,
                expected: Expected::NoStream,
                events: vec![
                    NewEvent::new("er.import_anchor", 1, json!({"blob":digest}))
                        .map_err(|e| ImportAnchorFailure::NotCommitted(input_eventlog(e)))?,
                ],
            }],
            meta,
        };
        let slot = Arc::new(Mutex::new(None));
        let guard = Arc::new(ImportGuard {
            authority: self.store.authority.clone(),
            tenant: self.store.tenant.clone(),
            subject: history.subject.clone(),
            record_ids: anchor
                .evidence
                .iter()
                .filter_map(|e| match e {
                    entity_store::asynchronous::LegacyEvidence::Envelope(saved) => {
                        Some(saved.entry.record_id().to_owned())
                    }
                    _ => None,
                })
                .collect(),
            slot: slot.clone(),
        });
        match self.store.backend.append_group_guarded(&group, guard).await {
            Ok(result) => {
                if validate_group_result(&result, 1).is_err() {
                    return Err(ImportAnchorFailure::Uncertain {
                        subject,
                        cause: ImportAnchorUncertainty::RecoveryUnavailable,
                    });
                }
                let returned = physical(&result.appends[0].events[0]);
                match self.store.capture_model().await {
                    Ok(model)
                        if model.anchors.get(&history.subject) == Some(&bytes)
                            && model.anchor_physical.get(&history.subject) == Some(&returned) =>
                    {
                        Ok(ImportAnchorOutcome {
                            assurance,
                            replayed: result.deduplicated,
                        })
                    }
                    _ => Err(ImportAnchorFailure::Uncertain {
                        subject: history.subject,
                        cause: ImportAnchorUncertainty::RecoveryUnavailable,
                    }),
                }
            }
            Err(EventLogError::UnknownCommit) => match self.store.capture_model().await {
                Ok(model) if model.anchors.get(&history.subject) == Some(&bytes) => {
                    Ok(ImportAnchorOutcome {
                        assurance,
                        replayed: true,
                    })
                }
                _ => Err(ImportAnchorFailure::Uncertain {
                    subject: history.subject,
                    cause: ImportAnchorUncertainty::UnknownCommit,
                }),
            },
            Err(EventLogError::Conflict { .. } | EventLogError::IdempotencyMismatch { .. }) => {
                match self.store.capture_model().await {
                    Ok(model) if model.anchors.get(&history.subject) == Some(&bytes) => {
                        Ok(ImportAnchorOutcome {
                            assurance,
                            replayed: true,
                        })
                    }
                    Ok(model) => Err(ImportAnchorFailure::NotCommitted(
                        AsyncStoreError::RevisionConflict {
                            subject: history.subject.clone(),
                            expected: Expect::Absent,
                            found: model.terminals.get(&history.subject).map(|i| i.revision),
                        },
                    )),
                    Err(_) => Err(ImportAnchorFailure::Uncertain {
                        subject: history.subject,
                        cause: ImportAnchorUncertainty::RecoveryUnavailable,
                    }),
                }
            }
            Err(EventLogError::GuardRefused { code }) => {
                let refusal = slot
                    .lock()
                    .map_err(|_| {
                        ImportAnchorFailure::NotCommitted(integrity(
                            "import guard refusal slot was poisoned",
                        ))
                    })?
                    .take();
                match refusal {
                    Some(refusal)
                        if refusal.code.as_str() == code
                            && refusal.code.matches(&refusal.error) =>
                    {
                        Err(ImportAnchorFailure::NotCommitted(refusal.error))
                    }
                    _ => Err(ImportAnchorFailure::NotCommitted(integrity(
                        "import guard refusal code and typed slot disagree",
                    ))),
                }
            }
            Err(error) => Err(ImportAnchorFailure::NotCommitted(map_append_error(error))),
        }
    }
}

struct ImportGuard {
    authority: Authority,
    tenant: TenantId,
    subject: Subject,
    record_ids: Vec<String>,
    slot: Arc<Mutex<Option<GuardRefusal>>>,
}
impl Guard for ImportGuard {
    fn check<'a>(
        &'a self,
        store: &'a mut dyn ProjectionStore,
    ) -> eventlog_core::BoxFuture<'a, Result<(), EventLogError>> {
        Box::pin(async move {
            let binding = store
                .get_for_update(binding_spec(), &self.tenant, "singleton")
                .await?;
            let Some(binding) = binding else {
                return self.refuse(integrity("binding row is absent"));
            };
            let body = tagged_body(&binding, "er.eventlog.binding-index/1")?;
            let found: Authority = serde_json::from_value(body["authority"].clone())
                .map_err(|_| EventLogError::Invalid("binding row is malformed".into()))?;
            if found != self.authority {
                return self.refuse(integrity("binding row authority changed"));
            }

            let mut records = self
                .record_ids
                .iter()
                .map(|id| {
                    record_key(&self.authority, id)
                        .map(|key| (key, id))
                        .map_err(|error| EventLogError::Invalid(error.to_string()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            records.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
            for (key, record_id) in records {
                if store
                    .get_for_update(record_spec(), &self.tenant, &key)
                    .await?
                    .is_some()
                {
                    return self.refuse(AsyncStoreError::RecordConflict {
                        record_id: record_id.clone(),
                    });
                }
            }

            let subject_key = subject_key(&self.authority, &self.subject)
                .map_err(|error| EventLogError::Invalid(error.to_string()))?;
            if let Some(row) = store
                .get_for_update(subject_spec(), &self.tenant, &subject_key)
                .await?
            {
                let body = tagged_body(&row, "er.eventlog.subject-index/1")?;
                let found = body.get("revision").and_then(Value::as_u64);
                return self.refuse(AsyncStoreError::RevisionConflict {
                    subject: self.subject.clone(),
                    expected: Expect::Absent,
                    found,
                });
            }
            Ok(())
        })
    }
}

impl ImportGuard {
    fn refuse(&self, error: AsyncStoreError) -> Result<(), EventLogError> {
        let code = GuardRefusalCode::for_error(&error);
        let mut slot = self
            .slot
            .lock()
            .map_err(|_| EventLogError::Invalid("import guard slot poisoned".into()))?;
        if slot.is_some() {
            return Err(EventLogError::Invalid(
                "import guard slot written twice".into(),
            ));
        }
        *slot = Some(GuardRefusal { code, error });
        Err(EventLogError::GuardRefused {
            code: code.as_str().into(),
        })
    }
}

#[derive(Default)]
struct CapturedModel {
    binding: Option<PhysicalRef>,
    histories: BTreeMap<Subject, SubjectHistory>,
    terminals: BTreeMap<Subject, EntityInstance>,
    records: BTreeMap<String, RecordLookup>,
    record_physical: BTreeMap<String, PhysicalRef>,
    batches: BTreeMap<BatchKey, StoredBatch>,
    anchors: BTreeMap<Subject, Vec<u8>>,
    anchor_physical: BTreeMap<Subject, PhysicalRef>,
}

struct PendingRecord {
    wrapper: RecordedEntryWrapper,
    entry: entity_store::asynchronous::RecordedEntry,
    request_bytes: Vec<u8>,
    batch_bytes: Vec<u8>,
    event: RecordedEvent,
}

fn build_model(
    authority: &Authority,
    capture: TenantCapture,
) -> Result<CapturedModel, AsyncStoreError> {
    let blobs: BTreeMap<String, Vec<u8>> = capture
        .blobs
        .into_iter()
        .map(|blob| (blob.digest, blob.bytes))
        .collect();
    let mut model = CapturedModel::default();
    let mut pending = Vec::new();
    for event in &capture.events {
        if event.is_redacted() || event.schema_version != 1 {
            return Err(integrity("redacted or unknown-version authority event"));
        }
        let digest = reference_digest(event)?;
        let bytes = blobs
            .get(digest)
            .ok_or_else(|| integrity("authority event references a missing blob"))?;
        match event.name.as_str() {
            "er.binding" => {
                verify_digest(BINDING_BLOB_DOMAIN, digest, bytes)?;
                let found = decode_binding(bytes)?;
                if found != *authority
                    || event.stream_type != "er.binding"
                    || event.stream_id != "singleton"
                    || event.version != 1
                    || model.binding.is_some()
                {
                    return Err(integrity("binding authority is duplicated or inconsistent"));
                }
                model.binding = Some(physical(event));
            }
            "er.recorded_entry" => {
                verify_digest(ENTRY_BLOB_DOMAIN, digest, bytes)?;
                let wrapper = decode_entry(bytes)?;
                require_authority(authority, &wrapper.authority)?;
                let subject: Subject = wrapper.subject.clone().into();
                if event.stream_type != "er.subject"
                    || event.stream_id != subject_stream_id(authority, &subject)?
                {
                    return Err(integrity("record reference is in another stream"));
                }
                let record_bytes =
                    get_bound_blob(&blobs, &wrapper.record_blob, RECORD_BLOB_DOMAIN)?;
                let entry = decode_record(record_bytes)?;
                if entry.subject() != subject {
                    return Err(integrity("record wrapper substitutes its subject"));
                }
                let request_bytes =
                    get_bound_blob(&blobs, &wrapper.request_blob, REQUEST_BLOB_DOMAIN)?.to_vec();
                if original_request_comparison_bytes(&entry)? != request_bytes {
                    return Err(integrity("request blob differs from record"));
                }
                let batch_bytes =
                    get_bound_blob(&blobs, &wrapper.batch_blob, BATCH_BLOB_DOMAIN)?.to_vec();
                pending.push(PendingRecord {
                    wrapper,
                    entry,
                    request_bytes,
                    batch_bytes,
                    event: event.clone(),
                });
            }
            "er.import_anchor" => {
                build_import(authority, event, bytes, digest, &blobs, &mut model)?
            }
            _ => return Err(integrity("unknown event exists in the bound ER tenant")),
        }
    }
    if model.binding.is_none() && !capture.events.is_empty() {
        return Err(integrity("authoritative events exist without a binding"));
    }
    build_committed(pending, &mut model)?;
    validate_projection_sets(authority, &capture.projections, &model)?;
    Ok(model)
}

fn build_import(
    authority: &Authority,
    event: &RecordedEvent,
    bytes: &[u8],
    digest: &str,
    blobs: &BTreeMap<String, Vec<u8>>,
    model: &mut CapturedModel,
) -> Result<(), AsyncStoreError> {
    verify_digest(ANCHOR_BLOB_DOMAIN, digest, bytes)?;
    let wrapper = decode_anchor(bytes)?;
    require_authority(authority, &wrapper.authority)?;
    let subject: Subject = wrapper.subject.clone().into();
    if event.stream_type != "er.subject"
        || event.stream_id != subject_stream_id(authority, &subject)?
    {
        return Err(integrity("import anchor is in another subject stream"));
    }
    if model.histories.contains_key(&subject) {
        return Err(corrupt(&subject, "a subject has more than one origin"));
    }
    let record_bytes = wrapper
        .evidence
        .iter()
        .filter_map(|item| match item {
            EvidenceWire::Envelope { record_blob, .. } => {
                Some(get_bound_blob(blobs, record_blob, RECORD_BLOB_DOMAIN).map(Vec::from))
            }
            _ => None,
        })
        .collect::<Result<Vec<_>, _>>()?;
    let history = history_from_anchor(&wrapper, &record_bytes)?;
    if let HistoryOrigin::Imported(anchor) = &history.origin {
        for evidence in &anchor.evidence {
            if let entity_store::asynchronous::LegacyEvidence::Envelope(saved) = evidence {
                let record_id = saved.entry.record_id().to_owned();
                if model
                    .records
                    .insert(record_id.clone(), RecordLookup::Imported(saved.clone()))
                    .is_some()
                {
                    return Err(corrupt(&subject, "global imported record identity repeats"));
                }
                model.record_physical.insert(record_id, physical(event));
            }
        }
        model
            .terminals
            .insert(subject.clone(), anchor.instance.clone());
    }
    model.anchors.insert(subject.clone(), bytes.to_vec());
    model
        .anchor_physical
        .insert(subject.clone(), physical(event));
    model.histories.insert(subject, history);
    Ok(())
}

fn build_committed(
    pending: Vec<PendingRecord>,
    model: &mut CapturedModel,
) -> Result<(), AsyncStoreError> {
    let mut grouped: BTreeMap<BatchKey, Vec<PendingRecord>> = BTreeMap::new();
    for record in pending {
        grouped
            .entry(record.wrapper.batch_key.clone().into())
            .or_default()
            .push(record);
    }
    for (key, mut group) in grouped {
        group.sort_by_key(|record| record.wrapper.member_index);
        let (decoded_key, members) = decode_batch(&group[0].batch_bytes)?;
        if decoded_key != key || group.len() != members.len() {
            return Err(integrity("batch references are incomplete"));
        }
        let mut stored = Vec::with_capacity(group.len());
        let mut prior_position = 0;
        for (index, record) in group.into_iter().enumerate() {
            let index_u64 =
                u64::try_from(index).map_err(|_| AsyncStoreError::PositionExhausted {
                    domain: "batch member".into(),
                })?;
            if record.wrapper.member_index != index_u64 || record.event.global_seq <= prior_position
            {
                return Err(integrity("batch member order or position is crossed"));
            }
            let member = &members[index];
            if member.entry != record.entry || member.request_bytes != record.request_bytes {
                return Err(integrity("batch member differs from reference"));
            }
            prior_position = record.event.global_seq;
            let subject = record.entry.subject();
            let position = RecordPosition {
                subject: record.event.version,
                store: record.event.global_seq,
            };
            let receipt = RecordReceipt {
                record_id: record.entry.record_id().to_owned(),
                subject: subject.clone(),
                kind: record.entry.kind(),
                revision: record.entry.revision(),
                position,
                batch_key: key.clone(),
                member_index: index_u64,
            };
            let saved = StoredRecord {
                entry: record.entry,
                position,
                receipt: receipt.clone(),
                expect: member.expect,
                request_bytes: record.request_bytes,
                record_bytes: get_record_bytes(&member.entry)?,
            };
            if model
                .records
                .insert(
                    receipt.record_id.clone(),
                    RecordLookup::Committed(saved.clone()),
                )
                .is_some()
            {
                return Err(AsyncStoreError::RecordConflict {
                    record_id: receipt.record_id,
                });
            }
            model
                .record_physical
                .insert(receipt.record_id.clone(), physical(&record.event));
            let history =
                model
                    .histories
                    .entry(subject.clone())
                    .or_insert_with(|| SubjectHistory {
                        subject: subject.clone(),
                        origin: HistoryOrigin::Genesis,
                        records: Vec::new(),
                    });
            history.records.push(saved.clone());
            stored.push(saved);
        }
        let receipt = match &key {
            BatchKey::SingleRecord(_) => CommitReceipt::Single(stored[0].receipt.clone()),
            BatchKey::Named(_) => CommitReceipt::Batch(BatchReceipt {
                key: key.clone(),
                members: stored.iter().map(|r| r.receipt.clone()).collect(),
            }),
        };
        if model
            .batches
            .insert(
                key.clone(),
                StoredBatch {
                    key,
                    records: stored,
                    comparison_bytes: group_batch_bytes(&members, &decoded_key)?,
                    receipt,
                },
            )
            .is_some()
        {
            return Err(AsyncStoreError::BatchConflict { key: decoded_key });
        }
    }
    for history in model.histories.values_mut() {
        history.records.sort_by_key(|record| record.position.store);
        let terminal = history
            .records
            .iter()
            .rev()
            .find_map(|record| match &record.entry {
                entity_store::asynchronous::RecordedEntry::Decision(commit) => {
                    Some(commit.instance.clone())
                }
                entity_store::asynchronous::RecordedEntry::Observation(_) => None,
            })
            .or_else(|| match &history.origin {
                HistoryOrigin::Imported(anchor) => Some(anchor.instance.clone()),
                HistoryOrigin::Genesis => None,
            })
            .ok_or_else(|| corrupt(&history.subject, "history has no state-producing record"))?;
        verify_subject_history(history, &terminal)?;
        model.terminals.insert(history.subject.clone(), terminal);
    }
    Ok(())
}

fn group_batch_bytes(members: &[AppendMember], key: &BatchKey) -> Result<Vec<u8>, AsyncStoreError> {
    batch_comparison_bytes(key, members)
}
fn get_record_bytes(
    entry: &entity_store::asynchronous::RecordedEntry,
) -> Result<Vec<u8>, AsyncStoreError> {
    record_comparison_bytes(entry)
}

fn validate_projection_sets(
    authority: &Authority,
    projections: &[eventlog_core::CapturedProjection],
    model: &CapturedModel,
) -> Result<(), AsyncStoreError> {
    if projections.len() != 4
        || projections
            .iter()
            .map(|p| p.specification.name)
            .ne(projection_specs().iter().map(|p| p.name))
    {
        return Err(integrity(
            "native capture did not return the exact projection set",
        ));
    }
    let expected = expected_projection_rows(authority, model)?;
    for (capture, expected_rows) in projections.iter().zip(expected) {
        let actual: BTreeMap<_, _> = capture.rows.iter().cloned().collect();
        if actual.len() != capture.rows.len() || actual != expected_rows {
            return Err(integrity(format!(
                "projection {} differs from authoritative events",
                capture.specification.name
            )));
        }
    }
    Ok(())
}

fn expected_projection_rows(
    authority: &Authority,
    model: &CapturedModel,
) -> Result<[BTreeMap<String, Value>; 4], AsyncStoreError> {
    let mut binding = BTreeMap::new();
    if let Some(physical) = &model.binding {
        let bytes = encode_binding(authority)?;
        binding.insert(
            "singleton".to_owned(),
            json!(["er.eventlog.binding-index/1", {
                "authority":authority,
                "binding_blob":framed_key(BINDING_BLOB_DOMAIN, &bytes)?,
                "physical":physical,
            }]),
        );
    }

    let mut records = BTreeMap::new();
    for (record_id, lookup) in &model.records {
        let physical = model
            .record_physical
            .get(record_id)
            .ok_or_else(|| integrity("record has no authoritative physical reference"))?;
        let row = match lookup {
            RecordLookup::Committed(saved) => {
                let batch = model
                    .batches
                    .get(&saved.receipt.batch_key)
                    .ok_or_else(|| integrity("committed record has no authoritative batch"))?;
                json!(["er.eventlog.record-index/1", {"entry": {
                    "kind":"committed", "authority":authority, "record_id":record_id,
                    "subject":SubjectWire::from(&saved.entry.subject()),
                    "record_kind":record_kind_name(saved.entry.kind()),
                    "revision":saved.entry.revision(),
                    "batch_key":crate::encoding::BatchKeyWire::from(&saved.receipt.batch_key),
                    "member_index":saved.receipt.member_index,
                    "record_blob":framed_key(RECORD_BLOB_DOMAIN, &saved.record_bytes)?,
                    "request_blob":framed_key(REQUEST_BLOB_DOMAIN, &saved.request_bytes)?,
                    "batch_blob":framed_key(BATCH_BLOB_DOMAIN, &batch.comparison_bytes)?,
                    "physical":physical,
                }}])
            }
            RecordLookup::Imported(saved) => {
                let subject = saved.entry.subject();
                let anchor = model
                    .anchors
                    .get(&subject)
                    .ok_or_else(|| integrity("imported record has no authoritative anchor"))?;
                json!(["er.eventlog.record-index/1", {"entry": {
                    "kind":"imported", "authority":authority, "record_id":record_id,
                    "subject":SubjectWire::from(&subject),
                    "record_kind":record_kind_name(saved.entry.kind()),
                    "revision":saved.entry.revision(),
                    "anchor_blob":framed_key(ANCHOR_BLOB_DOMAIN, anchor)?,
                    "evidence_index":imported_evidence_index(model, &subject, record_id)?,
                    "record_blob":framed_key(RECORD_BLOB_DOMAIN, &record_comparison_bytes(&saved.entry)?)?,
                    "anchor_physical":physical,
                }}])
            }
        };
        records.insert(record_key(authority, record_id)?, row);
    }

    let mut batches = BTreeMap::new();
    for (key, batch) in &model.batches {
        let mut members = Vec::with_capacity(batch.records.len());
        for saved in &batch.records {
            let physical = model
                .record_physical
                .get(saved.entry.record_id())
                .ok_or_else(|| integrity("batch member has no physical reference"))?;
            members.push(json!({
                "member_index":saved.receipt.member_index,
                "record_id":saved.entry.record_id(),
                "record_key":record_key(authority, saved.entry.record_id())?,
                "subject":SubjectWire::from(&saved.entry.subject()),
                "record_kind":record_kind_name(saved.entry.kind()),
                "revision":saved.entry.revision(),
                "record_blob":framed_key(RECORD_BLOB_DOMAIN, &saved.record_bytes)?,
                "request_blob":framed_key(REQUEST_BLOB_DOMAIN, &saved.request_bytes)?,
                "physical":physical,
            }));
        }
        batches.insert(
            physical_batch_key(authority, key)?,
            json!(["er.eventlog.batch-index/1", {
                "authority":authority,
                "batch_key":crate::encoding::BatchKeyWire::from(key),
                "batch_blob":framed_key(BATCH_BLOB_DOMAIN, &batch.comparison_bytes)?,
                "members":members,
            }]),
        );
    }

    let mut subjects = BTreeMap::new();
    for (subject, history) in &model.histories {
        let terminal = model
            .terminals
            .get(subject)
            .ok_or_else(|| integrity("subject has no terminal state"))?;
        let anchor = model.anchors.get(subject);
        let anchor_physical = model.anchor_physical.get(subject);
        let origin = match (&history.origin, anchor, anchor_physical) {
            (HistoryOrigin::Genesis, None, None) => json!({"kind":"genesis"}),
            (HistoryOrigin::Imported(_), Some(bytes), Some(physical)) => json!({
                "kind":"imported", "anchor_blob":framed_key(ANCHOR_BLOB_DOMAIN, bytes)?,
                "anchor_physical":physical,
            }),
            _ => {
                return Err(integrity(
                    "subject origin does not match its authoritative anchor",
                ));
            }
        };
        let last_decision = history.records.iter().rev().find(|saved| {
            matches!(
                saved.entry,
                entity_store::asynchronous::RecordedEntry::Decision(_)
            )
        });
        let state_source = if let Some(saved) = last_decision {
            json!({"kind":"decision", "record_blob":framed_key(RECORD_BLOB_DOMAIN, &saved.record_bytes)?})
        } else if let Some(bytes) = anchor {
            json!({"kind":"anchor", "anchor_blob":framed_key(ANCHOR_BLOB_DOMAIN, bytes)?})
        } else {
            return Err(integrity("subject has no authoritative state source"));
        };
        let physical_head = history
            .records
            .last()
            .and_then(|saved| model.record_physical.get(saved.entry.record_id()))
            .or(anchor_physical)
            .ok_or_else(|| integrity("subject has no authoritative physical head"))?;
        subjects.insert(
            subject_key(authority, subject)?,
            json!(["er.eventlog.subject-index/1", {
                "authority":authority, "subject":SubjectWire::from(subject), "origin":origin,
                "revision":terminal.revision, "state_source":state_source,
                "physical_head":physical_head,
            }]),
        );
    }
    Ok([binding, records, batches, subjects])
}

fn imported_evidence_index(
    model: &CapturedModel,
    subject: &Subject,
    record_id: &str,
) -> Result<usize, AsyncStoreError> {
    let history = model
        .histories
        .get(subject)
        .ok_or_else(|| integrity("imported record has no subject history"))?;
    let HistoryOrigin::Imported(anchor) = &history.origin else {
        return Err(integrity("imported record belongs to a genesis history"));
    };
    anchor
        .evidence
        .iter()
        .position(|item| matches!(item, entity_store::asynchronous::LegacyEvidence::Envelope(saved) if saved.entry.record_id() == record_id))
        .ok_or_else(|| integrity("imported record is absent from its anchor evidence"))
}

fn record_kind_name(kind: entity_store::asynchronous::RecordKind) -> &'static str {
    match kind {
        entity_store::asynchronous::RecordKind::Decision => "decision",
        entity_store::asynchronous::RecordKind::Observation => "observation",
    }
}

fn reference_digest(event: &RecordedEvent) -> Result<&str, AsyncStoreError> {
    let object = event
        .data
        .as_object()
        .ok_or_else(|| integrity("reference body is not an object"))?;
    if object.len() != 1 {
        return Err(integrity("reference body has unknown fields"));
    }
    object
        .get("blob")
        .and_then(Value::as_str)
        .ok_or_else(|| integrity("reference body has no blob"))
}
fn verify_digest(domain: &str, digest: &str, bytes: &[u8]) -> Result<(), AsyncStoreError> {
    if framed_key(domain, bytes)? == digest {
        Ok(())
    } else {
        Err(integrity("blob digest/domain mismatch"))
    }
}
fn get_bound_blob<'a>(
    blobs: &'a BTreeMap<String, Vec<u8>>,
    digest: &str,
    domain: &str,
) -> Result<&'a [u8], AsyncStoreError> {
    let bytes = blobs
        .get(digest)
        .ok_or_else(|| integrity("referenced blob is missing"))?;
    verify_digest(domain, digest, bytes)?;
    Ok(bytes)
}
fn require_authority(expected: &Authority, found: &Authority) -> Result<(), AsyncStoreError> {
    if expected == found {
        Ok(())
    } else {
        Err(integrity("reference substitutes authority"))
    }
}
fn corrupt(subject: &Subject, detail: impl Into<String>) -> AsyncStoreError {
    AsyncStoreError::CorruptHistory {
        subject: subject.clone(),
        detail: detail.into(),
    }
}
fn integrity(detail: impl Into<String>) -> AsyncStoreError {
    AsyncStoreError::ProviderIntegrity {
        provider: "eventlog".into(),
        detail: detail.into(),
    }
}
fn input_eventlog(error: EventLogError) -> AsyncStoreError {
    AsyncStoreError::InvalidInput(error.to_string())
}
fn map_read_error(error: EventLogError) -> AsyncStoreError {
    match error {
        EventLogError::Backend(v) => AsyncStoreError::Backend(v),
        EventLogError::Closed => AsyncStoreError::Unreachable {
            provider: "eventlog".into(),
            detail: "closed".into(),
        },
        other => integrity(other.to_string()),
    }
}
fn map_capture(error: CaptureError) -> AsyncStoreError {
    match error {
        CaptureError::Store(error) => map_read_error(error),
        other => integrity(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eventlog_core::BlobMigrationReport;

    fn subject() -> Subject {
        Subject::new("ticket", "fault-matrix").expect("subject")
    }

    #[test]
    fn every_guard_refusal_has_a_stable_code_and_typed_match() {
        let key = BatchKey::Named("fault-matrix".into());
        let cases = [
            (
                GuardRefusalCode::RevisionConflict,
                AsyncStoreError::RevisionConflict {
                    subject: subject(),
                    expected: Expect::Absent,
                    found: Some(1),
                },
                "er_revision_conflict",
            ),
            (
                GuardRefusalCode::RecordConflict,
                AsyncStoreError::RecordConflict {
                    record_id: "record".into(),
                },
                "er_record_conflict",
            ),
            (
                GuardRefusalCode::BatchConflict,
                AsyncStoreError::BatchConflict { key: key.clone() },
                "er_batch_conflict",
            ),
            (
                GuardRefusalCode::PreviouslyRecordedBatchEntries,
                AsyncStoreError::PreviouslyRecordedBatchEntries { indices: vec![0] },
                "er_previously_recorded_batch_entries",
            ),
            (
                GuardRefusalCode::CorruptHistory,
                AsyncStoreError::CorruptHistory {
                    subject: subject(),
                    detail: "corrupt".into(),
                },
                "er_corrupt_history",
            ),
            (
                GuardRefusalCode::ProviderIntegrity,
                integrity("provider"),
                "er_provider_integrity",
            ),
        ];
        for (code, error, wire) in cases {
            assert_eq!(code.as_str(), wire);
            assert_eq!(GuardRefusalCode::for_error(&error), code);
            assert!(code.matches(&error));
        }
    }

    #[test]
    fn every_eventlog_append_error_has_an_explicit_er_classification() {
        let cases = [
            (EventLogError::Invalid("invalid".into()), "integrity"),
            (
                EventLogError::GuardRefused {
                    code: "foreign".into(),
                },
                "integrity",
            ),
            (EventLogError::Overloaded, "unreachable"),
            (EventLogError::Closed, "unreachable"),
            (
                EventLogError::Deadline {
                    operation: "append",
                },
                "unreachable",
            ),
            (EventLogError::UnknownCommit, "integrity"),
            (EventLogError::BlobMigrationCommitUnknown, "integrity"),
            (
                EventLogError::BlobMigrationCompleted {
                    report: BlobMigrationReport {
                        upgraded: true,
                        trusted_legacy_rows: 1,
                    },
                    cleanup: Box::new(EventLogError::Backend("cleanup".into())),
                },
                "integrity",
            ),
            (
                EventLogError::Conflict {
                    expected: 0,
                    actual: 1,
                },
                "integrity",
            ),
            (
                EventLogError::IdempotencyMismatch { key: "key".into() },
                "integrity",
            ),
            (
                EventLogError::CausationDepthExceeded { depth: 2, limit: 1 },
                "integrity",
            ),
            (EventLogError::NotFound, "integrity"),
            (EventLogError::Backend("backend".into()), "backend"),
        ];
        for (error, class) in cases {
            let mapped = map_append_error(error);
            assert!(
                matches!(
                    (&mapped, class),
                    (AsyncStoreError::ProviderIntegrity { .. }, "integrity")
                        | (AsyncStoreError::Unreachable { .. }, "unreachable")
                        | (AsyncStoreError::Backend(_), "backend")
                ),
                "unexpected {class} classification: {mapped:?}"
            );
        }
    }
}
