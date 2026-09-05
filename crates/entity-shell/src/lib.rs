//! Provider-backed operations shared by the generic CLI, MCP and generated commands.
//!
//! This layer owns no transport and performs no filesystem or network IO itself. It asks the
//! provider it was given to load or commit, and keeps the crucial sequence in one place: compare
//! the revision an agent observed, evaluate against that exact instance, then commit atomically at
//! the same revision.

use std::fmt;

use entity_core::{CoreError, DomainEvent, EntityInstance, Registry, Runtime};
use entity_store::{Expect, RecordedCommit, Recording, Store, StoreError};
use serde_json::Value;

/// Why a provider-backed command produced no result.
#[derive(Debug)]
pub enum ShellError {
    /// The kernel refused the requested create or operation.
    Core(CoreError),
    /// The provider refused or failed.
    Store(StoreError),
    /// No subject exists under the requested identity.
    NotFound {
        /// Entity type.
        entity: String,
        /// Subject identity.
        id: String,
    },
    /// The caller acted on a revision other than the one currently stored.
    StaleRevision {
        /// Entity type.
        entity: String,
        /// Subject identity.
        id: String,
        /// Revision the caller observed.
        expected: u64,
        /// Revision currently stored.
        found: u64,
    },
    /// Recording metadata was invalid.
    Recording(String),
}

impl ShellError {
    /// Stable machine-readable kind.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Core(error) => error.kind(),
            Self::Store(StoreError::RevisionConflict { .. }) | Self::StaleRevision { .. } => {
                "revision_conflict"
            }
            Self::Store(StoreError::Unreachable { .. }) => "store_unreachable",
            Self::Store(StoreError::RecordConflict { .. }) => "record_conflict",
            Self::Store(StoreError::Backend(_)) => "store_backend",
            Self::NotFound { .. } => "not_found",
            Self::Recording(_) => "invalid_recording",
        }
    }

    /// Which boundary refused the command.
    #[must_use]
    pub fn boundary(&self) -> &'static str {
        match self {
            Self::Core(_) => "kernel",
            Self::Recording(_) => "input",
            Self::Store(_) | Self::NotFound { .. } | Self::StaleRevision { .. } => "store",
        }
    }
}

impl fmt::Display for ShellError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core(error) => error.fmt(formatter),
            Self::Store(error) => error.fmt(formatter),
            Self::NotFound { entity, id } => write!(formatter, "no stored {entity} has id {id}"),
            Self::StaleRevision {
                entity,
                id,
                expected,
                found,
            } => write!(
                formatter,
                "{entity} {id}: expected revision {expected}, found revision {found}"
            ),
            Self::Recording(detail) => formatter.write_str(detail),
        }
    }
}

impl std::error::Error for ShellError {}

impl From<CoreError> for ShellError {
    fn from(error: CoreError) -> Self {
        Self::Core(error)
    }
}

impl From<StoreError> for ShellError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// A validated runtime together with the provider that keeps its accepted decisions.
pub struct StoredRuntime<'a, S> {
    registry: &'a Registry,
    store: &'a mut S,
}

impl<'a, S> StoredRuntime<'a, S>
where
    S: Store,
{
    /// Builds the provider-backed runtime.
    #[must_use]
    pub const fn new(registry: &'a Registry, store: &'a mut S) -> Self {
        Self { registry, store }
    }

    /// Creates and records one subject.
    ///
    /// # Errors
    ///
    /// Kernel validation, invalid recording metadata, or a provider refusal.
    pub fn create(
        &mut self,
        entity: &str,
        version: u32,
        id: impl Into<String>,
        fields: Value,
        recording: &Recording,
    ) -> Result<RecordedCommit, ShellError> {
        let decision = Runtime::new(self.registry).create(entity, version, id, fields)?;
        let commit = RecordedCommit::new(decision, recording)
            .map_err(|error| ShellError::Recording(error.to_string()))?;
        self.store.commit_recorded(&commit, Expect::Absent)?;
        Ok(commit)
    }

    /// Loads a subject.
    ///
    /// # Errors
    ///
    /// Provider failure or no stored subject.
    pub fn get(&self, entity: &str, id: &str) -> Result<EntityInstance, ShellError> {
        self.store
            .load(entity, id)?
            .ok_or_else(|| ShellError::NotFound {
                entity: entity.to_owned(),
                id: id.to_owned(),
            })
    }

    /// Lists every stored identity for an entity type.
    ///
    /// # Errors
    ///
    /// Provider failure.
    pub fn list(&self, entity: &str) -> Result<Vec<String>, ShellError> {
        self.store.ids(entity).map_err(Into::into)
    }

    /// Returns every event for one stored subject.
    ///
    /// # Errors
    ///
    /// Provider failure or no stored subject.
    pub fn events(&self, entity: &str, id: &str) -> Result<Vec<DomainEvent>, ShellError> {
        let _ = self.get(entity, id)?;
        self.store.events(entity, id).map_err(Into::into)
    }

    /// Executes against the exact revision the caller observed and records the accepted decision.
    ///
    /// # Errors
    ///
    /// Missing or stale state, a kernel refusal, invalid recording metadata, or provider failure.
    pub fn execute(
        &mut self,
        entity: &str,
        id: &str,
        expected_revision: u64,
        operation: &str,
        arguments: Value,
        recording: &Recording,
    ) -> Result<RecordedCommit, ShellError> {
        let instance = self.get(entity, id)?;
        if let Some(history) = self.store.history() {
            if let Some(envelope) = history
                .records(entity, id)?
                .into_iter()
                .find(|entry| entry.record_id == recording.record_id)
            {
                let matches = (|| {
                    let record = &envelope.record;
                    let definition =
                        entity_core::ValidatedDefinition::new(record.definition.clone()?).ok()?;
                    let args =
                        entity_core::normalize_arguments(&definition, operation, arguments.clone())
                            .ok()?;
                    Some(
                        record.entity == entity
                            && record.id == id
                            && expected_revision.checked_add(1) == Some(record.revision)
                            && record.command
                                == entity_core::DecisionCommand::Execute {
                                    operation: operation.to_owned(),
                                    arguments: args,
                                }
                            && recording.seal(record.clone()).ok()? == envelope,
                    )
                })()
                .unwrap_or(false);
                if !matches {
                    return Err(StoreError::RecordConflict {
                        record_id: recording.record_id.clone(),
                    }
                    .into());
                }
                return Ok(RecordedCommit {
                    instance: envelope.record.result.clone(),
                    envelope,
                });
            }
        }
        if instance.revision != expected_revision {
            return Err(ShellError::StaleRevision {
                entity: entity.to_owned(),
                id: id.to_owned(),
                expected: expected_revision,
                found: instance.revision,
            });
        }
        let decision = Runtime::new(self.registry).execute(&instance, operation, arguments)?;
        let commit = RecordedCommit::new(decision, recording)
            .map_err(|error| ShellError::Recording(error.to_string()))?;
        self.store
            .commit_recorded(&commit, Expect::Revision(expected_revision))?;
        Ok(commit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use entity_core::EntityDefinition;
    use entity_store::MemoryStore;
    use serde_json::json;

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
            actor: Some("test".into()),
        }
    }

    #[test]
    fn an_exact_execute_retry_returns_the_original_commit_after_state_has_advanced() {
        let registry = registry();
        let mut store = MemoryStore::new();
        let mut runtime = StoredRuntime::new(&registry, &mut store);
        runtime
            .create("thing", 1, "one", json!({}), &recording("create"))
            .unwrap();
        let accepted = runtime
            .execute("thing", "one", 1, "finish", json!({}), &recording("finish"))
            .unwrap();
        assert_eq!(
            runtime
                .execute("thing", "one", 1, "finish", json!({}), &recording("finish"))
                .unwrap(),
            accepted
        );
        let changed = runtime.execute(
            "thing",
            "one",
            1,
            "finish",
            json!({"extra": true}),
            &recording("finish"),
        );
        assert!(
            matches!(changed, Err(ShellError::Store(StoreError::RecordConflict { record_id })) if record_id == "finish")
        );
        assert_eq!(runtime.get("thing", "one").unwrap().revision, 2);
    }

    #[test]
    fn stale_agent_intent_is_refused_before_the_kernel_or_store_changes_anything() {
        let registry = registry();
        let mut store = MemoryStore::new();
        let mut runtime = StoredRuntime::new(&registry, &mut store);
        runtime
            .create("thing", 1, "one", json!({}), &recording("create"))
            .expect("created");
        let error = runtime
            .execute("thing", "one", 9, "finish", json!({}), &recording("finish"))
            .expect_err("stale");
        assert!(
            matches!(
                error,
                ShellError::StaleRevision {
                    expected: 9,
                    found: 1,
                    ..
                }
            ),
            "the pre-kernel observed-revision guard must own this refusal: {error:?}"
        );
        assert_eq!(runtime.get("thing", "one").expect("held").revision, 1);
        assert!(runtime.events("thing", "one").expect("events").is_empty());
    }
}
