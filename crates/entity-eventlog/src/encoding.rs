use entity_store::{
    Expect,
    asynchronous::{
        AppendMember, AsyncStoreError, BatchKey, HistoryOrigin, ImportedRecordEvidence,
        KnownLegacyOrder, LegacyAnchor, LegacyCompleteness, LegacyEvidence, LegacyOrderDeclaration,
        RecordedEntry, Subject, SubjectHistory, canonical_domain_bytes,
        original_request_comparison_bytes, record_comparison_bytes,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest as _, Sha256};

#[cfg(test)]
thread_local! {
    /// Stored bytes this file has put through a JSON parser on the current thread.
    ///
    /// Charged by the only function here that reaches `from_slice`, not beside it: a document this
    /// file stops parsing stops calling [`parse_stored`] and therefore stops charging, and a
    /// document it still parses cannot avoid the charge without naming `serde_json::from_slice`
    /// directly — which nothing outside `parse_stored` now does.
    pub(crate) static PARSED_BYTES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// This file's only path from stored bytes to a JSON document.
fn parse_stored(bytes: &[u8]) -> Result<Value, AsyncStoreError> {
    #[cfg(test)]
    PARSED_BYTES.with(|charged| charged.set(charged.get().saturating_add(bytes.len() as u64)));
    serde_json::from_slice(bytes).map_err(enc)
}

pub(crate) const BINDING_BLOB_DOMAIN: &str = "er.eventlog.binding-blob-key/1";
pub(crate) const RECORD_BLOB_DOMAIN: &str = "er.eventlog.record-blob-key/1";
pub(crate) const REQUEST_BLOB_DOMAIN: &str = "er.eventlog.request-blob-key/1";
pub(crate) const BATCH_BLOB_DOMAIN: &str = "er.eventlog.batch-blob-key/1";
pub(crate) const ENTRY_BLOB_DOMAIN: &str = "er.eventlog.recorded-entry-blob-key/1";
pub(crate) const ANCHOR_BLOB_DOMAIN: &str = "er.eventlog.import-anchor-blob-key/1";

/// Exact logical and physical authority selected for one adapter.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Authority {
    /// Entity Runtime logical scope, preserved byte for byte.
    pub logical_scope: String,
    /// Eventlog tenant identity.
    pub tenant: String,
    /// Exact provider generation expected by the caller.
    pub stream_identity: String,
}

impl Authority {
    pub(crate) fn validate(&self) -> Result<(), AsyncStoreError> {
        if self.logical_scope.trim().is_empty() {
            return Err(AsyncStoreError::InvalidInput(
                "Eventlog authority requires a nonblank logical scope".into(),
            ));
        }
        if self.tenant.trim().is_empty() || self.stream_identity.is_empty() {
            return Err(AsyncStoreError::InvalidInput(
                "Eventlog authority requires tenant and stream identity".into(),
            ));
        }
        Ok(())
    }
}

/// Actual immutable coordinates of one Eventlog reference event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicalRef {
    /// Provider-minted event identity.
    pub event_id: String,
    /// Tenant-global commit position.
    pub global_seq: u64,
    /// Physical stream identity.
    pub stream_id: String,
    /// Physical stream version.
    pub stream_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecordedEntryWrapper {
    pub authority: Authority,
    pub batch_blob: String,
    pub batch_key: BatchKeyWire,
    pub member_index: u64,
    pub record_blob: String,
    pub request_blob: String,
    pub subject: SubjectWire,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum BatchKeyWire {
    Single((SingleTag, String)),
    Named((NamedTag, String)),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SingleTag {
    SingleRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NamedTag {
    Named,
}

impl From<&BatchKey> for BatchKeyWire {
    fn from(value: &BatchKey) -> Self {
        match value {
            BatchKey::SingleRecord(value) => Self::Single((SingleTag::SingleRecord, value.clone())),
            BatchKey::Named(value) => Self::Named((NamedTag::Named, value.clone())),
        }
    }
}

impl From<BatchKeyWire> for BatchKey {
    fn from(value: BatchKeyWire) -> Self {
        match value {
            BatchKeyWire::Single((_, value)) => Self::SingleRecord(value),
            BatchKeyWire::Named((_, value)) => Self::Named(value),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SubjectWire(pub String, pub String);

impl From<&Subject> for SubjectWire {
    fn from(value: &Subject) -> Self {
        Self(value.entity.clone(), value.id.clone())
    }
}

impl From<SubjectWire> for Subject {
    fn from(value: SubjectWire) -> Self {
        Self {
            entity: value.0,
            id: value.1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ImportAnchorWrapper {
    pub authority: Authority,
    pub completeness: CompletenessWire,
    pub evidence: Vec<EvidenceWire>,
    pub instance: entity_core::EntityInstance,
    pub order: OrderWire,
    pub subject: SubjectWire,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CompletenessWire {
    AvailableEvidenceOnly,
    CompleteSubject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OrderWire {
    PerKindOnly,
    Subject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum EvidenceWire {
    Envelope {
        known_order: KnownOrderWire,
        record_blob: String,
        source_id: String,
        source_locator: String,
    },
    Decision {
        decision: Box<entity_core::DecisionRecord>,
    },
    Event {
        event: Box<entity_core::DomainEvent>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum KnownOrderWire {
    PerKind((PerKindTag, u64)),
    Subject((SubjectOrderTag, u64)),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PerKindTag {
    PerKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SubjectOrderTag {
    Subject,
}

pub(crate) fn framed_key(domain: &str, bytes: &[u8]) -> Result<String, AsyncStoreError> {
    let length = u64::try_from(bytes.len()).map_err(|_| AsyncStoreError::PositionExhausted {
        domain: "Eventlog digest byte length".into(),
    })?;
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update([0]);
    hasher.update(length.to_be_bytes());
    hasher.update(bytes);
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

pub(crate) fn encode_binding(authority: &Authority) -> Result<Vec<u8>, AsyncStoreError> {
    canonical_domain_bytes(
        "er.eventlog.binding/1",
        serde_json::to_value(authority).map_err(enc)?,
    )
}

pub(crate) fn decode_binding(bytes: &[u8]) -> Result<Authority, AsyncStoreError> {
    decode_tagged("er.eventlog.binding/1", bytes, |value| {
        serde_json::from_value(value).map_err(enc)
    })
}

pub(crate) fn encode_entry(value: &RecordedEntryWrapper) -> Result<Vec<u8>, AsyncStoreError> {
    canonical_domain_bytes(
        "er.eventlog.recorded-entry/1",
        serde_json::to_value(value).map_err(enc)?,
    )
}

pub(crate) fn decode_entry(bytes: &[u8]) -> Result<RecordedEntryWrapper, AsyncStoreError> {
    decode_tagged("er.eventlog.recorded-entry/1", bytes, |value| {
        serde_json::from_value(value).map_err(enc)
    })
}

pub(crate) fn encode_anchor(value: &ImportAnchorWrapper) -> Result<Vec<u8>, AsyncStoreError> {
    canonical_domain_bytes(
        "er.eventlog.import-anchor/1",
        serde_json::to_value(value).map_err(enc)?,
    )
}

pub(crate) fn decode_anchor(bytes: &[u8]) -> Result<ImportAnchorWrapper, AsyncStoreError> {
    let value: Value = parse_stored(bytes)?;
    if value.get(0).and_then(Value::as_str) == Some("er.eventlog.import-anchor/2") {
        decode_tagged("er.eventlog.import-anchor/2", bytes, |value| {
            let bound: SourceBoundAnchorWrapper = serde_json::from_value(value).map_err(enc)?;
            validate_anchor_source(&bound.source_id, &bound.anchor)?;
            Ok(bound.anchor)
        })
    } else {
        decode_tagged("er.eventlog.import-anchor/1", bytes, |value| {
            serde_json::from_value(value).map_err(enc)
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceBoundAnchorWrapper {
    anchor: ImportAnchorWrapper,
    source_id: String,
}

pub(crate) fn encode_source_anchor(
    source_id: &str,
    anchor: &ImportAnchorWrapper,
) -> Result<Vec<u8>, AsyncStoreError> {
    validate_anchor_source(source_id, anchor)?;
    canonical_domain_bytes(
        "er.eventlog.import-anchor/2",
        serde_json::to_value(SourceBoundAnchorWrapper {
            anchor: anchor.clone(),
            source_id: source_id.to_owned(),
        })
        .map_err(enc)?,
    )
}

fn validate_anchor_source(
    source_id: &str,
    anchor: &ImportAnchorWrapper,
) -> Result<(), AsyncStoreError> {
    if source_id.trim().is_empty() || anchor.evidence.iter().any(|evidence| {
        matches!(evidence, EvidenceWire::Envelope { source_id: saved, .. } if saved != source_id)
    }) {
        return Err(invalid("import anchor source identity is absent or contradicts its evidence"));
    }
    Ok(())
}

fn decode_tagged<T>(
    domain: &str,
    bytes: &[u8],
    parse: impl FnOnce(Value) -> Result<T, AsyncStoreError>,
) -> Result<T, AsyncStoreError> {
    let value: Value = parse_stored(bytes)?;
    let array = value
        .as_array()
        .ok_or_else(|| invalid("a canonical document is a two-element array"))?;
    if array.len() != 2 || array[0].as_str() != Some(domain) {
        return Err(invalid(format!("expected {domain} framing")));
    }
    let parsed = parse(array[1].clone())?;
    let canonical = canonical_domain_bytes(domain, array[1].clone())?;
    if canonical != bytes {
        return Err(invalid("stored bytes are not canonical"));
    }
    Ok(parsed)
}

/// The framing check and the typed decode, over a record document however it was reached.
fn decode_record_body(document: &Value) -> Result<RecordedEntry, AsyncStoreError> {
    let array = document
        .as_array()
        .ok_or_else(|| invalid("record is not a tagged array"))?;
    if array.len() != 2
        || array[0]
            .as_str()
            .is_none_or(|tag| !tag.starts_with("er.record/"))
    {
        return Err(invalid("record framing is unknown"));
    }
    #[derive(Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
    enum Tagged {
        Decision {
            commit: Box<entity_store::RecordedCommit>,
        },
        Observation {
            observation: Box<entity_store::RecordedObservation>,
        },
    }
    Ok(
        match serde_json::from_value(array[1].clone()).map_err(enc)? {
            Tagged::Decision { commit } => RecordedEntry::Decision(*commit),
            Tagged::Observation { observation } => RecordedEntry::Observation(*observation),
        },
    )
}

pub(crate) fn decode_record(bytes: &[u8]) -> Result<RecordedEntry, AsyncStoreError> {
    let value: Value = parse_stored(bytes)?;
    let entry = decode_record_body(&value)?;
    if record_comparison_bytes(&entry)? != bytes {
        return Err(invalid("record bytes do not reproduce the complete record"));
    }
    Ok(entry)
}

/// The expectation exactly as `batch_comparison_bytes` writes one.
///
/// Rebuilt and compared because `deny_unknown_fields` does **not** cover an internally tagged
/// **unit** variant: `{"kind":"absent","surplus":1}` deserialises to `Expect::Absent` and writes
/// back without the surplus field, so a canonical document can still be a batch that is not its own
/// rendering. Measured, not assumed — deleting this comparison was tried and
/// `the_batch_key_expectation_and_member_wires_admit_exactly_one_shape_each` refused to agree.
fn expectation_value(expect: Expect) -> Value {
    match expect {
        Expect::Absent => serde_json::json!({"kind": "absent"}),
        Expect::Revision(revision) => serde_json::json!({"kind": "revision", "revision": revision}),
    }
}

/// One record, decoded from the document that already holds it.
///
/// [`decode_record`] takes bytes, so a batch member had to be rendered back to text and parsed a
/// second time to reach it — a second full decode of every record in the largest blob this store
/// binds. The property established is the same one and against the same canonical form: the
/// document reproduces the complete record it decoded to.
fn decode_record_document(document: &Value) -> Result<RecordedEntry, AsyncStoreError> {
    let entry = decode_record_body(document)?;
    if record_comparison_bytes(&entry)? != serde_json::to_vec(document).map_err(enc)? {
        return Err(invalid("record bytes do not reproduce the complete record"));
    }
    Ok(entry)
}

/// One committed batch, decoding each member record exactly once.
///
/// This used to end with `batch_comparison_bytes(&key, &decoded) != bytes` — the whole document
/// rebuilt from the decoded members and compared byte for byte — and to reach those members it
/// rendered each one back to text and parsed it again. **That comparison was proving two separate
/// things, and only one of them needed the second decode.**
///
/// * **The stored bytes are their own canonical rendering.** Checked here in one pass over the tree
///   that was just parsed, which covers the member records too — the part the second decode was
///   doing. It is not cosmetic: these bytes are the batch blob's digest preimage and the material a
///   retry is compared against, so two renderings of one batch are two batches that never
///   deduplicate.
/// * **Each member reproduces the record it decoded to.** Canonicality cannot see this, because the
///   round trip through the typed record is lossy in at least two documented places —
///   `EntityInstance` carries no `deny_unknown_fields`, and `DecisionRecord::removed` is
///   `skip_serializing_if` — so a perfectly canonical document can still decode to something that
///   does not write back to it. Checked per member against the same canonical form as before.
///
/// The old comparison also covered the batch key, the expectation and the member envelope by
/// rebuilding all three. Which of those still need rebuilding was measured, not argued, and the
/// answer was not the obvious one: the **key** does not, because `BatchKeyWire` is an untagged pair
/// of two-element tuples and every other shape fails `from_value`; the member **envelope** does
/// not, because `Member` is a struct with `deny_unknown_fields`; the **expectation** does, because
/// `deny_unknown_fields` does not cover an internally tagged *unit* variant, so
/// `{"kind":"absent","surplus":1}` decodes and writes back without the surplus.
/// `the_batch_key_expectation_and_member_wires_admit_exactly_one_shape_each` in `adapter.rs` holds
/// the first two to that, so widening one of them fails there rather than quietly needing a
/// rebuild back.
pub(crate) fn decode_batch(bytes: &[u8]) -> Result<(BatchKey, Vec<AppendMember>), AsyncStoreError> {
    let value: Value = parse_stored(bytes)?;
    if serde_json::to_vec(&value).map_err(enc)? != bytes {
        return Err(invalid("batch bytes are not canonical"));
    }
    let Value::Array(mut array) = value else {
        return Err(invalid("batch is not a tagged array"));
    };
    if array.len() != 3 || array[0].as_str() != Some("er.batch/1") {
        return Err(invalid("batch framing is unknown"));
    }
    let Some(Value::Array(members)) = array.pop() else {
        return Err(invalid("batch members are not an array"));
    };
    let stored_key = array.pop().expect("a three-element array");
    let key: BatchKey = serde_json::from_value::<BatchKeyWire>(stored_key)
        .map_err(enc)?
        .into();
    let mut decoded = Vec::with_capacity(members.len());
    for member in members {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Member {
            expect: Value,
            record: Value,
        }
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum ExpectWire {
            Absent,
            Revision { revision: u64 },
        }
        let Member { expect, record } = serde_json::from_value(member).map_err(enc)?;
        let expectation = match serde_json::from_value::<ExpectWire>(expect.clone()).map_err(enc)? {
            ExpectWire::Absent => Expect::Absent,
            ExpectWire::Revision { revision } => Expect::Revision(revision),
        };
        if expectation_value(expectation) != expect {
            return Err(invalid("batch bytes are not canonical"));
        }
        let entry = decode_record_document(&record)?;
        let request_bytes = original_request_comparison_bytes(&entry)?;
        decoded.push(AppendMember::new(expectation, entry, request_bytes));
    }
    Ok((key, decoded))
}

pub(crate) fn anchor_from_history(
    authority: Authority,
    history: &SubjectHistory,
    record_digests: &[String],
) -> Result<ImportAnchorWrapper, AsyncStoreError> {
    let HistoryOrigin::Imported(anchor) = &history.origin else {
        return Err(AsyncStoreError::InvalidInput(
            "an imported anchor requires imported history".into(),
        ));
    };
    let mut digest_index = 0usize;
    let mut evidence = Vec::with_capacity(anchor.evidence.len());
    for item in &anchor.evidence {
        evidence.push(match item {
            LegacyEvidence::Envelope(saved) => {
                let digest = record_digests
                    .get(digest_index)
                    .ok_or_else(|| invalid("missing envelope record digest"))?
                    .clone();
                digest_index += 1;
                let known_order = match saved.known_order {
                    KnownLegacyOrder::PerKind(v) => {
                        KnownOrderWire::PerKind((PerKindTag::PerKind, v))
                    }
                    KnownLegacyOrder::Subject(v) => {
                        KnownOrderWire::Subject((SubjectOrderTag::Subject, v))
                    }
                };
                EvidenceWire::Envelope {
                    known_order,
                    record_blob: digest,
                    source_id: saved.source_id.clone(),
                    source_locator: saved.source_locator.clone(),
                }
            }
            LegacyEvidence::Decision(decision) => EvidenceWire::Decision {
                decision: Box::new(decision.clone()),
            },
            LegacyEvidence::Event(event) => EvidenceWire::Event {
                event: Box::new(event.clone()),
            },
        });
    }
    Ok(ImportAnchorWrapper {
        authority,
        completeness: match anchor.completeness {
            LegacyCompleteness::AvailableEvidenceOnly => CompletenessWire::AvailableEvidenceOnly,
            LegacyCompleteness::CompleteSubject => CompletenessWire::CompleteSubject,
        },
        evidence,
        instance: anchor.instance.clone(),
        order: match anchor.order {
            LegacyOrderDeclaration::PerKindOnly => OrderWire::PerKindOnly,
            LegacyOrderDeclaration::Subject => OrderWire::Subject,
        },
        subject: SubjectWire::from(&history.subject),
    })
}

pub(crate) fn history_from_anchor(
    wrapper: &ImportAnchorWrapper,
    records: &[Vec<u8>],
) -> Result<SubjectHistory, AsyncStoreError> {
    let subject: Subject = wrapper.subject.clone().into();
    let mut record_index = 0usize;
    let mut evidence = Vec::with_capacity(wrapper.evidence.len());
    for item in &wrapper.evidence {
        evidence.push(match item {
            EvidenceWire::Envelope {
                known_order,
                source_id,
                source_locator,
                ..
            } => {
                let bytes = records
                    .get(record_index)
                    .ok_or_else(|| invalid("missing imported record bytes"))?;
                record_index += 1;
                LegacyEvidence::Envelope(ImportedRecordEvidence::new(
                    decode_record(bytes)?,
                    source_id.clone(),
                    source_locator.clone(),
                    match known_order {
                        KnownOrderWire::PerKind((_, v)) => KnownLegacyOrder::PerKind(*v),
                        KnownOrderWire::Subject((_, v)) => KnownLegacyOrder::Subject(*v),
                    },
                )?)
            }
            EvidenceWire::Decision { decision } => LegacyEvidence::Decision((**decision).clone()),
            EvidenceWire::Event { event } => LegacyEvidence::Event((**event).clone()),
        });
    }
    Ok(SubjectHistory {
        subject,
        origin: HistoryOrigin::Imported(LegacyAnchor {
            instance: wrapper.instance.clone(),
            completeness: match wrapper.completeness {
                CompletenessWire::AvailableEvidenceOnly => {
                    LegacyCompleteness::AvailableEvidenceOnly
                }
                CompletenessWire::CompleteSubject => LegacyCompleteness::CompleteSubject,
            },
            order: match wrapper.order {
                OrderWire::PerKindOnly => LegacyOrderDeclaration::PerKindOnly,
                OrderWire::Subject => LegacyOrderDeclaration::Subject,
            },
            evidence,
        }),
        records: Vec::new(),
    })
}

pub(crate) fn authority_subject_value(authority: &Authority, subject: &Subject) -> Value {
    serde_json::json!({"authority":authority,"subject":SubjectWire::from(subject)})
}

pub(crate) fn key_for_value(domain: &str, value: Value) -> Result<String, AsyncStoreError> {
    let bytes = canonical_value_bytes(value)?;
    framed_key(domain, &bytes)
}

pub(crate) fn canonical_value_bytes(value: Value) -> Result<Vec<u8>, AsyncStoreError> {
    let tagged = canonical_domain_bytes("_", value)?;
    Ok(tagged[5..tagged.len() - 1].to_vec())
}

fn enc(error: impl std::fmt::Display) -> AsyncStoreError {
    AsyncStoreError::Encoding(error.to_string())
}
fn invalid(detail: impl Into<String>) -> AsyncStoreError {
    AsyncStoreError::ProviderIntegrity {
        provider: "eventlog".into(),
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUTHORITY: &str =
        r#"{"logical_scope":" scope\nΩ ","stream_identity":"gen-A","tenant":"tenant-1"}"#;

    #[test]
    fn literal_reference_vectors_pin_framing_domains_lengths_and_escaping() {
        let binding = format!(r#"["er.eventlog.binding/1",{AUTHORITY}]"#);
        assert_eq!(binding.len(), 103);
        assert_eq!(
            framed_key(BINDING_BLOB_DOMAIN, binding.as_bytes()).unwrap(),
            "sha256:e60356735f17a02973d2dd10b411bbf7b0cd0b2c41a9a0f7dc163a54619e2307"
        );
        let subject = format!(r#"{{"authority":{AUTHORITY},"subject":["E","x"]}}"#);
        assert_eq!(subject.len(), 111);
        assert_eq!(
            framed_key("er.eventlog.subject-stream-key/1", subject.as_bytes()).unwrap(),
            "sha256:e44ba1084300377bf6c0e701352887ce840dcd0b7c8184093818eacc2fc71ce6"
        );
        let entry = format!(
            r#"["er.eventlog.recorded-entry/1",{{"authority":{AUTHORITY},"batch_blob":"sha256:04f3a904e0b2cf4d80f78cc7dc7153953ea08e4fcbdd3f0cc1bab9939a4242a1","batch_key":["named","b"],"member_index":0,"record_blob":"sha256:042c6cac5e47030b8ba26e5401ba28bbaadd29a4c34bd6969d003654c5cb5380","request_blob":"sha256:f147bb14b0c06bac672d2a3ecb12841d4edd2c1032664ce9fba10d160e9d5ebc","subject":["E","x"]}}]"#
        );
        assert_eq!(entry.len(), 451);
        assert_eq!(
            framed_key(ENTRY_BLOB_DOMAIN, entry.as_bytes()).unwrap(),
            "sha256:06171db59b58a192aa94380e439b29a8e2e3fa5d3c91af16d937ba54ced51636"
        );
    }

    #[test]
    fn closed_binding_decode_refuses_unknown_fields_and_noncanonical_order() {
        let canonical = format!(r#"["er.eventlog.binding/1",{AUTHORITY}]"#);
        assert_eq!(
            decode_binding(canonical.as_bytes()).unwrap().logical_scope,
            " scope\nΩ "
        );
        let unknown = canonical.replace("\"tenant\":", "\"unknown\":1,\"tenant\":");
        assert!(matches!(
            decode_binding(unknown.as_bytes()),
            Err(AsyncStoreError::Encoding(_))
        ));
        let reordered = r#"["er.eventlog.binding/1",{"tenant":"tenant-1","stream_identity":"gen-A","logical_scope":" scope\nΩ "}]"#;
        assert!(matches!(
            decode_binding(reordered.as_bytes()),
            Err(AsyncStoreError::ProviderIntegrity { .. })
        ));
    }

    #[test]
    fn a_digest_is_bound_to_its_exact_domain() {
        assert_ne!(
            framed_key(RECORD_BLOB_DOMAIN, b"same bytes").unwrap(),
            framed_key(REQUEST_BLOB_DOMAIN, b"same bytes").unwrap()
        );
    }

    #[test]
    fn source_bound_anchor_version_preserves_old_bytes_and_old_reader_refusal() {
        let old = br#"["er.eventlog.import-anchor/1",{"authority":{"logical_scope":"scope","stream_identity":"generation","tenant":"tenant"},"completeness":"available_evidence_only","evidence":[],"instance":{"entity":"ticket","fields":{"title":"bare"},"id":"bare","lifecycle_state":"open","revision":1,"version":1},"order":"per_kind_only","subject":["ticket","bare"]}]"#;
        let anchor = decode_anchor(old).expect("existing unbound anchor stays readable");
        assert_eq!(encode_anchor(&anchor).unwrap(), old.as_slice());
        let bound = encode_source_anchor("file/source-a", &anchor).unwrap();
        assert_eq!(decode_anchor(&bound).unwrap(), anchor);
        assert_ne!(bound, old.as_slice());
        assert_ne!(
            bound,
            encode_source_anchor("file/source-b", &anchor).unwrap()
        );
        let old_reader =
            decode_tagged::<ImportAnchorWrapper>("er.eventlog.import-anchor/1", &bound, |value| {
                serde_json::from_value(value).map_err(enc)
            });
        assert!(matches!(
            old_reader,
            Err(AsyncStoreError::ProviderIntegrity { .. })
        ));
        assert!(matches!(
            encode_source_anchor(" \n", &anchor),
            Err(AsyncStoreError::ProviderIntegrity { .. })
        ));

        let mut value: Value = serde_json::from_slice(&bound).unwrap();
        value[1]["unexpected"] = Value::Bool(true);
        let unknown =
            canonical_domain_bytes("er.eventlog.import-anchor/2", value[1].clone()).unwrap();
        assert!(matches!(
            decode_anchor(&unknown),
            Err(AsyncStoreError::Encoding(_))
        ));
        value[1].as_object_mut().unwrap().remove("unexpected");
        value[1]["source_id"] = Value::String(String::new());
        let empty =
            canonical_domain_bytes("er.eventlog.import-anchor/2", value[1].clone()).unwrap();
        assert!(matches!(
            decode_anchor(&empty),
            Err(AsyncStoreError::ProviderIntegrity { .. })
        ));
    }

    #[test]
    fn source_bound_anchor_refuses_a_contradictory_envelope_source() {
        let anchor = ImportAnchorWrapper {
            authority: serde_json::from_str(AUTHORITY).unwrap(),
            completeness: CompletenessWire::AvailableEvidenceOnly,
            evidence: vec![EvidenceWire::Envelope {
                known_order: KnownOrderWire::PerKind((PerKindTag::PerKind, 0)),
                record_blob: "sha256:record".into(),
                source_id: "file/source-a".into(),
                source_locator: "ticket/bare:0".into(),
            }],
            instance: entity_core::EntityInstance {
                entity: "ticket".into(),
                version: 1,
                id: "bare".into(),
                lifecycle_state: "open".into(),
                revision: 1,
                fields: serde_json::Map::new(),
            },
            order: OrderWire::PerKindOnly,
            subject: SubjectWire("ticket".into(), "bare".into()),
        };
        assert!(matches!(
            encode_source_anchor("file/source-b", &anchor),
            Err(AsyncStoreError::ProviderIntegrity { .. })
        ));
        let bytes = encode_source_anchor("file/source-a", &anchor).unwrap();
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value[1]["source_id"] = Value::String("file/source-b".into());
        let changed =
            canonical_domain_bytes("er.eventlog.import-anchor/2", value[1].clone()).unwrap();
        assert!(matches!(
            decode_anchor(&changed),
            Err(AsyncStoreError::ProviderIntegrity { .. })
        ));
    }
}
