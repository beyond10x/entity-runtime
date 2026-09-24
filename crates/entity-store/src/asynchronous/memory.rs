use std::{
    collections::BTreeMap,
    future,
    sync::{Arc, Mutex, MutexGuard},
};

use entity_core::EntityInstance;

use super::{
    batch_comparison_bytes, record_comparison_bytes, validate_entry_against_state,
    validate_imported_boundary, verify_imported_record, AppendOutcome, AppendRequest,
    AsyncRecordedReader, AsyncRecordedWriter, AsyncStateReader, AsyncStoreError, BatchKey,
    BatchReceipt, BoxFuture, CommitReceipt, CompleteStoreSnapshot, HistoryOrigin,
    ImportedRecordEvidence, LegacyEvidence, RecordLookup, RecordPosition, RecordReceipt,
    StoreCoverage, StoredBatch, StoredRecord, Subject, SubjectAssurance, SubjectHistory,
    SubjectSnapshot, WriteFailure,
};

/// Deterministic response behavior used to test uncertain and dropped append responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppendScript {
    /// Commit atomically, then report a typed uncertain response.
    CommitThenUncertain,
    /// Commit atomically, then leave the returned future pending so a caller can drop it.
    CommitThenPending,
}

#[derive(Debug, Clone, Default)]
struct MemoryState {
    instances: BTreeMap<Subject, EntityInstance>,
    histories: BTreeMap<Subject, SubjectHistory>,
    records: BTreeMap<String, RecordLookup>,
    batches: BTreeMap<BatchKey, StoredBatch>,
    next_store_position: Option<u64>,
    next_script: Option<AppendScript>,
    trace: Vec<String>,
}

impl MemoryState {
    fn empty() -> Self {
        Self {
            next_store_position: Some(0),
            ..Self::default()
        }
    }
}

/// A clone/validate/swap in-memory implementation of the complete async recorded contract.
#[derive(Debug, Clone)]
pub struct MemoryRecordedStore {
    state: Arc<Mutex<MemoryState>>,
}

impl Default for MemoryRecordedStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryRecordedStore {
    /// Constructs an empty reference store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MemoryState::empty())),
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, MemoryState>, AsyncStoreError> {
        self.state
            .lock()
            .map_err(|_| AsyncStoreError::Backend("reference-store lock was poisoned".to_owned()))
    }

    fn trace_call(&self, operation: String) -> Result<(), AsyncStoreError> {
        self.lock()?.trace.push(operation);
        Ok(())
    }

    /// Returns the deterministic port call trace used by ordering conformance tests.
    #[doc(hidden)]
    pub fn trace(&self) -> Vec<String> {
        self.lock()
            .map_or_else(|_| Vec::new(), |state| state.trace.clone())
    }

    /// Clears only the deterministic port call trace.
    #[doc(hidden)]
    pub fn clear_trace(&self) {
        if let Ok(mut state) = self.lock() {
            state.trace.clear();
        }
    }

    /// Scripts the next append response without changing append atomicity.
    #[doc(hidden)]
    pub fn script_next_append(&self, script: AppendScript) {
        if let Ok(mut state) = self.lock() {
            state.next_script = Some(script);
        }
    }

    /// Seeds one explicit imported boundary for deterministic conformance tests.
    ///
    /// This consumes already typed evidence; it is not a source importer.
    ///
    /// # Errors
    ///
    /// Invalid, duplicate, or conflicting boundary evidence.
    pub fn seed_imported(&self, history: SubjectHistory) -> Result<(), AsyncStoreError> {
        let HistoryOrigin::Imported(anchor) = &history.origin else {
            return Err(AsyncStoreError::InvalidInput(
                "seed_imported requires an explicit imported origin".to_owned(),
            ));
        };
        if !history.records.is_empty() {
            return Err(AsyncStoreError::InvalidInput(
                "seed_imported accepts a boundary before executor suffix records".to_owned(),
            ));
        }
        history.subject.validate()?;
        validate_imported_boundary(&history)?;
        if anchor.instance.entity != history.subject.entity
            || anchor.instance.id != history.subject.id
        {
            return Err(AsyncStoreError::CorruptHistory {
                subject: history.subject.clone(),
                detail: "anchor instance belongs to another subject".to_owned(),
            });
        }
        if anchor.instance.revision == 0 || anchor.instance.revision > i64::MAX as u64 {
            return Err(AsyncStoreError::CorruptHistory {
                subject: history.subject.clone(),
                detail: "anchor revision is outside the supported domain".to_owned(),
            });
        }
        let mut guard = self.lock()?;
        if guard.histories.contains_key(&history.subject) {
            return Err(AsyncStoreError::InvalidInput(
                "a subject may have only one genesis or imported boundary".to_owned(),
            ));
        }
        let mut next = guard.clone();
        for evidence in &anchor.evidence {
            if let LegacyEvidence::Envelope(evidence) = evidence {
                evidence.entry.validate()?;
                let record_id = evidence.entry.record_id().to_owned();
                if next.records.contains_key(&record_id) {
                    return Err(AsyncStoreError::RecordConflict { record_id });
                }
                next.records
                    .insert(record_id, RecordLookup::Imported(evidence.clone()));
            }
        }
        next.instances
            .insert(history.subject.clone(), anchor.instance.clone());
        next.histories.insert(history.subject.clone(), history);
        *guard = next;
        Ok(())
    }

    fn consume_script(&self) -> Result<Option<AppendScript>, AsyncStoreError> {
        Ok(self.lock()?.next_script.take())
    }

    fn append_transaction(&self, request: &AppendRequest) -> Result<AppendOutcome, WriteFailure> {
        let key = request
            .key
            .clone()
            .expect("the public writer validated a nonempty request key");
        let mut guard = self.lock().map_err(WriteFailure::NotCommitted)?;
        guard.trace.push("append".to_owned());
        let comparison =
            batch_comparison_bytes(&key, &request.members).map_err(WriteFailure::NotCommitted)?;
        if let Some(existing) = guard.batches.get(&key) {
            let same_requests = existing.records.len() == request.members.len()
                && existing
                    .records
                    .iter()
                    .zip(&request.members)
                    .all(|(stored, requested)| stored.request_bytes == requested.request_bytes);
            return if existing.comparison_bytes == comparison && same_requests {
                Ok(AppendOutcome::Committed {
                    receipt: existing.receipt.clone(),
                    replayed: true,
                })
            } else {
                Err(WriteFailure::NotCommitted(AsyncStoreError::BatchConflict {
                    key,
                }))
            };
        }

        let mut prior = Vec::new();
        for (index, member) in request.members.iter().enumerate() {
            if let Some(existing) = guard.records.get(member.entry.record_id()) {
                let same = match existing {
                    RecordLookup::Committed(stored) => {
                        stored.record_bytes
                            == record_comparison_bytes(&member.entry)
                                .map_err(WriteFailure::NotCommitted)?
                            && stored.request_bytes == member.request_bytes
                    }
                    RecordLookup::Imported(evidence) => {
                        record_comparison_bytes(&evidence.entry)
                            .map_err(WriteFailure::NotCommitted)?
                            == record_comparison_bytes(&member.entry)
                                .map_err(WriteFailure::NotCommitted)?
                    }
                };
                if !same {
                    return Err(WriteFailure::NotCommitted(
                        AsyncStoreError::RecordConflict {
                            record_id: member.entry.record_id().to_owned(),
                        },
                    ));
                }
                let index = u64::try_from(index).map_err(|_| {
                    WriteFailure::NotCommitted(AsyncStoreError::PositionExhausted {
                        domain: "member index".to_owned(),
                    })
                })?;
                prior.push((index, existing.clone()));
            }
        }
        if matches!(key, BatchKey::SingleRecord(_)) {
            if let Some((_, existing)) = prior.into_iter().next() {
                return match existing {
                    RecordLookup::Committed(stored) => Ok(AppendOutcome::Committed {
                        receipt: CommitReceipt::Single(stored.receipt),
                        replayed: true,
                    }),
                    RecordLookup::Imported(evidence) => {
                        let assurance = imported_assurance(&guard, &evidence)
                            .map_err(WriteFailure::NotCommitted)?;
                        Ok(AppendOutcome::Historical {
                            assurance,
                            evidence,
                        })
                    }
                };
            }
        } else if !prior.is_empty() {
            return Err(WriteFailure::NotCommitted(
                AsyncStoreError::PreviouslyRecordedBatchEntries {
                    indices: prior.into_iter().map(|(index, _)| index).collect(),
                },
            ));
        }

        let mut next = guard.clone();
        let mut overlay: BTreeMap<Subject, EntityInstance> = BTreeMap::new();
        let mut stored = Vec::with_capacity(request.members.len());
        for (index, member) in request.members.iter().enumerate() {
            let subject = member.entry.subject();
            let current = overlay
                .get(&subject)
                .or_else(|| next.instances.get(&subject));
            let resulting = validate_entry_against_state(&member.entry, member.expect, current)
                .map_err(WriteFailure::NotCommitted)?
                .ok_or_else(|| {
                    WriteFailure::NotCommitted(AsyncStoreError::Backend(
                        "validated record produced no materialized state".to_owned(),
                    ))
                })?;
            let subject_position = match next
                .histories
                .get(&subject)
                .and_then(|history| history.records.last())
            {
                Some(last) => last.position.subject.checked_add(1).ok_or_else(|| {
                    WriteFailure::NotCommitted(AsyncStoreError::PositionExhausted {
                        domain: format!("subject {}", subject.coordinate_id()),
                    })
                })?,
                None => 0,
            };
            next.histories
                .entry(subject.clone())
                .or_insert_with(|| SubjectHistory {
                    subject: subject.clone(),
                    origin: HistoryOrigin::Genesis,
                    records: Vec::new(),
                });
            let store_position = next.next_store_position.ok_or_else(|| {
                WriteFailure::NotCommitted(AsyncStoreError::PositionExhausted {
                    domain: "store".to_owned(),
                })
            })?;
            next.next_store_position = store_position.checked_add(1);
            let member_index = u64::try_from(index).map_err(|_| {
                WriteFailure::NotCommitted(AsyncStoreError::PositionExhausted {
                    domain: "member index".to_owned(),
                })
            })?;
            let position = RecordPosition {
                subject: subject_position,
                store: store_position,
            };
            let receipt = RecordReceipt {
                record_id: member.entry.record_id().to_owned(),
                subject: subject.clone(),
                kind: member.entry.kind(),
                revision: member.entry.revision(),
                position,
                batch_key: key.clone(),
                member_index,
            };
            let record = StoredRecord {
                entry: member.entry.clone(),
                position,
                receipt,
                expect: member.expect,
                request_bytes: member.request_bytes.clone(),
                lineage: None,
                record_bytes: record_comparison_bytes(&member.entry)
                    .map_err(WriteFailure::NotCommitted)?,
            };
            next.histories
                .get_mut(&subject)
                .expect("history was established during validation")
                .records
                .push(record.clone());
            overlay.insert(subject, resulting);
            stored.push(record);
        }

        for record in &stored {
            next.records.insert(
                record.entry.record_id().to_owned(),
                RecordLookup::Committed(record.clone()),
            );
        }
        next.instances.extend(overlay);
        let receipt = match &key {
            BatchKey::SingleRecord(_) => CommitReceipt::Single(stored[0].receipt.clone()),
            BatchKey::Named(_) => CommitReceipt::Batch(BatchReceipt {
                key: key.clone(),
                members: stored.iter().map(|record| record.receipt.clone()).collect(),
            }),
        };
        next.batches.insert(
            key.clone(),
            StoredBatch {
                key,
                records: stored,
                comparison_bytes: comparison,
                receipt: receipt.clone(),
            },
        );
        *guard = next;
        Ok(AppendOutcome::Committed {
            receipt,
            replayed: false,
        })
    }

    /// Sets the next store position for exact checked-allocation conformance tests.
    #[doc(hidden)]
    pub fn set_next_store_position_for_test(&self, position: u64) {
        if let Ok(mut state) = self.lock() {
            state.next_store_position = Some(position);
        }
    }

    /// Alters a stored decision result so the verifier's complete replay check can be mutated.
    #[doc(hidden)]
    pub fn tamper_decision_result_for_test(&self, record_id: &str) {
        if let Ok(mut state) = self.lock() {
            tamper_committed(&mut state, record_id, |record| {
                if let super::RecordedEntry::Decision(commit) = &mut record.entry {
                    commit
                        .envelope
                        .record
                        .result
                        .lifecycle_state
                        .push_str("-tampered");
                }
            });
        }
    }

    /// Alters one stored decision event so prefix retry verification must refuse it.
    #[doc(hidden)]
    pub fn tamper_record_event_for_test(&self, record_id: &str) {
        if let Ok(mut state) = self.lock() {
            tamper_committed(&mut state, record_id, |record| {
                if let super::RecordedEntry::Decision(commit) = &mut record.entry {
                    if let Some(event) = commit.envelope.record.events.first_mut() {
                        event.event_type.push_str("Tampered");
                    } else {
                        commit
                            .envelope
                            .record
                            .changed
                            .insert("tampered".to_owned(), serde_json::Value::Bool(true));
                    }
                }
            });
        }
    }

    /// Alters a receipt component so coordinate equality verification must refuse it.
    #[doc(hidden)]
    pub fn tamper_receipt_subject_for_test(&self, record_id: &str, id: &str) {
        if let Ok(mut state) = self.lock() {
            tamper_committed(&mut state, record_id, |record| {
                record.receipt.subject.id = id.to_owned();
            });
        }
    }

    /// Removes a saved imported definition to exercise unverifiable historical retry handling.
    #[doc(hidden)]
    pub fn remove_imported_definition_for_test(&self, record_id: &str) {
        if let Ok(mut state) = self.lock() {
            if let Some(RecordLookup::Imported(evidence)) = state.records.get_mut(record_id) {
                if let super::RecordedEntry::Decision(commit) = &mut evidence.entry {
                    commit.envelope.record.definition = None;
                }
            }
            for history in state.histories.values_mut() {
                if let HistoryOrigin::Imported(anchor) = &mut history.origin {
                    for item in &mut anchor.evidence {
                        if let LegacyEvidence::Envelope(evidence) = item {
                            if evidence.entry.record_id() == record_id {
                                if let super::RecordedEntry::Decision(commit) = &mut evidence.entry
                                {
                                    commit.envelope.record.definition = None;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn imported_assurance(
    state: &MemoryState,
    evidence: &ImportedRecordEvidence,
) -> Result<SubjectAssurance, AsyncStoreError> {
    let subject = evidence.entry.subject();
    let history = state
        .histories
        .get(&subject)
        .ok_or_else(|| AsyncStoreError::CorruptHistory {
            subject: subject.clone(),
            detail: "imported global record has no subject boundary".to_owned(),
        })?;
    verify_imported_record(history, evidence)
}

fn tamper_committed(state: &mut MemoryState, record_id: &str, mutate: impl Fn(&mut StoredRecord)) {
    if let Some(RecordLookup::Committed(record)) = state.records.get_mut(record_id) {
        mutate(record);
        refresh_record_bytes(record);
    }
    for history in state.histories.values_mut() {
        if let Some(record) = history
            .records
            .iter_mut()
            .find(|record| record.entry.record_id() == record_id)
        {
            mutate(record);
            refresh_record_bytes(record);
        }
    }
    for batch in state.batches.values_mut() {
        if let Some(record) = batch
            .records
            .iter_mut()
            .find(|record| record.entry.record_id() == record_id)
        {
            mutate(record);
            refresh_record_bytes(record);
        }
        let members: Vec<_> = batch
            .records
            .iter()
            .map(|record| {
                super::AppendMember::new(
                    record.expect,
                    record.entry.clone(),
                    record.request_bytes.clone(),
                )
            })
            .collect();
        if let Ok(bytes) = batch_comparison_bytes(&batch.key, &members) {
            batch.comparison_bytes = bytes;
        }
    }
}

fn refresh_record_bytes(record: &mut StoredRecord) {
    if let Ok(bytes) = record_comparison_bytes(&record.entry) {
        record.record_bytes = bytes;
    }
}

impl AsyncStateReader for MemoryRecordedStore {
    fn load<'a>(
        &'a self,
        subject: &'a Subject,
    ) -> BoxFuture<'a, Result<Option<EntityInstance>, AsyncStoreError>> {
        Box::pin(async move {
            self.trace_call(format!("load:{}", subject.coordinate_id()))?;
            Ok(self.lock()?.instances.get(subject).cloned())
        })
    }
}

impl AsyncRecordedReader for MemoryRecordedStore {
    fn lookup_record<'a>(
        &'a self,
        record_id: &'a str,
    ) -> BoxFuture<'a, Result<Option<RecordLookup>, AsyncStoreError>> {
        Box::pin(async move {
            self.trace_call(format!("lookup_record:{record_id}"))?;
            Ok(self.lock()?.records.get(record_id).cloned())
        })
    }

    fn lookup_batch<'a>(
        &'a self,
        key: &'a BatchKey,
    ) -> BoxFuture<'a, Result<Option<StoredBatch>, AsyncStoreError>> {
        Box::pin(async move {
            self.trace_call("lookup_batch".to_owned())?;
            Ok(self.lock()?.batches.get(key).cloned())
        })
    }

    fn history<'a>(
        &'a self,
        subject: &'a Subject,
    ) -> BoxFuture<'a, Result<SubjectHistory, AsyncStoreError>> {
        Box::pin(async move {
            self.trace_call(format!("history:{}", subject.coordinate_id()))?;
            Ok(self
                .lock()?
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
            self.trace_call("complete_snapshot".to_owned())?;
            let state = self.lock()?;
            let mut histories = Vec::with_capacity(state.histories.len());
            for (subject, history) in &state.histories {
                let terminal = state.instances.get(subject).cloned().ok_or_else(|| {
                    AsyncStoreError::CorruptHistory {
                        subject: subject.clone(),
                        detail: "complete snapshot has history without terminal state".to_owned(),
                    }
                })?;
                histories.push(SubjectSnapshot {
                    history: history.clone(),
                    terminal,
                });
            }
            Ok(CompleteStoreSnapshot {
                scope: scope.to_owned(),
                coverage: StoreCoverage::CompleteSnapshot,
                histories,
            })
        })
    }
}

impl AsyncRecordedWriter for MemoryRecordedStore {
    fn append(&self, request: AppendRequest) -> BoxFuture<'_, Result<AppendOutcome, WriteFailure>> {
        if let Err(error) = request.validate() {
            return Box::pin(async move { Err(WriteFailure::NotCommitted(error)) });
        }
        if request.members.is_empty() {
            return Box::pin(async { Ok(AppendOutcome::Empty) });
        }
        Box::pin(async move {
            let script = self.consume_script().map_err(WriteFailure::NotCommitted)?;
            let outcome = self.append_transaction(&request)?;
            match script {
                Some(AppendScript::CommitThenUncertain) => Err(WriteFailure::Uncertain {
                    key: request.key.expect("nonempty request has a key"),
                    cause: "scripted response loss after atomic commit".to_owned(),
                }),
                Some(AppendScript::CommitThenPending) => future::pending().await,
                None => Ok(outcome),
            }
        })
    }
}
