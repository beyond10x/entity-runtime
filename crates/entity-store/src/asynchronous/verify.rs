use std::collections::{BTreeMap, BTreeSet};

use entity_core::{
    create, decide_before_load, CoreError, DecisionCommand, EntityInstance, LoadedDecision,
    PreloadDecision, ValidatedDefinition,
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
                    create(&definition, record.id.clone(), Value::Object(input))
                }
                (
                    DecisionCommand::Execute {
                        operation,
                        arguments,
                        fulfillments,
                    },
                    Some(current),
                ) => (|| {
                    let prepared = match decide_before_load(
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
        match evidence {
            LegacyEvidence::Envelope(envelope) => {
                envelope.entry.validate().map_err(|error| {
                    corrupt(
                        &history.subject,
                        format!("imported envelope is invalid: {error}"),
                    )
                })?;
                validate_evidence_revision(
                    &history.subject,
                    envelope.entry.revision(),
                    anchor.instance.revision,
                    "imported envelope",
                )?;
                if envelope.entry.subject() != history.subject {
                    return Err(corrupt(
                        &history.subject,
                        "imported envelope names another subject",
                    ));
                }
                if let RecordedEntry::Decision(commit) = &envelope.entry {
                    validate_bare_decision(
                        &history.subject,
                        &commit.envelope.record,
                        anchor.instance.revision,
                    )?;
                    validate_anchor_result(
                        &history.subject,
                        &anchor.instance,
                        &commit.envelope.record,
                    )?;
                }
                if envelope.source_id.trim().is_empty() || envelope.source_locator.trim().is_empty()
                {
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
            LegacyEvidence::Decision(decision) => {
                validate_bare_decision(&history.subject, decision, anchor.instance.revision)?;
                validate_anchor_result(&history.subject, &anchor.instance, decision)?;
            }
            LegacyEvidence::Event(event) => {
                validate_legacy_event(&history.subject, event, anchor.instance.revision)?;
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
    history: &SubjectHistory,
    record: &StoredRecord,
    previous_position: Option<super::RecordPosition>,
    bytes_held: bool,
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
    let origin = match &history.origin {
        HistoryOrigin::Genesis => None,
        HistoryOrigin::Imported(anchor) => Some(anchor.instance.clone()),
    };
    if branched(history) {
        return verify_branched(history, origin, terminal);
    }
    verify_records_from(history, 0, origin, terminal, false)
}

/// Whether the history comes from a store whose records carry their lineage.
fn branched(history: &SubjectHistory) -> bool {
    history
        .records
        .iter()
        .any(|record| record.lineage.is_some())
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
    let origin = match &history.origin {
        HistoryOrigin::Genesis => None,
        HistoryOrigin::Imported(anchor) => Some(anchor.instance.clone()),
    };
    let walk = walk_branches(history, origin)?;
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
    let lineages = || {
        history
            .records
            .iter()
            .filter_map(|record| record.lineage.as_deref())
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
fn walk_branches(
    history: &SubjectHistory,
    origin: Option<EntityInstance>,
) -> Result<BranchWalk, AsyncStoreError> {
    let subject = &history.subject;
    subject.validate()?;
    validate_imported_boundary(history)?;
    let mut states: BTreeMap<String, Option<EntityInstance>> = BTreeMap::new();
    // For each record, the decisions it sits on: itself for a decision, and for an observation the
    // decision it observed. The imported anchor is a decision here, under its own digest.
    let mut sits_on: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // For each decision, every decision it descends from.
    let mut ancestors: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut decisions: BTreeSet<String> = BTreeSet::new();
    let mut ids: BTreeSet<&str> = BTreeSet::new();
    let mut anchor_digest: Option<String> = None;
    let mut previous_position = None;
    for record in &history.records {
        validate_stored_coordinates(history, record, previous_position, false)?;
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
                    ancestors.entry(parent.clone()).or_default();
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
        // A decision another one this record follows descends from is not a branch it joins.
        let on: Vec<String> = on
            .iter()
            .filter(|decision| {
                !on.iter().any(|other| {
                    other != *decision
                        && ancestors
                            .get(other)
                            .is_some_and(|descends| descends.contains(*decision))
                })
            })
            .cloned()
            .collect();
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
            let mut descends: BTreeSet<String> = BTreeSet::new();
            for decision in &on {
                descends.insert(decision.clone());
                if let Some(further) = ancestors.get(decision) {
                    descends.extend(further.iter().cloned());
                }
            }
            ancestors.insert(lineage.digest.clone(), descends);
            decisions.insert(lineage.digest.clone());
            sits_on.insert(lineage.digest.clone(), vec![lineage.digest.clone()]);
        } else {
            sits_on.insert(lineage.digest.clone(), on);
        }
        previous_position = Some(record.position);
    }
    let decided: BTreeSet<&String> = ancestors.values().flatten().collect();
    let heads = decisions
        .iter()
        .filter(|decision| !decided.contains(decision))
        .cloned()
        .collect();
    if let Some(anchor) = anchor_digest {
        states.entry(anchor).or_insert(origin);
    }
    Ok(BranchWalk { states, heads })
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
fn verify_branched(
    history: &SubjectHistory,
    origin: Option<EntityInstance>,
    terminal: &EntityInstance,
) -> Result<SubjectAssurance, AsyncStoreError> {
    let subject = &history.subject;
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
    Ok(match &history.origin {
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
    // A branched history is verified whole: a merge can place a record from one branch before
    // records a verified prefix already held, so no prefix of it is settled.
    if verified == 0 || verified > history.records.len() || branched(history) {
        return verify_subject_history(history, terminal);
    }
    verify_records_from(
        history,
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
    if branched(history) {
        return verify_subject_history(history, terminal);
    }
    match verified {
        Some((records, state)) if records > 0 && records <= history.records.len() => {
            verify_records_from(history, records, Some(state.clone()), terminal, true)
        }
        _ => {
            let origin = match &history.origin {
                HistoryOrigin::Genesis => None,
                HistoryOrigin::Imported(anchor) => Some(anchor.instance.clone()),
            };
            verify_records_from(history, 0, origin, terminal, true)
        }
    }
}

fn verify_records_from(
    history: &SubjectHistory,
    start: usize,
    mut current: Option<EntityInstance>,
    terminal: &EntityInstance,
    bytes_held: bool,
) -> Result<SubjectAssurance, AsyncStoreError> {
    history.subject.validate()?;
    validate_imported_boundary(history)?;
    let mut previous_position = start
        .checked_sub(1)
        .map(|last| history.records[last].position);
    let mut ids: BTreeSet<&str> = history.records[..start]
        .iter()
        .map(|record| record.entry.record_id())
        .collect();
    for record in &history.records[start..] {
        validate_stored_coordinates(history, record, previous_position, bytes_held)?;
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
    if snapshot.coverage != StoreCoverage::ExplicitSet {
        return Err(AsyncStoreError::InvalidInput(
            "editable transcripts cannot claim provider-complete coverage; use verify_complete_store"
                .to_owned(),
        ));
    }
    verify_histories(
        &snapshot.scope,
        &snapshot.histories,
        StoreCoverage::ExplicitSet,
    )
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
        &snapshot.histories,
        StoreCoverage::CompleteSnapshot,
    )
}

fn verify_histories(
    scope: &str,
    histories: &[super::SubjectSnapshot],
    coverage: StoreCoverage,
) -> Result<StoreAssurance, AsyncStoreError> {
    if scope.trim().is_empty() {
        return Err(AsyncStoreError::InvalidInput(
            "store verification requires a nonblank logical scope".to_owned(),
        ));
    }
    let mut assurances = Vec::with_capacity(histories.len());
    let mut subjects = BTreeSet::new();
    let mut record_ids: BTreeMap<String, Subject> = BTreeMap::new();
    let mut store_positions: BTreeMap<u64, Subject> = BTreeMap::new();
    let mut memberships: BTreeMap<(super::BatchKey, u64), (String, Subject, RecordKind)> =
        BTreeMap::new();
    let mut batch_orders: BTreeMap<super::BatchKey, Vec<(u64, u64, Subject)>> = BTreeMap::new();
    for subject_snapshot in histories {
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
