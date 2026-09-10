//! A recorded prefix whose state was recomputed by the kernel, reusable across storage and shells.

use std::collections::BTreeSet;

use crate::{Envelope, StoreError};
use entity_core::{DecisionRecord, EntityInstance, VerifiedReplay};

/// Complete subject envelopes coupled to an incrementally verified kernel prefix.
///
/// Fields are private and this type cannot be deserialized. Its only mutation verifies the next
/// envelope and recomputes the decision. Sharing it lets an async provider and shell use the same
/// proof rather than independently replaying the same history.
#[derive(Debug, Clone)]
pub struct VerifiedHistory {
    entity: String,
    id: String,
    records: Vec<Envelope<DecisionRecord>>,
    identities: BTreeSet<String>,
    replay: VerifiedReplay,
}

impl VerifiedHistory {
    /// An empty prefix for one exact subject, containing no state claim.
    #[must_use]
    pub fn new(entity: &str, id: &str) -> Self {
        Self {
            entity: entity.into(),
            id: id.into(),
            records: Vec::new(),
            identities: BTreeSet::new(),
            replay: VerifiedReplay::default(),
        }
    }

    /// The exact entity and subject identity this proof covers.
    #[must_use]
    pub fn subject(&self) -> (&str, &str) {
        (&self.entity, &self.id)
    }

    /// Envelopes whose complete decisions passed kernel replay, in append order.
    #[must_use]
    pub fn records(&self) -> &[Envelope<DecisionRecord>] {
        &self.records
    }

    /// The resulting state, absent for an empty prefix.
    #[must_use]
    pub fn instance(&self) -> Option<&EntityInstance> {
        self.replay.instance()
    }

    /// Verifies and extends this prefix; any refusal leaves it unchanged.
    ///
    /// # Errors
    /// Invalid provenance, another subject, duplicate record identity or a kernel replay refusal.
    pub fn append(&mut self, envelope: Envelope<DecisionRecord>) -> Result<(), StoreError> {
        envelope
            .validate()
            .map_err(|e| StoreError::Backend(e.to_string()))?;
        if envelope.record.entity != self.entity
            || envelope.record.id != self.id
            || self.identities.contains(&envelope.record_id)
        {
            return Err(StoreError::Backend(
                "recorded history has a wrong subject or duplicate record id".into(),
            ));
        }
        self.replay
            .advance(&envelope.record)
            .map_err(|e| StoreError::Backend(e.to_string()))?;
        self.identities.insert(envelope.record_id.clone());
        self.records.push(envelope);
        Ok(())
    }
}
