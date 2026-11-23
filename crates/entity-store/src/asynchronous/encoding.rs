use serde_json::{Map, Value};

use entity_core::{DecisionCommand, EntityDefinition};

use crate::Recording;

use super::{sha256::Sha256, AppendMember, AsyncStoreError, BatchKey, RecordedEntry};

/// The framing of a decision record that names its definition by digest.
///
/// Its body is the body the record would have in the framing [`record_domain`] gives it, with one
/// change: `commit.envelope.record.definition` is the definition's digest
/// ([`definition_digest`]) as a string instead of the definition. One framing serves every
/// semantics, because the semantics is read from the definition the digest resolves to. A record
/// without a definition, and every observation, has nothing to name and keeps its framing.
pub const RECORD_BY_DIGEST: &str = "er.record/5";

/// The framing of a definition document: `["er.definition/1", <definition>]`, canonical.
pub const DEFINITION_FRAMING: &str = "er.definition/1";

/// The domain a definition document's digest is framed in.
///
/// The framing is Eventlog's blob key — `domain`, a zero byte, the document's length as a
/// big-endian `u64`, the document — so the digest a record names is the key its definition blob
/// is stored under.
pub const DEFINITION_DIGEST_DOMAIN: &str = "er.definition-key/1";

/// How a record spells its definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordForm {
    /// The definition is inside the record: `er.record/1` to `er.record/4`, every record written
    /// before 0.33.0. Read and compared in this form so that its bytes and digests never change.
    DefinitionEmbedded,
    /// The definition is named by digest ([`RECORD_BY_DIGEST`]); what new records are written in.
    DefinitionByDigest,
}

impl RecordForm {
    /// The form one stored record document is in, read from its framing alone.
    #[must_use]
    pub fn of_record(bytes: &[u8]) -> Self {
        match record_framing(bytes) {
            Ok(RECORD_BY_DIGEST) => Self::DefinitionByDigest,
            _ => Self::DefinitionEmbedded,
        }
    }

    /// The form one stored `er.batch/1` document's members are in.
    ///
    /// A batch is written by one writer at once, so its members share a form. A batch none of whose
    /// members names a definition is the same bytes in both forms.
    #[must_use]
    pub fn of_batch(bytes: &[u8]) -> Self {
        #[derive(serde::Deserialize)]
        struct Member {
            record: (String, serde::de::IgnoredAny),
        }
        let scanned: Result<(serde::de::IgnoredAny, serde::de::IgnoredAny, Vec<Member>), _> =
            serde_json::from_slice(bytes);
        match scanned {
            Ok((_, _, members))
                if members
                    .iter()
                    .any(|member| member.record.0 == RECORD_BY_DIGEST) =>
            {
                Self::DefinitionByDigest
            }
            _ => Self::DefinitionEmbedded,
        }
    }
}

/// The canonical definition document a record's definition digest is the digest of.
///
/// # Errors
///
/// Serialization failure without numeric coercion.
pub fn definition_comparison_bytes(
    definition: &EntityDefinition,
) -> Result<Vec<u8>, AsyncStoreError> {
    definition_serialized();
    canonical_domain_bytes(
        DEFINITION_FRAMING,
        serde_json::to_value(definition)
            .map_err(|error| AsyncStoreError::Encoding(error.to_string()))?,
    )
}

/// The digest a record names a definition document by: `sha256:` and 64 lowercase hex digits of
/// SHA-256 over [`DEFINITION_DIGEST_DOMAIN`], a zero byte, the document's length as a big-endian
/// `u64`, and the document.
#[must_use]
pub fn definition_digest(document: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(DEFINITION_DIGEST_DOMAIN.as_bytes());
    hasher.update(&[0]);
    hasher.update(&(document.len() as u64).to_be_bytes());
    hasher.update(document);
    format!("sha256:{}", super::sha256::hex(&hasher.finalize()))
}

/// The definition one entry carries, if it is a decision that carries one.
#[must_use]
pub fn record_definition(entry: &RecordedEntry) -> Option<&EntityDefinition> {
    match entry {
        RecordedEntry::Decision(commit) => commit.envelope.record.definition.as_ref(),
        RecordedEntry::Observation(_) => None,
    }
}

#[cfg(test)]
const DEFINITION_POINTER: &str = "/commit/envelope/record/definition";

/// The digests of definitions their caller already serialized, so that encoding a record that
/// names one of them by digest ([`RECORD_BY_DIGEST`]) serializes it no more.
///
/// A writer serializes each definition its records name once, for the definition blob it binds
/// with them, and has its digest then. The caller vouches that each digest is its definition's.
/// A definition is recognised by identity or by value; one this does not hold is serialized
/// and hashed, once per encoding pass.
#[derive(Debug, Default, Clone)]
pub struct KnownDefinitions<'a> {
    known: Vec<(&'a EntityDefinition, &'a str)>,
}

impl<'a> KnownDefinitions<'a> {
    /// Nothing known: every definition is serialized and hashed, once per encoding pass.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Holds `digest` as the digest of `definition`'s canonical document
    /// ([`definition_comparison_bytes`], [`definition_digest`]).
    pub fn insert(&mut self, definition: &'a EntityDefinition, digest: &'a str) {
        self.known.push((definition, digest));
    }

    fn get(&self, definition: &EntityDefinition) -> Option<&'a str> {
        self.known
            .iter()
            .find(|(known, _)| std::ptr::eq(*known, definition))
            .or_else(|| self.known.iter().find(|(known, _)| *known == definition))
            .map(|(_, digest)| *digest)
    }
}

/// The digests one encoding pass names definitions by.
///
/// A batch's members usually share one definition, and the batch and each member's record are
/// encoded together on every write, so a definition the caller did not already hash is hashed
/// once per pass and recognised by value after that.
struct DefinitionDigests<'k, 'a> {
    known: &'k KnownDefinitions<'a>,
    hashed: Vec<(EntityDefinition, String)>,
}

impl<'k, 'a> DefinitionDigests<'k, 'a> {
    fn new(known: &'k KnownDefinitions<'a>) -> Self {
        Self {
            known,
            hashed: Vec::new(),
        }
    }

    fn digest(&mut self, definition: &EntityDefinition) -> Result<String, AsyncStoreError> {
        if let Some(digest) = self.known.get(definition) {
            return Ok(digest.to_owned());
        }
        if let Some((_, digest)) = self.hashed.iter().find(|(seen, _)| seen == definition) {
            return Ok(digest.clone());
        }
        #[cfg(feature = "kpi-counters")]
        crate::kpi_counters::add(&crate::kpi_counters::BY_DIGEST_DEFINITION_SERIALIZATIONS, 1);
        let digest = definition_digest(&definition_comparison_bytes(definition)?);
        self.hashed.push((definition.clone(), digest.clone()));
        Ok(digest)
    }
}

/// One record's framing and body in `form`.
fn framed_record(
    form: RecordForm,
    entry: &RecordedEntry,
    digests: &mut DefinitionDigests<'_, '_>,
) -> Result<(&'static str, Value), AsyncStoreError> {
    if let (RecordForm::DefinitionByDigest, RecordedEntry::Decision(commit)) = (form, entry) {
        if let Some(definition) = &commit.envelope.record.definition {
            let digest = digests.digest(definition)?;
            return Ok((RECORD_BY_DIGEST, tagged_record_naming(commit, &digest)?));
        }
    }
    Ok((record_domain(entry), tagged_record(entry)?))
}

/// Counts one serialization of a whole definition document: what a record that names its
/// definition by digest exists to stop paying for on every encoding.
fn definition_serialized() {
    #[cfg(test)]
    DEFINITION_SERIALIZATIONS.with(|count| count.set(count.get() + 1));
}

#[cfg(test)]
thread_local! {
    /// Definition documents this thread serialized ([`definition_serialized`]).
    static DEFINITION_SERIALIZATIONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// A decision commit spelled with its definition named by `digest`: serialized field for field as
/// [`RecordedCommit`](crate::RecordedCommit) is, except that the definition is the digest. The
/// definition is not serialized only to be discarded.
///
/// [`commit_naming`] destructures each type whole, so a field added to `RecordedCommit`,
/// `Envelope` or `DecisionRecord` does not compile until it is spelled here too.
#[derive(serde::Serialize)]
struct CommitNaming<'a> {
    instance: &'a entity_core::EntityInstance,
    envelope: EnvelopeNaming<'a>,
}

#[derive(serde::Serialize)]
struct EnvelopeNaming<'a> {
    record_id: &'a str,
    recorded_at: &'a str,
    correlation: &'a Option<String>,
    causation: &'a Option<String>,
    actor: &'a Option<String>,
    record: RecordNaming<'a>,
}

/// `DecisionRecord`'s fields, with the serde attributes it carries.
#[derive(serde::Serialize)]
struct RecordNaming<'a> {
    definition: &'a str,
    command: &'a DecisionCommand,
    entity: &'a str,
    id: &'a str,
    revision: u64,
    from_state: &'a Option<String>,
    to_state: &'a str,
    result: &'a entity_core::EntityInstance,
    changed: &'a Map<String, Value>,
    #[serde(skip_serializing_if = "no_removals")]
    removed: &'a std::collections::BTreeSet<String>,
    events: &'a [entity_core::DomainEvent],
    #[serde(skip_serializing_if = "absent")]
    outcome: &'a Option<String>,
    #[serde(skip_serializing_if = "absent")]
    effect: &'a Option<entity_core::DecisionEffect>,
    #[serde(skip_serializing_if = "absent")]
    response: &'a Option<Map<String, Value>>,
}

#[allow(clippy::trivially_copy_pass_by_ref, clippy::ref_option_ref)]
fn absent<T>(value: &&Option<T>) -> bool {
    value.is_none()
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn no_removals(removed: &&std::collections::BTreeSet<String>) -> bool {
    removed.is_empty()
}

fn commit_naming<'a>(commit: &'a crate::RecordedCommit, digest: &'a str) -> CommitNaming<'a> {
    let crate::RecordedCommit { instance, envelope } = commit;
    let crate::Envelope {
        record_id,
        recorded_at,
        correlation,
        causation,
        actor,
        record,
    } = envelope;
    let entity_core::DecisionRecord {
        definition: _,
        command,
        entity,
        id,
        revision,
        from_state,
        to_state,
        result,
        changed,
        removed,
        events,
        outcome,
        effect,
        response,
    } = record;
    CommitNaming {
        instance,
        envelope: EnvelopeNaming {
            record_id,
            recorded_at,
            correlation,
            causation,
            actor,
            record: RecordNaming {
                definition: digest,
                command,
                entity,
                id,
                revision: *revision,
                from_state,
                to_state,
                result,
                changed,
                removed,
                events,
                outcome,
                effect,
                response,
            },
        },
    }
}

/// The tagged body of a decision record that names its definition by `digest`.
fn tagged_record_naming(
    commit: &crate::RecordedCommit,
    digest: &str,
) -> Result<Value, AsyncStoreError> {
    let mut tagged = Map::new();
    tagged.insert("kind".to_owned(), Value::String("decision".to_owned()));
    tagged.insert(
        "commit".to_owned(),
        serde_json::to_value(commit_naming(commit, digest))
            .map_err(|error| AsyncStoreError::Encoding(error.to_string()))?,
    );
    Ok(Value::Object(tagged))
}

/// Encodes one decision record in [`RECORD_BY_DIGEST`] naming `digest`, without hashing.
///
/// For a provider that has just resolved `digest` to a definition blob, held that blob to its
/// digest, and decoded the entry's definition from it: re-encoding the record to check its stored
/// bytes then needs neither the definition serialized again nor hashed again. The definition the
/// entry carries, if any, is not read, not even to be serialized and discarded; the caller vouches
/// that `digest` is its digest.
///
/// # Errors
///
/// [`AsyncStoreError::Encoding`] for an observation, or serialization failure.
pub fn record_comparison_bytes_naming(
    entry: &RecordedEntry,
    digest: &str,
) -> Result<Vec<u8>, AsyncStoreError> {
    let RecordedEntry::Decision(commit) = entry else {
        return Err(AsyncStoreError::Encoding(
            "only a decision record names a definition".to_owned(),
        ));
    };
    canonical_domain_bytes(RECORD_BY_DIGEST, tagged_record_naming(commit, digest)?)
}

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
            if commit.envelope.record.definition.is_some() {
                definition_serialized();
            }
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

/// The record framing one entry is spelled in while its definition is embedded
/// ([`RecordForm::DefinitionEmbedded`]), and the semantic domain its request framing follows.
///
/// A `service/1` decision's record carries keys `er.record/1` has never carried — `outcome`,
/// `effect`, `response`, and a `create` command with an `arguments` key. The governing rule for
/// these formats is that a change to their shape, framing or scalar spelling needs a new version
/// domain, so that is what a `service/1` decision takes. A `kernel/1` decision and every
/// observation keep the bytes they have. A decision that carries a definition is written in
/// [`RECORD_BY_DIGEST`] since 0.33.0; this domain still selects its request framing.
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

/// Encodes one complete typed record in the form new records are written in: a decision that
/// carries a definition names it by digest ([`RECORD_BY_DIGEST`]); every other record in the
/// framing [`record_domain`] gives it.
///
/// # Errors
///
/// Serialization failure without numeric coercion.
pub fn record_comparison_bytes(entry: &RecordedEntry) -> Result<Vec<u8>, AsyncStoreError> {
    record_comparison_bytes_in(RecordForm::DefinitionByDigest, entry)
}

/// Encodes one complete typed record in `form`.
///
/// A record read from a store is compared in the form it was stored in
/// ([`RecordForm::of_record`]), so a record written before 0.33.0 keeps its bytes and digest.
///
/// # Errors
///
/// Serialization failure without numeric coercion.
pub fn record_comparison_bytes_in(
    form: RecordForm,
    entry: &RecordedEntry,
) -> Result<Vec<u8>, AsyncStoreError> {
    record_comparison_bytes_known(form, entry, &KnownDefinitions::new())
}

/// [`record_comparison_bytes_in`], naming a definition `known` holds by the digest it holds for
/// it rather than serializing and hashing the definition again.
///
/// # Errors
///
/// Serialization failure without numeric coercion.
pub fn record_comparison_bytes_known(
    form: RecordForm,
    entry: &RecordedEntry,
    known: &KnownDefinitions<'_>,
) -> Result<Vec<u8>, AsyncStoreError> {
    let (domain, body) = framed_record(form, entry, &mut DefinitionDigests::new(known))?;
    canonical_domain_bytes(domain, body)
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

/// Encodes one complete ordered batch in the normative `er.batch/1` domain, each member in the
/// form new records are written in (see [`record_comparison_bytes`]).
///
/// # Errors
///
/// Serialization failure without numeric coercion.
pub fn batch_comparison_bytes(
    key: &BatchKey,
    members: &[AppendMember],
) -> Result<Vec<u8>, AsyncStoreError> {
    batch_comparison_bytes_in(RecordForm::DefinitionByDigest, key, members)
}

/// Encodes one complete ordered batch with every member record in `form`.
///
/// A stored batch is compared in the form it was stored in ([`RecordForm::of_batch`]), so a retry
/// of a batch written before 0.33.0 still finds it the same batch.
///
/// # Errors
///
/// Serialization failure without numeric coercion.
pub fn batch_comparison_bytes_in(
    form: RecordForm,
    key: &BatchKey,
    members: &[AppendMember],
) -> Result<Vec<u8>, AsyncStoreError> {
    batch_comparison_bytes_known(form, key, members, &KnownDefinitions::new())
}

/// [`batch_comparison_bytes_in`], naming each definition `known` holds by the digest it holds for
/// it rather than serializing and hashing the definition again.
///
/// # Errors
///
/// Serialization failure without numeric coercion.
pub fn batch_comparison_bytes_known(
    form: RecordForm,
    key: &BatchKey,
    members: &[AppendMember],
    known: &KnownDefinitions<'_>,
) -> Result<Vec<u8>, AsyncStoreError> {
    key.validate()?;
    let mut digests = DefinitionDigests::new(known);
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
        let (domain, body) = framed_record(form, &member.entry, &mut digests)?;
        encoded.push(serde_json::json!({
            "expect": expectation,
            "record": [domain, body]
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

    /// A `service/1` decision and an observation of it, as a store receives them.
    fn service_entries() -> (RecordedEntry, RecordedEntry) {
        let definition = serde_json::from_value(serde_json::json!({
            "entity": "invocation",
            "version": 1,
            "semantics": "service/1",
            "identity": { "field": "invocation_id" },
            "schema": { "fields": {
                "invocation_id": { "type": "string", "required": true },
                "operation": { "type": "string", "required": true }
            }},
            "lifecycle": { "initial": "settled", "states": ["settled"] },
            "create": { "emit": { "type": "Invoked", "payload": { "op": "$fields.operation" } } }
        }))
        .expect("definition parses");
        let mut registry = entity_core::Registry::new();
        registry.register(definition).expect("registers");
        let decision = entity_core::Runtime::new(&registry)
            .create(
                "invocation",
                1,
                "s:inv-1",
                serde_json::json!({ "invocation_id": "inv-1", "operation": "read" }),
            )
            .expect("creates");
        let recording = |id: &str| crate::Recording {
            record_id: id.to_owned(),
            recorded_at: "2026-11-23T00:00:00Z".to_owned(),
            correlation: None,
            causation: None,
            actor: None,
        };
        let commit = crate::RecordedCommit::new(decision, &recording("create")).expect("records");
        let observation = RecordedEntry::Observation(crate::RecordedObservation {
            entity: commit.instance.entity.clone(),
            id: commit.instance.id.clone(),
            revision: 1,
            envelope: recording("observe")
                .seal(serde_json::json!({ "seen": true }))
                .expect("seals"),
        });
        (RecordedEntry::Decision(commit), observation)
    }

    /// The bytes a decision record holds name its definition by the digest of the definition
    /// document, and are otherwise the embedded record's bytes: the definition is the only thing
    /// taken out. An observation names nothing and keeps its framing in both forms.
    #[test]
    fn a_decision_record_holds_its_definition_digest_and_nothing_else_of_the_definition() {
        let (decision, observation) = service_entries();
        let definition = record_definition(&decision).expect("carries a definition");
        let document = definition_comparison_bytes(definition).expect("definition document");
        let digest = definition_digest(&document);
        assert!(
            digest.starts_with("sha256:") && digest.len() == 71,
            "{digest}"
        );

        let embedded =
            record_comparison_bytes_in(RecordForm::DefinitionEmbedded, &decision).expect("embeds");
        let named = record_comparison_bytes(&decision).expect("names");
        assert_eq!(record_framing(&embedded).expect("framed"), "er.record/2");
        assert_eq!(record_framing(&named).expect("framed"), RECORD_BY_DIGEST);
        let body = read_record_in_domain(RECORD_BY_DIGEST, &named).expect("reads");
        assert_eq!(
            body.pointer(DEFINITION_POINTER),
            Some(&Value::String(digest.clone()))
        );
        // Exactly the definition's own body (the document minus its framing) became the digest.
        let definition_body = &document[DEFINITION_FRAMING.len() + 4..document.len() - 1];
        let expected = String::from_utf8(embedded.clone())
            .expect("utf-8")
            .replacen("er.record/2", RECORD_BY_DIGEST, 1)
            .replacen(
                std::str::from_utf8(definition_body).expect("utf-8"),
                &format!("\"{digest}\""),
                1,
            );
        assert_eq!(String::from_utf8(named.clone()).expect("utf-8"), expected);
        assert_eq!(
            record_comparison_bytes_naming(&decision, &digest).expect("names"),
            named
        );
        assert!(matches!(
            record_comparison_bytes_naming(&observation, &digest),
            Err(AsyncStoreError::Encoding(_))
        ));
        assert_eq!(
            record_comparison_bytes(&observation).expect("encodes"),
            record_comparison_bytes_in(RecordForm::DefinitionEmbedded, &observation)
                .expect("encodes")
        );
        assert_eq!(
            RecordForm::of_record(&named),
            RecordForm::DefinitionByDigest
        );
        assert_eq!(
            RecordForm::of_record(&embedded),
            RecordForm::DefinitionEmbedded
        );

        let members = vec![
            AppendMember::new(crate::Expect::Absent, decision.clone(), Vec::new()),
            AppendMember::new(crate::Expect::Revision(1), observation, Vec::new()),
        ];
        let key = BatchKey::Named("pair".to_owned());
        let batch = batch_comparison_bytes(&key, &members).expect("batch");
        let old = batch_comparison_bytes_in(RecordForm::DefinitionEmbedded, &key, &members)
            .expect("batch");
        assert_eq!(RecordForm::of_batch(&batch), RecordForm::DefinitionByDigest);
        assert_eq!(RecordForm::of_batch(&old), RecordForm::DefinitionEmbedded);
        let text = String::from_utf8(batch).expect("utf-8");
        assert!(text.contains(&String::from_utf8(named).expect("utf-8")));
        assert!(!text.contains("\"lifecycle\""), "{text}");
    }

    /// A record written before 0.33.0 is re-encoded to exactly its stored bytes in the embedded
    /// form, so its digest, and every chain over it, does not move.
    #[test]
    fn the_embedded_form_reproduces_a_record_written_before_records_named_their_definitions() {
        let stored = include_str!("../../tests/fixtures/service_2_record_3.json").trim_end();
        let document: Value = serde_json::from_str(stored).expect("fixture parses");
        let commit: crate::RecordedCommit =
            serde_json::from_value(document[1]["commit"].clone()).expect("decodes");
        let entry = RecordedEntry::Decision(commit);
        assert_eq!(
            record_comparison_bytes_in(RecordForm::DefinitionEmbedded, &entry).expect("encodes"),
            stored.as_bytes()
        );
        assert_eq!(
            RecordForm::of_record(stored.as_bytes()),
            RecordForm::DefinitionEmbedded
        );
        assert_eq!(
            record_framing(&record_comparison_bytes(&entry).expect("encodes")).expect("framed"),
            RECORD_BY_DIGEST
        );
    }

    /// Definition documents serialized while `work` ran on this thread.
    fn serialized(work: impl FnOnce()) -> u64 {
        let before = DEFINITION_SERIALIZATIONS.with(std::cell::Cell::get);
        work();
        DEFINITION_SERIALIZATIONS.with(std::cell::Cell::get) - before
    }

    /// Encoding a record that names its definition by digest (`er.record/5`) serializes no
    /// definition document: not when it is given the digest, as a read that re-encodes a stored
    /// record is, and not when its caller already knows the digest, as a write that serialized the
    /// definition once for its blob does. Only a definition nobody hashed yet is serialized, once
    /// per encoding pass, and the embedded form serializes it as part of the record.
    #[test]
    fn encoding_a_record_by_digest_serializes_no_definition_document() {
        let (decision, _) = service_entries();
        let definition = record_definition(&decision).expect("carries a definition");
        let digest = definition_digest(&definition_comparison_bytes(definition).expect("document"));
        let mut known = KnownDefinitions::new();
        known.insert(definition, &digest);
        let key = BatchKey::Named("serializations".to_owned());
        let members = [
            AppendMember::new(crate::Expect::Absent, decision.clone(), Vec::new()),
            AppendMember::new(crate::Expect::Absent, decision.clone(), Vec::new()),
        ];

        assert_eq!(
            serialized(|| {
                record_comparison_bytes_naming(&decision, &digest).expect("names");
            }),
            0,
            "a record re-encoded under the digest it names"
        );
        assert_eq!(
            serialized(|| {
                record_comparison_bytes_known(RecordForm::DefinitionByDigest, &decision, &known)
                    .expect("names");
            }),
            0,
            "a record whose definition the writer already hashed"
        );
        assert_eq!(
            serialized(|| {
                batch_comparison_bytes_known(
                    RecordForm::DefinitionByDigest,
                    &key,
                    &members,
                    &known,
                )
                .expect("names");
            }),
            0,
            "a batch whose definitions the writer already hashed"
        );
        assert_eq!(
            serialized(|| {
                batch_comparison_bytes(&key, &members).expect("names");
            }),
            1,
            "a batch hashes a definition nobody hashed once for all its members"
        );
        assert_eq!(
            serialized(|| {
                record_comparison_bytes_in(RecordForm::DefinitionEmbedded, &decision)
                    .expect("embeds");
            }),
            1,
            "the embedded form carries the definition"
        );
        assert_eq!(
            record_comparison_bytes_known(RecordForm::DefinitionByDigest, &decision, &known)
                .expect("names"),
            record_comparison_bytes(&decision).expect("names"),
            "the known digest names the record as hashing it does"
        );
    }
}
