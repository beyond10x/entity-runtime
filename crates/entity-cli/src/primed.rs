//! One read of the subject for a stored `execute`.
//!
//! Without `--expected-revision` the command acts on whatever revision the store holds now. Asking
//! the shell for that revision and then executing made the shell read the same subject again, and
//! on the File Store each read is the whole subject document with its complete history. Here the
//! command reads state and history once, takes the revision from it, and hands that same reading
//! to the shell's first load through [`Primed`]. The commit is still guarded by that revision under
//! the provider's write lock, so a writer that moved the subject in between is refused exactly as
//! before.

use std::cell::Cell;

use entity_core::{Decision, DomainEvent, EntityInstance, Registry};
use entity_shell::{ShellError, StoredRuntime};
use entity_store::{
    EventProvider, Expect, HistoryProvider, LoadedSubject, RecordedCommit, RecordedObservation,
    Recording, StateProvider, Store, StoreError,
};
use serde_json::Value;

/// A stored `execute`, as the command line gave it.
pub(crate) struct ExecuteRequest<'a> {
    pub(crate) entity: &'a str,
    pub(crate) id: &'a str,
    pub(crate) expected_revision: Option<u64>,
    pub(crate) operation: &'a str,
    pub(crate) arguments: Value,
    pub(crate) recording: &'a Recording,
}

/// Executes against the revision the caller named, or the one stored now, reading the subject
/// once in either case.
pub(crate) fn execute<S: Store>(
    registry: &Registry,
    store: &mut S,
    request: ExecuteRequest<'_>,
) -> Result<RecordedCommit, ShellError> {
    let ExecuteRequest {
        entity,
        id,
        expected_revision,
        operation,
        arguments,
        recording,
    } = request;
    if let Some(revision) = expected_revision {
        return StoredRuntime::new(registry, store)
            .execute(entity, id, revision, operation, arguments, recording);
    }
    let loaded = store
        .load_with_history(entity, id)?
        .ok_or_else(|| ShellError::NotFound {
            entity: entity.to_owned(),
            id: id.to_owned(),
        })?;
    let revision = loaded.instance.revision;
    let mut primed = Primed {
        inner: store,
        loaded: Cell::new(Some((entity.to_owned(), id.to_owned(), loaded))),
    };
    StoredRuntime::new(registry, &mut primed)
        .execute(entity, id, revision, operation, arguments, recording)
}

/// A provider whose next `load_with_history` of one subject answers with a reading already made.
///
/// The reading is handed out once; every later call, and every other method, goes to the provider.
struct Primed<'s, S> {
    inner: &'s mut S,
    loaded: Cell<Option<(String, String, LoadedSubject)>>,
}

impl<S: Store> StateProvider for Primed<'_, S> {
    fn load(&self, entity: &str, id: &str) -> Result<Option<EntityInstance>, StoreError> {
        self.inner.load(entity, id)
    }

    fn revision_of(&self, entity: &str, id: &str) -> Result<Option<u64>, StoreError> {
        self.inner.revision_of(entity, id)
    }

    fn ids(&self, entity: &str) -> Result<Vec<String>, StoreError> {
        self.inner.ids(entity)
    }
}

impl<S: Store> EventProvider for Primed<'_, S> {
    fn events(&self, entity: &str, id: &str) -> Result<Vec<DomainEvent>, StoreError> {
        self.inner.events(entity, id)
    }
}

impl<S: Store> Store for Primed<'_, S> {
    fn history(&self) -> Option<&dyn HistoryProvider> {
        self.inner.history()
    }

    fn load_with_history(
        &self,
        entity: &str,
        id: &str,
    ) -> Result<Option<LoadedSubject>, StoreError> {
        if let Some((held_entity, held_id, loaded)) = self.loaded.take() {
            if held_entity == entity && held_id == id {
                return Ok(Some(loaded));
            }
        }
        self.inner.load_with_history(entity, id)
    }

    fn commit(&mut self, decision: &Decision, expect: Expect) -> Result<(), StoreError> {
        self.loaded.take();
        self.inner.commit(decision, expect)
    }

    fn commit_recorded(
        &mut self,
        commit: &RecordedCommit,
        expect: Expect,
    ) -> Result<(), StoreError> {
        self.loaded.take();
        self.inner.commit_recorded(commit, expect)
    }

    fn observe(&mut self, observation: &RecordedObservation) -> Result<(), StoreError> {
        self.loaded.take();
        self.inner.observe(observation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use entity_core::EntityDefinition;
    use entity_store::MemoryStore;
    use serde_json::json;

    /// Counts every read of a subject a caller asks the provider for.
    struct Counting {
        inner: MemoryStore,
        reads: Cell<usize>,
    }

    impl Counting {
        fn read(&self) {
            self.reads.set(self.reads.get() + 1);
        }
    }

    impl StateProvider for Counting {
        fn load(&self, entity: &str, id: &str) -> Result<Option<EntityInstance>, StoreError> {
            self.read();
            self.inner.load(entity, id)
        }
        fn ids(&self, entity: &str) -> Result<Vec<String>, StoreError> {
            self.inner.ids(entity)
        }
    }

    impl EventProvider for Counting {
        fn events(&self, entity: &str, id: &str) -> Result<Vec<DomainEvent>, StoreError> {
            self.inner.events(entity, id)
        }
    }

    impl HistoryProvider for Counting {
        fn records(
            &self,
            entity: &str,
            id: &str,
        ) -> Result<Vec<entity_store::Envelope<entity_core::DecisionRecord>>, StoreError> {
            self.read();
            self.inner.records(entity, id)
        }
        fn observations(
            &self,
            entity: &str,
            id: &str,
        ) -> Result<Vec<RecordedObservation>, StoreError> {
            self.inner.observations(entity, id)
        }
    }

    impl Store for Counting {
        fn history(&self) -> Option<&dyn HistoryProvider> {
            Some(self)
        }
        fn load_with_history(
            &self,
            entity: &str,
            id: &str,
        ) -> Result<Option<LoadedSubject>, StoreError> {
            self.read();
            self.inner.load_with_history(entity, id)
        }
        fn commit(&mut self, decision: &Decision, expect: Expect) -> Result<(), StoreError> {
            self.inner.commit(decision, expect)
        }
        fn commit_recorded(
            &mut self,
            commit: &RecordedCommit,
            expect: Expect,
        ) -> Result<(), StoreError> {
            self.inner.commit_recorded(commit, expect)
        }
    }

    fn registry() -> Registry {
        let definition: EntityDefinition = serde_json::from_value(json!({
            "entity": "thing",
            "schema": {},
            "lifecycle": { "initial": "new", "states": ["new", "done"] },
            "operations": { "finish": { "transitions": [{ "from": "new", "to": "done" }] } }
        }))
        .expect("definition");
        let mut registry = Registry::new();
        registry.register(definition).expect("valid");
        registry
    }

    fn recording(id: &str) -> Recording {
        Recording {
            record_id: id.into(),
            recorded_at: "2026-08-31T10:00:00Z".into(),
            correlation: None,
            causation: None,
            actor: None,
        }
    }

    fn request<'a>(recording: &'a Recording, expected: Option<u64>) -> ExecuteRequest<'a> {
        ExecuteRequest {
            entity: "thing",
            id: "one",
            expected_revision: expected,
            operation: "finish",
            arguments: json!({}),
            recording,
        }
    }

    #[test]
    fn an_execute_without_a_revision_reads_the_subject_once() {
        let registry = registry();
        let mut store = Counting {
            inner: MemoryStore::new(),
            reads: Cell::new(0),
        };
        StoredRuntime::new(&registry, &mut store)
            .create("thing", 1, "one", json!({}), &recording("create"))
            .expect("created");
        store.reads.set(0);
        let finish = recording("finish");
        let accepted = execute(&registry, &mut store, request(&finish, None)).expect("executed");
        assert_eq!(accepted.instance.revision, 2);
        assert_eq!(
            store.reads.get(),
            1,
            "the revision lookup and the shell's retry check share one read of the subject"
        );

        store.reads.set(0);
        let retried = execute(&registry, &mut store, request(&finish, Some(1))).expect("retry");
        assert_eq!(
            retried, accepted,
            "a named revision still recovers the original"
        );
        assert_eq!(
            store.reads.get(),
            1,
            "a named revision reads the subject once too"
        );
    }

    #[test]
    fn an_execute_without_a_revision_on_a_missing_subject_is_not_found() {
        let registry = registry();
        let mut store = MemoryStore::new();
        let finish = recording("finish");
        let error = execute(&registry, &mut store, request(&finish, None)).expect_err("absent");
        assert!(
            matches!(&error, ShellError::NotFound { entity, id } if entity == "thing" && id == "one"),
            "{error:?}"
        );
    }
}
