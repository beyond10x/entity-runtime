use std::collections::{BTreeMap, BTreeSet};

use entity_core::{create, execute, DecisionCommand, EntityInstance, ValidatedDefinition};
use serde_json::Value;

use crate::Expect;

use super::{
    original_request_comparison_bytes, record_comparison_bytes, AsyncStoreError,
    CompleteStoreSnapshot, HistoryOrigin, KnownLegacyOrder, LegacyEvidence, LegacyOrderDeclaration,
    RecordKind, RecordedEntry, StoreAssurance, StoredRecord, Subject, SubjectAssurance,
    SubjectHistory,
};

fn corrupt(subject: &Subject, detail: impl Into<String>) -> AsyncStoreError {
    AsyncStoreError::CorruptHistory {
        subject: subject.clone(),
        detail: detail.into(),
    }
}

fn validate_revision(subject: &Subject, revision: u64) -> Result<(), AsyncStoreError> {
    if revision == 0 || revision > i64::MAX as u64 {
        return Err(corrupt(
            subject,
            format!("revision {revision} is outside 1..={}", i64::MAX),
        ));
    }
    Ok(())
}

fn check_expect(
    subject: &Subject,
    expect: Expect,
    current: Option<&EntityInstance>,
) -> Result<(), AsyncStoreError> {
    let found = current.map(|instance| instance.revision);
    let agrees = matches!((expect, found), (Expect::Absent, None))
        || matches!((expect, found), (Expect::Revision(expected), Some(actual)) if expected == actual);
    if agrees {
        Ok(())
    } else {
        Err(AsyncStoreError::RevisionConflict {
            subject: subject.clone(),
            expected: expect,
            found,
        })
    }
}

pub(crate) fn validate_entry_against_state(
    entry: &RecordedEntry,
    expect: Expect,
    current: Option<&EntityInstance>,
) -> Result<Option<EntityInstance>, AsyncStoreError> {
    entry.validate()?;
    let subject = entry.subject();
    validate_revision(&subject, entry.revision())?;
    check_expect(&subject, expect, current)?;

    match entry {
        RecordedEntry::Observation(observation) => {
            let Some(current) = current else {
                return Err(AsyncStoreError::RevisionConflict {
                    subject,
                    expected: expect,
                    found: None,
                });
            };
            if observation.revision != current.revision {
                return Err(AsyncStoreError::RevisionConflict {
                    subject,
                    expected: Expect::Revision(observation.revision),
                    found: Some(current.revision),
                });
            }
            Ok(Some(current.clone()))
        }
        RecordedEntry::Decision(commit) => {
            let record = &commit.envelope.record;
            let definition = record.definition.clone().ok_or_else(|| {
                corrupt(
                    &subject,
                    "a newly committed decision requires its saved definition",
                )
            })?;
            let definition = ValidatedDefinition::new(definition).map_err(|error| {
                corrupt(&subject, format!("saved definition is invalid: {error}"))
            })?;
            let recomputed = match (&record.command, current) {
                (DecisionCommand::Create { fields }, None) => create(
                    &definition,
                    record.id.clone(),
                    Value::Object(fields.clone()),
                ),
                (
                    DecisionCommand::Execute {
                        operation,
                        arguments,
                    },
                    Some(current),
                ) => execute(
                    &definition,
                    current,
                    operation,
                    Value::Object(arguments.clone()),
                ),
                (DecisionCommand::Create { .. }, Some(_)) => {
                    return Err(corrupt(&subject, "creation appears after a predecessor"))
                }
                (DecisionCommand::Execute { .. }, None) => {
                    return Err(corrupt(
                        &subject,
                        "history begins with execution, not creation",
                    ))
                }
                (DecisionCommand::LegacyImport, _) => {
                    return Err(corrupt(
                        &subject,
                        "legacy import may only establish an explicit anchor",
                    ))
                }
            }
            .map_err(|error| corrupt(&subject, format!("saved command refuses: {error}")))?;
            if recomputed.record != *record || recomputed.instance != commit.instance {
                return Err(corrupt(
                    &subject,
                    "stored decision differs from recomputation of its saved definition and command",
                ));
            }
            Ok(Some(recomputed.instance))
        }
    }
}

pub(crate) fn validate_imported_boundary(history: &SubjectHistory) -> Result<(), AsyncStoreError> {
    let HistoryOrigin::Imported(anchor) = &history.origin else {
        return Ok(());
    };
    if anchor.instance.entity != history.subject.entity || anchor.instance.id != history.subject.id
    {
        return Err(corrupt(
            &history.subject,
            "anchor instance belongs to another subject",
        ));
    }
    validate_revision(&history.subject, anchor.instance.revision)?;
    let mut ids = BTreeSet::new();
    for evidence in &anchor.evidence {
        if let LegacyEvidence::Envelope(envelope) = evidence {
            envelope.entry.validate().map_err(|error| {
                corrupt(
                    &history.subject,
                    format!("imported envelope is invalid: {error}"),
                )
            })?;
            if envelope.entry.subject() != history.subject {
                return Err(corrupt(
                    &history.subject,
                    "imported envelope names another subject",
                ));
            }
            if envelope.source_id.trim().is_empty() || envelope.source_locator.trim().is_empty() {
                return Err(corrupt(
                    &history.subject,
                    "imported envelope has blank source coordinates",
                ));
            }
            if !matches!(
                (anchor.order, envelope.known_order),
                (
                    LegacyOrderDeclaration::PerKindOnly,
                    KnownLegacyOrder::PerKind(_)
                ) | (
                    LegacyOrderDeclaration::Subject,
                    KnownLegacyOrder::Subject(_)
                )
            ) {
                return Err(corrupt(
                    &history.subject,
                    "imported envelope order contradicts the anchor declaration",
                ));
            }
            if !ids.insert(envelope.entry.record_id()) {
                return Err(corrupt(
                    &history.subject,
                    "an imported record identity appears more than once",
                ));
            }
        }
    }
    Ok(())
}

/// Validates one imported global-record lookup against its explicit subject boundary.
///
/// This establishes only that the complete preserved evidence occurs at the named trusted
/// boundary. It does not invent a committed receipt or claim to have replayed imported genesis.
///
/// # Errors
///
/// Typed corruption when the boundary is invalid or does not contain the exact evidence.
pub fn verify_imported_record(
    history: &SubjectHistory,
    evidence: &super::ImportedRecordEvidence,
) -> Result<SubjectAssurance, AsyncStoreError> {
    validate_imported_boundary(history)?;
    let HistoryOrigin::Imported(anchor) = &history.origin else {
        return Err(corrupt(
            &history.subject,
            "imported lookup has no imported subject boundary",
        ));
    };
    if evidence.entry.subject() != history.subject {
        return Err(corrupt(
            &history.subject,
            "imported lookup names another subject",
        ));
    }
    let found = anchor
        .evidence
        .iter()
        .any(|item| matches!(item, LegacyEvidence::Envelope(saved) if saved == evidence));
    if !found {
        return Err(corrupt(
            &history.subject,
            "imported lookup is absent from its claimed boundary",
        ));
    }
    Ok(SubjectAssurance::VerifiedAfterBoundary {
        subject: history.subject.clone(),
        anchor_revision: anchor.instance.revision,
    })
}

fn validate_stored_coordinates(
    history: &SubjectHistory,
    record: &StoredRecord,
    previous_position: Option<super::RecordPosition>,
) -> Result<(), AsyncStoreError> {
    let subject = &history.subject;
    if record.entry.subject() != *subject {
        return Err(corrupt(subject, "stored record names another subject"));
    }
    if previous_position.is_some_and(|previous| record.position.subject <= previous.subject) {
        return Err(corrupt(
            subject,
            "subject physical positions are not strictly increasing",
        ));
    }
    if previous_position.is_some_and(|previous| record.position.store <= previous.store) {
        return Err(corrupt(
            subject,
            "store physical positions do not follow subject history order",
        ));
    }
    let receipt = &record.receipt;
    if receipt.record_id != record.entry.record_id()
        || receipt.subject != *subject
        || receipt.kind != record.entry.kind()
        || receipt.revision != record.entry.revision()
        || receipt.position != record.position
    {
        return Err(corrupt(
            subject,
            "record receipt components do not reproduce the stored record",
        ));
    }
    receipt.batch_key.validate().map_err(|error| {
        corrupt(
            subject,
            format!("record receipt carries an invalid key: {error}"),
        )
    })?;
    if let super::BatchKey::SingleRecord(record_id) = &receipt.batch_key {
        if record_id != &receipt.record_id || receipt.member_index != 0 {
            return Err(corrupt(
                subject,
                "single-record receipt does not reproduce its record identity and zero index",
            ));
        }
    }
    let encoded = record_comparison_bytes(&record.entry).map_err(|error| {
        corrupt(
            subject,
            format!("complete record cannot be encoded for comparison: {error}"),
        )
    })?;
    if encoded != record.record_bytes {
        return Err(corrupt(
            subject,
            "stored er.record/1 comparison bytes differ from the complete record",
        ));
    }
    let request = original_request_comparison_bytes(&record.entry).map_err(|error| {
        corrupt(
            subject,
            format!("complete record cannot reproduce its request bytes: {error}"),
        )
    })?;
    if request != record.request_bytes {
        return Err(corrupt(
            subject,
            "stored er.request/1 comparison bytes differ from the complete record",
        ));
    }
    Ok(())
}

/// Verifies one complete mixed subject history against its supplied terminal state.
///
/// # Errors
///
/// Typed corruption for any coordinate, replay, observation, or materialization mismatch.
pub fn verify_subject_history(
    history: &SubjectHistory,
    terminal: &EntityInstance,
) -> Result<SubjectAssurance, AsyncStoreError> {
    history.subject.validate()?;
    validate_imported_boundary(history)?;
    let mut current = match &history.origin {
        HistoryOrigin::Genesis => None,
        HistoryOrigin::Imported(anchor) => Some(anchor.instance.clone()),
    };
    let mut previous_position = None;
    let mut ids = BTreeSet::new();
    for record in &history.records {
        validate_stored_coordinates(history, record, previous_position)?;
        if !ids.insert(record.entry.record_id()) {
            return Err(corrupt(
                &history.subject,
                "one global record identity repeats within the subject history",
            ));
        }
        current = validate_entry_against_state(&record.entry, record.expect, current.as_ref())
            .map_err(|error| match error {
                AsyncStoreError::CorruptHistory { .. } => error,
                other => corrupt(
                    &history.subject,
                    format!("stored entry does not follow its verified predecessor: {other}"),
                ),
            })?;
        previous_position = Some(record.position);
    }
    let current = current.ok_or_else(|| {
        corrupt(
            &history.subject,
            "genesis history contains no creation decision",
        )
    })?;
    if current != *terminal {
        return Err(corrupt(
            &history.subject,
            "supplied terminal state differs from verified history",
        ));
    }
    Ok(match &history.origin {
        HistoryOrigin::Genesis => SubjectAssurance::VerifiedFromGenesis {
            subject: history.subject.clone(),
        },
        HistoryOrigin::Imported(anchor) => SubjectAssurance::VerifiedAfterBoundary {
            subject: history.subject.clone(),
            anchor_revision: anchor.instance.revision,
        },
    })
}

/// Verifies the immutable prefix through one committed record without consulting current state.
///
/// # Errors
///
/// Typed corruption when the record is absent or any prefix evidence fails.
pub fn verify_subject_prefix(
    history: &SubjectHistory,
    record_id: &str,
) -> Result<SubjectAssurance, AsyncStoreError> {
    let index = history
        .records
        .iter()
        .position(|record| record.entry.record_id() == record_id)
        .ok_or_else(|| {
            corrupt(
                &history.subject,
                format!("record {record_id:?} is absent from its claimed history"),
            )
        })?;
    let mut prefix = history.clone();
    prefix.records.truncate(index + 1);
    let terminal = prefix.records[..=index]
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
        .ok_or_else(|| {
            corrupt(
                &prefix.subject,
                "verified prefix has no state-producing decision",
            )
        })?;
    verify_subject_history(&prefix, &terminal)
}

/// Verifies cross-subject identities, positions, and batch coordinates in one explicit scope.
///
/// # Errors
///
/// Typed corruption for any subject-local or cross-subject inconsistency.
pub fn verify_store_histories(
    snapshot: &CompleteStoreSnapshot,
) -> Result<StoreAssurance, AsyncStoreError> {
    if snapshot.scope.trim().is_empty() {
        return Err(AsyncStoreError::InvalidInput(
            "store verification requires a nonblank logical scope".to_owned(),
        ));
    }
    let mut assurances = Vec::with_capacity(snapshot.histories.len());
    let mut subjects = BTreeSet::new();
    let mut record_ids: BTreeMap<String, Subject> = BTreeMap::new();
    let mut store_positions: BTreeMap<u64, Subject> = BTreeMap::new();
    let mut memberships: BTreeMap<(super::BatchKey, u64), (String, Subject, RecordKind)> =
        BTreeMap::new();
    let mut batch_orders: BTreeMap<super::BatchKey, Vec<(u64, u64, Subject)>> = BTreeMap::new();
    for subject_snapshot in &snapshot.histories {
        let history = &subject_snapshot.history;
        if !subjects.insert(history.subject.clone()) {
            return Err(corrupt(
                &history.subject,
                "subject appears more than once in the supplied scope",
            ));
        }
        assurances.push(verify_subject_history(history, &subject_snapshot.terminal)?);
        if let HistoryOrigin::Imported(anchor) = &history.origin {
            for evidence in &anchor.evidence {
                if let LegacyEvidence::Envelope(envelope) = evidence {
                    if record_ids
                        .insert(
                            envelope.entry.record_id().to_owned(),
                            history.subject.clone(),
                        )
                        .is_some()
                    {
                        return Err(corrupt(
                            &history.subject,
                            "global record identity repeats across supplied histories",
                        ));
                    }
                }
            }
        }
        for record in &history.records {
            if record_ids
                .insert(record.entry.record_id().to_owned(), history.subject.clone())
                .is_some()
            {
                return Err(corrupt(
                    &history.subject,
                    "global record identity repeats across supplied histories",
                ));
            }
            if store_positions
                .insert(record.position.store, history.subject.clone())
                .is_some()
            {
                return Err(corrupt(
                    &history.subject,
                    "store physical position repeats across supplied histories",
                ));
            }
            let coordinate = (
                record.receipt.batch_key.clone(),
                record.receipt.member_index,
            );
            let components = (
                record.receipt.record_id.clone(),
                record.receipt.subject.clone(),
                record.receipt.kind,
            );
            if memberships
                .insert(coordinate, components.clone())
                .is_some_and(|prior| prior != components)
            {
                return Err(corrupt(
                    &history.subject,
                    "one batch member coordinate names inconsistent records",
                ));
            }
            batch_orders
                .entry(record.receipt.batch_key.clone())
                .or_default()
                .push((
                    record.receipt.member_index,
                    record.position.store,
                    history.subject.clone(),
                ));
        }
    }
    for members in batch_orders.values_mut() {
        members.sort_by_key(|(index, _, _)| *index);
        for adjacent in members.windows(2) {
            if adjacent[0].1 >= adjacent[1].1 {
                return Err(corrupt(
                    &adjacent[1].2,
                    "batch member order disagrees with store physical order",
                ));
            }
        }
    }
    assurances.sort_by(|left, right| assurance_subject(left).cmp(assurance_subject(right)));
    Ok(StoreAssurance {
        scope: snapshot.scope.clone(),
        coverage: snapshot.coverage,
        subjects: assurances,
    })
}

fn assurance_subject(assurance: &SubjectAssurance) -> &Subject {
    match assurance {
        SubjectAssurance::VerifiedFromGenesis { subject }
        | SubjectAssurance::VerifiedAfterBoundary { subject, .. } => subject,
    }
}
