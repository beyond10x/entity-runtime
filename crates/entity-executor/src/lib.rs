//! Pure asynchronous execution over complete recorded storage ports.
//!
//! The kernel remains synchronous and deterministic. This crate orders asynchronous identity,
//! history, state, and append calls around it without selecting a runtime, clock, identifier, or
//! external authority.

use std::{collections::BTreeMap, fmt};

use entity_core::{
    create as decide_create, normalize_arguments, CoreError, DecisionCommand, EntityInstance,
    Registry, Runtime, ValidatedDefinition,
};
use entity_store::{
    asynchronous::{
        batch_comparison_bytes, canonical_domain_bytes, original_request_comparison_bytes,
        request_domain, verify_imported_record, verify_subject_prefix, AppendMember, AppendOutcome,
        AppendRequest, AsyncRecordedStore, AsyncStoreError, BatchKey, CommitReceipt, RecordLookup,
        RecordedEntry, StoredBatch, StoredRecord, Subject, SubjectAssurance, WriteFailure,
    },
    Expect, RecordedCommit, RecordedObservation, Recording,
};
use serde_json::Value;

/// A complete caller-supplied create request.
#[derive(Debug, Clone, PartialEq)]
pub struct CreateRequest {
    /// Subject to create.
    pub subject: Subject,
    /// Exact definition version selected by the caller.
    pub definition_version: u32,
    /// Creation fields before kernel defaults and validation.
    pub fields: Value,
    /// Complete caller-supplied recording metadata.
    pub recording: Recording,
}

/// A complete caller-supplied execute request with an explicit predecessor.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecuteRequest {
    /// Subject to execute.
    pub subject: Subject,
    /// Exact predecessor revision the caller acted on.
    pub expected_revision: u64,
    /// Declared operation name.
    pub operation: String,
    /// Operation arguments before kernel defaults and validation.
    pub arguments: Value,
    /// Complete caller-supplied recording metadata.
    pub recording: Recording,
}

/// One action in an ordered atomic recorded batch.
#[derive(Debug, Clone, PartialEq)]
pub enum BatchAction {
    /// Create one subject.
    Create(CreateRequest),
    /// Execute one operation against an exact predecessor.
    Execute(ExecuteRequest),
    /// Append non-state-changing evidence at an exact revision.
    Observe(RecordedObservation),
}

impl BatchAction {
    fn subject(&self) -> Subject {
        match self {
            Self::Create(request) => request.subject.clone(),
            Self::Execute(request) => request.subject.clone(),
            Self::Observe(observation) => Subject {
                entity: observation.entity.clone(),
                id: observation.id.clone(),
            },
        }
    }

    fn record_id(&self) -> &str {
        match self {
            Self::Create(request) => &request.recording.record_id,
            Self::Execute(request) => &request.recording.record_id,
            Self::Observe(observation) => &observation.envelope.record_id,
        }
    }

    fn validate_shape(&self) -> Result<(), ExecutionError> {
        self.subject().validate().map_err(ExecutionError::Store)?;
        match self {
            Self::Create(request) => {
                validate_recording(&request.recording)?;
                if request.definition_version == 0 {
                    return Err(ExecutionError::Store(AsyncStoreError::InvalidInput(
                        "create requires a positive definition version".to_owned(),
                    )));
                }
            }
            Self::Execute(request) => {
                validate_recording(&request.recording)?;
                validate_revision(request.expected_revision)?;
                if request.operation.trim().is_empty() {
                    return Err(ExecutionError::Store(AsyncStoreError::InvalidInput(
                        "execute requires a nonblank operation".to_owned(),
                    )));
                }
            }
            Self::Observe(observation) => observation.validate().map_err(|error| {
                ExecutionError::Store(AsyncStoreError::InvalidInput(error.to_string()))
            })?,
        }
        Ok(())
    }
}

/// A typed decision, integrity, storage, or outcome refusal.
#[derive(Debug)]
pub enum ExecutionError {
    /// The deterministic kernel refused the command.
    Core(CoreError),
    /// A read, comparison, or integrity check refused.
    Store(AsyncStoreError),
    /// The mandatory recorded append failed or remained uncertain.
    Write(WriteFailure),
}

impl ExecutionError {
    /// Returns the store refusal whether it arose before or during a proved-not-committed append.
    #[must_use]
    pub const fn store_error(&self) -> Option<&AsyncStoreError> {
        match self {
            Self::Store(error) | Self::Write(WriteFailure::NotCommitted(error)) => Some(error),
            Self::Core(_) | Self::Write(WriteFailure::Uncertain { .. }) => None,
        }
    }

    /// Whether the refusal is specifically an optimistic predecessor conflict.
    #[must_use]
    pub fn is_revision_conflict(&self) -> bool {
        matches!(
            self.store_error(),
            Some(AsyncStoreError::RevisionConflict { .. })
        )
    }
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core(error) => error.fmt(formatter),
            Self::Store(error) => error.fmt(formatter),
            Self::Write(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ExecutionError {}

impl From<CoreError> for ExecutionError {
    fn from(error: CoreError) -> Self {
        Self::Core(error)
    }
}

impl From<AsyncStoreError> for ExecutionError {
    fn from(error: AsyncStoreError) -> Self {
        Self::Store(error)
    }
}

/// The runtime-neutral executor over a registry and object-safe asynchronous recorded store.
#[derive(Clone, Copy)]
pub struct Executor<'a> {
    registry: &'a Registry,
    store: &'a dyn AsyncRecordedStore,
}

impl<'a> Executor<'a> {
    /// Constructs an executor without selecting a runtime or any implicit authority.
    #[must_use]
    pub const fn new(registry: &'a Registry, store: &'a dyn AsyncRecordedStore) -> Self {
        Self { registry, store }
    }

    /// Creates and records one subject under its record-id single namespace.
    ///
    /// # Errors
    ///
    /// Invalid input, kernel refusal, conflicting or corrupt evidence, or append failure.
    pub async fn create(&self, request: CreateRequest) -> Result<AppendOutcome, ExecutionError> {
        let key = BatchKey::SingleRecord(request.recording.record_id.clone());
        self.batch(key, vec![BatchAction::Create(request)]).await
    }

    /// Executes and records one operation under its record-id single namespace.
    ///
    /// # Errors
    ///
    /// Invalid input, kernel refusal, conflicting or corrupt evidence, or append failure.
    pub async fn execute(&self, request: ExecuteRequest) -> Result<AppendOutcome, ExecutionError> {
        let key = BatchKey::SingleRecord(request.recording.record_id.clone());
        self.batch(key, vec![BatchAction::Execute(request)]).await
    }

    /// Records one non-state-changing observation under its record-id single namespace.
    ///
    /// # Errors
    ///
    /// Invalid input, conflicting or corrupt evidence, revision conflict, or append failure.
    pub async fn observe(
        &self,
        observation: RecordedObservation,
    ) -> Result<AppendOutcome, ExecutionError> {
        let key = BatchKey::SingleRecord(observation.envelope.record_id.clone());
        self.batch(key, vec![BatchAction::Observe(observation)])
            .await
    }

    /// Executes one ordered transaction-local batch and appends it atomically.
    ///
    /// Identity and batch retries are recovered and verified before current state or registry
    /// lookup. An empty batch is inert and does not validate or consume `key`.
    ///
    /// # Errors
    ///
    /// Invalid input, kernel refusal, conflicting or corrupt evidence, or append failure.
    pub async fn batch(
        &self,
        key: BatchKey,
        actions: Vec<BatchAction>,
    ) -> Result<AppendOutcome, ExecutionError> {
        if actions.is_empty() {
            return Ok(AppendOutcome::Empty);
        }
        key.validate()?;
        let mut ids = std::collections::BTreeSet::new();
        for action in &actions {
            action.validate_shape()?;
            if !ids.insert(action.record_id()) {
                return Err(ExecutionError::Store(AsyncStoreError::DuplicateRecordId {
                    record_id: action.record_id().to_owned(),
                }));
            }
        }
        if let BatchKey::SingleRecord(record_id) = &key {
            if actions.len() != 1 || actions[0].record_id() != record_id {
                return Err(ExecutionError::Store(AsyncStoreError::InvalidInput(
                    "SingleRecord requires exactly one action with the same record id".to_owned(),
                )));
            }
        }

        if let Some(recovered) = self.recover_existing(&key, &actions).await? {
            return Ok(recovered);
        }

        let mut overlay: BTreeMap<Subject, Option<EntityInstance>> = BTreeMap::new();
        let mut members = Vec::with_capacity(actions.len());
        for action in &actions {
            let subject = action.subject();
            if !overlay.contains_key(&subject) {
                let state = self.store.load(&subject).await?;
                overlay.insert(subject.clone(), state);
            }
            let current = overlay
                .get(&subject)
                .expect("subject was loaded before decision")
                .as_ref();
            let (expect, entry) = self.decide(action, current)?;
            let request_bytes = request_comparison_bytes(action, &entry)?;
            let next = match &entry {
                RecordedEntry::Decision(commit) => Some(commit.instance.clone()),
                RecordedEntry::Observation(_) => current.cloned(),
            };
            overlay.insert(subject, next);
            members.push(AppendMember::new(expect, entry, request_bytes));
        }
        let request = AppendRequest::new(key.clone(), members)?;
        match self.store.append(request).await {
            Ok(AppendOutcome::Committed { replayed: true, .. }) => {
                self.recover_existing(&key, &actions).await?.ok_or_else(|| {
                    ExecutionError::Store(AsyncStoreError::Backend(
                        "append reported replay but recovery found no committed identity"
                            .to_owned(),
                    ))
                })
            }
            Ok(outcome) => Ok(outcome),
            Err(uncertain @ WriteFailure::Uncertain { .. }) => self
                .recover_existing(&key, &actions)
                .await?
                .ok_or(ExecutionError::Write(uncertain)),
            Err(error) => Err(ExecutionError::Write(error)),
        }
    }

    fn decide(
        &self,
        action: &BatchAction,
        current: Option<&EntityInstance>,
    ) -> Result<(Expect, RecordedEntry), ExecutionError> {
        match action {
            BatchAction::Create(request) => {
                if let Some(current) = current {
                    return Err(ExecutionError::Store(AsyncStoreError::RevisionConflict {
                        subject: request.subject.clone(),
                        expected: Expect::Absent,
                        found: Some(current.revision),
                    }));
                }
                let decision = Runtime::new(self.registry).create(
                    &request.subject.entity,
                    request.definition_version,
                    request.subject.id.clone(),
                    request.fields.clone(),
                )?;
                let commit =
                    RecordedCommit::new(decision, &request.recording).map_err(|error| {
                        ExecutionError::Store(AsyncStoreError::InvalidInput(error.to_string()))
                    })?;
                Ok((Expect::Absent, RecordedEntry::Decision(commit)))
            }
            BatchAction::Execute(request) => {
                let current = current.ok_or_else(|| {
                    ExecutionError::Store(AsyncStoreError::RevisionConflict {
                        subject: request.subject.clone(),
                        expected: Expect::Revision(request.expected_revision),
                        found: None,
                    })
                })?;
                if current.revision != request.expected_revision {
                    return Err(ExecutionError::Store(AsyncStoreError::RevisionConflict {
                        subject: request.subject.clone(),
                        expected: Expect::Revision(request.expected_revision),
                        found: Some(current.revision),
                    }));
                }
                let decision = Runtime::new(self.registry).execute(
                    current,
                    &request.operation,
                    request.arguments.clone(),
                )?;
                let commit =
                    RecordedCommit::new(decision, &request.recording).map_err(|error| {
                        ExecutionError::Store(AsyncStoreError::InvalidInput(error.to_string()))
                    })?;
                Ok((
                    Expect::Revision(request.expected_revision),
                    RecordedEntry::Decision(commit),
                ))
            }
            BatchAction::Observe(observation) => {
                let subject = action.subject();
                let current = current.ok_or_else(|| {
                    ExecutionError::Store(AsyncStoreError::RevisionConflict {
                        subject: subject.clone(),
                        expected: Expect::Revision(observation.revision),
                        found: None,
                    })
                })?;
                if current.revision != observation.revision {
                    return Err(ExecutionError::Store(AsyncStoreError::RevisionConflict {
                        subject,
                        expected: Expect::Revision(observation.revision),
                        found: Some(current.revision),
                    }));
                }
                Ok((
                    Expect::Revision(observation.revision),
                    RecordedEntry::Observation(observation.clone()),
                ))
            }
        }
    }

    async fn recover_existing(
        &self,
        key: &BatchKey,
        actions: &[BatchAction],
    ) -> Result<Option<AppendOutcome>, ExecutionError> {
        if let Some(batch) = self.store.lookup_batch(key).await? {
            validate_stored_batch(key, actions.len(), &batch)?;
            for (action, stored) in actions.iter().zip(&batch.records) {
                match self.store.lookup_record(stored.entry.record_id()).await? {
                    Some(RecordLookup::Committed(global)) if global == *stored => {}
                    Some(_) | None => {
                        return Err(ExecutionError::Store(AsyncStoreError::CorruptHistory {
                            subject: stored.entry.subject(),
                            detail: "batch member disagrees with the global record index"
                                .to_owned(),
                        }))
                    }
                }
                self.match_committed(action, stored)?;
                self.verify_committed_prefix(stored).await?;
            }
            return Ok(Some(AppendOutcome::Committed {
                receipt: batch.receipt,
                replayed: true,
            }));
        }

        let mut prior = Vec::new();
        for (index, action) in actions.iter().enumerate() {
            if let Some(lookup) = self.store.lookup_record(action.record_id()).await? {
                let assurance = match &lookup {
                    RecordLookup::Committed(stored) => {
                        self.match_committed(action, stored)?;
                        self.verify_committed_prefix(stored).await?;
                        None
                    }
                    RecordLookup::Imported(evidence) => {
                        self.match_imported(action, evidence)?;
                        Some(self.verify_imported(evidence).await?)
                    }
                };
                let index = u64::try_from(index).map_err(|_| {
                    ExecutionError::Store(AsyncStoreError::PositionExhausted {
                        domain: "member index".to_owned(),
                    })
                })?;
                prior.push((index, lookup, assurance));
            }
        }
        if prior.is_empty() {
            return Ok(None);
        }
        match key {
            BatchKey::Named(_) => Err(ExecutionError::Store(
                AsyncStoreError::PreviouslyRecordedBatchEntries {
                    indices: prior.into_iter().map(|(index, _, _)| index).collect(),
                },
            )),
            BatchKey::SingleRecord(_) => match prior.into_iter().next() {
                Some((_, RecordLookup::Committed(stored), _)) => {
                    Ok(Some(AppendOutcome::Committed {
                        receipt: CommitReceipt::Single(stored.receipt),
                        replayed: true,
                    }))
                }
                Some((_, RecordLookup::Imported(evidence), Some(assurance))) => {
                    Ok(Some(AppendOutcome::Historical {
                        assurance,
                        evidence,
                    }))
                }
                None | Some((_, RecordLookup::Imported(_), None)) => Ok(None),
            },
        }
    }

    fn match_committed(
        &self,
        action: &BatchAction,
        stored: &StoredRecord,
    ) -> Result<(), ExecutionError> {
        let subject = action.subject();
        let requested = request_comparison_bytes(action, &stored.entry)
            .map_err(|error| map_retry_error(&subject, action.record_id(), error, false))?;
        if requested != stored.request_bytes {
            return Err(ExecutionError::Store(AsyncStoreError::RecordConflict {
                record_id: action.record_id().to_owned(),
            }));
        }
        Ok(())
    }

    fn match_imported(
        &self,
        action: &BatchAction,
        evidence: &entity_store::asynchronous::ImportedRecordEvidence,
    ) -> Result<(), ExecutionError> {
        let subject = action.subject();
        let requested = request_comparison_bytes(action, &evidence.entry)
            .map_err(|error| map_retry_error(&subject, action.record_id(), error, true))?;
        let original = original_request_comparison_bytes(&evidence.entry)
            .map_err(|error| map_retry_error(&subject, action.record_id(), error, true))?;
        if requested != original {
            return Err(ExecutionError::Store(AsyncStoreError::RecordConflict {
                record_id: action.record_id().to_owned(),
            }));
        }
        Ok(())
    }

    async fn verify_committed_prefix(&self, stored: &StoredRecord) -> Result<(), ExecutionError> {
        let subject = stored.entry.subject();
        let history = self.store.history(&subject).await?;
        let occurrence = history
            .records
            .iter()
            .find(|record| record.entry.record_id() == stored.entry.record_id())
            .ok_or_else(|| {
                ExecutionError::Store(AsyncStoreError::CorruptHistory {
                    subject: subject.clone(),
                    detail: "global record is absent from its claimed history".to_owned(),
                })
            })?;
        if occurrence != stored {
            return Err(ExecutionError::Store(AsyncStoreError::CorruptHistory {
                subject,
                detail: "global record disagrees with its immutable history occurrence".to_owned(),
            }));
        }
        verify_subject_prefix(&history, stored.entry.record_id())?;
        Ok(())
    }

    async fn verify_imported(
        &self,
        evidence: &entity_store::asynchronous::ImportedRecordEvidence,
    ) -> Result<SubjectAssurance, ExecutionError> {
        let subject = evidence.entry.subject();
        let history = self.store.history(&subject).await?;
        verify_imported_record(&history, evidence).map_err(ExecutionError::Store)
    }
}

fn validate_stored_batch(
    key: &BatchKey,
    action_count: usize,
    batch: &StoredBatch,
) -> Result<(), ExecutionError> {
    let conflict = || ExecutionError::Store(AsyncStoreError::BatchConflict { key: key.clone() });
    let corrupt = |detail: &str| match batch.records.first() {
        Some(record) => ExecutionError::Store(AsyncStoreError::CorruptHistory {
            subject: record.entry.subject(),
            detail: detail.to_owned(),
        }),
        None => ExecutionError::Store(AsyncStoreError::BatchConflict { key: key.clone() }),
    };
    if &batch.key != key || batch.records.len() != action_count {
        return Err(conflict());
    }
    let members: Vec<AppendMember> = batch
        .records
        .iter()
        .map(|record| {
            AppendMember::new(
                record.expect,
                record.entry.clone(),
                record.request_bytes.clone(),
            )
        })
        .collect();
    if batch_comparison_bytes(key, &members)? != batch.comparison_bytes {
        return Err(corrupt(
            "stored batch comparison bytes differ from its complete records",
        ));
    }
    for (index, record) in batch.records.iter().enumerate() {
        let index = u64::try_from(index).map_err(|_| {
            ExecutionError::Store(AsyncStoreError::PositionExhausted {
                domain: "member index".to_owned(),
            })
        })?;
        if record.receipt.batch_key != *key || record.receipt.member_index != index {
            return Err(corrupt(
                "stored batch member receipt disagrees with its key or index",
            ));
        }
    }
    match &batch.receipt {
        CommitReceipt::Single(receipt)
            if matches!(key, BatchKey::SingleRecord(_))
                && batch.records.len() == 1
                && receipt == &batch.records[0].receipt => {}
        CommitReceipt::Batch(receipt)
            if matches!(key, BatchKey::Named(_))
                && receipt.key == *key
                && receipt.members
                    == batch
                        .records
                        .iter()
                        .map(|record| record.receipt.clone())
                        .collect::<Vec<_>>() => {}
        _ => return Err(corrupt("stored batch receipt disagrees with its members")),
    }
    Ok(())
}

fn validate_recording(recording: &Recording) -> Result<(), ExecutionError> {
    recording
        .seal(())
        .map(|_| ())
        .map_err(|error| ExecutionError::Store(AsyncStoreError::InvalidInput(error.to_string())))
}

fn validate_revision(revision: u64) -> Result<(), ExecutionError> {
    if revision == 0 || revision > i64::MAX as u64 {
        return Err(ExecutionError::Store(AsyncStoreError::InvalidInput(
            format!("revision {revision} is outside 1..={}", i64::MAX),
        )));
    }
    Ok(())
}

fn recording_value(recording: &Recording) -> Value {
    serde_json::json!({
        "record_id": recording.record_id,
        "recorded_at": recording.recorded_at,
        "actor": recording.actor,
        "correlation": recording.correlation,
        "causation": recording.causation,
    })
}

fn request_comparison_bytes(
    action: &BatchAction,
    original: &RecordedEntry,
) -> Result<Vec<u8>, AsyncStoreError> {
    match (action, original) {
        (BatchAction::Create(request), RecordedEntry::Decision(commit)) => {
            let definition = saved_definition(original)?;
            let decision = decide_create(
                &definition,
                request.subject.id.clone(),
                request.fields.clone(),
            )
            .map_err(|error| AsyncStoreError::InvalidInput(error.to_string()))?;
            let DecisionCommand::Create { fields, arguments } = decision.record.command else {
                return Err(AsyncStoreError::Backend(
                    "create normalization did not produce a create command".to_owned(),
                ));
            };
            if commit.envelope.record.entity != request.subject.entity
                || commit.envelope.record.id != request.subject.id
            {
                return Err(AsyncStoreError::RecordConflict {
                    record_id: request.recording.record_id.clone(),
                });
            }
            // A `service/1` creation reconstructs the caller's **arguments**; a `kernel/1` one
            // reconstructs its fields, in the framing and the shape it has always used.
            let domain = request_domain(original);
            if domain == "er.request/2" {
                canonical_domain_bytes(
                    domain,
                    serde_json::json!({
                        "kind": "create",
                        "subject": [request.subject.entity, request.subject.id],
                        "definition_version": request.definition_version,
                        "arguments": arguments,
                        "recording": recording_value(&request.recording),
                    }),
                )
            } else {
                canonical_domain_bytes(
                    domain,
                    serde_json::json!({
                        "kind": "create",
                        "subject": [request.subject.entity, request.subject.id],
                        "definition_version": request.definition_version,
                        "fields": fields,
                        "recording": recording_value(&request.recording),
                    }),
                )
            }
        }
        (BatchAction::Execute(request), RecordedEntry::Decision(commit)) => {
            let definition = saved_definition(original)?;
            let arguments =
                normalize_arguments(&definition, &request.operation, request.arguments.clone())
                    .map_err(|error| AsyncStoreError::InvalidInput(error.to_string()))?;
            if commit.envelope.record.entity != request.subject.entity
                || commit.envelope.record.id != request.subject.id
            {
                return Err(AsyncStoreError::RecordConflict {
                    record_id: request.recording.record_id.clone(),
                });
            }
            canonical_domain_bytes(
                request_domain(original),
                serde_json::json!({
                    "kind": "execute",
                    "subject": [request.subject.entity, request.subject.id],
                    "expected_revision": request.expected_revision,
                    "operation": request.operation,
                    "arguments": arguments,
                    "recording": recording_value(&request.recording),
                }),
            )
        }
        (BatchAction::Observe(observation), RecordedEntry::Observation(original)) => {
            if observation.entity != original.entity || observation.id != original.id {
                return Err(AsyncStoreError::RecordConflict {
                    record_id: observation.envelope.record_id.clone(),
                });
            }
            canonical_domain_bytes(
                request_domain(&RecordedEntry::Observation(original.clone())),
                serde_json::json!({"kind": "observation", "observation": observation}),
            )
        }
        _ => Err(AsyncStoreError::RecordConflict {
            record_id: action.record_id().to_owned(),
        }),
    }
}

fn saved_definition(original: &RecordedEntry) -> Result<ValidatedDefinition, AsyncStoreError> {
    let RecordedEntry::Decision(commit) = original else {
        return Err(AsyncStoreError::InvalidInput(
            "a decision request cannot match an observation".to_owned(),
        ));
    };
    commit
        .envelope
        .record
        .definition
        .clone()
        .ok_or_else(|| AsyncStoreError::HistoricalRetryUnverifiable {
            record_id: commit.envelope.record_id.clone(),
            detail: "saved definition is unavailable".to_owned(),
        })
        .and_then(|definition| {
            ValidatedDefinition::new(definition).map_err(|error| AsyncStoreError::CorruptHistory {
                subject: original.subject(),
                detail: format!("saved definition is invalid: {error}"),
            })
        })
}

fn map_retry_error(
    subject: &Subject,
    record_id: &str,
    error: AsyncStoreError,
    imported: bool,
) -> ExecutionError {
    if imported {
        match error {
            AsyncStoreError::HistoricalRetryUnverifiable { .. } => ExecutionError::Store(error),
            AsyncStoreError::InvalidInput(_) => {
                ExecutionError::Store(AsyncStoreError::RecordConflict {
                    record_id: record_id.to_owned(),
                })
            }
            other => ExecutionError::Store(other),
        }
    } else {
        match error {
            AsyncStoreError::HistoricalRetryUnverifiable { detail, .. } => {
                ExecutionError::Store(AsyncStoreError::CorruptHistory {
                    subject: subject.clone(),
                    detail,
                })
            }
            AsyncStoreError::InvalidInput(_) => {
                ExecutionError::Store(AsyncStoreError::RecordConflict {
                    record_id: record_id.to_owned(),
                })
            }
            other => ExecutionError::Store(other),
        }
    }
}

/// Safe minimal polling helpers for runtime-independent asynchronous conformance tests.
pub mod test_support {
    use std::{
        future::Future,
        pin::Pin,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        task::{Context, Poll, Wake, Waker},
    };

    struct TestWake(AtomicBool);

    impl Wake for TestWake {
        fn wake(self: Arc<Self>) {
            self.0.store(true, Ordering::Release);
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.0.store(true, Ordering::Release);
        }
    }

    /// Polls a future to completion when every pending step wakes synchronously.
    ///
    /// This intentionally refuses an unwoken pending future instead of selecting a runtime,
    /// sleeping, or assuming that pending proves rollback.
    pub fn block_on<F: Future>(future: F) -> F::Output {
        let wake = Arc::new(TestWake(AtomicBool::new(true)));
        let waker = Waker::from(wake.clone());
        let mut context = Context::from_waker(&waker);
        let mut future = Box::pin(future);
        loop {
            wake.0.store(false, Ordering::Release);
            match Future::poll(future.as_mut(), &mut context) {
                Poll::Ready(output) => return output,
                Poll::Pending if wake.0.swap(false, Ordering::AcqRel) => continue,
                Poll::Pending => panic!("future remained pending without a wake"),
            }
        }
    }

    /// Polls one already pinned future exactly once with a safe standard-library waker.
    pub fn poll_once<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
        let wake = Arc::new(TestWake(AtomicBool::new(false)));
        let waker = Waker::from(wake);
        let mut context = Context::from_waker(&waker);
        Future::poll(future, &mut context)
    }
}
