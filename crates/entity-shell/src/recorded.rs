//! Retry identity shared by synchronous and asynchronous command entrypoints.

use entity_core::{normalize_arguments, DecisionCommand, DecisionRecord, ValidatedDefinition};
use entity_store::{Envelope, RecordedCommit, Recording, StoreError};
use serde_json::Value;

pub(crate) fn execute_retry(
    envelope: &Envelope<DecisionRecord>,
    subject: (&str, &str),
    expected_revision: u64,
    operation: &str,
    arguments: Value,
    recording: &Recording,
) -> Result<RecordedCommit, StoreError> {
    let matches = (|| {
        let record = &envelope.record;
        let definition = ValidatedDefinition::new(record.definition.clone()?).ok()?;
        let arguments = normalize_arguments(&definition, operation, arguments).ok()?;
        Some(
            record.entity == subject.0
                && record.id == subject.1
                && expected_revision.checked_add(1) == Some(record.revision)
                && record.command
                    == DecisionCommand::Execute {
                        operation: operation.into(),
                        arguments,
                    }
                && recording.seal(record.clone()).ok()? == *envelope,
        )
    })()
    .unwrap_or(false);
    if !matches {
        return Err(StoreError::RecordConflict {
            record_id: recording.record_id.clone(),
        });
    }
    Ok(RecordedCommit {
        instance: envelope.record.result.clone(),
        envelope: envelope.clone(),
    })
}
