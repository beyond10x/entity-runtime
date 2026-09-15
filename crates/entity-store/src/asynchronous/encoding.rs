use serde_json::{Map, Value};

use entity_core::DecisionCommand;

use crate::Recording;

use super::{AppendMember, AsyncStoreError, BatchKey, RecordedEntry};

fn canonicalize(value: Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut keys: Vec<String> = object.keys().cloned().collect();
            keys.sort();
            let mut ordered = Map::new();
            for key in keys {
                if let Some(value) = object.get(&key) {
                    ordered.insert(key, canonicalize(value.clone()));
                }
            }
            Value::Object(ordered)
        }
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize).collect()),
        scalar => scalar,
    }
}

/// Encodes one domain tag and complete JSON value as canonical compact UTF-8 bytes.
///
/// # Errors
///
/// Serialization failure without numeric coercion.
pub fn canonical_domain_bytes(domain: &str, value: Value) -> Result<Vec<u8>, AsyncStoreError> {
    serde_json::to_vec(&canonicalize(serde_json::json!([domain, value])))
        .map_err(|error| AsyncStoreError::Encoding(error.to_string()))
}

fn tagged_record(entry: &RecordedEntry) -> Result<Value, AsyncStoreError> {
    let mut tagged = Map::new();
    match entry {
        RecordedEntry::Decision(commit) => {
            tagged.insert("kind".to_owned(), Value::String("decision".to_owned()));
            tagged.insert(
                "commit".to_owned(),
                serde_json::to_value(commit)
                    .map_err(|error| AsyncStoreError::Encoding(error.to_string()))?,
            );
        }
        RecordedEntry::Observation(observation) => {
            tagged.insert("kind".to_owned(), Value::String("observation".to_owned()));
            tagged.insert(
                "observation".to_owned(),
                serde_json::to_value(observation)
                    .map_err(|error| AsyncStoreError::Encoding(error.to_string()))?,
            );
        }
    }
    Ok(Value::Object(tagged))
}

/// Encodes one complete typed record in the normative `er.record/1` domain.
///
/// # Errors
///
/// Serialization failure without numeric coercion.
pub fn record_comparison_bytes(entry: &RecordedEntry) -> Result<Vec<u8>, AsyncStoreError> {
    canonical_domain_bytes("er.record/1", tagged_record(entry)?)
}

fn recording_value(recording: &Recording) -> Value {
    serde_json::json!({
        "record_id": recording.record_id,
        "recorded_at": recording.recorded_at,
        "actor": recording.actor,
        "correlation": recording.correlation,
        "causation": recording.causation,
    })
}

/// Reconstructs the canonical original request bytes from one complete saved record.
///
/// # Errors
///
/// [`AsyncStoreError::HistoricalRetryUnverifiable`] for a legacy decision without its original
/// command or a malformed execute revision, and encoding failures otherwise.
pub fn original_request_comparison_bytes(
    entry: &RecordedEntry,
) -> Result<Vec<u8>, AsyncStoreError> {
    match entry {
        RecordedEntry::Decision(commit) => {
            let record = &commit.envelope.record;
            let recording = Recording {
                record_id: commit.envelope.record_id.clone(),
                recorded_at: commit.envelope.recorded_at.clone(),
                correlation: commit.envelope.correlation.clone(),
                causation: commit.envelope.causation.clone(),
                actor: commit.envelope.actor.clone(),
            };
            match &record.command {
                DecisionCommand::Create { fields } => canonical_domain_bytes(
                    "er.request/1",
                    serde_json::json!({
                        "kind": "create",
                        "subject": [record.entity, record.id],
                        "definition_version": record.result.version,
                        "fields": fields,
                        "recording": recording_value(&recording),
                    }),
                ),
                DecisionCommand::Execute {
                    operation,
                    arguments,
                } => canonical_domain_bytes(
                    "er.request/1",
                    serde_json::json!({
                        "kind": "execute",
                        "subject": [record.entity, record.id],
                        "expected_revision": record.revision.checked_sub(1).ok_or_else(|| {
                            AsyncStoreError::HistoricalRetryUnverifiable {
                                record_id: entry.record_id().to_owned(),
                                detail: "execute record has no positive predecessor".to_owned(),
                            }
                        })?,
                        "operation": operation,
                        "arguments": arguments,
                        "recording": recording_value(&recording),
                    }),
                ),
                DecisionCommand::LegacyImport => {
                    Err(AsyncStoreError::HistoricalRetryUnverifiable {
                        record_id: entry.record_id().to_owned(),
                        detail: "legacy import has no original command".to_owned(),
                    })
                }
            }
        }
        RecordedEntry::Observation(observation) => canonical_domain_bytes(
            "er.request/1",
            serde_json::json!({"kind": "observation", "observation": observation}),
        ),
    }
}

fn key_value(key: &BatchKey) -> Value {
    match key {
        BatchKey::SingleRecord(record_id) => serde_json::json!(["single_record", record_id]),
        BatchKey::Named(batch_id) => serde_json::json!(["named", batch_id]),
    }
}

/// Encodes one complete ordered batch in the normative `er.batch/1` domain.
///
/// # Errors
///
/// Serialization failure without numeric coercion.
pub fn batch_comparison_bytes(
    key: &BatchKey,
    members: &[AppendMember],
) -> Result<Vec<u8>, AsyncStoreError> {
    key.validate()?;
    let mut encoded = Vec::with_capacity(members.len());
    for member in members {
        let expectation = match member.expect {
            crate::Expect::Absent => serde_json::json!({"kind": "absent"}),
            crate::Expect::Revision(revision) => {
                serde_json::json!({"kind": "revision", "revision": revision})
            }
        };
        encoded.push(serde_json::json!({
            "expect": expectation,
            "record": ["er.record/1", tagged_record(&member.entry)?]
        }));
    }
    serde_json::to_vec(&canonicalize(serde_json::json!([
        "er.batch/1",
        key_value(key),
        encoded
    ])))
    .map_err(|error| AsyncStoreError::Encoding(error.to_string()))
}

/// Derives one normative member coordinate identity from its key and checked u64 index.
///
/// # Errors
///
/// A blank batch key or JSON encoding failure.
pub fn member_id(key: &BatchKey, index: u64) -> Result<String, AsyncStoreError> {
    key.validate()?;
    serde_json::to_string(&serde_json::json!([key_value(key), index]))
        .map_err(|error| AsyncStoreError::Encoding(error.to_string()))
}
