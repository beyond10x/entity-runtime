use std::{
    borrow::Borrow,
    collections::{BTreeMap, BTreeSet},
};

use entity_core::{
    recompute_before_load, recompute_create, CoreError, DecisionCommand, EntityInstance,
    LoadedDecision, PreloadDecision, ValidatedDefinition,
};
use serde_json::Value;

use crate::Expect;

use super::{
    original_request_comparison_bytes, record_comparison_bytes, AsyncStoreError,
    CompleteStoreSnapshot, HistoryOrigin, KnownLegacyOrder, LegacyEvidence, LegacyOrderDeclaration,
    RecordKind, RecordedEntry, StoreAssurance, StoreCoverage, StoredRecord, Subject,
    SubjectAssurance, SubjectHistory,
};

fn corrupt(subject: &Subject, detail: impl Into<String>) -> AsyncStoreError {
    AsyncStoreError::CorruptHistory {
        subject: subject.clone(),
        detail: detail.into(),
    }
}

/// The parts of one subject history a verifier reads, however its reader holds the records.
///
/// [`SubjectHistory`] owns its records. A reader that keeps each record once and shares it between
/// several indexes holds them behind a pointer instead, and copying every record into a
/// `SubjectHistory` only to verify it would cost exactly the copy the sharing exists to avoid.
/// Every history verifier here reads through this view, so an owned history and a shared one are
/// verified by the same code, check for check.
struct HistoryView<'a, R> {
    subject: &'a Subject,
    origin: &'a HistoryOrigin,
    records: &'a [R],
}

impl<R> Clone for HistoryView<'_, R> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<R> Copy for HistoryView<'_, R> {}

impl<'a> HistoryView<'a, StoredRecord> {
    fn of(history: &'a SubjectHistory) -> Self {
        Self {
            subject: &history.subject,
            origin: &history.origin,
            records: &history.records,
        }
    }
}

impl<'a, R: Borrow<StoredRecord>> HistoryView<'a, R> {
    fn record(self, index: usize) -> &'a StoredRecord {
        self.records[index].borrow()
    }

    fn iter(self) -> impl Iterator<Item = &'a StoredRecord> {
        self.records.iter().map(Borrow::borrow)
    }
}

/// The state a history's records continue from: none at genesis, the anchor's after an import.
fn origin_state(origin: &HistoryOrigin) -> Option<EntityInstance> {
    match origin {
        HistoryOrigin::Genesis => None,
        HistoryOrigin::Imported(anchor) => Some(anchor.instance.clone()),
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

/// Validates one complete entry against its transaction-local predecessor.
///
/// This is the pure admission seam used by recorded providers. It applies the same expectation,
/// saved-command replay, observation, and revision checks as history verification without doing IO.
///
/// # Errors
///
/// A typed input, revision, or history-integrity refusal.
pub fn validate_entry_against_state(
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
            // A stored decision is recomputed as `replay` recomputes one: its response is compared
            // with the record, not validated against its declared schema, so a decision answered
            // before responses were checked (R-169) still verifies.
            let recomputed = match (&record.command, current) {
                // A `service/1` creation is rerun from the **arguments** it was decided on, which
                // is what re-selects its branch; a `kernel/1` creation's input is its fields and it
                // records no arguments, so this is the call it has always made. The test is the
                // definition's semantics alone, which is the same test `record_domain` and `replay`
                // make: a `service/1` creation that declares no branches records its own input as
                // its arguments, so all three read one answer.
                (DecisionCommand::Create { fields, arguments }, None) => {
                    let input = if definition.semantics.has_service_semantics() {
                        arguments.clone()
                    } else {
                        fields.clone()
                    };
                    recompute_create(&definition, record.id.clone(), Value::Object(input))
                }
                (
                    DecisionCommand::Execute {
                        operation,
                        arguments,
                        fulfillments,
                    },
                    Some(current),
                ) => (|| {
                    let prepared = match recompute_before_load(
                        &definition,
                        record.id.clone(),
                        operation,
                        Value::Object(arguments.clone()),
                    )? {
                        PreloadDecision::Load(prepared) => prepared,
                        PreloadDecision::Refused(refusal) => {
                            return Err(CoreError::Refused {
                                outcome: refusal.outcome,
                                error: refusal.error,
                                message: refusal.message,
                            })
                        }
                    };
                    match prepared.select_with(current)? {
                        LoadedDecision::Complete(evaluation) => evaluation.into_decision(),
                        LoadedDecision::NeedsFulfillment(prepared) => {
                            prepared.complete(fulfillments.clone())?.into_decision()
                        }
                    }
                })(),
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

/// The ordering scope an imported envelope's known position is counted in.
#[derive(Clone, Copy)]
enum OrderScope {
    /// `PerKind`: decisions and observations each count from their own start.
    Kind(RecordKind),
    /// `Subject`: one mixed sequence for the whole subject.
    Subject,
}

impl OrderScope {
    /// A sortable key for the scope; `RecordKind` itself is not ordered.
    fn rank(self) -> u8 {
        match self {
            Self::Kind(RecordKind::Decision) => 0,
            Self::Kind(RecordKind::Observation) => 1,
            Self::Subject => 2,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Kind(RecordKind::Decision) => "decision",
            Self::Kind(RecordKind::Observation) => "observation",
            Self::Subject => "subject",
        }
    }
}

pub(crate) fn validate_imported_boundary(history: &SubjectHistory) -> Result<(), AsyncStoreError> {
    validate_imported_boundary_of(&history.subject, &history.origin)
}

fn validate_imported_boundary_of(
    subject: &Subject,
    origin: &HistoryOrigin,
) -> Result<(), AsyncStoreError> {
    let HistoryOrigin::Imported(anchor) = origin else {
        return Ok(());
    };
    if anchor.instance.entity != subject.entity || anchor.instance.id != subject.id {
        return Err(corrupt(
            subject,
            "anchor instance belongs to another subject",
        ));
    }
    validate_revision(subject, anchor.instance.revision)?;
    let mut ids = BTreeSet::new();
    let mut positions = BTreeSet::new();
    for evidence in &anchor.evidence {
        match evidence {
            LegacyEvidence::Envelope(envelope) => {
                envelope.entry.validate().map_err(|error| {
                    corrupt(subject, format!("imported envelope is invalid: {error}"))
                })?;
                validate_evidence_revision(
                    subject,
                    envelope.entry.revision(),
                    anchor.instance.revision,
                    "imported envelope",
                )?;
                if envelope.entry.subject() != *subject {
                    return Err(corrupt(subject, "imported envelope names another subject"));
                }
                if let RecordedEntry::Decision(commit) = &envelope.entry {
                    validate_bare_decision(
                        subject,
                        &commit.envelope.record,
                        anchor.instance.revision,
                    )?;
                    validate_anchor_result(subject, &anchor.instance, &commit.envelope.record)?;
                }
                if envelope.source_id.trim().is_empty() || envelope.source_locator.trim().is_empty()
                {
                    return Err(corrupt(
                        subject,
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
                        subject,
                        "imported envelope order contradicts the anchor declaration",
                    ));
                }
                if !ids.insert(envelope.entry.record_id()) {
                    return Err(corrupt(
                        subject,
                        "an imported record identity appears more than once",
                    ));
                }
                // A known position orders its record against every other record of the same
                // scope, so two records claiming one position leave that pair unordered. Only
                // reuse is refused: a source with partial evidence keeps its gaps, and a decision
                // and an observation may share a per-kind index because they count separately.
                let (scope, position) = match envelope.known_order {
                    KnownLegacyOrder::PerKind(index) => {
                        (OrderScope::Kind(envelope.entry.kind()), index)
                    }
                    KnownLegacyOrder::Subject(position) => (OrderScope::Subject, position),
                };
                if !positions.insert((scope.rank(), position)) {
                    return Err(corrupt(
                        subject,
                        format!(
                            "two imported records claim {} position {position}",
                            scope.name()
                        ),
                    ));
                }
            }
            LegacyEvidence::Decision(decision) => {
                validate_bare_decision(subject, decision, anchor.instance.revision)?;
                validate_anchor_result(subject, &anchor.instance, decision)?;
            }
            LegacyEvidence::Event(event) => {
                validate_legacy_event(subject, event, anchor.instance.revision)?;
            }
        }
    }
    Ok(())
}

fn validate_anchor_result(
    subject: &Subject,
    anchor: &EntityInstance,
    decision: &entity_core::DecisionRecord,
) -> Result<(), AsyncStoreError> {
    if decision.revision == anchor.revision && decision.result != *anchor {
        return Err(corrupt(
            subject,
            "the latest available imported decision differs from the terminal anchor",
        ));
    }
    Ok(())
}

fn validate_evidence_revision(
    subject: &Subject,
    revision: u64,
    anchor_revision: u64,
    evidence_kind: &str,
) -> Result<(), AsyncStoreError> {
    validate_revision(subject, revision)?;
    if revision > anchor_revision {
        return Err(corrupt(
            subject,
            format!(
                "{evidence_kind} revision {revision} is later than anchor revision {anchor_revision}"
            ),
        ));
    }
    Ok(())
}

fn validate_bare_decision(
    subject: &Subject,
    decision: &entity_core::DecisionRecord,
    anchor_revision: u64,
) -> Result<(), AsyncStoreError> {
    validate_evidence_revision(
        subject,
        decision.revision,
        anchor_revision,
        "imported decision",
    )?;
    if decision.entity != subject.entity
        || decision.id != subject.id
        || decision.result.entity != subject.entity
        || decision.result.id != subject.id
        || decision.result.revision != decision.revision
        || decision.result.lifecycle_state != decision.to_state
    {
        return Err(corrupt(
            subject,
            "imported decision components do not reproduce its subject, revision, and result",
        ));
    }
    for event in &decision.events {
        validate_legacy_event(subject, event, anchor_revision)?;
        if event.revision != decision.revision {
            return Err(corrupt(
                subject,
                "imported decision event names another decision revision",
            ));
        }
    }
    Ok(())
}

fn validate_legacy_event(
    subject: &Subject,
    event: &entity_core::DomainEvent,
    anchor_revision: u64,
) -> Result<(), AsyncStoreError> {
    validate_evidence_revision(subject, event.revision, anchor_revision, "imported event")?;
    if event.entity != subject.entity || event.id != subject.id {
        return Err(corrupt(subject, "imported event names another subject"));
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
    subject: &Subject,
    record: &StoredRecord,
    previous_position: Option<super::RecordPosition>,
    bytes_held: bool,
) -> Result<(), AsyncStoreError> {
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
    if bytes_held {
        return Ok(());
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
    verify_whole(HistoryView::of(history), terminal)
}

/// [`verify_subject_history`] of a history however its records are held.
fn verify_whole<R: Borrow<StoredRecord>>(
    history: HistoryView<'_, R>,
    terminal: &EntityInstance,
) -> Result<SubjectAssurance, AsyncStoreError> {
    let origin = origin_state(history.origin);
    if branched(history) {
        return verify_branched(history, origin, terminal);
    }
    verify_records_from(history, 0, origin, terminal, false)
}

/// Whether the history comes from a store whose records carry their lineage.
fn branched<R: Borrow<StoredRecord>>(history: HistoryView<'_, R>) -> bool {
    history.iter().any(|record| record.lineage.is_some())
}

/// Each branch head of a history whose records carry their lineage, with the state it reached.
///
/// A head is a decision no other decision follows, in digest order: only a record that decides a
/// state can start or keep a branch. An observation hangs off the decision it observed and is never
/// a head, so a history in which one branch observed a state and another decided on it, or in
/// which two branches only observed one state, has one head. A linear history has one. An imported
/// history whose records are all observations has the anchor's digest as its one head.
///
/// # Errors
///
/// Every check [`verify_subject_history`] makes of a branched history, except that more than one
/// head is an answer here rather than [`AsyncStoreError::Forked`].
pub fn branch_heads(
    history: &SubjectHistory,
) -> Result<Vec<(String, Option<EntityInstance>)>, AsyncStoreError> {
    let walk = walk_branches(HistoryView::of(history), origin_state(&history.origin))?;
    Ok(walk
        .heads
        .into_iter()
        .map(|head| {
            let state = walk.states.get(head.as_str()).cloned().flatten();
            (head, state)
        })
        .collect())
}

/// The digests of the records no other record follows, observations included, in digest order.
///
/// These are the heads a branchable provider holds the subject's stream to, which differ from
/// [`branch_heads`] once an observation was merged beside another record: the provider extends a
/// stream with more than one of them only by an append that names every one, even when the
/// history has one decision head and has not forked. A history with no lineage has none.
#[must_use]
pub fn branch_tips(history: &SubjectHistory) -> Vec<String> {
    branch_tips_of_records(&history.records)
}

/// [`branch_tips`] of a subject's records, however its reader holds them.
///
/// Not part of the documented surface: it exists for the Eventlog adapter, whose verified model
/// keeps each committed record once and shares it between its record, history and batch indexes,
/// so it holds no [`SubjectHistory`] to pass without copying every record of the subject first.
/// Any other reader holds a `SubjectHistory` and calls [`branch_tips`], which answers the same.
#[doc(hidden)]
#[must_use]
pub fn branch_tips_of_records<R: Borrow<StoredRecord>>(records: &[R]) -> Vec<String> {
    let lineages = || {
        records
            .iter()
            .filter_map(|record| record.borrow().lineage.as_deref())
    };
    let followed: BTreeSet<&str> = lineages()
        .flat_map(|lineage| lineage.parents.iter().map(String::as_str))
        .collect();
    let tips: BTreeSet<&str> = lineages()
        .map(|lineage| lineage.digest.as_str())
        .filter(|digest| !followed.contains(digest))
        .collect();
    tips.into_iter().map(str::to_owned).collect()
}

/// A branched history replayed record by record.
struct BranchWalk {
    /// Every record's resulting state by digest, and the imported anchor's under its digest.
    states: BTreeMap<String, Option<EntityInstance>>,
    /// The decisions no other decision follows, in digest order.
    heads: Vec<String>,
}

/// Replays a branched history record by record on the state of the decisions it follows.
///
/// Each record is replayed on the state of the decision it follows, not on the record before it in
/// position order: two branches that each decided on one state both follow that state. An
/// observation is followed through to the decision it observed, so a record whose parents are
/// that decision's observations — or that decision and an observation of an earlier one — follows
/// that one decision. A record that follows more than one decision, none descending from another,
/// is a merge decision. It is decided on one of their states at the highest revision any of them
/// reached, so its own revision passes every branch it joins. An observation cannot join
/// branches.
fn walk_branches<R: Borrow<StoredRecord>>(
    history: HistoryView<'_, R>,
    origin: Option<EntityInstance>,
) -> Result<BranchWalk, AsyncStoreError> {
    let subject = history.subject;
    subject.validate()?;
    validate_imported_boundary_of(subject, history.origin)?;
    let mut states: BTreeMap<String, Option<EntityInstance>> = BTreeMap::new();
    // For each record, the decisions it sits on: itself for a decision, and for an observation the
    // decision it observed. The imported anchor is a decision here, under its own digest.
    let mut sits_on: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // For each decision, the decisions it directly follows. Keeping every decision's whole ancestor
    // set instead held N(N−1)/2 entries for a chain of N decisions; the full ancestry is walked only
    // where a record follows more than one decision and the walk is the question.
    let mut parents: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut decisions: BTreeSet<String> = BTreeSet::new();
    let mut ids: BTreeSet<&str> = BTreeSet::new();
    let mut anchor_digest: Option<String> = None;
    let mut previous_position = None;
    for record in history.iter() {
        validate_stored_coordinates(subject, record, previous_position, false)?;
        if !ids.insert(record.entry.record_id()) {
            return Err(corrupt(
                subject,
                "one global record identity repeats within the subject history",
            ));
        }
        let lineage = record.lineage.as_ref().ok_or_else(|| {
            corrupt(
                subject,
                "a branched history holds a record that names no lineage",
            )
        })?;
        let mut on: Vec<String> = Vec::new();
        for parent in &lineage.parents {
            let sits = match sits_on.get(parent.as_str()) {
                Some(sits) => sits.clone(),
                // An imported history's anchor is stored beside its records, not among them:
                // the one digest a record follows that no record carries is the anchor's.
                None if matches!(history.origin, HistoryOrigin::Imported(_))
                    && anchor_digest.get_or_insert_with(|| parent.clone()) == parent =>
                {
                    decisions.insert(parent.clone());
                    parents.entry(parent.clone()).or_default();
                    vec![parent.clone()]
                }
                None => {
                    return Err(corrupt(
                        subject,
                        "a record follows one that is not an earlier record of its subject",
                    ));
                }
            };
            for decision in sits {
                if !on.contains(&decision) {
                    on.push(decision);
                }
            }
        }
        // A decision another one this record follows descends from is not a branch it joins. One
        // followed decision joins nothing, so only a record that follows several walks ancestry.
        let on: Vec<String> = if on.len() < 2 {
            on
        } else {
            on.iter()
                .filter(|decision| {
                    !on.iter()
                        .any(|other| other != *decision && descends_from(&parents, other, decision))
                })
                .cloned()
                .collect()
        };
        let state_of = |decision: &String| match states.get(decision) {
            Some(state) => state.clone(),
            None => origin.clone(),
        };
        let is_decision = matches!(record.entry, RecordedEntry::Decision(_));
        let mut merged = None;
        let current = if on.is_empty() {
            origin.clone()
        } else if on.len() == 1 {
            state_of(&on[0])
        } else if !is_decision {
            return Err(corrupt(
                subject,
                "an observation follows branches only a merge decision may join",
            ));
        } else {
            let bases: Vec<Option<EntityInstance>> = on.iter().map(state_of).collect();
            let highest = bases.iter().flatten().map(|state| state.revision).max();
            let raised = |state: &Option<EntityInstance>| {
                state.clone().map(|mut state| {
                    if let Some(revision) = highest {
                        state.revision = revision;
                    }
                    state
                })
            };
            // A merge decision was decided on one of its heads, at the highest revision any
            // reached. Which one is not in the lineage, whose parents a store may keep in any
            // order, so the head it reproduces from is the one it was decided on.
            merged = bases.iter().map(raised).find_map(|base| {
                validate_entry_against_state(&record.entry, record.expect, base.as_ref()).ok()
            });
            raised(&bases[0])
        };
        let next = match merged {
            Some(next) => next,
            None => validate_entry_against_state(&record.entry, record.expect, current.as_ref())
                .map_err(|error| match error {
                    AsyncStoreError::CorruptHistory { .. } => error,
                    other => corrupt(
                        subject,
                        format!("stored entry does not follow its verified parent: {other}"),
                    ),
                })?,
        };
        if states.insert(lineage.digest.clone(), next).is_some() {
            return Err(corrupt(
                subject,
                "one record digest repeats within the subject history",
            ));
        }
        if is_decision {
            parents.insert(lineage.digest.clone(), on);
            decisions.insert(lineage.digest.clone());
            sits_on.insert(lineage.digest.clone(), vec![lineage.digest.clone()]);
        } else {
            sits_on.insert(lineage.digest.clone(), on);
        }
        previous_position = Some(record.position);
    }
    // A decision another decision descends from is followed directly by some decision on the way,
    // so the heads are the decisions no decision directly follows.
    let followed: BTreeSet<&String> = parents.values().flatten().collect();
    let heads = decisions
        .iter()
        .filter(|decision| !followed.contains(decision))
        .cloned()
        .collect();
    #[cfg(test)]
    RETAINED_PARENT_ENTRIES.with(|retained| retained.set(parents.values().map(Vec::len).sum()));
    if let Some(anchor) = anchor_digest {
        states.entry(anchor).or_insert(origin);
    }
    Ok(BranchWalk { states, heads })
}

#[cfg(test)]
thread_local! {
    /// The parent entries the last branch walk on this thread retained, for the tests that hold a
    /// chain of N decisions to N − 1.
    static RETAINED_PARENT_ENTRIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Decisions visited by ancestry walks on this thread.
    static ANCESTRY_STEPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Whether `decision` descends from `ancestor` through the decisions each one directly follows.
fn descends_from(parents: &BTreeMap<String, Vec<String>>, decision: &str, ancestor: &str) -> bool {
    let mut pending: Vec<&str> = vec![decision];
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    while let Some(next) = pending.pop() {
        #[cfg(test)]
        ANCESTRY_STEPS.with(|steps| steps.set(steps.get() + 1));
        for parent in parents.get(next).into_iter().flatten() {
            if parent == ancestor {
                return true;
            }
            if seen.insert(parent.as_str()) {
                pending.push(parent.as_str());
            }
        }
    }
    false
}

/// Verifies a history whose records name the records they follow, as a store merged under
/// version control keeps them. A history with more than one decision head has forked, and has no
/// one state until a merge joins it; observations beside a decision do not fork it.
///
/// # Errors
///
/// [`AsyncStoreError::Forked`] with the heads for an unjoined fork; typed corruption for a record
/// with no lineage, a parent that is not an earlier record of the subject, a repeated digest, and
/// every check the linear verification makes.
fn verify_branched<R: Borrow<StoredRecord>>(
    history: HistoryView<'_, R>,
    origin: Option<EntityInstance>,
    terminal: &EntityInstance,
) -> Result<SubjectAssurance, AsyncStoreError> {
    let subject = history.subject;
    let BranchWalk { states, heads } = walk_branches(history, origin.clone())?;
    if heads.len() > 1 {
        return Err(AsyncStoreError::Forked {
            subject: subject.clone(),
            heads,
        });
    }
    let current = heads
        .first()
        .and_then(|head| states.get(head).cloned().flatten())
        .or(origin)
        .ok_or_else(|| corrupt(subject, "genesis history contains no creation decision"))?;
    if current != *terminal {
        return Err(corrupt(
            subject,
            "supplied terminal state differs from verified history",
        ));
    }
    Ok(match history.origin {
        HistoryOrigin::Genesis => SubjectAssurance::VerifiedFromGenesis {
            subject: subject.clone(),
        },
        HistoryOrigin::Imported(anchor) => SubjectAssurance::VerifiedAfterBoundary {
            subject: subject.clone(),
            anchor_revision: anchor.instance.revision,
        },
    })
}

/// Verifies the records a subject history gained after its first `verified` records.
///
/// A reader that already verified `history.records[..verified]` with [`verify_subject_history`]
/// — or with this function — and reached `verified_state` has established every check that
/// prefix can fail, and replaying it again would only repeat them: decoding, re-encoding and
/// re-deciding each of its records. This runs exactly those checks on the suffix, continuing from
/// the verified state and the last verified position, and holds the result to `terminal` as the
/// whole verification does. The caller is responsible for the prefix being the one it verified;
/// with `verified == 0` this is [`verify_subject_history`].
///
/// # Errors
///
/// Typed corruption for any coordinate, replay, observation, or materialization mismatch in the
/// suffix, or a terminal the suffix does not reach.
pub fn verify_subject_history_extension(
    history: &SubjectHistory,
    verified: usize,
    verified_state: &EntityInstance,
    terminal: &EntityInstance,
) -> Result<SubjectAssurance, AsyncStoreError> {
    let view = HistoryView::of(history);
    // A branched history is verified whole: a merge can place a record from one branch before
    // records a verified prefix already held, so no prefix of it is settled.
    if verified == 0 || verified > history.records.len() || branched(view) {
        return verify_subject_history(history, terminal);
    }
    verify_records_from(
        view,
        verified,
        Some(verified_state.clone()),
        terminal,
        false,
    )
}

/// Unsound unless the caller has already checked every stored record's record and request bytes.
///
/// Not part of the documented surface: it exists for the Eventlog adapter, whose reads decode
/// every record from its bound blob and refuse one that does not reproduce it. Any other reader
/// must call [`verify_subject_history`] or [`verify_subject_history_extension`], which compare
/// those bytes themselves. Passing a history whose bytes were not checked admits forged record
/// and request bytes on the caller's word.
///
/// [`verify_subject_history_extension`] — or [`verify_subject_history`] without a verified
/// prefix — for a reader that decoded every stored record of `history` from exactly its
/// `record_bytes`, and has already held those bytes to be [`record_comparison_bytes`] of the entry
/// they decoded to and its `request_bytes` to be [`original_request_comparison_bytes`] of it,
/// refusing the record otherwise.
///
/// Those two comparisons are then the only checks not made again: re-encoding each record twice
/// more to compare bytes the reader has just compared is most of what verifying a record that
/// embeds a large definition costs. Every coordinate, receipt, identity, replay, observation and
/// terminal check is made exactly as the verifier it stands for makes it, and a branched history
/// is verified whole by [`verify_subject_history`], comparisons included. A reader that has not
/// held a record's bytes itself must call one of those two instead: this function cannot tell.
///
/// # Errors
///
/// Every refusal [`verify_subject_history_extension`] makes, except that it does not re-compare the
/// two byte strings the caller has already held.
#[doc(hidden)]
pub fn verify_subject_history_with_checked_bytes(
    history: &SubjectHistory,
    verified: Option<(usize, &EntityInstance)>,
    terminal: &EntityInstance,
) -> Result<SubjectAssurance, AsyncStoreError> {
    verify_with_checked_bytes(HistoryView::of(history), verified, terminal)
}

/// Unsound unless the caller has already checked every stored record's record and request bytes.
///
/// [`verify_subject_history_with_checked_bytes`] of a history its reader holds in parts — the
/// subject, the origin and the records in physical order, each behind whatever pointer the reader
/// shares it through — with exactly the same checks, the same precondition and the same refusals.
///
/// Not part of the documented surface: it exists for the Eventlog adapter, whose verified model
/// keeps each committed record once and shares that one allocation between its record, history and
/// batch indexes. It holds no [`SubjectHistory`] to pass, and assembling one would copy every
/// record of the subject — decoded definition and both byte strings — on every verification. Any
/// other reader must call [`verify_subject_history`] or [`verify_subject_history_extension`].
///
/// # Errors
///
/// Every refusal [`verify_subject_history_with_checked_bytes`] makes of the same history.
#[doc(hidden)]
pub fn verify_shared_subject_history_with_checked_bytes<R: Borrow<StoredRecord>>(
    subject: &Subject,
    origin: &HistoryOrigin,
    records: &[R],
    verified: Option<(usize, &EntityInstance)>,
    terminal: &EntityInstance,
) -> Result<SubjectAssurance, AsyncStoreError> {
    verify_with_checked_bytes(
        HistoryView {
            subject,
            origin,
            records,
        },
        verified,
        terminal,
    )
}

fn verify_with_checked_bytes<R: Borrow<StoredRecord>>(
    history: HistoryView<'_, R>,
    verified: Option<(usize, &EntityInstance)>,
    terminal: &EntityInstance,
) -> Result<SubjectAssurance, AsyncStoreError> {
    if branched(history) {
        return verify_whole(history, terminal);
    }
    match verified {
        Some((records, state)) if records > 0 && records <= history.records.len() => {
            verify_records_from(history, records, Some(state.clone()), terminal, true)
        }
        _ => verify_records_from(history, 0, origin_state(history.origin), terminal, true),
    }
}

fn verify_records_from<R: Borrow<StoredRecord>>(
    history: HistoryView<'_, R>,
    start: usize,
    mut current: Option<EntityInstance>,
    terminal: &EntityInstance,
    bytes_held: bool,
) -> Result<SubjectAssurance, AsyncStoreError> {
    let subject = history.subject;
    subject.validate()?;
    validate_imported_boundary_of(subject, history.origin)?;
    let mut previous_position = start
        .checked_sub(1)
        .map(|last| history.record(last).position);
    let mut ids: BTreeSet<&str> = history.records[..start]
        .iter()
        .map(|record| record.borrow().entry.record_id())
        .collect();
    for record in history.records[start..].iter().map(Borrow::borrow) {
        validate_stored_coordinates(subject, record, previous_position, bytes_held)?;
        if !ids.insert(record.entry.record_id()) {
            return Err(corrupt(
                subject,
                "one global record identity repeats within the subject history",
            ));
        }
        current = validate_entry_against_state(&record.entry, record.expect, current.as_ref())
            .map_err(|error| match error {
                AsyncStoreError::CorruptHistory { .. } => error,
                other => corrupt(
                    subject,
                    format!("stored entry does not follow its verified predecessor: {other}"),
                ),
            })?;
        previous_position = Some(record.position);
    }
    let current =
        current.ok_or_else(|| corrupt(subject, "genesis history contains no creation decision"))?;
    if current != *terminal {
        return Err(corrupt(
            subject,
            "supplied terminal state differs from verified history",
        ));
    }
    Ok(match history.origin {
        HistoryOrigin::Genesis => SubjectAssurance::VerifiedFromGenesis {
            subject: subject.clone(),
        },
        HistoryOrigin::Imported(anchor) => SubjectAssurance::VerifiedAfterBoundary {
            subject: subject.clone(),
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
    // The prefix is read in place: copying the history only to truncate it cloned every record of
    // the subject, the suffix included, on every call.
    let prefix = HistoryView {
        subject: &history.subject,
        origin: &history.origin,
        records: &history.records[..=index],
    };
    let terminal = prefix
        .records
        .iter()
        .rev()
        .find_map(|record| match &record.entry {
            RecordedEntry::Decision(commit) => Some(commit.instance.clone()),
            RecordedEntry::Observation(_) => None,
        })
        .or_else(|| match prefix.origin {
            HistoryOrigin::Imported(anchor) => Some(anchor.instance.clone()),
            HistoryOrigin::Genesis => None,
        })
        .ok_or_else(|| {
            corrupt(
                prefix.subject,
                "verified prefix has no state-producing decision",
            )
        })?;
    verify_whole(prefix, &terminal)
}

/// Verifies cross-subject identities, positions, and batch coordinates in one explicit scope.
///
/// # Errors
///
/// Typed corruption for any subject-local or cross-subject inconsistency.
pub fn verify_store_histories(
    snapshot: &CompleteStoreSnapshot,
) -> Result<StoreAssurance, AsyncStoreError> {
    if snapshot.coverage != StoreCoverage::ExplicitSet {
        return Err(AsyncStoreError::InvalidInput(
            "editable transcripts cannot claim provider-complete coverage; use verify_complete_store"
                .to_owned(),
        ));
    }
    verify_histories(
        &snapshot.scope,
        snapshot.histories.iter().map(snapshot_parts),
        StoreCoverage::ExplicitSet,
    )
}

/// [`verify_store_histories`] of an explicit set held as borrowed histories and terminal states.
///
/// A caller that owns its histories and keeps them after verifying, as the legacy acquisition
/// does, would otherwise copy every history and terminal state into a [`CompleteStoreSnapshot`]
/// only to verify it.
pub(crate) fn verify_explicit_histories<'a>(
    scope: &str,
    histories: impl IntoIterator<Item = (&'a SubjectHistory, &'a EntityInstance)>,
) -> Result<StoreAssurance, AsyncStoreError> {
    verify_histories(scope, histories, StoreCoverage::ExplicitSet)
}

fn snapshot_parts(snapshot: &super::SubjectSnapshot) -> (&SubjectHistory, &EntityInstance) {
    (&snapshot.history, &snapshot.terminal)
}

/// Obtains and verifies one fresh provider-owned complete snapshot.
///
/// Completeness provenance is the direct invocation of the provider port, whose implementation
/// promises one consistently captured logical scope. The returned capture is checked before it is
/// exposed for editing, and only this path can return [`StoreCoverage::CompleteSnapshot`].
///
/// # Errors
///
/// Provider refusal, scope substitution, or any subject-local or cross-subject inconsistency.
pub async fn verify_complete_store(
    reader: &dyn super::AsyncRecordedReader,
    scope: &str,
) -> Result<StoreAssurance, AsyncStoreError> {
    if scope.trim().is_empty() {
        return Err(AsyncStoreError::InvalidInput(
            "store verification requires a nonblank logical scope".to_owned(),
        ));
    }
    let snapshot = reader.complete_snapshot(scope).await?;
    if snapshot.scope != scope || snapshot.coverage != StoreCoverage::CompleteSnapshot {
        return Err(AsyncStoreError::InvalidInput(
            "provider snapshot did not reproduce the requested scope and complete marker"
                .to_owned(),
        ));
    }
    verify_histories(
        &snapshot.scope,
        snapshot.histories.iter().map(snapshot_parts),
        StoreCoverage::CompleteSnapshot,
    )
}

fn verify_histories<'a>(
    scope: &str,
    histories: impl IntoIterator<Item = (&'a SubjectHistory, &'a EntityInstance)>,
    coverage: StoreCoverage,
) -> Result<StoreAssurance, AsyncStoreError> {
    if scope.trim().is_empty() {
        return Err(AsyncStoreError::InvalidInput(
            "store verification requires a nonblank logical scope".to_owned(),
        ));
    }
    let histories = histories.into_iter();
    let mut assurances = Vec::with_capacity(histories.size_hint().0);
    let mut subjects = BTreeSet::new();
    let mut record_ids: BTreeMap<String, Subject> = BTreeMap::new();
    let mut store_positions: BTreeMap<u64, Subject> = BTreeMap::new();
    let mut memberships: BTreeMap<(super::BatchKey, u64), (String, Subject, RecordKind)> =
        BTreeMap::new();
    let mut batch_orders: BTreeMap<super::BatchKey, Vec<(u64, u64, Subject)>> = BTreeMap::new();
    for (history, terminal) in histories {
        if !subjects.insert(history.subject.clone()) {
            return Err(corrupt(
                &history.subject,
                "subject appears more than once in the supplied scope",
            ));
        }
        assurances.push(verify_subject_history(history, terminal)?);
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
        scope: scope.to_owned(),
        coverage,
        subjects: assurances,
    })
}

fn assurance_subject(assurance: &SubjectAssurance) -> &Subject {
    match assurance {
        SubjectAssurance::VerifiedFromGenesis { subject }
        | SubjectAssurance::VerifiedAfterBoundary { subject, .. } => subject,
    }
}

#[cfg(test)]
mod tests {
    use entity_core::{Decision, Registry, Runtime};
    use serde_json::json;

    use super::*;
    use crate::{
        asynchronous::{
            BatchKey, ImportedRecordEvidence, LegacyAnchor, LegacyCompleteness, Lineage,
            RecordPosition, RecordReceipt,
        },
        RecordedCommit, RecordedObservation, Recording,
    };

    fn recording(record_id: &str) -> Recording {
        Recording {
            record_id: record_id.to_owned(),
            recorded_at: "2026-11-22T00:00:00Z".to_owned(),
            actor: None,
            correlation: None,
            causation: None,
        }
    }

    /// A created and a touched ticket, each as a complete recorded decision, and the touched state.
    fn two_decisions() -> (RecordedEntry, RecordedEntry, EntityInstance) {
        let registry = runtime_registry();
        let runtime = Runtime::new(&registry);
        let created = runtime
            .create("ticket", 1, "one", json!({}))
            .expect("create decides");
        let touched = runtime
            .execute(&created.instance, "touch", json!({}))
            .expect("touch decides");
        let anchor = touched.instance.clone();
        let entry = |decision: Decision, record_id: &str| {
            RecordedEntry::Decision(
                RecordedCommit::new(decision, &recording(record_id)).expect("recording is valid"),
            )
        };
        (
            entry(created, "record-created"),
            entry(touched, "record-touched"),
            anchor,
        )
    }

    fn observed(record_id: &str) -> RecordedEntry {
        RecordedEntry::Observation(RecordedObservation {
            entity: "ticket".to_owned(),
            id: "one".to_owned(),
            revision: 1,
            envelope: recording(record_id)
                .seal(json!({ "seen": true }))
                .expect("recording is valid"),
        })
    }

    fn imported_boundary(
        order: LegacyOrderDeclaration,
        anchor: EntityInstance,
        evidence: Vec<(RecordedEntry, KnownLegacyOrder)>,
    ) -> SubjectHistory {
        SubjectHistory {
            subject: Subject::new("ticket", "one").expect("subject"),
            origin: HistoryOrigin::Imported(LegacyAnchor {
                instance: anchor,
                completeness: LegacyCompleteness::AvailableEvidenceOnly,
                order,
                evidence: evidence
                    .into_iter()
                    .enumerate()
                    .map(|(at, (entry, known_order))| {
                        LegacyEvidence::Envelope(
                            ImportedRecordEvidence::new(
                                entry,
                                "source",
                                format!("records/{at}"),
                                known_order,
                            )
                            .expect("complete imported envelope"),
                        )
                    })
                    .collect(),
            }),
            records: Vec::new(),
        }
    }

    fn refusal_detail(history: &SubjectHistory) -> String {
        match validate_imported_boundary(history) {
            Err(AsyncStoreError::CorruptHistory { detail, .. }) => detail,
            other => panic!("expected a corrupt-history refusal, got {other:?}"),
        }
    }

    #[test]
    fn two_imported_decisions_claiming_one_per_kind_index_are_refused() {
        let (created, touched, anchor) = two_decisions();
        let history = imported_boundary(
            LegacyOrderDeclaration::PerKindOnly,
            anchor,
            vec![
                (created, KnownLegacyOrder::PerKind(0)),
                (touched, KnownLegacyOrder::PerKind(0)),
            ],
        );

        assert_eq!(
            refusal_detail(&history),
            "two imported records claim decision position 0"
        );
    }

    #[test]
    fn two_imported_records_claiming_one_subject_position_are_refused() {
        let (created, touched, anchor) = two_decisions();
        let history = imported_boundary(
            LegacyOrderDeclaration::Subject,
            anchor,
            vec![
                (created, KnownLegacyOrder::Subject(0)),
                (observed("record-seen"), KnownLegacyOrder::Subject(3)),
                (touched, KnownLegacyOrder::Subject(3)),
            ],
        );

        assert_eq!(
            refusal_detail(&history),
            "two imported records claim subject position 3"
        );
    }

    #[test]
    fn imported_positions_keep_their_gaps_and_kinds_count_separately() {
        let (created, touched, anchor) = two_decisions();
        let per_kind = imported_boundary(
            LegacyOrderDeclaration::PerKindOnly,
            anchor.clone(),
            vec![
                (created.clone(), KnownLegacyOrder::PerKind(0)),
                (observed("record-seen"), KnownLegacyOrder::PerKind(0)),
                (touched.clone(), KnownLegacyOrder::PerKind(7)),
            ],
        );
        validate_imported_boundary(&per_kind)
            .expect("a decision and an observation share index 0, and 1 to 6 are simply unknown");

        let subject = imported_boundary(
            LegacyOrderDeclaration::Subject,
            anchor,
            vec![
                (created, KnownLegacyOrder::Subject(2)),
                (observed("record-seen"), KnownLegacyOrder::Subject(5)),
                (touched, KnownLegacyOrder::Subject(9)),
            ],
        );
        validate_imported_boundary(&subject).expect("subject positions with gaps are unique");
    }

    fn runtime_registry() -> Registry {
        let definition = serde_json::from_value(json!({
            "entity": "ticket",
            "version": 1,
            "schema": { "fields": {} },
            "lifecycle": { "initial": "open", "states": ["open"] },
            "operations": {
                "touch": { "transitions": [{ "from": "open", "to": "open" }] }
            }
        }))
        .expect("definition parses");
        let mut registry = Registry::new();
        registry.register(definition).expect("definition validates");
        registry
    }

    fn digest(name: &str) -> String {
        format!("{name:0>64}")
    }

    /// A history built record by record, each naming the digests it follows.
    struct Builder {
        history: SubjectHistory,
    }

    impl Builder {
        fn new() -> Self {
            Self {
                history: SubjectHistory {
                    subject: Subject::new("ticket", "one").expect("subject"),
                    origin: HistoryOrigin::Genesis,
                    records: Vec::new(),
                },
            }
        }

        fn push(&mut self, name: &str, decision: Decision, expect: Expect, parents: &[&str]) {
            let position = self.history.records.len() as u64 + 1;
            let record_id = format!("record-{name}");
            let recording = Recording {
                record_id: record_id.clone(),
                recorded_at: "2026-11-22T00:00:00Z".to_owned(),
                actor: None,
                correlation: None,
                causation: None,
            };
            let entry = RecordedEntry::Decision(
                RecordedCommit::new(decision, &recording).expect("recording is valid"),
            );
            let revision = match &entry {
                RecordedEntry::Decision(commit) => commit.instance.revision,
                RecordedEntry::Observation(_) => unreachable!("only decisions are pushed"),
            };
            let position = RecordPosition {
                subject: position,
                store: position,
            };
            let receipt = RecordReceipt {
                record_id: record_id.clone(),
                subject: self.history.subject.clone(),
                kind: entry.kind(),
                revision,
                position,
                batch_key: BatchKey::SingleRecord(record_id),
                member_index: 0,
            };
            self.history.records.push(StoredRecord {
                request_bytes: original_request_comparison_bytes(&entry).expect("request bytes"),
                record_bytes: record_comparison_bytes(&entry).expect("record bytes"),
                entry,
                position,
                receipt,
                expect,
                lineage: Some(Box::new(Lineage {
                    digest: digest(name),
                    parents: parents.iter().map(|parent| digest(parent)).collect(),
                })),
            });
        }
    }

    fn retained_parent_entries() -> usize {
        RETAINED_PARENT_ENTRIES.with(std::cell::Cell::get)
    }

    fn ancestry_steps() -> usize {
        ANCESTRY_STEPS.with(std::cell::Cell::get)
    }

    fn head_digests(history: &SubjectHistory) -> Vec<String> {
        branch_heads(history)
            .expect("the history verifies")
            .into_iter()
            .map(|(head, _)| head)
            .collect()
    }

    #[test]
    fn a_chain_of_decisions_has_one_head_and_retains_one_parent_entry_per_decision() {
        let registry = runtime_registry();
        let runtime = Runtime::new(&registry);
        let mut builder = Builder::new();
        let mut state = runtime
            .create("ticket", 1, "one", json!({}))
            .expect("create decides");
        builder.push("0", state.clone(), Expect::Absent, &[]);
        let decisions = 64u64;
        for index in 1..decisions {
            let next = runtime
                .execute(&state.instance, "touch", json!({}))
                .expect("touch decides");
            builder.push(
                &index.to_string(),
                next.clone(),
                Expect::Revision(index),
                &[&(index - 1).to_string()],
            );
            state = next;
        }
        let steps_before = ancestry_steps();

        let heads = branch_heads(&builder.history).expect("the chain verifies");

        assert_eq!(heads.len(), 1);
        assert_eq!(heads[0].0, digest(&(decisions - 1).to_string()));
        assert_eq!(heads[0].1.as_ref(), Some(&state.instance));
        // Each decision keeps the one decision it follows: N − 1 entries, where every decision's
        // whole ancestor set held N(N − 1) / 2 = 2016.
        assert_eq!(retained_parent_entries(), 63);
        // No record follows more than one decision, so no ancestry is walked.
        assert_eq!(ancestry_steps() - steps_before, 0);
    }

    #[test]
    fn a_fork_has_both_heads_and_a_merge_joins_them_into_one() {
        let registry = runtime_registry();
        let runtime = Runtime::new(&registry);
        let mut builder = Builder::new();
        let created = runtime
            .create("ticket", 1, "one", json!({}))
            .expect("create decides");
        builder.push("0", created.clone(), Expect::Absent, &[]);
        let left = runtime
            .execute(&created.instance, "touch", json!({}))
            .expect("touch decides");
        builder.push("a", left.clone(), Expect::Revision(1), &["0"]);
        let right = runtime
            .execute(&created.instance, "touch", json!({}))
            .expect("touch decides");
        builder.push("b", right, Expect::Revision(1), &["0"]);

        assert_eq!(
            head_digests(&builder.history),
            vec![digest("a"), digest("b")]
        );

        // The merge is decided on the left head at the highest revision either head reached.
        let mut base = left.instance.clone();
        base.revision = 2;
        let merge = runtime
            .execute(&base, "touch", json!({}))
            .expect("merge decides");
        builder.push("m", merge.clone(), Expect::Revision(2), &["a", "b"]);
        let steps_before = ancestry_steps();

        let heads = branch_heads(&builder.history).expect("the merged history verifies");

        assert_eq!(heads.len(), 1);
        assert_eq!(heads[0].0, digest("m"));
        assert_eq!(heads[0].1.as_ref(), Some(&merge.instance));
        // Validating the merge asks whether either joined head descends from the other.
        assert!(ancestry_steps() > steps_before);
    }

    #[test]
    fn a_record_that_also_names_an_ancestor_of_its_parent_joins_no_branch() {
        let registry = runtime_registry();
        let runtime = Runtime::new(&registry);
        let mut builder = Builder::new();
        let created = runtime
            .create("ticket", 1, "one", json!({}))
            .expect("create decides");
        builder.push("0", created.clone(), Expect::Absent, &[]);
        let left = runtime
            .execute(&created.instance, "touch", json!({}))
            .expect("touch decides");
        builder.push("a", left.clone(), Expect::Revision(1), &["0"]);
        let right = runtime
            .execute(&created.instance, "touch", json!({}))
            .expect("touch decides");
        builder.push("b", right, Expect::Revision(1), &["0"]);
        // Naming "0" beside "a" adds nothing: "a" descends from it, so this follows "a" alone and
        // the fork stays open.
        let next = runtime
            .execute(&left.instance, "touch", json!({}))
            .expect("touch decides");
        builder.push("c", next, Expect::Revision(2), &["a", "0"]);

        assert_eq!(
            head_digests(&builder.history),
            vec![digest("b"), digest("c")]
        );
    }
}
