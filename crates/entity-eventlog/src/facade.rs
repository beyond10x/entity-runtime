//! Compatible synchronous provider reads and complete recorded writes over the Eventlog bridge.

use entity_core::{DecisionRecord, DomainEvent, EntityInstance, Registry};
use entity_executor::{BatchAction, CreateRequest, ExecuteRequest};
use entity_query::{
    DocumentPage, DocumentQuery, DocumentQueryProvider, QueryError, query_ordered_instances,
};
use entity_store::{
    Envelope, EventProvider, Expect, HistoryProvider, LegacyStoreSnapshot, RecordedCommit,
    RecordedObservation, StateProvider, StoreError,
    asynchronous::{
        AppendMember, AppendOutcome, AppendRequest, AsyncStoreError, BatchKey,
        CompleteStoreSnapshot, HistoryOrigin, LegacyEvidence, RecordLookup, RecordedEntry,
        StoredBatch, Subject, SubjectAssurance, SubjectHistory, original_request_comparison_bytes,
    },
};

#[cfg(feature = "file")]
use crate::sync::ProvisionAuthority;
use crate::{
    Authority, EventlogOperationContext,
    sync::{
        BridgeConfig, BridgeStartError, CallWait, EventlogRecordedStoreOwner,
        EventlogRecordedStoreProvisioner, RecordedEventlogBridge, ShutdownMode, ShutdownOutcome,
        SyncExecutionError, SyncImportError, SyncReadError, SyncWriteError,
    },
};

/// Eventlog-backed File facade with complete recorded receipts and atomic groups.
#[cfg(feature = "file")]
#[derive(Debug)]
pub struct EventlogFileStore {
    facade: RecordedProviderFacade,
}

#[cfg(feature = "file")]
impl EventlogFileStore {
    /// Opens an already provisioned File Eventlog authority without mutating it.
    ///
    /// # Errors
    ///
    /// Worker/runtime construction or exact authority verification failure.
    pub fn open(
        path: impl Into<std::path::PathBuf>,
        registry: Registry,
        authority: Authority,
        limits: eventlog_core::CaptureLimits,
        config: BridgeConfig,
    ) -> Result<Self, BridgeStartError> {
        let facade = RecordedProviderFacade::start(
            registry,
            EventlogRecordedStoreOwner::File {
                path: path.into(),
                authority,
                limits,
            },
            config,
        )?;
        Ok(Self { facade })
    }

    /// Explicitly provisions and binds a File Eventlog authority before opening it.
    ///
    /// # Errors
    ///
    /// Native preparation, binding conflict/uncertainty, or open verification failure.
    pub fn provision(
        path: impl Into<std::path::PathBuf>,
        registry: Registry,
        authority: ProvisionAuthority,
        context: EventlogOperationContext,
        limits: eventlog_core::CaptureLimits,
        config: BridgeConfig,
    ) -> Result<Self, BridgeStartError> {
        let facade = RecordedProviderFacade::provision(
            registry,
            EventlogRecordedStoreProvisioner::File {
                path: path.into(),
                authority,
                limits,
            },
            context,
            config,
        )?;
        Ok(Self { facade })
    }

    /// The complete receipt-preserving facade.
    #[must_use]
    pub const fn recorded(&self) -> &RecordedProviderFacade {
        &self.facade
    }

    /// Closes admission and reports actual native retirement.
    pub fn shutdown(&mut self, mode: ShutdownMode, wait: CallWait) -> ShutdownOutcome {
        self.facade.shutdown(mode, wait)
    }
}

#[cfg(feature = "file")]
impl StateProvider for EventlogFileStore {
    fn load(&self, entity: &str, id: &str) -> Result<Option<EntityInstance>, StoreError> {
        self.facade.load(entity, id)
    }

    fn ids(&self, entity: &str) -> Result<Vec<String>, StoreError> {
        self.facade.ids(entity)
    }
}

#[cfg(feature = "file")]
impl EventProvider for EventlogFileStore {
    fn events(&self, entity: &str, id: &str) -> Result<Vec<DomainEvent>, StoreError> {
        self.facade.events(entity, id)
    }
}

#[cfg(feature = "file")]
impl HistoryProvider for EventlogFileStore {
    fn records(&self, entity: &str, id: &str) -> Result<Vec<Envelope<DecisionRecord>>, StoreError> {
        self.facade.records(entity, id)
    }

    fn observations(&self, entity: &str, id: &str) -> Result<Vec<RecordedObservation>, StoreError> {
        self.facade.observations(entity, id)
    }
}

#[cfg(feature = "file")]
impl DocumentQueryProvider for EventlogFileStore {
    fn query_documents(&self, query: &DocumentQuery) -> Result<DocumentPage, QueryError> {
        self.facade.query_documents(query)
    }
}

/// Synchronous complete-record provider facade over one worker-owned Eventlog authority.
pub struct RecordedProviderFacade {
    authority: Authority,
    bridge: RecordedEventlogBridge,
}

impl std::fmt::Debug for RecordedProviderFacade {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RecordedProviderFacade")
            .field("authority", &self.authority)
            .finish_non_exhaustive()
    }
}

impl RecordedProviderFacade {
    /// Opens an already provisioned and exactly bound provider on its dedicated worker.
    ///
    /// # Errors
    ///
    /// Worker/runtime construction or conclusive provider-open refusal.
    pub fn start(
        registry: Registry,
        owner: EventlogRecordedStoreOwner,
        config: BridgeConfig,
    ) -> Result<Self, BridgeStartError> {
        let authority = owner.authority().clone();
        let bridge = RecordedEventlogBridge::start(registry, owner, config)?;
        Ok(Self { authority, bridge })
    }

    /// Explicitly provisions the fixed projections and immutable binding before opening.
    ///
    /// # Errors
    ///
    /// Native preparation, binding conflict/uncertainty, worker construction, or open refusal.
    pub fn provision(
        registry: Registry,
        owner: EventlogRecordedStoreProvisioner,
        context: EventlogOperationContext,
        config: BridgeConfig,
    ) -> Result<Self, BridgeStartError> {
        let (bridge, authority) =
            RecordedEventlogBridge::provision_and_start(registry, owner, context, config)?;
        Ok(Self { authority, bridge })
    }

    /// Exact bound logical scope.
    #[must_use]
    pub fn scope(&self) -> &str {
        &self.authority.logical_scope
    }

    /// Exact immutable logical/physical authority verified by this facade.
    #[must_use]
    pub const fn authority(&self) -> &Authority {
        &self.authority
    }

    /// Supplies caller-owned operational facts to one write operation.
    #[must_use]
    pub fn operation(
        &self,
        context: EventlogOperationContext,
    ) -> crate::sync::SyncEventlogOperation<'_> {
        self.bridge.operation(context)
    }

    /// Loads current state while retaining the bridge's typed read outcome.
    pub fn load_recorded(
        &self,
        subject: &Subject,
        wait: CallWait,
    ) -> Result<Option<EntityInstance>, SyncReadError> {
        self.bridge.load(subject, wait)
    }

    /// Loads mixed subject history while retaining imported-boundary semantics.
    pub fn read_history(
        &self,
        subject: &Subject,
        wait: CallWait,
    ) -> Result<SubjectHistory, SyncReadError> {
        self.bridge.history(subject, wait)
    }

    /// Looks up one original global record identity.
    pub fn lookup_record(
        &self,
        record_id: &str,
        wait: CallWait,
    ) -> Result<Option<RecordLookup>, SyncReadError> {
        self.bridge.lookup_record(record_id, wait)
    }

    /// Looks up one original single-record or named batch claim.
    pub fn lookup_batch(
        &self,
        key: &BatchKey,
        wait: CallWait,
    ) -> Result<Option<StoredBatch>, SyncReadError> {
        self.bridge.lookup_batch(key, wait)
    }

    /// Obtains the provider-owned complete bound snapshot.
    pub fn complete_snapshot(
        &self,
        wait: CallWait,
    ) -> Result<CompleteStoreSnapshot, SyncReadError> {
        self.bridge
            .complete_snapshot(&self.authority.logical_scope, wait)
    }

    /// Appends an already closed complete-record request.
    pub fn append(
        &self,
        context: EventlogOperationContext,
        request: AppendRequest,
        wait: CallWait,
    ) -> Result<AppendOutcome, SyncWriteError> {
        self.operation(context).append(request, wait)
    }

    /// Appends one complete recorded decision under its original global identity.
    pub fn commit_recorded(
        &self,
        context: EventlogOperationContext,
        commit: RecordedCommit,
        expect: Expect,
        wait: CallWait,
    ) -> Result<AppendOutcome, SyncWriteError> {
        let entry = RecordedEntry::Decision(commit);
        let request_bytes = original_request_comparison_bytes(&entry).map_err(|error| {
            SyncWriteError::Write(entity_store::asynchronous::WriteFailure::NotCommitted(
                error,
            ))
        })?;
        let key = BatchKey::SingleRecord(entry.record_id().to_owned());
        let request =
            AppendRequest::new(key, vec![AppendMember::new(expect, entry, request_bytes)])
                .map_err(|error| {
                    SyncWriteError::Write(entity_store::asynchronous::WriteFailure::NotCommitted(
                        error,
                    ))
                })?;
        self.append(context, request, wait)
    }

    /// Appends an ordered named group of complete recorded decisions as one Eventlog group.
    pub fn commit_recorded_batch(
        &self,
        context: EventlogOperationContext,
        key: impl Into<String>,
        commits: Vec<(RecordedCommit, Expect)>,
        wait: CallWait,
    ) -> Result<AppendOutcome, SyncWriteError> {
        let members = commits
            .into_iter()
            .map(|(commit, expect)| {
                let entry = RecordedEntry::Decision(commit);
                let request_bytes = original_request_comparison_bytes(&entry)?;
                Ok(AppendMember::new(expect, entry, request_bytes))
            })
            .collect::<Result<Vec<_>, AsyncStoreError>>()
            .map_err(|error| {
                SyncWriteError::Write(entity_store::asynchronous::WriteFailure::NotCommitted(
                    error,
                ))
            })?;
        let request =
            AppendRequest::new(BatchKey::Named(key.into()), members).map_err(|error| {
                SyncWriteError::Write(entity_store::asynchronous::WriteFailure::NotCommitted(
                    error,
                ))
            })?;
        self.append(context, request, wait)
    }

    /// Executes creation through the one existing executor/kernel path.
    pub fn create(
        &self,
        context: EventlogOperationContext,
        request: CreateRequest,
        wait: CallWait,
    ) -> Result<AppendOutcome, SyncExecutionError> {
        self.operation(context).create(request, wait)
    }

    /// Executes a named operation through the one existing executor/kernel path.
    pub fn execute(
        &self,
        context: EventlogOperationContext,
        request: ExecuteRequest,
        wait: CallWait,
    ) -> Result<AppendOutcome, SyncExecutionError> {
        self.operation(context).execute(request, wait)
    }

    /// Records a non-state-changing observation with its original identity.
    pub fn observe(
        &self,
        context: EventlogOperationContext,
        observation: RecordedObservation,
        wait: CallWait,
    ) -> Result<AppendOutcome, SyncExecutionError> {
        self.operation(context).observe(observation, wait)
    }

    /// Executes ordered heterogeneous actions through the existing executor.
    pub fn execute_batch(
        &self,
        context: EventlogOperationContext,
        key: BatchKey,
        actions: Vec<BatchAction>,
        wait: CallWait,
    ) -> Result<AppendOutcome, SyncExecutionError> {
        self.operation(context).batch(key, actions, wait)
    }

    /// Imports every validated legacy boundary, then proves exact typed destination equivalence.
    ///
    /// A partial import remains safely resumable: settled equal anchors recover, while the first
    /// unequal or uncertain subject is returned with the assurances already established.
    pub fn import_legacy(
        &self,
        snapshot: LegacyStoreSnapshot,
        context: EventlogOperationContext,
        wait: CallWait,
    ) -> Result<LegacyImportReport, LegacyImportError> {
        // The public acquisition document remains inspectable for compatibility. Reconstructing it
        // here prevents a caller from mutating a previously validated document between acquisition
        // and import, and establishes every cross-subject/global-identity invariant before the
        // first destination write.
        let snapshot = LegacyStoreSnapshot::new(snapshot.source_id, snapshot.histories)
            .map_err(LegacyImportError::Source)?;
        let mut settled = Vec::with_capacity(snapshot.histories.len());
        let mut replayed = 0;
        for history in &snapshot.histories {
            match self
                .operation(context.clone())
                .import_anchor(history.clone(), wait)
            {
                Ok(outcome) => {
                    replayed += usize::from(outcome.replayed);
                    settled.push(outcome.assurance);
                }
                Err(error) => {
                    return Err(LegacyImportError::Subject {
                        subject: history.subject.clone(),
                        settled,
                        error: Box::new(error),
                    });
                }
            }
        }
        let captured = self
            .complete_snapshot(wait)
            .map_err(LegacyImportError::VerificationRead)?;
        verify_import_equivalence(&snapshot, &captured).map_err(LegacyImportError::Verification)?;
        Ok(LegacyImportReport {
            source_id: snapshot.source_id,
            subjects: settled,
            replayed,
        })
    }

    /// Closes admission and reports actual provider retirement.
    pub fn shutdown(&mut self, mode: ShutdownMode, wait: CallWait) -> ShutdownOutcome {
        self.bridge.shutdown(mode, wait)
    }
}

/// Settled result of one complete legacy import attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyImportReport {
    /// Exact source identity.
    pub source_id: String,
    /// Assurance established for every sorted subject.
    pub subjects: Vec<SubjectAssurance>,
    /// Number of exact subject anchors recovered from a prior attempt.
    pub replayed: usize,
}

/// Import failure retaining partial progress and exact uncertainty.
#[derive(Debug)]
pub enum LegacyImportError {
    /// The supplied acquisition document is no longer a valid complete source boundary.
    Source(StoreError),
    /// One subject import refused or became uncertain.
    Subject {
        /// Subject whose boundary did not settle.
        subject: Subject,
        /// Sorted earlier subject assurances already settled.
        settled: Vec<SubjectAssurance>,
        /// Exact bridge/import failure.
        error: Box<SyncImportError>,
    },
    /// The provider-owned post-import snapshot could not be obtained.
    VerificationRead(SyncReadError),
    /// The provider-owned snapshot differed from the acquired source facts.
    Verification(String),
}

fn verify_import_equivalence(
    source: &LegacyStoreSnapshot,
    destination: &CompleteStoreSnapshot,
) -> Result<(), String> {
    if destination.scope.trim().is_empty() {
        return Err("destination substituted a blank logical scope".to_owned());
    }
    if destination.histories.len() != source.histories.len() {
        return Err("destination subject set differs from the acquired source".to_owned());
    }
    for (source, destination) in source.histories.iter().zip(&destination.histories) {
        if source.subject != destination.history.subject {
            return Err("destination subject order or identity differs from the source".to_owned());
        }
        if source.origin != destination.history.origin {
            return Err(format!(
                "destination imported evidence differs for {:?}",
                source.subject
            ));
        }
        if !destination.history.records.is_empty() {
            return Err(format!(
                "destination grew a suffix during import for {:?}",
                source.subject
            ));
        }
        let HistoryOrigin::Imported(anchor) = &source.origin else {
            return Err("source history lost its imported boundary".to_owned());
        };
        if destination.terminal != anchor.instance {
            return Err(format!(
                "destination terminal state differs for {:?}",
                source.subject
            ));
        }
    }
    Ok(())
}

impl StateProvider for RecordedProviderFacade {
    fn load(&self, entity: &str, id: &str) -> Result<Option<EntityInstance>, StoreError> {
        let subject = Subject::new(entity, id).map_err(async_store_error)?;
        self.load_recorded(&subject, CallWait::Forever)
            .map_err(sync_read_error)
    }

    fn ids(&self, entity: &str) -> Result<Vec<String>, StoreError> {
        let snapshot = self
            .complete_snapshot(CallWait::Forever)
            .map_err(sync_read_error)?;
        Ok(snapshot
            .histories
            .into_iter()
            .filter(|entry| entry.history.subject.entity == entity)
            .map(|entry| entry.history.subject.id)
            .collect())
    }
}

impl EventProvider for RecordedProviderFacade {
    fn events(&self, entity: &str, id: &str) -> Result<Vec<DomainEvent>, StoreError> {
        let subject = Subject::new(entity, id).map_err(async_store_error)?;
        let history = self
            .read_history(&subject, CallWait::Forever)
            .map_err(sync_read_error)?;
        let mut events = Vec::new();
        if let HistoryOrigin::Imported(anchor) = history.origin {
            for evidence in anchor.evidence {
                match evidence {
                    LegacyEvidence::Envelope(evidence) => {
                        events.extend(evidence.entry.events().iter().cloned());
                    }
                    LegacyEvidence::Decision(record) => events.extend(record.events),
                    LegacyEvidence::Event(event) => events.push(event),
                }
            }
        }
        for stored in history.records {
            events.extend(stored.entry.events().iter().cloned());
        }
        events.sort_by_key(|event| event.revision);
        Ok(events)
    }
}

impl HistoryProvider for RecordedProviderFacade {
    fn records(&self, entity: &str, id: &str) -> Result<Vec<Envelope<DecisionRecord>>, StoreError> {
        let subject = Subject::new(entity, id).map_err(async_store_error)?;
        let history = self
            .read_history(&subject, CallWait::Forever)
            .map_err(sync_read_error)?;
        let mut records = Vec::new();
        if let HistoryOrigin::Imported(anchor) = history.origin {
            records.extend(
                anchor
                    .evidence
                    .into_iter()
                    .filter_map(|evidence| match evidence {
                        LegacyEvidence::Envelope(evidence) => match evidence.entry {
                            RecordedEntry::Decision(commit) => Some(commit.envelope),
                            RecordedEntry::Observation(_) => None,
                        },
                        LegacyEvidence::Decision(_) | LegacyEvidence::Event(_) => None,
                    }),
            );
        }
        records.extend(
            history
                .records
                .into_iter()
                .filter_map(|record| match record.entry {
                    RecordedEntry::Decision(commit) => Some(commit.envelope),
                    RecordedEntry::Observation(_) => None,
                }),
        );
        Ok(records)
    }

    fn observations(&self, entity: &str, id: &str) -> Result<Vec<RecordedObservation>, StoreError> {
        let subject = Subject::new(entity, id).map_err(async_store_error)?;
        let history = self
            .read_history(&subject, CallWait::Forever)
            .map_err(sync_read_error)?;
        let mut observations = Vec::new();
        if let HistoryOrigin::Imported(anchor) = history.origin {
            observations.extend(anchor.evidence.into_iter().filter_map(
                |evidence| match evidence {
                    LegacyEvidence::Envelope(evidence) => match evidence.entry {
                        RecordedEntry::Observation(observation) => Some(observation),
                        RecordedEntry::Decision(_) => None,
                    },
                    LegacyEvidence::Decision(_) | LegacyEvidence::Event(_) => None,
                },
            ));
        }
        observations.extend(
            history
                .records
                .into_iter()
                .filter_map(|record| match record.entry {
                    RecordedEntry::Observation(observation) => Some(observation),
                    RecordedEntry::Decision(_) => None,
                }),
        );
        Ok(observations)
    }
}

impl DocumentQueryProvider for RecordedProviderFacade {
    fn query_documents(&self, query: &DocumentQuery) -> Result<DocumentPage, QueryError> {
        let snapshot = self
            .complete_snapshot(CallWait::Forever)
            .map_err(sync_read_error)
            .map_err(QueryError::Store)?;
        query_ordered_instances(
            query,
            snapshot
                .histories
                .into_iter()
                .filter(|entry| entry.terminal.entity == query.entity)
                .map(|entry| entry.terminal),
        )
    }
}

fn async_store_error(error: AsyncStoreError) -> StoreError {
    match error {
        AsyncStoreError::RevisionConflict {
            subject,
            expected,
            found,
        } => StoreError::RevisionConflict {
            entity: subject.entity,
            id: subject.id,
            expected,
            found,
        },
        AsyncStoreError::RecordConflict { record_id }
        | AsyncStoreError::DuplicateRecordId { record_id } => {
            StoreError::RecordConflict { record_id }
        }
        AsyncStoreError::Unreachable { provider, detail } => {
            StoreError::Unreachable { provider, detail }
        }
        other => StoreError::Backend(other.to_string()),
    }
}

fn sync_read_error(error: SyncReadError) -> StoreError {
    match error {
        SyncReadError::Store(error) => async_store_error(error),
        SyncReadError::Rejected(rejection) => StoreError::Unreachable {
            provider: "entity-eventlog bridge".to_owned(),
            detail: format!("read rejected before dispatch: {rejection:?}"),
        },
        SyncReadError::AfterDispatch(outcome) => StoreError::Unreachable {
            provider: "entity-eventlog bridge".to_owned(),
            detail: format!("read result lost after dispatch: {outcome:?}"),
        },
    }
}
