//! The reference provider: everything in a map, nothing on a disk.
//!
//! Its job is to be the implementation every other provider is checked against, and to make the
//! concurrency rule testable without a database. It performs no IO at all, so a test using it
//! cannot be flaky for a reason that has nothing to do with what it is testing.
//!
//! It is **not** a cache and not a fallback. A process that exits has forgotten everything, which
//! is stated here rather than discovered later.

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::ops::Bound;

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

    /// The instances of one entity type whose identity sorts strictly after `after`, in identity
    /// order, borrowed.
    ///
    /// For a keyset page: it seeks into the ordered map rather than walking or copying the
    /// instances that come before, so a caller copies only the rows its page keeps. An empty
    /// `after` starts at the first identity, and an instance whose identity is empty is never
    /// returned — the same lower bound a query cursor's empty identity means.
    pub fn instances_after<'a>(
        &'a self,
        entity: &'a str,
        after: &str,
    ) -> impl Iterator<Item = &'a EntityInstance> + 'a {
        self.instances
            .range((
                Bound::Excluded((entity.to_owned(), after.to_owned())),
                Bound::Unbounded,
            ))
            .take_while(move |((held, _), _)| held == entity)
            .map(|(_, instance)| instance)
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
        // sorting is what the trait promises, and a `BTreeMap` is how this provider keeps it. The
        // listing seeks to the entity's first key and stops at the next entity, so it never walks
        // the keys of other types, not even for a type the store does not hold.
        Ok(self
            .instances
            .range((entity.to_owned(), String::new())..)
            .take_while(|((held, _), _)| held == entity)
            .map(|((_, id), _)| id.clone())
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
        // The stage is the transaction: it holds only the keys the batch touches, every
        // expectation sees earlier members through it, and an error drops it before either map
        // has changed. Copying the whole store instead made a one-member batch cost every
        // unrelated subject.
        let mut staged: BTreeMap<Key, (EntityInstance, Vec<DomainEvent>)> = BTreeMap::new();
        for commit in commits {
            let instance = &commit.decision.instance;
            let key = (instance.entity.clone(), instance.id.clone());
            let found = match staged.get(&key) {
                Some((held, _)) => Some(held.revision),
                None => self.instances.get(&key).map(|held| held.revision),
            };
            check(&instance.entity, &instance.id, commit.expect, found)?;
            let events = &commit.decision.record.events;
            match staged.entry(key) {
                Entry::Occupied(mut entry) => {
                    let (held, staged_events) = entry.get_mut();
                    held.clone_from(instance);
                    staged_events.extend(events.iter().cloned());
                }
                Entry::Vacant(entry) => {
                    entry.insert((instance.clone(), events.clone()));
                }
            }
        }
        // Nothing fallible from here on: every member lands, or (above) none did.
        for (key, (instance, events)) in staged {
            if !events.is_empty() {
                self.events.entry(key.clone()).or_default().extend(events);
            }
            self.instances.insert(key, instance);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use entity_core::{Registry, Runtime};
    use serde_json::json;

    use super::*;

    fn registry() -> Registry {
        let definition = serde_json::from_value(json!({
            "entity": "ticket",
            "version": 1,
            "schema": { "fields": { "title": { "type": "string", "required": true } } },
            "lifecycle": { "initial": "open", "states": ["open"] },
            "create": { "emit": { "type": "TicketOpened", "payload": { "ticket": "$id" } } },
            "operations": {
                "rename": {
                    "arguments": { "fields": { "title": { "type": "string", "required": true } } },
                    "transitions": [{ "from": "open", "to": "open" }],
                    "set": { "title": "$args.title" },
                    "emits": [{ "type": "TicketRenamed", "payload": { "title": "$args.title" } }]
                }
            }
        }))
        .expect("parses");
        let mut registry = Registry::new();
        registry.register(definition).expect("validates");
        registry
    }

    fn created(registry: &Registry, id: &str) -> Decision {
        Runtime::new(registry)
            .create("ticket", 1, id, json!({ "title": id }))
            .expect("permitted")
    }

    fn renamed(registry: &Registry, instance: &EntityInstance, title: &str) -> Decision {
        Runtime::new(registry)
            .execute(instance, "rename", json!({ "title": title }))
            .expect("permitted")
    }

    #[test]
    fn a_batch_whose_later_member_is_refused_leaves_every_subject_as_it_was() {
        let registry = registry();
        let mut store = MemoryStore::new();
        let held = created(&registry, "held");
        store.commit(&held, Expect::Absent).expect("accepted");
        let before = store.clone();

        let second = renamed(&registry, &held.instance, "moved in the batch");
        // The third member expects the revision the store holds, which the second member already
        // moved past inside the batch: the stage, not the store, is what it must be checked against.
        let stale = renamed(&registry, &held.instance, "expects the old revision");
        let refused = store.commit_batch(&[
            AtomicCommit::new(second, Expect::Revision(1)),
            AtomicCommit::new(created(&registry, "fresh"), Expect::Absent),
            AtomicCommit::new(stale, Expect::Revision(1)),
        ]);

        assert_eq!(
            refused,
            Err(StoreError::RevisionConflict {
                entity: "ticket".into(),
                id: "held".into(),
                expected: Expect::Revision(1),
                found: Some(2),
            })
        );
        assert_eq!(
            store.instances, before.instances,
            "no member's state landed"
        );
        assert_eq!(store.events, before.events, "no member's events landed");
    }

    #[test]
    fn a_batch_that_moves_one_subject_twice_publishes_its_last_state_and_every_event_in_order() {
        let registry = registry();
        let mut store = MemoryStore::new();
        let untouched = created(&registry, "untouched");
        store.commit(&untouched, Expect::Absent).expect("accepted");
        let first = created(&registry, "moved");
        let second = renamed(&registry, &first.instance, "second");
        let third = renamed(&registry, &second.instance, "third");

        store
            .commit_batch(&[
                AtomicCommit::new(first, Expect::Absent),
                AtomicCommit::new(second, Expect::Revision(1)),
                AtomicCommit::new(third.clone(), Expect::Revision(2)),
            ])
            .expect("accepted");

        assert_eq!(store.load("ticket", "moved"), Ok(Some(third.instance)));
        let revisions: Vec<u64> = store
            .events("ticket", "moved")
            .expect("events")
            .iter()
            .map(|event| event.revision)
            .collect();
        assert_eq!(revisions, [1, 2, 3]);
        assert_eq!(
            store.load("ticket", "untouched"),
            Ok(Some(untouched.instance))
        );
    }

    #[test]
    fn ids_lists_exactly_one_entity_type_and_nothing_for_a_type_it_does_not_hold() {
        let mut store = MemoryStore::new();
        for (entity, id) in [
            ("a", "1"),
            ("ticket", ""),
            ("ticket", "1"),
            ("ticketz", "0"),
        ] {
            let instance = EntityInstance {
                id: id.into(),
                entity: entity.into(),
                version: 1,
                revision: 1,
                lifecycle_state: "open".into(),
                fields: Default::default(),
            };
            store
                .commit(
                    &Decision::legacy_import(instance, Vec::new()),
                    Expect::Absent,
                )
                .expect("accepted");
        }
        assert_eq!(store.ids("ticket"), Ok(vec![String::new(), "1".to_owned()]));
        assert_eq!(store.ids("ticketz"), Ok(vec!["0".to_owned()]));
        assert_eq!(store.ids("tick"), Ok(Vec::new()));
        assert_eq!(store.ids("absent"), Ok(Vec::new()));
        assert_eq!(store.ids(""), Ok(Vec::new()));
    }

    #[test]
    fn instances_after_seeks_past_the_cursor_and_stays_inside_one_entity_type() {
        let mut store = MemoryStore::new();
        for (entity, id) in [("a", "1"), ("ticket", ""), ("ticket", "1"), ("ticket", "2")] {
            let instance = EntityInstance {
                id: id.into(),
                entity: entity.into(),
                version: 1,
                revision: 1,
                lifecycle_state: "open".into(),
                fields: Default::default(),
            };
            store
                .commit(
                    &Decision::legacy_import(instance, Vec::new()),
                    Expect::Absent,
                )
                .expect("accepted");
        }
        store
            .commit(
                &Decision::legacy_import(
                    EntityInstance {
                        id: "0".into(),
                        entity: "ticketz".into(),
                        version: 1,
                        revision: 1,
                        lifecycle_state: "open".into(),
                        fields: Default::default(),
                    },
                    Vec::new(),
                ),
                Expect::Absent,
            )
            .expect("accepted");
        let ids = |after: &str| -> Vec<String> {
            store
                .instances_after("ticket", after)
                .map(|instance| format!("{}/{}", instance.entity, instance.id))
                .collect()
        };
        assert_eq!(ids(""), ["ticket/1", "ticket/2"]);
        assert_eq!(ids("1"), ["ticket/2"]);
        assert_eq!(ids("2"), Vec::<String>::new());
    }
}
