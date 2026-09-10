//! Recorded execution that yields to caller-selected asynchronous storage.
//!
//! Complete decision history is authoritative here: the executor replays pinned definitions
//! before acting or returning a retry. A legacy snapshot without complete records is refused.

use std::collections::BTreeSet;

use entity_core::{
    DecisionCommand, DecisionRecord, EntityInstance, Registry, Runtime, ValidatedDefinition,
};
use entity_store::{
    asynchronous::AsyncRecordedStore, Envelope, Expect, RecordedCommit, RecordedObservation,
    Recording, StoreError,
};
use serde_json::Value;

use crate::ShellError;

/// A kernel registry and a complete asynchronous recorded provider.
pub struct AsyncStoredRuntime<'a, S: ?Sized> {
    registry: &'a Registry,
    store: &'a mut S,
}

impl<'a, S: AsyncRecordedStore + ?Sized> AsyncStoredRuntime<'a, S> {
    /// Selects a registry and provider without starting an executor or choosing authority.
    pub const fn new(registry: &'a Registry, store: &'a mut S) -> Self {
        Self { registry, store }
    }

    /// Creates and durably records a subject, or returns its exact original creation on retry.
    ///
    /// # Errors
    /// Kernel or recording validation, unverified history, or a provider refusal.
    pub async fn create(
        &mut self,
        entity: &str,
        version: u32,
        id: &str,
        fields: Value,
        recording: &Recording,
    ) -> Result<RecordedCommit, ShellError> {
        let records = self.store.records(entity, id).await?;
        verify_history(entity, id, &records)?;
        let decision = if let Some(previous) = records
            .iter()
            .find(|entry| entry.record_id == recording.record_id)
        {
            let definition = previous
                .record
                .definition
                .clone()
                .expect("verified history pins a definition");
            if definition.version != version
                || !matches!(previous.record.command, DecisionCommand::Create { .. })
            {
                return Err(conflict(recording));
            }
            let definition =
                ValidatedDefinition::new(definition).map_err(entity_core::CoreError::from)?;
            let decision = entity_core::create(&definition, id.to_owned(), fields)
                .map_err(|_| conflict(recording))?;
            let candidate =
                RecordedCommit::new(decision, recording).map_err(|_| conflict(recording))?;
            if candidate.envelope != *previous {
                return Err(conflict(recording));
            }
            return Ok(candidate);
        } else {
            Runtime::new(self.registry).create(entity, version, id, fields)?
        };
        let commit = RecordedCommit::new(decision, recording)
            .map_err(|error| ShellError::Recording(error.to_string()))?;
        self.store.commit_recorded(&commit, Expect::Absent).await?;
        Ok(commit)
    }

    /// Replays complete recorded history to return a verified state.
    ///
    /// # Errors
    /// Missing subjects, incomplete or tampered histories, or provider failure.
    pub async fn get(&mut self, entity: &str, id: &str) -> Result<EntityInstance, ShellError> {
        let records = self.store.records(entity, id).await?;
        self.state(entity, id, &records).await
    }

    async fn state(
        &mut self,
        entity: &str,
        id: &str,
        records: &[Envelope<DecisionRecord>],
    ) -> Result<EntityInstance, ShellError> {
        if let Some(instance) = verify_history(entity, id, records)? {
            return Ok(instance);
        }
        // A legacy snapshot is not absence. Never manufacture genesis records from current state.
        if self.store.load(entity, id).await?.is_some() {
            return Err(StoreError::Backend(
                "subject has state but no complete recorded history".into(),
            )
            .into());
        }
        Err(ShellError::NotFound {
            entity: entity.into(),
            id: id.into(),
        })
    }

    /// Enumerates provider identities; this does not attest that their histories are replayable.
    ///
    /// # Errors
    /// Provider failure.
    pub async fn list(&mut self, entity: &str) -> Result<Vec<String>, ShellError> {
        Ok(self.store.ids(entity).await?)
    }

    /// Executes at the caller's observed revision, recording the complete accepted decision.
    ///
    /// Exact retries use their original pinned definition even when the registry changes.
    /// A later concurrent writer is fenced by the provider's commit expectation.
    ///
    /// # Errors
    /// Missing/stale state, unverified history, kernel/recording refusal or storage failure.
    pub async fn execute(
        &mut self,
        entity: &str,
        id: &str,
        expected_revision: u64,
        operation: &str,
        arguments: Value,
        recording: &Recording,
    ) -> Result<RecordedCommit, ShellError> {
        let records = self.store.records(entity, id).await?;
        let instance = self.state(entity, id, &records).await?;
        if let Some(envelope) = records
            .iter()
            .find(|entry| entry.record_id == recording.record_id)
        {
            let definition = ValidatedDefinition::new(
                envelope
                    .record
                    .definition
                    .clone()
                    .expect("verified history pins a definition"),
            )
            .map_err(entity_core::CoreError::from)?;
            let arguments = entity_core::normalize_arguments(&definition, operation, arguments)
                .map_err(|_| conflict(recording))?;
            if expected_revision.checked_add(1) != Some(envelope.record.revision)
                || envelope.record.command
                    != (DecisionCommand::Execute {
                        operation: operation.into(),
                        arguments,
                    })
                || recording
                    .seal(envelope.record.clone())
                    .map_err(|_| conflict(recording))?
                    != *envelope
            {
                return Err(conflict(recording));
            }
            return Ok(RecordedCommit {
                instance: envelope.record.result.clone(),
                envelope: envelope.clone(),
            });
        }
        if instance.revision != expected_revision {
            return Err(ShellError::StaleRevision {
                entity: entity.into(),
                id: id.into(),
                expected: expected_revision,
                found: instance.revision,
            });
        }
        let decision = Runtime::new(self.registry).execute(&instance, operation, arguments)?;
        let commit = RecordedCommit::new(decision, recording)
            .map_err(|error| ShellError::Recording(error.to_string()))?;
        self.store
            .commit_recorded(&commit, Expect::Revision(expected_revision))
            .await?;
        Ok(commit)
    }

    /// Appends caller-supplied evidence without changing the subject revision.
    ///
    /// The provider owns exact observation retry and revision checks, including retries after
    /// later decisions. This method does not invent a time, actor or observation result.
    ///
    /// # Errors
    /// Invalid observation metadata, revision/record-id conflict, or provider failure.
    pub async fn observe(&mut self, observation: &RecordedObservation) -> Result<(), ShellError> {
        observation.validate()?;
        Ok(self.store.observe(observation).await?)
    }
}

fn conflict(recording: &Recording) -> ShellError {
    StoreError::RecordConflict {
        record_id: recording.record_id.clone(),
    }
    .into()
}

fn verify_history(
    entity: &str,
    id: &str,
    records: &[Envelope<DecisionRecord>],
) -> Result<Option<EntityInstance>, ShellError> {
    if records.is_empty() {
        return Ok(None);
    }
    let mut identities = BTreeSet::new();
    for envelope in records {
        envelope
            .validate()
            .map_err(|error| StoreError::Backend(error.to_string()))?;
        if envelope.record.entity != entity
            || envelope.record.id != id
            || !identities.insert(&envelope.record_id)
        {
            return Err(StoreError::Backend(
                "recorded history has a wrong subject or duplicate record id".into(),
            )
            .into());
        }
    }
    let decisions: Vec<_> = records.iter().map(|entry| entry.record.clone()).collect();
    Ok(Some(entity_core::replay(&decisions)?))
}
