//! Typed, read-only acquisition of stores which predate the recorded Eventlog authority.

use std::collections::BTreeSet;

use crate::{
    asynchronous::{
        verify_store_histories, CompleteStoreSnapshot, HistoryOrigin, LegacyEvidence,
        StoreCoverage, SubjectHistory, SubjectSnapshot,
    },
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
        let explicit = CompleteStoreSnapshot {
            scope: source_id.clone(),
            coverage: StoreCoverage::ExplicitSet,
            histories: histories
                .iter()
                .map(|history| {
                    let HistoryOrigin::Imported(anchor) = &history.origin else {
                        unreachable!("checked above")
                    };
                    SubjectSnapshot {
                        history: history.clone(),
                        terminal: anchor.instance.clone(),
                    }
                })
                .collect(),
        };
        verify_store_histories(&explicit)
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
