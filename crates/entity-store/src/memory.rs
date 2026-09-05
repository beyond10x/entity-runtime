//! The reference provider: everything in a map, nothing on a disk.
//!
//! Its job is to be the implementation every other provider is checked against, and to make the
//! concurrency rule testable without a database. It performs no IO at all, so a test using it
//! cannot be flaky for a reason that has nothing to do with what it is testing.
//!
//! It is **not** a cache and not a fallback. A process that exits has forgotten everything, which
//! is stated here rather than discovered later.

use std::collections::BTreeMap;

use entity_core::{Decision, DecisionRecord, DomainEvent, EntityInstance};
use serde_json::Value;

use crate::{
    check, AtomicBatchStore, AtomicCommit, Envelope, EventProvider, Expect, HistoryProvider,
    RecordedCommit, RecordedObservation, StateProvider, Store, StoreError,
};

/// The key an instance is held under: its entity type and its identity.
///
/// Two entity types may use the same identity and mean different things, so the type is part of the
/// key rather than a field beside it.
type Key = (String, String);

/// An in-memory [`Store`].
#[derive(Debug, Clone, Default)]
pub struct MemoryStore {
    instances: BTreeMap<Key, EntityInstance>,
    events: BTreeMap<Key, Vec<DomainEvent>>,
    records: BTreeMap<Key, Vec<Envelope<DecisionRecord>>>,
    observations: BTreeMap<Key, Vec<RecordedObservation>>,
    record_ids: BTreeMap<String, Value>,
}

impl MemoryStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many instances it holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.instances.len()
    }

    /// `true` when it holds none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// Every instance it holds, in key order.
    pub fn instances(&self) -> impl Iterator<Item = &EntityInstance> {
        self.instances.values()
    }
}

impl StateProvider for MemoryStore {
    fn load(&self, entity: &str, id: &str) -> Result<Option<EntityInstance>, StoreError> {
        Ok(self
            .instances
            .get(&(entity.to_owned(), id.to_owned()))
            .cloned())
    }

    fn ids(&self, entity: &str) -> Result<Vec<String>, StoreError> {
        // The map is ordered by `(entity, id)`, so the ids of one entity come out sorted already;
        // sorting is what the trait promises, and a `BTreeMap` is how this provider keeps it.
        Ok(self
            .instances
            .keys()
            .filter(|(held, _)| held == entity)
            .map(|(_, id)| id.clone())
            .collect())
    }
}

impl EventProvider for MemoryStore {
    fn events(&self, entity: &str, id: &str) -> Result<Vec<DomainEvent>, StoreError> {
        let key = (entity.to_owned(), id.to_owned());
        let mut events = self.events.get(&key).cloned().unwrap_or_default();
        if let Some(records) = self.records.get(&key) {
            for record in records {
                events.extend(record.record.events.iter().cloned());
            }
        }
        events.sort_by_key(|event| event.revision);
        Ok(events)
    }
}

impl HistoryProvider for MemoryStore {
    fn records(&self, entity: &str, id: &str) -> Result<Vec<Envelope<DecisionRecord>>, StoreError> {
        Ok(self
            .records
            .get(&(entity.to_owned(), id.to_owned()))
            .cloned()
            .unwrap_or_default())
    }

    fn observations(&self, entity: &str, id: &str) -> Result<Vec<RecordedObservation>, StoreError> {
        Ok(self
            .observations
            .get(&(entity.to_owned(), id.to_owned()))
            .cloned()
            .unwrap_or_default())
    }
}

impl Store for MemoryStore {
    fn history(&self) -> Option<&dyn HistoryProvider> {
        Some(self)
    }
    fn commit(&mut self, decision: &Decision, expect: Expect) -> Result<(), StoreError> {
        let instance = &decision.instance;
        let key = (instance.entity.clone(), instance.id.clone());

        // Checked before anything is written, so a refusal leaves the store exactly as it was.
        let found = self.instances.get(&key).map(|held| held.revision);
        check(&instance.entity, &instance.id, expect, found)?;

        // One `&mut self`, two maps, no await and no fallible step between them: the state and the
        // events land together or not at all. A provider over a real database owes the same
        // guarantee through a transaction, and `commit` is one call so that it can give one.
        self.instances.insert(key.clone(), instance.clone());
        if !decision.record.events.is_empty() {
            self.events
                .entry(key)
                .or_default()
                .extend(decision.record.events.iter().cloned());
        }
        Ok(())
    }

    fn commit_recorded(
        &mut self,
        commit: &RecordedCommit,
        expect: Expect,
    ) -> Result<(), StoreError> {
        commit.validate()?;
        let key = (commit.instance.entity.clone(), commit.instance.id.clone());
        let document =
            serde_json::to_value(commit).map_err(|error| StoreError::Backend(error.to_string()))?;
        if let Some(existing) = self.record_ids.get(&commit.envelope.record_id) {
            return if existing == &document {
                Ok(())
            } else {
                Err(StoreError::RecordConflict {
                    record_id: commit.envelope.record_id.clone(),
                })
            };
        }
        if let Some(existing) = self
            .records
            .get(&key)
            .into_iter()
            .flatten()
            .find(|existing| existing.record_id == commit.envelope.record_id)
        {
            return if existing == &commit.envelope
                && self.instances.get(&key) == Some(&commit.instance)
            {
                Ok(())
            } else {
                Err(StoreError::RecordConflict {
                    record_id: commit.envelope.record_id.clone(),
                })
            };
        }
        let found = self.instances.get(&key).map(|held| held.revision);
        check(&commit.instance.entity, &commit.instance.id, expect, found)?;
        self.instances.insert(key.clone(), commit.instance.clone());
        self.records
            .entry(key)
            .or_default()
            .push(commit.envelope.clone());
        self.record_ids
            .insert(commit.envelope.record_id.clone(), document);
        Ok(())
    }

    fn observe(&mut self, observation: &RecordedObservation) -> Result<(), StoreError> {
        observation.validate()?;
        let key = (observation.entity.clone(), observation.id.clone());
        let document = serde_json::to_value(observation)
            .map_err(|error| StoreError::Backend(error.to_string()))?;
        if let Some(existing) = self.record_ids.get(&observation.envelope.record_id) {
            return if existing == &document {
                Ok(())
            } else {
                Err(StoreError::RecordConflict {
                    record_id: observation.envelope.record_id.clone(),
                })
            };
        }
        if let Some(existing) = self
            .observations
            .get(&key)
            .into_iter()
            .flatten()
            .find(|existing| existing.envelope.record_id == observation.envelope.record_id)
        {
            return if existing == observation {
                Ok(())
            } else {
                Err(StoreError::RecordConflict {
                    record_id: observation.envelope.record_id.clone(),
                })
            };
        }
        check(
            &observation.entity,
            &observation.id,
            Expect::Revision(observation.revision),
            self.instances.get(&key).map(|held| held.revision),
        )?;
        self.observations
            .entry(key)
            .or_default()
            .push(observation.clone());
        self.record_ids
            .insert(observation.envelope.record_id.clone(), document);
        Ok(())
    }
}

impl AtomicBatchStore for MemoryStore {
    fn commit_batch(&mut self, commits: &[AtomicCommit]) -> Result<(), StoreError> {
        // The clone is the transaction: every expectation sees earlier entries in `candidate`, and
        // an error drops it without exposing a prefix through either map.
        let mut candidate = self.clone();
        for commit in commits {
            candidate.commit(&commit.decision, commit.expect)?;
        }
        *self = candidate;
        Ok(())
    }
}
