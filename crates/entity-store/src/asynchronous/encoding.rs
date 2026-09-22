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

/// The record framing one entry is spelled in.
///
/// A `service/1` decision's record carries keys `er.record/1` has never carried — `outcome`,
/// `effect`, `response`, and a `create` command with an `arguments` key. The governing rule for
/// these formats is that a change to their shape, framing or scalar spelling needs a new version
/// domain, so that is what a `service/1` decision takes. A `kernel/1` decision and every
/// observation keep the bytes they have.
#[must_use]
pub fn record_domain(entry: &RecordedEntry) -> &'static str {
    match entry {
        RecordedEntry::Decision(commit) => match &commit.envelope.record.definition {
            Some(definition) if definition.semantics.has_operation_fulfillment() => "er.record/4",
            Some(definition) if definition.semantics.has_conditional_presence() => "er.record/3",
            Some(definition) if definition.semantics.is_service_1() => "er.record/2",
            _ => "er.record/1",
        },
        RecordedEntry::Observation(_) => "er.record/1",
    }
}

/// The original-request framing one entry is spelled in, by the same rule.
#[must_use]
pub fn request_domain(entry: &RecordedEntry) -> &'static str {
    match record_domain(entry) {
        "er.record/2" => "er.request/2",
        "er.record/3" => "er.request/3",
        "er.record/4" => "er.request/4",
        _ => "er.request/1",
    }
}

/// Encodes one complete typed record in its normative domain.
///
/// # Errors
///
/// Serialization failure without numeric coercion.
pub fn record_comparison_bytes(entry: &RecordedEntry) -> Result<Vec<u8>, AsyncStoreError> {
    canonical_domain_bytes(record_domain(entry), tagged_record(entry)?)
}

/// The framing a comparison document is written in, read **without parsing its payload**.
///
/// The framing tag is the first element of the array, so a reader that does not know a framing
/// refuses the document there rather than after reading a body it was not written for.
///
/// # Errors
///
/// [`AsyncStoreError::Encoding`] when the document does not begin with a quoted framing tag.
pub fn record_framing(bytes: &[u8]) -> Result<&str, AsyncStoreError> {
    let malformed = || {
        AsyncStoreError::Encoding(
            "a comparison document begins with its framing tag as a quoted string".to_owned(),
        )
    };
    let text = std::str::from_utf8(bytes).map_err(|_| malformed())?;
    let rest = text.strip_prefix("[\"").ok_or_else(malformed)?;
    let end = rest.find('"').ok_or_else(malformed)?;
    Ok(&rest[..end])
}

/// Refuses a comparison document whose framing this reader does not know, by the name of that
/// framing, before its payload is parsed at all.
///
/// # Errors
///
/// [`AsyncStoreError::Encoding`] naming the framing that was found, or a malformed document.
pub fn read_record_in_domain(domain: &str, bytes: &[u8]) -> Result<Value, AsyncStoreError> {
    let found = record_framing(bytes)?;
    if found != domain {
        return Err(AsyncStoreError::Encoding(format!(
            "this reader knows the framing {domain} and the document is framed {found}"
        )));
    }
    let document: Value = serde_json::from_slice(bytes)
        .map_err(|error| AsyncStoreError::Encoding(error.to_string()))?;
    document
        .as_array()
        .and_then(|elements| elements.get(1).cloned())
        .ok_or_else(|| {
            AsyncStoreError::Encoding("a comparison document carries two elements".to_owned())
        })
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
            let domain = request_domain(entry);
            match &record.command {
                // A `service/1` creation's *original request* is the caller's **arguments**, not
                // the fields the branch produced: reconstructing it as the fields would hand a
                // retry a request the caller never sent, which is what these bytes exist to
                // prevent. A `kernel/1` creation's input is its fields and its shape is unchanged.
                DecisionCommand::Create {
                    fields: _,
                    arguments,
                } if matches!(domain, "er.request/2" | "er.request/3" | "er.request/4") => {
                    canonical_domain_bytes(
                        domain,
                        serde_json::json!({
                            "kind": "create",
                            "subject": [record.entity, record.id],
                            "definition_version": record.result.version,
                            "arguments": arguments,
                            "recording": recording_value(&recording),
                        }),
                    )
                }
                DecisionCommand::Create { fields, .. } => canonical_domain_bytes(
                    domain,
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
                    fulfillments,
                } if domain == "er.request/4" => canonical_domain_bytes(
                    domain,
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
                        "fulfillments": fulfillments,
                        "recording": recording_value(&recording),
                    }),
                ),
                DecisionCommand::Execute {
                    operation,
                    arguments,
                    fulfillments: _,
                } => canonical_domain_bytes(
                    domain,
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
            request_domain(entry),
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
        // Each member carries its own record framing. The batch tag does **not** move: a reader
        // that walks a batch meets the member's framing and refuses there, by the name of the
        // framing it does not know, and a batch whose members are all `/1` keeps its bytes.
        encoded.push(serde_json::json!({
            "expect": expectation,
            "record": [record_domain(&member.entry), tagged_record(&member.entry)?]
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
