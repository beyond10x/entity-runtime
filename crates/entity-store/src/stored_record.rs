//! Decision records that name their definition by digest, for the stores that keep one document
//! per record in a `history` table: `entity-sqlite` and `entity-postgres`.
//!
//! A [`DecisionRecord`] carries the whole definition snapshot its decision ran under, and every
//! record of one entity type carries the same one. Up to 0.33.x those stores wrote each record as
//! `serde_json` of its [`Envelope`], definition included: the *embedded* form. New records are
//! written in the *by-digest* form instead,
//!
//! ```text
//! ["er.stored-record/1", "sha256:…", { …the envelope, with "definition": null… }]
//! ```
//!
//! and the definition document is kept once per store under its digest. The document and the
//! digest are the ones the Eventlog adapter uses for `er.record/5`
//! ([`definition_comparison_bytes`], [`definition_digest`]), so one definition has one name in
//! every provider. A record without a definition is written as `["er.stored-record/1", null, …]`.
//!
//! Records already stored keep their bytes: a reader takes each document in the form it is in
//! ([`StoredForm::of`]), and a retry is compared in the stored record's own form
//! ([`same_record`]). An older reader refuses a by-digest document instead of misreading it: it
//! reads the array as an envelope by position, and the object where `correlation` belongs is not
//! a string.

use std::collections::BTreeMap;

use entity_core::{DecisionRecord, DomainEvent, EntityDefinition};

use crate::asynchronous::{definition_comparison_bytes, definition_digest, DEFINITION_FRAMING};
use crate::{Envelope, StoreError};

/// The framing of a decision record document in the by-digest form.
pub const STORED_RECORD_BY_DIGEST: &str = "er.stored-record/1";

/// The first bytes of every by-digest document, and of no embedded one: an embedded document is a
/// JSON object.
const BY_DIGEST_PREFIX: &str = "[\"er.stored-record/1\",";

/// How one stored decision document spells its definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredForm {
    /// The definition is inside the record: every record written before 0.34.0.
    Embedded,
    /// The definition is named by digest ([`STORED_RECORD_BY_DIGEST`]): every new record.
    ByDigest,
}

impl StoredForm {
    /// The form of one stored decision document, read from its first bytes.
    #[must_use]
    pub fn of(document: &str) -> Self {
        if document.starts_with(BY_DIGEST_PREFIX) {
            Self::ByDigest
        } else {
            Self::Embedded
        }
    }
}

/// A definition document kept once per store, under its digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredDefinition {
    /// `sha256:` and 64 lowercase hex digits ([`definition_digest`]).
    pub digest: String,
    /// The canonical `["er.definition/1", <definition>]` document the digest is the digest of.
    pub document: String,
}

/// One decision record as a store writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredDecision {
    /// The history document, in the by-digest form.
    pub document: String,
    /// The definition the document names, to be kept beside it; `None` when the record has none.
    pub definition: Option<StoredDefinition>,
}

fn encoding(error: &impl std::fmt::Display) -> StoreError {
    StoreError::Backend(format!("serialising the decision envelope: {error}"))
}

/// The definition document and digest of `definition`.
///
/// # Errors
///
/// [`StoreError::Backend`] when the definition does not serialize.
pub fn stored_definition(definition: &EntityDefinition) -> Result<StoredDefinition, StoreError> {
    let bytes = definition_comparison_bytes(definition).map_err(|error| encoding(&error))?;
    let digest = definition_digest(&bytes);
    let document = String::from_utf8(bytes).map_err(|error| encoding(&error))?;
    Ok(StoredDefinition { digest, document })
}

/// `envelope` in the by-digest form, and the definition it names.
///
/// # Errors
///
/// [`StoreError::Backend`] when the envelope does not serialize.
pub fn store_decision(envelope: &Envelope<DecisionRecord>) -> Result<StoredDecision, StoreError> {
    let definition = envelope
        .record
        .definition
        .as_ref()
        .map(stored_definition)
        .transpose()?;
    let document = by_digest_document(
        envelope,
        definition
            .as_ref()
            .map(|definition| definition.digest.as_str()),
    )?;
    Ok(StoredDecision {
        document,
        definition,
    })
}

fn by_digest_document(
    envelope: &Envelope<DecisionRecord>,
    digest: Option<&str>,
) -> Result<String, StoreError> {
    let mut named = envelope.clone();
    named.record.definition = None;
    serde_json::to_string(&(STORED_RECORD_BY_DIGEST, digest, &named))
        .map_err(|error| encoding(&error))
}

/// `envelope` in the embedded form every store wrote before 0.34.0.
///
/// # Errors
///
/// [`StoreError::Backend`] when the envelope does not serialize.
pub fn embedded_document(envelope: &Envelope<DecisionRecord>) -> Result<String, StoreError> {
    serde_json::to_string(envelope).map_err(|error| encoding(&error))
}

/// Whether `existing`, a stored decision document, is `envelope`: compared in the form `existing`
/// is in, so a retry of a record written before 0.34.0 is still the same record.
///
/// `stored` is `envelope` in the by-digest form, as [`store_decision`] wrote it.
///
/// # Errors
///
/// [`StoreError::Backend`] when the envelope does not serialize.
pub fn same_record(
    existing: &str,
    envelope: &Envelope<DecisionRecord>,
    stored: &StoredDecision,
) -> Result<bool, StoreError> {
    Ok(match StoredForm::of(existing) {
        StoredForm::ByDigest => existing == stored.document,
        StoredForm::Embedded => existing == embedded_document(envelope)?,
    })
}

fn parsing(error: &impl std::fmt::Display) -> StoreError {
    StoreError::Backend(format!("parsing a history record: {error}"))
}

/// The definitions one read has already resolved and checked, by digest.
///
/// One subject's records, and one listing's subjects, usually name one definition; it is fetched,
/// hashed and decoded once per read rather than once per record.
#[derive(Debug, Default)]
pub struct DefinitionCache(BTreeMap<String, EntityDefinition>);

impl DefinitionCache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The definition stored under `digest`, fetched with `fetch` when this cache has not seen it.
    ///
    /// The fetched document must hash to `digest`, be a `["er.definition/1", <definition>]`
    /// document, and be exactly the canonical bytes of the definition it decodes to.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the store holds no definition under `digest`, or one that
    /// fails any of those checks, or when `fetch` fails.
    pub fn resolve(
        &mut self,
        digest: &str,
        fetch: impl FnOnce(&str) -> Result<Option<String>, StoreError>,
    ) -> Result<EntityDefinition, StoreError> {
        if let Some(definition) = self.0.get(digest) {
            return Ok(definition.clone());
        }
        let document = fetch(digest)?.ok_or_else(|| {
            StoreError::Backend(format!(
                "a history record names definition {digest}, which this store does not hold"
            ))
        })?;
        let definition = checked_definition(digest, &document)?;
        self.0.insert(digest.to_owned(), definition.clone());
        Ok(definition)
    }
}

fn checked_definition(digest: &str, document: &str) -> Result<EntityDefinition, StoreError> {
    let integrity = |detail: &str| {
        StoreError::Backend(format!(
            "the definition stored under {digest} {detail}; the store was changed outside this \
             provider"
        ))
    };
    if definition_digest(document.as_bytes()) != digest {
        return Err(integrity("does not hash to its digest"));
    }
    let (framing, definition): (String, EntityDefinition) =
        serde_json::from_str(document).map_err(|_| integrity("is not a definition document"))?;
    if framing != DEFINITION_FRAMING {
        return Err(integrity("is not a definition document"));
    }
    let canonical = definition_comparison_bytes(&definition)
        .map_err(|_| integrity("does not serialize again"))?;
    if canonical != document.as_bytes() {
        return Err(integrity(
            "is not the canonical document of what it decodes to",
        ));
    }
    Ok(definition)
}

type ByDigest = (String, Option<String>, Envelope<DecisionRecord>);

fn by_digest(document: &str) -> Result<(Option<String>, Envelope<DecisionRecord>), StoreError> {
    let (framing, digest, envelope): ByDigest =
        serde_json::from_str(document).map_err(|error| parsing(&error))?;
    if framing != STORED_RECORD_BY_DIGEST {
        return Err(parsing(&format!("unknown framing {framing:?}")));
    }
    if digest.is_some() && envelope.record.definition.is_some() {
        return Err(parsing(
            &"a record names its definition by digest and also embeds one",
        ));
    }
    Ok((digest, envelope))
}

/// One stored decision document as its envelope, in either form; a by-digest document's
/// definition is resolved through `cache`, fetching with `fetch`.
///
/// # Errors
///
/// [`StoreError::Backend`] when the document is not a decision record in either form, or its
/// definition does not resolve ([`DefinitionCache::resolve`]).
pub fn read_decision(
    document: &str,
    cache: &mut DefinitionCache,
    fetch: impl FnOnce(&str) -> Result<Option<String>, StoreError>,
) -> Result<Envelope<DecisionRecord>, StoreError> {
    match StoredForm::of(document) {
        StoredForm::Embedded => serde_json::from_str(document).map_err(|error| parsing(&error)),
        StoredForm::ByDigest => {
            let (digest, mut envelope) = by_digest(document)?;
            if let Some(digest) = digest {
                envelope.record.definition = Some(cache.resolve(&digest, fetch)?);
            }
            Ok(envelope)
        }
    }
}

/// The events of one stored decision document in either form, without resolving its definition.
///
/// # Errors
///
/// [`StoreError::Backend`] when the document is not a decision record in either form.
pub fn decision_events(document: &str) -> Result<Vec<DomainEvent>, StoreError> {
    let envelope: Envelope<DecisionRecord> = match StoredForm::of(document) {
        StoredForm::Embedded => serde_json::from_str(document).map_err(|error| parsing(&error))?,
        StoredForm::ByDigest => by_digest(document)?.1,
    };
    Ok(envelope.record.events)
}
