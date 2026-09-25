use serde_json::{Map, Value};

use entity_core::DecisionCommand;

use crate::Recording;

use super::{AppendMember, AsyncStoreError, BatchKey, RecordedEntry};

/// Orders every object's keys, consuming the value rather than cloning it.
///
/// Every subtree is moved into its place exactly once. The encoder this replaced cloned each
/// subtree again at every level of nesting above it, which for a record embedding a whole
/// definition was most of the cost of verifying a store. The sort is kept although serde_json's
/// default map is already ordered: a consumer that enables `preserve_order` unifies that feature
/// into this crate, and the bytes must not depend on who else is in the build.
fn canonicalize(value: Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut entries: Vec<(String, Value)> = object.into_iter().collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            let mut ordered = Map::new();
            for (key, value) in entries {
                ordered.insert(key, canonicalize(value));
            }
            Value::Object(ordered)
        }
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize).collect()),
        scalar => scalar,
    }
}

/// Whether `serde_json::Map` is the ordered map in this build, so that every object is already in
/// the order [`canonicalize`] would put it in.
///
/// It is unless a crate in the build enables serde_json's `preserve_order`, which unifies into
/// this crate and makes a map remember insertion order instead. The two backends are told apart
/// by what they do, once: an ordered map iterates `"a"` before `"b"` whichever was inserted first.
fn maps_are_ordered() -> bool {
    static ORDERED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ORDERED.get_or_init(|| {
        let mut probe = Map::new();
        probe.insert("b".to_owned(), Value::Null);
        probe.insert("a".to_owned(), Value::Null);
        probe.keys().next().map(String::as_str) == Some("a")
    })
}

/// Encodes one domain tag and complete JSON value as canonical compact UTF-8 bytes.
///
/// Where every map is already ordered, `canonicalize` would move every subtree into a new map
/// with the same entries in the same order, and it is skipped: the bytes are the same bytes. That
/// rebuild was most of the cost of encoding a record that embeds a large definition, which every
/// read that verifies a record pays.
///
/// # Errors
///
/// Serialization failure without numeric coercion.
pub fn canonical_domain_bytes(domain: &str, value: Value) -> Result<Vec<u8>, AsyncStoreError> {
    let document = serde_json::json!([domain, value]);
    let document = if maps_are_ordered() {
        document
    } else {
        canonicalize(document)
    };
    serde_json::to_vec(&document).map_err(|error| AsyncStoreError::Encoding(error.to_string()))
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
    let document = serde_json::json!(["er.batch/1", key_value(key), encoded]);
    let document = if maps_are_ordered() {
        document
    } else {
        canonicalize(document)
    };
    serde_json::to_vec(&document).map_err(|error| AsyncStoreError::Encoding(error.to_string()))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The canonicalization every comparison document was written with until 0.19.0, kept
    /// verbatim as the oracle: it re-cloned each subtree once per level of nesting above it.
    fn canonicalize_by_cloning(value: Value) -> Value {
        match value {
            Value::Object(object) => {
                let mut keys: Vec<String> = object.keys().cloned().collect();
                keys.sort();
                let mut ordered = Map::new();
                for key in keys {
                    if let Some(value) = object.get(&key) {
                        ordered.insert(key, canonicalize_by_cloning(value.clone()));
                    }
                }
                Value::Object(ordered)
            }
            Value::Array(values) => {
                Value::Array(values.into_iter().map(canonicalize_by_cloning).collect())
            }
            scalar => scalar,
        }
    }

    fn oracle_bytes(domain: &str, value: Value) -> Vec<u8> {
        serde_json::to_vec(&canonicalize_by_cloning(serde_json::json!([domain, value])))
            .expect("oracle encodes")
    }

    /// A fixed-seed generator of JSON documents: nested objects and arrays, keys that differ only
    /// in case, in length or by a non-ASCII byte, exact numbers beyond `f64`, escapes and nulls.
    struct Corpus(u64);

    impl Corpus {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn below(&mut self, bound: u64) -> u64 {
            self.next() % bound
        }

        fn key(&mut self) -> String {
            const KEYS: [&str; 12] = [
                "a",
                "A",
                "aa",
                "a\u{0}",
                "ä",
                "z",
                "Z",
                "10",
                "9",
                "",
                "kind",
                "\u{1F600}",
            ];
            let base = KEYS[usize::try_from(self.below(KEYS.len() as u64)).expect("small")];
            format!("{base}{}", self.below(3))
        }

        fn value(&mut self, depth: u32) -> Value {
            let choice = if depth == 0 {
                self.below(5)
            } else {
                self.below(7)
            };
            match choice {
                0 => Value::Null,
                1 => Value::Bool(self.below(2) == 0),
                2 => serde_json::from_str(match self.below(6) {
                    0 => "12345678901234567890123456789",
                    1 => "-0.000000000000000000000000001",
                    2 => "1e400",
                    3 => "0",
                    4 => "-7",
                    _ => "3.141592653589793238462643383279",
                })
                .expect("an exact number parses"),
                3 => Value::String(format!("s\"\\\n\u{7f}é{}", self.next())),
                4 => Value::String(String::new()),
                5 => Value::Array((0..self.below(4)).map(|_| self.value(depth - 1)).collect()),
                _ => {
                    let mut object = Map::new();
                    for _ in 0..self.below(6) {
                        let key = self.key();
                        let value = self.value(depth - 1);
                        object.insert(key, value);
                    }
                    Value::Object(object)
                }
            }
        }
    }

    /// Canonical bytes are an interchange format: the digest preimage of every stored blob and
    /// the material every retry is compared against. The encoder stopped re-cloning subtrees, and
    /// this holds its output byte for byte against the encoder it replaced — on a generated
    /// corpus, on a document nested far deeper than any record, and on the committed fixture.
    #[test]
    fn canonical_bytes_are_byte_identical_to_the_cloning_encoder_they_replaced() {
        let mut corpus = Corpus(0x9e37_79b9_7f4a_7c15);
        for case in 0..2_000 {
            let value = corpus.value(6);
            assert_eq!(
                canonical_domain_bytes("er.record/1", value.clone()).expect("encodes"),
                oracle_bytes("er.record/1", value.clone()),
                "generated case {case} encodes differently: {value}"
            );
        }

        let mut deep = Value::String("leaf".repeat(64));
        for level in 0..96 {
            let mut object = Map::new();
            object.insert(format!("z{level}"), deep);
            object.insert("a".into(), Value::Array(vec![Value::Null, level.into()]));
            deep = Value::Object(object);
        }
        assert_eq!(
            canonical_domain_bytes("er.batch/1", deep.clone()).expect("encodes"),
            oracle_bytes("er.batch/1", deep)
        );

        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/service_2_record_3.json"))
                .expect("fixture parses");
        assert_eq!(
            canonical_domain_bytes("er.record/3", fixture.clone()).expect("encodes"),
            oracle_bytes("er.record/3", fixture)
        );
    }

    /// In a build whose maps are ordered, the encoder skips the reordering pass, so the test above
    /// no longer reaches it. The pass is still what a `preserve_order` build encodes with, and it
    /// is held here to the same oracle directly, on the same corpus.
    #[test]
    fn the_reordering_pass_a_preserve_order_build_encodes_with_is_the_oracle() {
        let mut corpus = Corpus(0x9e37_79b9_7f4a_7c15);
        for case in 0..2_000 {
            let value = corpus.value(6);
            assert_eq!(
                serde_json::to_vec(&canonicalize(value.clone())).expect("encodes"),
                serde_json::to_vec(&canonicalize_by_cloning(value.clone())).expect("encodes"),
                "generated case {case} reorders differently: {value}"
            );
        }
        assert!(
            maps_are_ordered(),
            "this workspace builds serde_json without preserve_order, so the skip is what it runs"
        );
    }
}
