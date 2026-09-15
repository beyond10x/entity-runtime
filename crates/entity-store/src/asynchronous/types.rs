use std::fmt;

use entity_core::{DecisionCommand, DecisionRecord, DomainEvent, EntityInstance};
use serde::{Deserialize, Serialize};

use crate::{Expect, RecordedCommit, RecordedObservation};

/// One entity instance addressed by its opaque entity name and instance identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subject {
    /// The declared entity name.
    pub entity: String,
    /// The caller-supplied opaque instance identity.
    pub id: String,
}

impl Subject {
    /// Constructs a nonblank subject without changing either component.
    ///
    /// # Errors
    ///
    /// [`AsyncStoreError::InvalidInput`] when either component is blank.
    pub fn new(entity: impl Into<String>, id: impl Into<String>) -> Result<Self, AsyncStoreError> {
        let subject = Self {
            entity: entity.into(),
            id: id.into(),
        };
        subject.validate()?;
        Ok(subject)
    }

    /// Checks the two opaque identity components.
    ///
    /// # Errors
    ///
    /// [`AsyncStoreError::InvalidInput`] when either component is blank.
    pub fn validate(&self) -> Result<(), AsyncStoreError> {
        if self.entity.trim().is_empty() || self.id.trim().is_empty() {
            return Err(AsyncStoreError::InvalidInput(
                "a subject requires nonblank entity and instance identities".to_owned(),
            ));
        }
        Ok(())
    }

    /// Returns the normative injective compact-JSON coordinate identity.
    #[must_use]
    pub fn coordinate_id(&self) -> String {
        serde_json::to_string(&[&self.entity, &self.id])
            .expect("two strings always serialize as JSON")
    }
}

/// The disjoint namespace used to claim one record or an ordered named batch.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchKey {
    /// A one-record request keyed by that record's global identity.
    SingleRecord(String),
    /// A caller-named ordered batch.
    Named(String),
}

impl BatchKey {
    /// Returns the caller's opaque key value.
    #[must_use]
    pub fn value(&self) -> &str {
        match self {
            Self::SingleRecord(value) | Self::Named(value) => value,
        }
    }

    /// Checks that a nonempty request has a nonblank key.
    ///
    /// # Errors
    ///
    /// [`AsyncStoreError::InvalidInput`] for a blank key.
    pub fn validate(&self) -> Result<(), AsyncStoreError> {
        if self.value().trim().is_empty() {
            return Err(AsyncStoreError::InvalidInput(
                "a nonempty recorded request requires a nonblank batch key".to_owned(),
            ));
        }
        Ok(())
    }

    /// Returns the normative compact-JSON coordinate identity.
    ///
    /// # Errors
    ///
    /// [`AsyncStoreError::InvalidInput`] for a blank key.
    pub fn coordinate_id(&self) -> Result<String, AsyncStoreError> {
        self.validate()?;
        let value = match self {
            Self::SingleRecord(value) => serde_json::json!(["single_record", value]),
            Self::Named(value) => serde_json::json!(["named", value]),
        };
        serde_json::to_string(&value).map_err(|error| AsyncStoreError::Encoding(error.to_string()))
    }
}

/// Which complete record occupies one physical history position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    /// A state-changing kernel decision.
    Decision,
    /// Non-state-changing evidence about one exact revision.
    Observation,
}

/// A closed complete record accepted by the asynchronous store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(
    clippy::large_enum_variant,
    reason = "the contract keeps each complete recorded payload inline and closed"
)]
pub enum RecordedEntry {
    /// A complete recorded kernel decision.
    Decision(RecordedCommit),
    /// A complete non-state-changing observation.
    Observation(RecordedObservation),
}

impl RecordedEntry {
    /// Returns the record's subject.
    #[must_use]
    pub fn subject(&self) -> Subject {
        match self {
            Self::Decision(commit) => Subject {
                entity: commit.instance.entity.clone(),
                id: commit.instance.id.clone(),
            },
            Self::Observation(observation) => Subject {
                entity: observation.entity.clone(),
                id: observation.id.clone(),
            },
        }
    }

    /// Returns the globally unique caller-supplied record identity.
    #[must_use]
    pub fn record_id(&self) -> &str {
        match self {
            Self::Decision(commit) => &commit.envelope.record_id,
            Self::Observation(observation) => &observation.envelope.record_id,
        }
    }

    /// Returns the entity revision this entry records.
    #[must_use]
    pub fn revision(&self) -> u64 {
        match self {
            Self::Decision(commit) => commit.instance.revision,
            Self::Observation(observation) => observation.revision,
        }
    }

    /// Returns the closed record kind.
    #[must_use]
    pub const fn kind(&self) -> RecordKind {
        match self {
            Self::Decision(_) => RecordKind::Decision,
            Self::Observation(_) => RecordKind::Observation,
        }
    }

    /// Returns the nested ordered domain events, or an empty slice for an observation.
    #[must_use]
    pub fn events(&self) -> &[DomainEvent] {
        match self {
            Self::Decision(commit) => &commit.envelope.record.events,
            Self::Observation(_) => &[],
        }
    }

    /// Checks the complete record's own structural invariants.
    ///
    /// # Errors
    ///
    /// A typed invalid-input or corruption error.
    pub fn validate(&self) -> Result<(), AsyncStoreError> {
        self.subject().validate()?;
        match self {
            Self::Decision(commit) => commit
                .validate()
                .map_err(|error| AsyncStoreError::InvalidInput(error.to_string())),
            Self::Observation(observation) => observation
                .validate()
                .map_err(|error| AsyncStoreError::InvalidInput(error.to_string())),
        }
    }

    /// Returns the decision command when this is a decision.
    #[must_use]
    pub fn decision_command(&self) -> Option<&DecisionCommand> {
        match self {
            Self::Decision(commit) => Some(&commit.envelope.record.command),
            Self::Observation(_) => None,
        }
    }
}

/// Distinct subject-local and store-wide physical positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordPosition {
    /// Monotone physical position within one subject's post-genesis or post-anchor history.
    pub subject: u64,
    /// Monotone physical position within the logical store.
    pub store: u64,
}

/// The immutable receipt for one newly accepted stored record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordReceipt {
    /// The global record identity.
    pub record_id: String,
    /// The record's subject.
    pub subject: Subject,
    /// The record's closed kind.
    pub kind: RecordKind,
    /// The entity revision recorded.
    pub revision: u64,
    /// Subject-local and store-global physical positions.
    pub position: RecordPosition,
    /// The key under which this member was originally committed.
    pub batch_key: BatchKey,
    /// Its checked index in the original request.
    pub member_index: u64,
}

/// The immutable receipt for one caller-named batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchReceipt {
    /// The original named key.
    pub key: BatchKey,
    /// Ordered immutable member receipts.
    pub members: Vec<RecordReceipt>,
}

/// A receipt preserves whether storage accepted one record or a named batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitReceipt {
    /// One record, including a member originally committed in a named batch.
    Single(RecordReceipt),
    /// One complete caller-named batch.
    Batch(BatchReceipt),
}

impl CommitReceipt {
    /// Returns the receipt members without erasing the receipt variant.
    #[must_use]
    pub fn members(&self) -> &[RecordReceipt] {
        match self {
            Self::Single(receipt) => std::slice::from_ref(receipt),
            Self::Batch(receipt) => &receipt.members,
        }
    }
}

/// What an asynchronous append established.
#[derive(Debug, Clone, PartialEq)]
#[allow(
    clippy::large_enum_variant,
    reason = "historical outcomes return complete preserved evidence without another public wrapper"
)]
pub enum AppendOutcome {
    /// An empty batch performed no store operation.
    Empty,
    /// Fresh or replay-recovered storage acceptance.
    Committed {
        /// The original immutable receipt.
        receipt: CommitReceipt,
        /// Whether this response recovered an already committed identity.
        replayed: bool,
    },
    /// Matching imported evidence without an invented committed receipt.
    Historical {
        /// The preserved typed evidence.
        evidence: ImportedRecordEvidence,
        /// The explicitly limited assurance boundary.
        assurance: SubjectAssurance,
    },
}

impl AppendOutcome {
    /// Returns a committed receipt, never one fabricated for historical evidence.
    #[must_use]
    pub const fn receipt(&self) -> Option<&CommitReceipt> {
        match self {
            Self::Committed { receipt, .. } => Some(receipt),
            Self::Empty | Self::Historical { .. } => None,
        }
    }

    /// Whether the response recovered an already committed effect.
    #[must_use]
    pub const fn replayed(&self) -> bool {
        matches!(self, Self::Committed { replayed: true, .. })
    }
}

/// One complete record proposed to an atomic append.
#[derive(Debug, Clone, PartialEq)]
pub struct AppendMember {
    /// The transaction-local predecessor expectation.
    pub expect: Expect,
    /// The complete decision or observation.
    pub entry: RecordedEntry,
    /// Canonical `er.request/1` bytes used for exact retry comparison.
    pub request_bytes: Vec<u8>,
}

impl AppendMember {
    /// Pairs an expectation, complete entry, and exact request comparison bytes.
    #[must_use]
    pub fn new(expect: Expect, entry: RecordedEntry, request_bytes: Vec<u8>) -> Self {
        Self {
            expect,
            entry,
            request_bytes,
        }
    }
}

/// One ordered atomic append request.
#[derive(Debug, Clone, PartialEq)]
pub struct AppendRequest {
    /// No key for the uniquely inert empty request.
    pub key: Option<BatchKey>,
    /// Complete members in caller order.
    pub members: Vec<AppendMember>,
}

impl AppendRequest {
    /// Constructs and validates one nonempty append request.
    ///
    /// # Errors
    ///
    /// Invalid keys, a malformed single-record namespace, or duplicate record identities.
    pub fn new(key: BatchKey, members: Vec<AppendMember>) -> Result<Self, AsyncStoreError> {
        let request = Self {
            key: Some(key),
            members,
        };
        request.validate()?;
        Ok(request)
    }

    /// Constructs the unique inert request, which owns no key and performs no IO.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            key: None,
            members: Vec::new(),
        }
    }

    /// Checks the complete public request shape and comparison material.
    ///
    /// This validator is also enforced by every writer implementation because callers may
    /// directly construct or later mutate the public key and member fields.
    ///
    /// # Errors
    ///
    /// Invalid empty/nonempty shape, keys, single-record identity, members or comparison bytes,
    /// or a record identity repeated anywhere in the request.
    pub fn validate(&self) -> Result<(), AsyncStoreError> {
        let key = match (&self.key, self.members.is_empty()) {
            (None, true) => return Ok(()),
            (None, false) => {
                return Err(AsyncStoreError::InvalidInput(
                    "nonempty append has no batch key".to_owned(),
                ));
            }
            (Some(_), true) => {
                return Err(AsyncStoreError::InvalidInput(
                    "use AppendRequest::empty for an inert empty batch".to_owned(),
                ));
            }
            (Some(key), false) => key,
        };
        key.validate()?;
        if let BatchKey::SingleRecord(record_id) = key {
            if self.members.len() != 1 || self.members[0].entry.record_id() != record_id {
                return Err(AsyncStoreError::InvalidInput(
                    "SingleRecord requires exactly one member with the same record id".to_owned(),
                ));
            }
        }

        let mut ids = std::collections::BTreeSet::new();
        for member in &self.members {
            if !ids.insert(member.entry.record_id()) {
                return Err(AsyncStoreError::DuplicateRecordId {
                    record_id: member.entry.record_id().to_owned(),
                });
            }
        }
        for member in &self.members {
            member.entry.validate()?;
            if super::original_request_comparison_bytes(&member.entry)? != member.request_bytes {
                return Err(AsyncStoreError::InvalidInput(format!(
                    "request bytes do not reproduce complete record {:?}",
                    member.entry.record_id()
                )));
            }
        }
        Ok(())
    }
}

/// One record and the exact coordinates and comparison material accepted with it.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredRecord {
    /// The complete typed record.
    pub entry: RecordedEntry,
    /// Its physical coordinates.
    pub position: RecordPosition,
    /// Its immutable original receipt.
    pub receipt: RecordReceipt,
    /// The original expectation.
    pub expect: Expect,
    /// Exact canonical request comparison bytes.
    pub request_bytes: Vec<u8>,
    /// Exact canonical complete-record comparison bytes.
    pub record_bytes: Vec<u8>,
}

impl StoredRecord {
    /// Returns the record kind.
    #[must_use]
    pub const fn kind(&self) -> RecordKind {
        self.entry.kind()
    }
}

/// One durable named or single-record claim and its complete original members.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredBatch {
    /// Original key.
    pub key: BatchKey,
    /// Ordered complete members.
    pub records: Vec<StoredRecord>,
    /// Exact `er.batch/1` comparison bytes.
    pub comparison_bytes: Vec<u8>,
    /// Original immutable receipt.
    pub receipt: CommitReceipt,
}

/// The known ordering of one preserved legacy envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnownLegacyOrder {
    /// Position within the legacy decision or observation kind only.
    PerKind(u64),
    /// Known mixed position within one subject.
    Subject(u64),
}

/// Complete preserved envelope evidence that owns no new receipt coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportedRecordEvidence {
    /// The complete preserved envelope.
    pub entry: RecordedEntry,
    /// Caller-supplied source/import identity.
    pub source_id: String,
    /// Exact locator within that source.
    pub source_locator: String,
    /// Only the ordering the source actually established.
    pub known_order: KnownLegacyOrder,
}

impl ImportedRecordEvidence {
    /// Constructs complete imported evidence without minting receipt or global-order facts.
    ///
    /// # Errors
    ///
    /// Blank source coordinates or an invalid record.
    pub fn new(
        entry: RecordedEntry,
        source_id: impl Into<String>,
        source_locator: impl Into<String>,
        known_order: KnownLegacyOrder,
    ) -> Result<Self, AsyncStoreError> {
        entry.validate()?;
        let evidence = Self {
            entry,
            source_id: source_id.into(),
            source_locator: source_locator.into(),
            known_order,
        };
        if evidence.source_id.trim().is_empty() || evidence.source_locator.trim().is_empty() {
            return Err(AsyncStoreError::InvalidInput(
                "imported evidence requires nonblank source identity and locator".to_owned(),
            ));
        }
        Ok(evidence)
    }
}

/// Evidence that may be retained at an explicit legacy boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(
    clippy::large_enum_variant,
    reason = "legacy evidence preserves the existing complete concrete payload types"
)]
pub enum LegacyEvidence {
    /// An available complete envelope whose global record identity is reserved.
    Envelope(ImportedRecordEvidence),
    /// A bare complete decision without an available envelope identity.
    Decision(DecisionRecord),
    /// A bare domain event without an available envelope identity.
    Event(DomainEvent),
}

/// How complete the evidence available before an import boundary is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyCompleteness {
    /// Only the evidence explicitly carried by the anchor is known.
    AvailableEvidenceOnly,
    /// The source asserts complete subject evidence through the anchor.
    CompleteSubject,
}

/// What ordering the legacy source actually establishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyOrderDeclaration {
    /// Decisions and observations have only separate per-kind order.
    PerKindOnly,
    /// A mixed subject-local order is known.
    Subject,
}

/// The explicit trust boundary before a verifiable newly recorded suffix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyAnchor {
    /// Exact materialized state trusted at the boundary.
    pub instance: EntityInstance,
    /// Declared evidence completeness.
    pub completeness: LegacyCompleteness,
    /// Declared available order.
    pub order: LegacyOrderDeclaration,
    /// Available typed evidence, without invented coordinates.
    pub evidence: Vec<LegacyEvidence>,
}

/// Where verifiable subject history begins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryOrigin {
    /// The first stored decision is creation at revision one.
    Genesis,
    /// Verification begins after a trusted explicit legacy anchor.
    Imported(LegacyAnchor),
}

/// One ordered mixed subject-local history.
#[derive(Debug, Clone, PartialEq)]
pub struct SubjectHistory {
    /// The one subject this history covers.
    pub subject: Subject,
    /// Genesis or explicit imported boundary.
    pub origin: HistoryOrigin,
    /// Only post-genesis or post-anchor newly stored records, in physical order.
    pub records: Vec<StoredRecord>,
}

/// The closed result of a global record identity lookup.
#[derive(Debug, Clone, PartialEq)]
pub enum RecordLookup {
    /// A newly accepted record with complete receipt coordinates.
    Committed(StoredRecord),
    /// Preserved historical evidence without an invented receipt.
    Imported(ImportedRecordEvidence),
}

/// The boundary a subject verifier actually established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubjectAssurance {
    /// Complete recomputation from creation.
    VerifiedFromGenesis {
        /// Covered subject.
        subject: Subject,
    },
    /// Recomputed post-anchor suffix only.
    VerifiedAfterBoundary {
        /// Covered subject.
        subject: Subject,
        /// Trusted boundary revision.
        anchor_revision: u64,
    },
}

/// Whether a cross-subject verification input is selected or provider-complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreCoverage {
    /// An explicitly supplied set that makes no hidden-subject claim.
    ExplicitSet,
    /// A provider-owned consistent complete snapshot.
    CompleteSnapshot,
}

/// One subject history paired with the supplied terminal materialization.
#[derive(Debug, Clone, PartialEq)]
pub struct SubjectSnapshot {
    /// Mixed history.
    pub history: SubjectHistory,
    /// Supplied terminal state.
    pub terminal: EntityInstance,
}

/// Editable cross-subject transcript and provider-capture data.
///
/// [`StoreCoverage::CompleteSnapshot`] is descriptive data, not provenance. Passing an arbitrary
/// value with that marker to `verify_store_histories` is refused. Whole-provider assurance is
/// available only through [`crate::asynchronous::verify_complete_store`], which obtains a fresh
/// capture from the port.
#[derive(Debug, Clone, PartialEq)]
pub struct CompleteStoreSnapshot {
    /// Caller- or provider-named logical store scope.
    pub scope: String,
    /// Whether completeness is asserted by the provider.
    pub coverage: StoreCoverage,
    /// Exact covered histories and terminal states.
    pub histories: Vec<SubjectSnapshot>,
}

/// Assurance over exactly the named scope and subjects supplied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreAssurance {
    /// Logical store scope.
    pub scope: String,
    /// Input completeness declaration.
    pub coverage: StoreCoverage,
    /// Subject-local assurances in subject order.
    pub subjects: Vec<SubjectAssurance>,
}

/// A typed asynchronous storage or integrity refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsyncStoreError {
    /// Caller input violates the closed contract.
    InvalidInput(String),
    /// JSON comparison material could not be encoded.
    Encoding(String),
    /// Current state disagreed with the explicit predecessor expectation.
    RevisionConflict {
        /// Subject in conflict.
        subject: Subject,
        /// Expected predecessor.
        expected: Expect,
        /// Actual current revision, if present.
        found: Option<u64>,
    },
    /// One global record identity names different complete bytes or request intent.
    RecordConflict {
        /// Conflicting identity.
        record_id: String,
    },
    /// One request names the same global record identity more than once.
    DuplicateRecordId {
        /// Repeated identity.
        record_id: String,
    },
    /// One batch identity names a different ordered batch.
    BatchConflict {
        /// Conflicting key.
        key: BatchKey,
    },
    /// A new named batch included records accepted before that batch claim.
    PreviouslyRecordedBatchEntries {
        /// Exact requested member indices already present.
        indices: Vec<u64>,
    },
    /// Saved immutable evidence is inconsistent or cannot be recomputed.
    CorruptHistory {
        /// Subject whose evidence failed.
        subject: Subject,
        /// Concrete mismatch.
        detail: String,
    },
    /// Imported evidence lacks facts needed to prove the request is identical.
    HistoricalRetryUnverifiable {
        /// Global imported record identity.
        record_id: String,
        /// Missing or inconsistent fact.
        detail: String,
    },
    /// A physical coordinate cannot be allocated in its checked u64 domain.
    PositionExhausted {
        /// Coordinate domain.
        domain: String,
    },
    /// The named authority could not be reached, which proves neither absence nor rollback.
    Unreachable {
        /// Provider name.
        provider: String,
        /// Concrete failure.
        detail: String,
    },
    /// The provider answered with an internal failure.
    Backend(String),
}

impl fmt::Display for AsyncStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(detail) => write!(formatter, "invalid recorded request: {detail}"),
            Self::Encoding(detail) => {
                write!(formatter, "record comparison encoding failed: {detail}")
            }
            Self::RevisionConflict {
                subject,
                expected,
                found,
            } => write!(
                formatter,
                "{} {}: expected {expected}, found {}",
                subject.entity,
                subject.id,
                found.map_or_else(
                    || "nothing".to_owned(),
                    |revision| format!("revision {revision}")
                )
            ),
            Self::RecordConflict { record_id } => {
                write!(
                    formatter,
                    "record id {record_id:?} already names different intent"
                )
            }
            Self::DuplicateRecordId { record_id } => {
                write!(
                    formatter,
                    "record id {record_id:?} repeats within one request"
                )
            }
            Self::BatchConflict { key } => write!(
                formatter,
                "batch key {key:?} already names different members"
            ),
            Self::PreviouslyRecordedBatchEntries { indices } => write!(
                formatter,
                "new named batch contains prior records at indices {indices:?}"
            ),
            Self::CorruptHistory { subject, detail } => write!(
                formatter,
                "history for {} {} is corrupt: {detail}",
                subject.entity, subject.id
            ),
            Self::HistoricalRetryUnverifiable { record_id, detail } => write!(
                formatter,
                "historical retry {record_id:?} cannot be verified: {detail}"
            ),
            Self::PositionExhausted { domain } => {
                write!(formatter, "{domain} physical position is exhausted")
            }
            Self::Unreachable { provider, detail } => {
                write!(formatter, "{provider} could not be reached: {detail}")
            }
            Self::Backend(detail) => write!(formatter, "the recorded store failed: {detail}"),
        }
    }
}

impl std::error::Error for AsyncStoreError {}

/// A write failure distinguishes proved rollback from an uncertain outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteFailure {
    /// The provider proved no part of the request was committed.
    NotCommitted(AsyncStoreError),
    /// The response was lost or authority unavailable after commit may have happened.
    Uncertain {
        /// Original key that must be used for recovery.
        key: BatchKey,
        /// Concrete uncertainty cause.
        cause: String,
    },
}

impl fmt::Display for WriteFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotCommitted(error) => error.fmt(formatter),
            Self::Uncertain { key, cause } => {
                write!(
                    formatter,
                    "commit outcome for {key:?} is uncertain: {cause}"
                )
            }
        }
    }
}

impl std::error::Error for WriteFailure {}
