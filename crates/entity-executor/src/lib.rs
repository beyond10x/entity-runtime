//! Pure asynchronous execution over complete recorded storage ports.
//!
//! The kernel remains synchronous and deterministic. This crate orders asynchronous identity,
//! history, state, and append calls around it without selecting a runtime, clock, identifier, or
//! external authority.

use std::{collections::BTreeMap, fmt};

use entity_core::{
    create as decide_create, normalize_arguments, CoreError, DecisionCommand, EntityInstance,
    Evaluation, LoadedDecision, OperationFieldAction, PreloadDecision, Registry, Runtime,
    ValidatedDefinition,
};
use entity_store::{
    asynchronous::{
        batch_comparison_bytes, canonical_domain_bytes, original_request_comparison_bytes,
        request_domain, verify_imported_record, verify_subject_history,
        verify_subject_history_extension, verify_subject_prefix, AppendMember, AppendOutcome,
        AppendRequest, AsyncRecordedStore, AsyncStoreError, BatchKey, CommitReceipt, HistoryOrigin,
        MergeBase, RecordLookup, RecordedEntry, RecordedRefusal, RefusalReason, StoredBatch,
        StoredRecord, Subject, SubjectAssurance, SubjectHistory, WriteFailure,
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
    /// Exact post-load actions for a selected `service/3` branch.
    pub fulfillments: BTreeMap<String, OperationFieldAction>,
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
    /// Join a subject two merged branches both changed: execute one operation on the state the
    /// `first` head reached, at the highest revision any head reached.
    Merge(MergeRequest),
}

/// A batch action whose operation definition is selected before reading subject state.
///
/// Explicit versions never select the latest registry definition. Existing [`BatchAction`]
/// callers retain their row-derived definition authority.
#[derive(Debug, Clone, PartialEq)]
pub enum VersionedBatchAction {
    /// Create using the version already carried by the request.
    Create(CreateRequest),
    /// Execute under the caller's exact definition version.
    Execute {
        /// Positive registered definition version.
        definition_version: u32,
        /// Operation and predecessor supplied by the caller.
        request: ExecuteRequest,
    },
    /// Append non-state-changing evidence at an exact revision.
    Observe(RecordedObservation),
    /// Select input refusals before reading the merge's history.
    Merge {
        /// Positive registered definition version.
        definition_version: u32,
        /// Operation and merge head supplied by the caller.
        request: MergeRequest,
    },
}

struct BoundAction {
    action: BatchAction,
    definition_version: Option<u32>,
}

impl std::ops::Deref for BoundAction {
    type Target = BatchAction;

    fn deref(&self) -> &Self::Target {
        &self.action
    }
}

impl From<BatchAction> for BoundAction {
    fn from(action: BatchAction) -> Self {
        Self {
            action,
            definition_version: None,
        }
    }
}

impl From<VersionedBatchAction> for BoundAction {
    fn from(action: VersionedBatchAction) -> Self {
        match action {
            VersionedBatchAction::Create(request) => BatchAction::Create(request).into(),
            VersionedBatchAction::Observe(observation) => BatchAction::Observe(observation).into(),
            VersionedBatchAction::Execute {
                definition_version,
                request,
            } => Self {
                action: BatchAction::Execute(request),
                definition_version: Some(definition_version),
            },
            VersionedBatchAction::Merge {
                definition_version,
                request,
            } => Self {
                action: BatchAction::Merge(request),
                definition_version: Some(definition_version),
            },
        }
    }
}

#[derive(Default)]
struct PreparedAction<'a> {
    operation: Option<entity_core::PreparedOperation<'a>>,
    creation: Option<Result<Evaluation, CoreError>>,
}

/// One merge decision over a forked subject.
#[derive(Debug, Clone, PartialEq)]
pub struct MergeRequest {
    /// The operation, whose `expected_revision` is the highest revision any head reached.
    pub execute: ExecuteRequest,
    /// The digest of the head the operation is decided on.
    pub first: String,
}

impl BatchAction {
    fn subject(&self) -> Subject {
        match self {
            Self::Create(request) => request.subject.clone(),
            Self::Execute(request) => request.subject.clone(),
            Self::Merge(request) => request.execute.subject.clone(),
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
            Self::Merge(request) => &request.execute.recording.record_id,
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
            Self::Merge(request) => {
                Self::Execute(request.execute.clone()).validate_shape()?;
                if request.first.trim().is_empty() {
                    return Err(ExecutionError::Store(AsyncStoreError::InvalidInput(
                        "merge requires the head it is decided on".to_owned(),
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
    /// Where refusals are recorded, when the caller asked for them to be.
    refusals: Option<&'a dyn entity_store::asynchronous::AsyncRefusalRecorder>,
}

impl<'a> Executor<'a> {
    /// Constructs an executor without selecting a runtime or any implicit authority.
    #[must_use]
    pub const fn new(registry: &'a Registry, store: &'a dyn AsyncRecordedStore) -> Self {
        Self::with_refusals(registry, store, None)
    }

    /// An executor that records every command it refuses, through `refusals`.
    ///
    /// A kernel refusal, an expectation conflict and a write to a forked subject are recorded
    /// before the refusal is returned; malformed input is not, because it is no decision.
    #[must_use]
    pub const fn recording_refusals(
        registry: &'a Registry,
        store: &'a dyn AsyncRecordedStore,
        refusals: &'a dyn entity_store::asynchronous::AsyncRefusalRecorder,
    ) -> Self {
        Self::with_refusals(registry, store, Some(refusals))
    }

    const fn with_refusals(
        registry: &'a Registry,
        store: &'a dyn AsyncRecordedStore,
        refusals: Option<&'a dyn entity_store::asynchronous::AsyncRefusalRecorder>,
    ) -> Self {
        Self {
            registry,
            store,
            refusals,
        }
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
    /// The loaded row selects the definition version. Use [`Self::execute_versioned`] when
    /// input refusals must precede subject existence and revision checks.
    ///
    /// # Errors
    ///
    /// Invalid input, kernel refusal, conflicting or corrupt evidence, or append failure.
    pub async fn execute(&self, request: ExecuteRequest) -> Result<AppendOutcome, ExecutionError> {
        let key = BatchKey::SingleRecord(request.recording.record_id.clone());
        self.batch(key, vec![BatchAction::Execute(request)]).await
    }

    /// Executes with an explicit definition, deciding input refusals before loading state.
    ///
    /// Exact retries use their saved definition before consulting the current registry.
    ///
    /// # Errors
    ///
    /// Invalid version or input, kernel refusal, definition/predecessor mismatch, conflicting
    /// or corrupt recorded identity, or append failure.
    pub async fn execute_versioned(
        &self,
        request: ExecuteRequest,
        definition_version: u32,
    ) -> Result<AppendOutcome, ExecutionError> {
        let key = BatchKey::SingleRecord(request.recording.record_id.clone());
        self.batch_versioned(
            key,
            vec![VersionedBatchAction::Execute {
                definition_version,
                request,
            }],
        )
        .await
    }

    /// Executes an atomic batch with caller-selected operation definition versions.
    ///
    /// Each fresh operation's input refusal is evaluated before its state or merge-history
    /// read. Empty batches are inert; saved identities are recovered before current definitions.
    ///
    /// # Errors
    ///
    /// Invalid version or input, kernel refusal, definition/predecessor mismatch, conflicting
    /// or corrupt recorded identity, or append failure.
    pub async fn batch_versioned(
        &self,
        key: BatchKey,
        actions: Vec<VersionedBatchAction>,
    ) -> Result<AppendOutcome, ExecutionError> {
        self.batch_bound(key, actions.into_iter().map(BoundAction::from).collect())
            .await
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
    /// Operations derive their definition versions from loaded rows; use
    /// [`Self::batch_versioned`] for input refusals independent of subject existence.
    ///
    /// # Errors
    ///
    /// Invalid input, kernel refusal, conflicting or corrupt evidence, or append failure.
    pub async fn batch(
        &self,
        key: BatchKey,
        actions: Vec<BatchAction>,
    ) -> Result<AppendOutcome, ExecutionError> {
        self.batch_bound(key, actions.into_iter().map(BoundAction::from).collect())
            .await
    }

    async fn batch_bound(
        &self,
        key: BatchKey,
        actions: Vec<BoundAction>,
    ) -> Result<AppendOutcome, ExecutionError> {
        if actions.is_empty() {
            return Ok(AppendOutcome::Empty);
        }
        key.validate()?;
        let mut ids = std::collections::BTreeSet::new();
        for action in &actions {
            action.validate_shape()?;
            if action.definition_version == Some(0) {
                return Err(AsyncStoreError::InvalidInput(
                    "execute requires a positive definition version".to_owned(),
                )
                .into());
            }
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

        let decided = self.decide_and_append(&key, &actions).await;
        if let (Err(error), Some(recorder)) = (&decided, self.refusals) {
            if let Some(reason) = refusal_reason(error) {
                let refusal = RecordedRefusal {
                    key: key.clone(),
                    subjects: actions.iter().map(|action| action.subject()).collect(),
                    request: serde_json::Value::Array(
                        actions.iter().map(bound_action_value).collect(),
                    ),
                    recording: first_recording(&actions[0]),
                    reason,
                };
                recorder
                    .record_refusal(&refusal)
                    .await
                    .map_err(ExecutionError::Store)?;
            }
        }
        decided
    }

    fn prepare(&self, action: &BoundAction) -> Result<PreparedAction<'_>, ExecutionError> {
        match &action.action {
            BatchAction::Create(request) => {
                count_creation_kernel_call();
                let creation = Runtime::new(self.registry).decide_create(
                    &request.subject.entity,
                    request.definition_version,
                    request.subject.id.clone(),
                    request.fields.clone(),
                );
                if let Ok(Evaluation::Refused(refusal)) = creation {
                    return Err(CoreError::Refused {
                        outcome: refusal.outcome,
                        error: refusal.error,
                        message: refusal.message,
                    }
                    .into());
                }
                // Only declared refusals precede existence. Preserve other creation errors'
                // existing ordering, and reuse this decision if the subject is absent.
                Ok(PreparedAction {
                    creation: Some(creation),
                    operation: None,
                })
            }
            BatchAction::Execute(request)
            | BatchAction::Merge(MergeRequest {
                execute: request, ..
            }) => {
                let Some(version) = action.definition_version else {
                    return Ok(PreparedAction::default());
                };
                let definition = self
                    .registry
                    .get(&request.subject.entity, version)
                    .ok_or_else(|| CoreError::EntityNotRegistered {
                        entity: request.subject.entity.clone(),
                        version,
                    })?;
                match entity_core::decide_before_load(
                    definition,
                    request.subject.id.clone(),
                    &request.operation,
                    request.arguments.clone(),
                )? {
                    PreloadDecision::Load(operation) => Ok(PreparedAction {
                        operation: Some(operation),
                        creation: None,
                    }),
                    PreloadDecision::Refused(refusal) => Err(CoreError::Refused {
                        outcome: refusal.outcome,
                        error: refusal.error,
                        message: refusal.message,
                    }
                    .into()),
                }
            }
            BatchAction::Observe(_) => Ok(PreparedAction::default()),
        }
    }

    /// Decides every action on the state the store holds and appends the result as one batch.
    async fn decide_and_append(
        &self,
        key: &BatchKey,
        actions: &[BoundAction],
    ) -> Result<AppendOutcome, ExecutionError> {
        let key = key.clone();
        let mut overlay: BTreeMap<Subject, Option<EntityInstance>> = BTreeMap::new();
        let mut members = Vec::with_capacity(actions.len());
        for action in actions {
            let prepared = self.prepare(action)?;
            let subject = action.subject();
            if let BatchAction::Merge(request) = &action.action {
                let merge = self.merge_base(request).await?;
                let (expect, entry) = self.decide(
                    &BatchAction::Execute(request.execute.clone()),
                    Some(&merge.base),
                    prepared,
                )?;
                let request_bytes = request_comparison_bytes(
                    &BatchAction::Execute(request.execute.clone()),
                    &entry,
                )?;
                if let RecordedEntry::Decision(commit) = &entry {
                    overlay.insert(subject, Some(commit.instance.clone()));
                }
                members.push(AppendMember::new(expect, entry, request_bytes).merging(merge));
                continue;
            }
            if !overlay.contains_key(&subject) {
                let state = self.store.load(&subject).await?;
                overlay.insert(subject.clone(), state);
            }
            let current = overlay
                .get(&subject)
                .expect("subject was loaded before decision")
                .as_ref();
            let (expect, entry) = self.decide(action, current, prepared)?;
            let request_bytes = fresh_request_comparison_bytes(action, &entry)?;
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
                self.recover_existing(&key, actions).await?.ok_or_else(|| {
                    ExecutionError::Store(AsyncStoreError::Backend(
                        "append reported replay but recovery found no committed identity"
                            .to_owned(),
                    ))
                })
            }
            Ok(outcome) => Ok(outcome),
            Err(uncertain @ WriteFailure::Uncertain { .. }) => self
                .recover_existing(&key, actions)
                .await?
                .ok_or(ExecutionError::Write(uncertain)),
            Err(error) => Err(ExecutionError::Write(error)),
        }
    }

    /// The heads a merge joins and the state it is decided on, from the subject's history.
    async fn merge_base(&self, request: &MergeRequest) -> Result<MergeBase, ExecutionError> {
        let subject = &request.execute.subject;
        let history = self.store.history(subject).await?;
        let heads = entity_store::asynchronous::branch_heads(&history)?;
        if heads.len() < 2 {
            return Err(ExecutionError::Store(AsyncStoreError::InvalidInput(
                "a merge names a subject that has not forked".to_owned(),
            )));
        }
        let highest = heads
            .iter()
            .filter_map(|(_, state)| state.as_ref().map(|state| state.revision))
            .max();
        let Some((_, Some(first))) = heads.iter().find(|(digest, _)| *digest == request.first)
        else {
            return Err(ExecutionError::Store(AsyncStoreError::InvalidInput(
                "a merge is decided on a head the subject does not have".to_owned(),
            )));
        };
        if highest != Some(request.execute.expected_revision) {
            return Err(ExecutionError::Store(AsyncStoreError::RevisionConflict {
                subject: subject.clone(),
                expected: Expect::Revision(request.execute.expected_revision),
                found: highest,
            }));
        }
        let mut base = first.clone();
        base.revision = request.execute.expected_revision;
        // The merge appends over every record no record follows, which is what the provider holds
        // the stream to: a branch that observed after its last decision ends in the observation,
        // not in the decision head the merge is decided on.
        let mut digests = entity_store::asynchronous::branch_tips(&history);
        // The first head leads when it is a tip, so the store records it as the merge decision's
        // first parent. One an observation follows is not a tip, and is named by `base` alone.
        if let Some(at) = digests.iter().position(|digest| *digest == request.first) {
            let first = digests.remove(at);
            digests.insert(0, first);
        }
        Ok(MergeBase {
            heads: digests,
            base,
        })
    }

    fn decide(
        &self,
        action: &BatchAction,
        current: Option<&EntityInstance>,
        prepared: PreparedAction<'_>,
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
                let decision = prepared
                    .creation
                    .expect("create was prepared")?
                    .into_decision()?;
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
                let runtime = Runtime::new(self.registry);
                let prepared = match prepared.operation {
                    Some(prepared) => prepared,
                    None => match runtime.decide_before_load(
                        &request.subject.entity,
                        current.version,
                        request.subject.id.clone(),
                        &request.operation,
                        request.arguments.clone(),
                    )? {
                        PreloadDecision::Load(prepared) => prepared,
                        PreloadDecision::Refused(refusal) => {
                            return Err(CoreError::Refused {
                                outcome: refusal.outcome,
                                error: refusal.error,
                                message: refusal.message,
                            }
                            .into())
                        }
                    },
                };
                let evaluation = match prepared.select_with(current)? {
                    LoadedDecision::Complete(evaluation) => {
                        if let Evaluation::Accepted(decision) = &evaluation {
                            if !request.fulfillments.is_empty() {
                                return Err(CoreError::FulfillmentKeysMismatch {
                                    operation: request.operation.clone(),
                                    outcome: decision.record.outcome.clone().unwrap_or_default(),
                                    missing: Vec::new(),
                                    extra: request.fulfillments.keys().cloned().collect(),
                                }
                                .into());
                            }
                        }
                        evaluation
                    }
                    LoadedDecision::NeedsFulfillment(prepared) => {
                        prepared.complete(request.fulfillments.clone())?
                    }
                };
                let decision = evaluation.into_decision()?;
                let commit =
                    RecordedCommit::new(decision, &request.recording).map_err(|error| {
                        ExecutionError::Store(AsyncStoreError::InvalidInput(error.to_string()))
                    })?;
                Ok((
                    Expect::Revision(request.expected_revision),
                    RecordedEntry::Decision(commit),
                ))
            }
            BatchAction::Merge(request) => self.decide(
                &BatchAction::Execute(request.execute.clone()),
                current,
                prepared,
            ),
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
        actions: &[BoundAction],
    ) -> Result<Option<AppendOutcome>, ExecutionError> {
        // One history per subject for the whole recovery: members on one subject are checked
        // against the same read, and their prefixes are verified incrementally.
        let mut histories = RecoveredHistories::default();
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
                self.verify_committed_prefix(&mut histories, stored).await?;
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
                        self.verify_committed_prefix(&mut histories, stored).await?;
                        None
                    }
                    RecordLookup::Imported(evidence) => {
                        self.match_imported(action, evidence)?;
                        Some(self.verify_imported(&mut histories, evidence).await?)
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
        action: &BoundAction,
        stored: &StoredRecord,
    ) -> Result<(), ExecutionError> {
        match_saved_version(action, &stored.entry)?;
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
        action: &BoundAction,
        evidence: &entity_store::asynchronous::ImportedRecordEvidence,
    ) -> Result<(), ExecutionError> {
        match_saved_version(action, &evidence.entry)?;
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

    async fn verify_committed_prefix(
        &self,
        histories: &mut RecoveredHistories,
        stored: &StoredRecord,
    ) -> Result<(), ExecutionError> {
        let subject = stored.entry.subject();
        let recovered = histories.get(self.store, &subject).await?;
        let index = recovered
            .history
            .records
            .iter()
            .position(|record| record.entry.record_id() == stored.entry.record_id())
            .ok_or_else(|| {
                ExecutionError::Store(AsyncStoreError::CorruptHistory {
                    subject: subject.clone(),
                    detail: "global record is absent from its claimed history".to_owned(),
                })
            })?;
        if recovered.history.records[index] != *stored {
            return Err(ExecutionError::Store(AsyncStoreError::CorruptHistory {
                subject,
                detail: "global record disagrees with its immutable history occurrence".to_owned(),
            }));
        }
        recovered.verify_prefix_through(index)?;
        Ok(())
    }

    async fn verify_imported(
        &self,
        histories: &mut RecoveredHistories,
        evidence: &entity_store::asynchronous::ImportedRecordEvidence,
    ) -> Result<SubjectAssurance, ExecutionError> {
        let subject = evidence.entry.subject();
        let recovered = histories.get(self.store, &subject).await?;
        verify_imported_record(&recovered.history, evidence).map_err(ExecutionError::Store)
    }
}

/// The subject histories one recovery read, each read at most once.
///
/// A retried batch whose members are the first K decisions of one subject used to read that
/// history K times and verify K prefixes from genesis, 1 + … + K records in all. Reading it once
/// and verifying each member's prefix as an extension of the longest prefix already verified
/// checks every record once, with exactly the checks a prefix verification makes.
#[derive(Default)]
struct RecoveredHistories {
    subjects: BTreeMap<Subject, RecoveredHistory>,
}

impl RecoveredHistories {
    async fn get(
        &mut self,
        store: &dyn AsyncRecordedStore,
        subject: &Subject,
    ) -> Result<&mut RecoveredHistory, ExecutionError> {
        if !self.subjects.contains_key(subject) {
            let history = store.history(subject).await?;
            self.subjects
                .insert(subject.clone(), RecoveredHistory::new(history));
        }
        Ok(self
            .subjects
            .get_mut(subject)
            .expect("the subject's history was read above"))
    }
}

/// One subject history and the longest prefix of it verified so far.
struct RecoveredHistory {
    history: SubjectHistory,
    /// `history.records[..prefix.records.len()]`, copied once as it grows: the extension verifier
    /// reads an owned history, and each record is copied into it at most once.
    prefix: SubjectHistory,
    /// The state the verified prefix reached, once a prefix is verified.
    state: Option<EntityInstance>,
}

impl RecoveredHistory {
    fn new(history: SubjectHistory) -> Self {
        let prefix = SubjectHistory {
            subject: history.subject.clone(),
            origin: history.origin.clone(),
            records: Vec::new(),
        };
        Self {
            history,
            prefix,
            state: None,
        }
    }

    /// What [`verify_subject_prefix`] verifies for the record at `index`, without repeating the
    /// checks of a shorter prefix this recovery already verified.
    fn verify_prefix_through(&mut self, index: usize) -> Result<(), AsyncStoreError> {
        let verified = self.prefix.records.len();
        if index < verified {
            // A member that precedes one already verified: its own terminal still has to be held,
            // so it is verified on its own, in place.
            verify_subject_prefix(&self.history, self.history.records[index].entry.record_id())?;
            return Ok(());
        }
        self.prefix
            .records
            .extend_from_slice(&self.history.records[verified..=index]);
        let Some(terminal) = prefix_terminal(&self.prefix) else {
            // No state to hold the prefix to: the whole verifier names that refusal.
            verify_subject_prefix(&self.history, self.history.records[index].entry.record_id())?;
            return Ok(());
        };
        match &self.state {
            // A branched prefix is verified whole by the extension verifier itself.
            Some(state) if verified > 0 => {
                verify_subject_history_extension(&self.prefix, verified, state, &terminal)?;
            }
            _ => {
                verify_subject_history(&self.prefix, &terminal)?;
            }
        }
        self.state = Some(terminal);
        Ok(())
    }
}

/// The state a verified prefix must reach: its last decision's, or the import anchor's.
fn prefix_terminal(prefix: &SubjectHistory) -> Option<EntityInstance> {
    prefix
        .records
        .iter()
        .rev()
        .find_map(|record| match &record.entry {
            RecordedEntry::Decision(commit) => Some(commit.instance.clone()),
            RecordedEntry::Observation(_) => None,
        })
        .or_else(|| match &prefix.origin {
            HistoryOrigin::Imported(anchor) => Some(anchor.instance.clone()),
            HistoryOrigin::Genesis => None,
        })
}

fn match_saved_version(action: &BoundAction, entry: &RecordedEntry) -> Result<(), ExecutionError> {
    if let Some(version) = action.definition_version {
        let matches = match entry {
            RecordedEntry::Decision(commit) => {
                commit.instance.version == version
                    && commit
                        .envelope
                        .record
                        .definition
                        .as_ref()
                        .is_none_or(|definition| definition.version == version)
            }
            RecordedEntry::Observation(_) => false,
        };
        if !matches {
            return Err(AsyncStoreError::RecordConflict {
                record_id: action.record_id().to_owned(),
            }
            .into());
        }
    }
    Ok(())
}

fn bound_action_value(action: &BoundAction) -> Value {
    let mut value = action_value(action);
    if let Some(version) = action.definition_version {
        value["definition_version"] = version.into();
    }
    value
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

/// The comparison bytes of a request whose record `decided` was just decided from it.
///
/// A fresh create's record carries the normalized command the creation kernel produced from this
/// very request, so its bytes are encoded from that record; deciding the creation a second time
/// only reproduced the same command at the cost of a second kernel run. A retry never comes here:
/// it is compared through [`request_comparison_bytes`], which decides the request independently.
fn fresh_request_comparison_bytes(
    action: &BatchAction,
    decided: &RecordedEntry,
) -> Result<Vec<u8>, AsyncStoreError> {
    if let (BatchAction::Create(request), RecordedEntry::Decision(commit)) = (action, decided) {
        if let DecisionCommand::Create { fields, arguments } = &commit.envelope.record.command {
            return create_request_bytes(request, decided, fields, arguments);
        }
    }
    request_comparison_bytes(action, decided)
}

/// Encodes a create request from its normalized creation fields and arguments.
fn create_request_bytes(
    request: &CreateRequest,
    original: &RecordedEntry,
    fields: &serde_json::Map<String, Value>,
    arguments: &serde_json::Map<String, Value>,
) -> Result<Vec<u8>, AsyncStoreError> {
    // A `service/1` creation reconstructs the caller's **arguments**; a `kernel/1` one
    // reconstructs its fields, in the framing and the shape it has always used.
    let domain = request_domain(original);
    if matches!(domain, "er.request/2" | "er.request/3" | "er.request/4") {
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

#[cfg(test)]
thread_local! {
    /// Creation-kernel runs on this thread, for the tests that hold a fresh create to one.
    static CREATION_KERNEL_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Counts one creation-kernel run in tests; nothing outside them.
fn count_creation_kernel_call() {
    #[cfg(test)]
    CREATION_KERNEL_CALLS.with(|calls| calls.set(calls.get() + 1));
}

fn request_comparison_bytes(
    action: &BatchAction,
    original: &RecordedEntry,
) -> Result<Vec<u8>, AsyncStoreError> {
    if let BatchAction::Merge(request) = action {
        return request_comparison_bytes(&BatchAction::Execute(request.execute.clone()), original);
    }
    match (action, original) {
        (BatchAction::Create(request), RecordedEntry::Decision(commit)) => {
            // A retry is compared independently: the request is decided again under the saved
            // definition, so a different request cannot borrow the stored record's command.
            let definition = saved_definition(original)?;
            count_creation_kernel_call();
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
            create_request_bytes(request, original, &fields, &arguments)
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
            let domain = request_domain(original);
            if domain == "er.request/4" {
                canonical_domain_bytes(
                    domain,
                    serde_json::json!({
                        "kind": "execute",
                        "subject": [request.subject.entity, request.subject.id],
                        "expected_revision": request.expected_revision,
                        "operation": request.operation,
                        "arguments": arguments,
                        "fulfillments": request.fulfillments,
                        "recording": recording_value(&request.recording),
                    }),
                )
            } else {
                if !request.fulfillments.is_empty() {
                    return Err(AsyncStoreError::RecordConflict {
                        record_id: request.recording.record_id.clone(),
                    });
                }
                canonical_domain_bytes(
                    domain,
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

/// The refusal a command's error records, or `None` for an error that is no decision.
fn refusal_reason(error: &ExecutionError) -> Option<RefusalReason> {
    let store = match error {
        ExecutionError::Core(core) => {
            return Some(RefusalReason::Kernel {
                message: core.to_string(),
            });
        }
        ExecutionError::Store(store) => store,
        ExecutionError::Write(WriteFailure::NotCommitted(store)) => store,
        _ => return None,
    };
    match store {
        AsyncStoreError::RevisionConflict {
            subject,
            expected,
            found,
        } => Some(RefusalReason::Conflict {
            subject: subject.clone(),
            expected: match expected {
                Expect::Absent => None,
                Expect::Revision(revision) => Some(*revision),
            },
            found: *found,
        }),
        AsyncStoreError::Forked { subject, heads } => Some(RefusalReason::Forked {
            subject: subject.clone(),
            heads: heads.clone(),
        }),
        _ => None,
    }
}

/// One action as the store received it, for the refusal record.
fn action_value(action: &BatchAction) -> serde_json::Value {
    match action {
        BatchAction::Create(request) => serde_json::json!({
            "kind": "create",
            "subject": [request.subject.entity, request.subject.id],
            "definition_version": request.definition_version,
            "fields": request.fields,
        }),
        BatchAction::Execute(request) => serde_json::json!({
            "kind": "execute",
            "subject": [request.subject.entity, request.subject.id],
            "expected_revision": request.expected_revision,
            "operation": request.operation,
            "arguments": request.arguments,
        }),
        BatchAction::Merge(request) => serde_json::json!({
            "kind": "merge",
            "first": request.first,
            "execute": action_value(&BatchAction::Execute(request.execute.clone())),
        }),
        BatchAction::Observe(observation) => serde_json::json!({
            "kind": "observe",
            "subject": [observation.entity, observation.id],
            "revision": observation.revision,
        }),
    }
}

fn first_recording(action: &BatchAction) -> entity_store::Recording {
    match action {
        BatchAction::Create(request) => request.recording.clone(),
        BatchAction::Execute(request) => request.recording.clone(),
        BatchAction::Merge(request) => request.execute.recording.clone(),
        BatchAction::Observe(observation) => entity_store::Recording {
            record_id: observation.envelope.record_id.clone(),
            recorded_at: observation.envelope.recorded_at.clone(),
            correlation: observation.envelope.correlation.clone(),
            causation: observation.envelope.causation.clone(),
            actor: observation.envelope.actor.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use entity_store::asynchronous::{
        AsyncRecordedReader, AsyncRecordedWriter, AsyncStateReader, BoxFuture,
        CompleteStoreSnapshot, MemoryRecordedStore,
    };
    use serde_json::json;

    use super::*;

    /// A memory store that counts the subject histories read from it.
    #[derive(Default)]
    struct HistoryCountingStore {
        inner: MemoryRecordedStore,
        history_reads: AtomicUsize,
    }

    impl AsyncStateReader for HistoryCountingStore {
        fn load<'a>(
            &'a self,
            subject: &'a Subject,
        ) -> BoxFuture<'a, Result<Option<EntityInstance>, AsyncStoreError>> {
            self.inner.load(subject)
        }
    }

    impl AsyncRecordedReader for HistoryCountingStore {
        fn lookup_record<'a>(
            &'a self,
            record_id: &'a str,
        ) -> BoxFuture<'a, Result<Option<RecordLookup>, AsyncStoreError>> {
            self.inner.lookup_record(record_id)
        }

        fn lookup_batch<'a>(
            &'a self,
            key: &'a BatchKey,
        ) -> BoxFuture<'a, Result<Option<StoredBatch>, AsyncStoreError>> {
            self.inner.lookup_batch(key)
        }

        fn history<'a>(
            &'a self,
            subject: &'a Subject,
        ) -> BoxFuture<'a, Result<SubjectHistory, AsyncStoreError>> {
            self.history_reads.fetch_add(1, Ordering::SeqCst);
            self.inner.history(subject)
        }

        fn complete_snapshot<'a>(
            &'a self,
            scope: &'a str,
        ) -> BoxFuture<'a, Result<CompleteStoreSnapshot, AsyncStoreError>> {
            self.inner.complete_snapshot(scope)
        }
    }

    impl AsyncRecordedWriter for HistoryCountingStore {
        fn append(
            &self,
            request: AppendRequest,
        ) -> BoxFuture<'_, Result<AppendOutcome, WriteFailure>> {
            self.inner.append(request)
        }
    }

    fn registry() -> Registry {
        let definition = serde_json::from_value(json!({
            "entity": "ticket",
            "version": 1,
            "schema": {
                "fields": {
                    "title": { "type": "string", "required": true },
                    "count": { "type": "integer", "default": 0 }
                }
            },
            "lifecycle": { "initial": "open", "states": ["open"] },
            "operations": {
                "touch": {
                    "transitions": [{ "from": "open", "to": "open" }],
                    "arguments": { "fields": {} },
                    "emits": []
                }
            }
        }))
        .expect("definition parses");
        let mut registry = Registry::new();
        registry.register(definition).expect("definition validates");
        registry
    }

    fn recording(record_id: &str) -> Recording {
        Recording {
            record_id: record_id.to_owned(),
            recorded_at: "2026-11-22T00:00:00Z".to_owned(),
            correlation: None,
            causation: None,
            actor: None,
        }
    }

    fn create(id: &str, title: &str, record_id: &str) -> CreateRequest {
        CreateRequest {
            subject: Subject::new("ticket", id).expect("subject"),
            definition_version: 1,
            fields: json!({ "title": title }),
            recording: recording(record_id),
        }
    }

    fn touch(id: &str, revision: u64, record_id: &str) -> ExecuteRequest {
        ExecuteRequest {
            subject: Subject::new("ticket", id).expect("subject"),
            expected_revision: revision,
            operation: "touch".to_owned(),
            arguments: json!({}),
            fulfillments: BTreeMap::new(),
            recording: recording(record_id),
        }
    }

    /// The first `decisions` decisions on `id`, then one creation of `other`.
    fn consecutive_decisions(id: &str, decisions: u64, other: &str) -> Vec<BatchAction> {
        let mut actions = vec![BatchAction::Create(create(id, id, &format!("{id}-0")))];
        for revision in 1..decisions {
            actions.push(BatchAction::Execute(touch(
                id,
                revision,
                &format!("{id}-{revision}"),
            )));
        }
        actions.push(BatchAction::Create(create(
            other,
            other,
            &format!("{other}-0"),
        )));
        actions
    }

    fn creation_kernel_calls() -> usize {
        CREATION_KERNEL_CALLS.with(std::cell::Cell::get)
    }

    #[test]
    fn a_retried_named_batch_reads_each_subject_history_once() {
        let registry = registry();
        let store = HistoryCountingStore::default();
        let executor = Executor::new(&registry, &store);
        let key = BatchKey::Named("consecutive".to_owned());
        let first =
            test_support::block_on(executor.batch(key.clone(), consecutive_decisions("a", 6, "b")))
                .expect("the batch commits");
        assert!(matches!(
            first,
            AppendOutcome::Committed {
                replayed: false,
                ..
            }
        ));
        store.history_reads.store(0, Ordering::SeqCst);

        let retried =
            test_support::block_on(executor.batch(key, consecutive_decisions("a", 6, "b")))
                .expect("the lost reply is recovered");

        assert!(matches!(
            retried,
            AppendOutcome::Committed { replayed: true, .. }
        ));
        // Seven members on two subjects: one read of each subject's history, not one per member.
        assert_eq!(store.history_reads.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_retried_named_batch_still_refuses_a_changed_member_after_sharing_its_history() {
        let registry = registry();
        let store = HistoryCountingStore::default();
        let executor = Executor::new(&registry, &store);
        let key = BatchKey::Named("consecutive".to_owned());
        test_support::block_on(executor.batch(key.clone(), consecutive_decisions("a", 4, "b")))
            .expect("the batch commits");
        let mut changed = consecutive_decisions("a", 4, "b");
        changed[4] = BatchAction::Create(create("b", "another title", "b-0"));

        let error = test_support::block_on(executor.batch(key, changed))
            .expect_err("a changed member is no retry");

        assert!(
            matches!(
                error.store_error(),
                Some(AsyncStoreError::RecordConflict { record_id }) if record_id == "b-0"
            ),
            "unexpected refusal: {error:?}"
        );
    }

    #[test]
    fn a_fresh_create_runs_the_creation_kernel_once() {
        let registry = registry();
        let store = MemoryRecordedStore::new();
        let executor = Executor::new(&registry, &store);
        let before = creation_kernel_calls();

        let outcome = test_support::block_on(executor.create(create("a", "first", "a-create")))
            .expect("the create commits");

        assert!(matches!(
            outcome,
            AppendOutcome::Committed {
                replayed: false,
                ..
            }
        ));
        assert_eq!(creation_kernel_calls() - before, 1);
    }

    #[test]
    fn a_create_retry_is_compared_by_deciding_the_request_again() {
        let registry = registry();
        let store = MemoryRecordedStore::new();
        let executor = Executor::new(&registry, &store);
        test_support::block_on(executor.create(create("a", "first", "a-create")))
            .expect("the create commits");
        let before = creation_kernel_calls();

        let replayed = test_support::block_on(executor.create(create("a", "first", "a-create")))
            .expect("the identical retry replays");
        assert!(matches!(
            replayed,
            AppendOutcome::Committed { replayed: true, .. }
        ));
        // The retry decides the request again rather than trusting the stored record.
        assert_eq!(creation_kernel_calls() - before, 1);

        let error = test_support::block_on(executor.create(create("a", "second", "a-create")))
            .expect_err("a different request under the same record id is refused");
        assert!(
            matches!(
                error.store_error(),
                Some(AsyncStoreError::RecordConflict { record_id }) if record_id == "a-create"
            ),
            "unexpected refusal: {error:?}"
        );
    }
}
