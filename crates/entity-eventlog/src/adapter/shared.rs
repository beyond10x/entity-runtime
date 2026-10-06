//! The verified model's indexes, each reaching a committed record through one shared allocation.
//!
//! A committed record is reachable three ways: by its id, through its subject's history and through
//! its batch. Each index used to own a copy, and a record embeds its saved definition and both of its
//! canonical byte strings, so a model held every record three times over and a handle's memory of
//! the histories it verified held it once more. Here each index holds an [`Arc`] of the one
//! [`StoredRecord`] the model admitted.
//!
//! The public answers — [`RecordLookup`], [`SubjectHistory`], [`StoredBatch`] — own their records,
//! so a read materialises its answer from these at the API boundary, record for record what the
//! owning indexes answered. `Debug` renders each index as the answer it materialises, because the
//! model is pinned by digest over its `Debug` text.

use std::{fmt, sync::Arc};

use entity_store::asynchronous::{
    BatchKey, CommitReceipt, HistoryOrigin, ImportedRecordEvidence, RecordLookup, StoredBatch,
    StoredRecord, Subject, SubjectHistory,
};

/// One committed record as the model holds it: admitted once, shared by every index naming it.
pub(super) type SharedRecord = Arc<StoredRecord>;

/// What a record id names: a [`RecordLookup`] whose committed record is shared.
#[derive(Clone, PartialEq)]
pub(super) enum ModelLookup {
    /// A newly accepted record, the same allocation its history and batch hold.
    Committed(SharedRecord),
    /// Preserved historical evidence. Boxed so that every committed entry, a pointer, does not
    /// reserve the evidence's inline size as well.
    Imported(Box<ImportedRecordEvidence>),
}

impl ModelLookup {
    /// The public answer, owning a copy of the record.
    pub(super) fn to_public(&self) -> RecordLookup {
        match self {
            Self::Committed(record) => RecordLookup::Committed(StoredRecord::clone(record)),
            Self::Imported(evidence) => {
                RecordLookup::Imported(ImportedRecordEvidence::clone(evidence))
            }
        }
    }
}

impl fmt::Debug for ModelLookup {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.to_public(), formatter)
    }
}

/// One subject's history: a [`SubjectHistory`] whose records are shared.
#[derive(Clone, PartialEq)]
pub(super) struct ModelHistory {
    pub(super) subject: Subject,
    pub(super) origin: HistoryOrigin,
    /// Post-genesis or post-anchor records, in physical order once the history is settled.
    pub(super) records: Vec<SharedRecord>,
}

impl ModelHistory {
    /// A subject's history before its first record.
    pub(super) fn genesis(subject: Subject) -> Self {
        Self {
            subject,
            origin: HistoryOrigin::Genesis,
            records: Vec::new(),
        }
    }

    /// The public answer, owning a copy of every record.
    pub(super) fn to_public(&self) -> SubjectHistory {
        SubjectHistory {
            subject: self.subject.clone(),
            origin: self.origin.clone(),
            records: self
                .records
                .iter()
                .map(|record| StoredRecord::clone(record))
                .collect(),
        }
    }
}

/// An imported history enters the model owning the records `history_from_anchor` decoded.
impl From<SubjectHistory> for ModelHistory {
    fn from(history: SubjectHistory) -> Self {
        Self {
            subject: history.subject,
            origin: history.origin,
            records: history.records.into_iter().map(Arc::new).collect(),
        }
    }
}

impl fmt::Debug for ModelHistory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.to_public(), formatter)
    }
}

/// A read's answer is the model's history when it is that history record for record.
#[cfg(test)]
impl PartialEq<ModelHistory> for SubjectHistory {
    fn eq(&self, model: &ModelHistory) -> bool {
        self.subject == model.subject
            && self.origin == model.origin
            && self.records.len() == model.records.len()
            && self
                .records
                .iter()
                .zip(&model.records)
                .all(|(answered, held)| *answered == **held)
    }
}

/// One committed batch: a [`StoredBatch`] whose members are shared.
#[derive(Clone, PartialEq)]
pub(super) struct ModelBatch {
    pub(super) key: BatchKey,
    /// Ordered members, the same allocations their record ids and histories reach.
    pub(super) records: Vec<SharedRecord>,
    /// Exact `er.batch/1` comparison bytes: the bound batch blob, shared with the capture it was
    /// moved out of rather than copied from it when the model's builder owned that capture.
    pub(super) comparison_bytes: Arc<Vec<u8>>,
    pub(super) receipt: CommitReceipt,
}

impl ModelBatch {
    /// The public answer, owning a copy of every member.
    pub(super) fn to_public(&self) -> StoredBatch {
        StoredBatch {
            key: self.key.clone(),
            records: self
                .records
                .iter()
                .map(|record| StoredRecord::clone(record))
                .collect(),
            comparison_bytes: Vec::clone(&self.comparison_bytes),
            receipt: self.receipt.clone(),
        }
    }
}

impl fmt::Debug for ModelBatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.to_public(), formatter)
    }
}
