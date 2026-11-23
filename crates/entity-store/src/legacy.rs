//! Typed, read-only acquisition of stores which predate the recorded Eventlog authority.

use std::collections::BTreeSet;

use crate::{
    asynchronous::{verify_explicit_histories, HistoryOrigin, LegacyEvidence, SubjectHistory},
    StoreError,
};

/// A complete acquisition from one explicitly identified legacy source.
///
/// Every history ends at an imported boundary and has no newly recorded suffix. The document
/// carries only facts the source could establish; validating it never upgrades those facts into
/// genesis replay, cross-subject chronology, batch membership, or committed receipts.
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyStoreSnapshot {
    /// Stable caller-selected identity of the physical source.
    pub source_id: String,
    /// Sorted imported subject boundaries.
    pub histories: Vec<SubjectHistory>,
}

impl LegacyStoreSnapshot {
    /// Constructs and validates one complete acquisition document.
    ///
    /// # Errors
    ///
    /// Blank or substituted source identity, a non-import boundary, a suffix record, duplicate
    /// subject/global identity, or invalid typed evidence.
    pub fn new(
        source_id: impl Into<String>,
        mut histories: Vec<SubjectHistory>,
    ) -> Result<Self, StoreError> {
        let source_id = source_id.into();
        if source_id.trim().is_empty() {
            return Err(StoreError::Backend(
                "legacy acquisition requires a nonblank source identity".to_owned(),
            ));
        }
        histories.sort_by(|left, right| left.subject.cmp(&right.subject));
        let mut subjects = BTreeSet::new();
        for history in &histories {
            if !subjects.insert(history.subject.clone()) {
                return Err(StoreError::Backend(format!(
                    "legacy acquisition repeats subject {:?}",
                    history.subject
                )));
            }
            if !history.records.is_empty() {
                return Err(StoreError::Backend(format!(
                    "legacy acquisition for {:?} contains a post-boundary suffix",
                    history.subject
                )));
            }
            let HistoryOrigin::Imported(anchor) = &history.origin else {
                return Err(StoreError::Backend(format!(
                    "legacy acquisition for {:?} does not declare an imported boundary",
                    history.subject
                )));
            };
            for evidence in &anchor.evidence {
                if let LegacyEvidence::Envelope(evidence) = evidence {
                    if evidence.source_id != source_id {
                        return Err(StoreError::Backend(format!(
                            "legacy evidence for {:?} substitutes source identity {:?}",
                            history.subject, evidence.source_id
                        )));
                    }
                }
            }
        }
        // Verified in place: each history is held to its own anchor's instance as its terminal,
        // exactly what an explicit-set snapshot of copies of both was verified against.
        verify_explicit_histories(
            &source_id,
            histories.iter().map(|history| {
                let HistoryOrigin::Imported(anchor) = &history.origin else {
                    unreachable!("checked above")
                };
                (history, &anchor.instance)
            }),
        )
        .map_err(|error| StoreError::Backend(format!("invalid legacy acquisition: {error}")))?;
        Ok(Self {
            source_id,
            histories,
        })
    }
}

/// A provider-specific, read-only acquisition of its complete available legacy evidence.
pub trait LegacyStoreSource {
    /// Acquires every subject and all evidence the source can establish under one source boundary.
    ///
    /// # Errors
    ///
    /// Provider failure, concurrent source change, corrupt bytes, or invalid evidence.
    fn acquire_legacy(&mut self, source_id: &str) -> Result<LegacyStoreSnapshot, StoreError>;
}

#[cfg(test)]
mod tests {
    use entity_core::EntityInstance;
    use serde_json::json;

    use super::*;
    use crate::asynchronous::{LegacyAnchor, LegacyCompleteness, LegacyOrderDeclaration, Subject};

    fn imported(id: &str, anchor_id: &str) -> SubjectHistory {
        SubjectHistory {
            subject: Subject::new("ticket", id).expect("subject"),
            origin: HistoryOrigin::Imported(LegacyAnchor {
                instance: EntityInstance {
                    entity: "ticket".to_owned(),
                    id: anchor_id.to_owned(),
                    version: 1,
                    revision: 1,
                    lifecycle_state: "open".to_owned(),
                    fields: [("body".to_owned(), json!("z".repeat(1024)))]
                        .into_iter()
                        .collect(),
                },
                order: LegacyOrderDeclaration::PerKindOnly,
                completeness: LegacyCompleteness::AvailableEvidenceOnly,
                evidence: Vec::new(),
            }),
            records: Vec::new(),
        }
    }

    #[test]
    fn a_legacy_snapshot_keeps_the_histories_it_verified_in_place() {
        let inputs = vec![imported("b", "b"), imported("a", "a")];

        let snapshot = LegacyStoreSnapshot::new("source", inputs.clone()).expect("valid");

        assert_eq!(snapshot.source_id, "source");
        assert_eq!(
            snapshot.histories,
            vec![inputs[1].clone(), inputs[0].clone()]
        );
    }

    #[test]
    fn a_legacy_snapshot_still_holds_each_history_to_its_anchor_when_verified_in_place() {
        let error =
            LegacyStoreSnapshot::new("source", vec![imported("a", "a"), imported("b", "c")])
                .expect_err("an anchor of another subject is refused");

        match error {
            StoreError::Backend(message) => assert!(
                message.starts_with("invalid legacy acquisition:")
                    && message.contains("anchor instance belongs to another subject"),
                "unexpected refusal: {message}"
            ),
            other => panic!("unexpected refusal: {other:?}"),
        }
    }
}
