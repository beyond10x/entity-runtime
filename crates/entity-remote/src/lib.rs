//! A store that lives somewhere else.
//!
//! Centralized storage: several machines writing one record of truth, so *which copy is
//! authoritative* stops being a question anybody has to answer by hand.
//!
//! # There is no HTTP client in this crate, on purpose
//!
//! What is here is the **protocol**: the three requests a store needs and the answers to them.
//! [`Transport`] is how they travel, and it is a trait the caller implements — with whatever client,
//! TLS, retry policy, auth and timeouts their deployment already has.
//!
//! Shipping one would have meant choosing an HTTP stack, a TLS backend and a runtime on an adopter's
//! behalf, and pulling all three into a repository whose kernel has two dependencies. It would also
//! have made the gate reach a network to test anything, and this repository's gate does not reach
//! networks — so the tests would have been mocked anyway, which is what
//! [`LoopbackTransport`] is, honestly labelled.
//!
//! # Unreachable is not absent
//!
//! Every transport failure becomes [`StoreError::Unreachable`], never a `None`. A remote that did
//! not answer has said **nothing** about whether the instance exists; a caller treating silence as
//! *no such thing* creates a duplicate, or tells somebody their record is gone because a switch was
//! rebooting. This is the condition language's third value, at the store boundary.
//!
//! # The wire is JSON and versioned
//!
//! Both sides speak [`WIRE_VERSION`]. A request naming a version this build does not know is
//! refused by name rather than parsed as much as possible — a partial read of a protocol nobody
//! agreed on is how two deployments come to disagree quietly.

use entity_core::{Decision, DecisionRecord, DomainEvent, EntityInstance};
use entity_store::{
    Envelope, EventProvider, Expect, HistoryProvider, RecordedCommit, RecordedObservation,
    RecordedStore, StateProvider, Store, StoreError,
};
use serde::{Deserialize, Serialize};

pub mod hybrid;
pub mod loopback;

pub use hybrid::{
    Authority, Divergence, Freshness, Hybrid, OnDivergence, Policy, Read, ReadPath, StoreSide,
    WhenUnreachable,
};
pub use loopback::LoopbackTransport;

/// The wire format both sides speak.
/// The protocol version this build speaks.
///
/// # Why this went to `/2` in 0.9.0
///
/// [`Answer`] is a tagged enum with `deny_unknown_fields`, so **adding a variant is a breaking wire
/// change**: a peer built against `/1` cannot decode `{"answer":"refused"}`. 0.8.0 added
/// [`Answer::Refused`] and [`Answer::Unreachable`] and left the version at `/1`, which made the
/// refusal undecodable by exactly the peer it exists to inform — it would have arrived as a decode
/// failure, which is the `Backend` outcome that change set out to avoid.
///
/// Bumping refuses an old peer outright instead, by name, which is what a version is for.
///
/// # And to `/3` for enumeration
///
/// [`Ask::Ids`] and [`Answer::Ids`] are new variants on the same two tagged enums, so the same rule
/// applies: a `/2` peer cannot decode either, and is told so by name rather than handed a decode
/// failure (`story:store-enumeration`).
pub const WIRE_VERSION: &str = "entity.store/4";

/// A wire-owned JSON document.
///
/// Runtime and provider structs never occur in protocol variants, so an internal field addition
/// cannot silently change this enum's shape. Endpoints convert explicitly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireDocument {
    /// Canonical JSON for the transported value.
    pub document: serde_json::Value,
}

impl WireDocument {
    fn encode<T: Serialize>(value: &T) -> Result<Self, StoreError> {
        serde_json::to_value(value)
            .map(|document| Self { document })
            .map_err(|error| StoreError::Backend(format!("encoding a wire document: {error}")))
    }

    fn decode<T: serde::de::DeserializeOwned>(self) -> Result<T, StoreError> {
        serde_json::from_value(self.document)
            .map_err(|error| StoreError::Backend(format!("decoding a wire document: {error}")))
    }
}

/// What one side asks the other.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    /// The wire format. Refused when it is not one this build knows.
    pub version: String,
    /// What is being asked.
    pub ask: Ask,
}

impl Request {
    /// A request at this build's wire version.
    #[must_use]
    pub fn new(ask: Ask) -> Self {
        Self {
            version: WIRE_VERSION.to_owned(),
            ask,
        }
    }
}

/// The four things a store is ever asked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "ask", rename_all = "snake_case", deny_unknown_fields)]
pub enum Ask {
    /// The instance under this identity, if any.
    Load {
        /// The entity type.
        entity: String,
        /// The identity.
        id: String,
    },
    /// Every identity held under this entity type, sorted.
    Ids {
        /// The entity type.
        entity: String,
    },
    /// Every event for this identity, oldest first.
    Events {
        /// The entity type.
        entity: String,
        /// The identity.
        id: String,
    },
    /// Complete decision envelopes for this identity, oldest first.
    Records {
        /// The entity type.
        entity: String,
        /// The identity.
        id: String,
    },
    /// Complete observation envelopes for this identity, oldest first.
    Observations {
        /// The entity type.
        entity: String,
        /// The identity.
        id: String,
    },
    /// Write a decision, if the store holds what was expected.
    Commit {
        /// The decision to write.
        decision: WireDocument,
        /// What the writer expected to find.
        expect: Expectation,
    },
    /// Write a complete decision envelope.
    CommitRecorded {
        /// The recorded commit.
        commit: WireDocument,
        /// What the writer expected to find.
        expect: Expectation,
    },
    /// Append a non-state-changing recorded observation.
    Observe {
        /// The observation to append.
        observation: WireDocument,
    },
}

/// [`Expect`] as it travels.
///
/// Its own type rather than serialising `Expect` directly: the wire shape is a published surface
/// and must not change because an internal enum gained a variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "expect", rename_all = "snake_case", deny_unknown_fields)]
pub enum Expectation {
    /// Nothing is stored under this identity.
    Absent,
    /// Exactly this revision is.
    Revision {
        /// The revision expected.
        revision: u64,
    },
}

impl From<Expect> for Expectation {
    fn from(expect: Expect) -> Self {
        match expect {
            Expect::Absent => Self::Absent,
            Expect::Revision(revision) => Self::Revision { revision },
        }
    }
}

impl From<Expectation> for Expect {
    fn from(expectation: Expectation) -> Self {
        match expectation {
            Expectation::Absent => Self::Absent,
            Expectation::Revision { revision } => Self::Revision(revision),
        }
    }
}

/// What the other side answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case", deny_unknown_fields)]
pub enum Answer {
    /// The instance, or `None` when nothing is stored.
    Instance {
        /// What was found.
        instance: Option<WireDocument>,
    },
    /// The events, oldest first.
    Events {
        /// What was found.
        events: Vec<WireDocument>,
    },
    /// Ordered decision or observation documents.
    Documents {
        /// What was found.
        documents: Vec<WireDocument>,
    },
    /// The identities held, sorted.
    Ids {
        /// What was found.
        ids: Vec<String>,
    },
    /// The write landed.
    Committed,
    /// The store refused: the expectation did not match.
    Conflict {
        /// The entity type.
        entity: String,
        /// The identity.
        id: String,
        /// What was expected.
        expected: Expectation,
        /// What was there.
        found: Option<u64>,
    },
    /// One idempotency identity was reused for different bytes.
    RecordConflict {
        /// The conflicting caller-supplied identity.
        record_id: String,
    },
    /// The far side failed for a reason of its own.
    Failed {
        /// What went wrong.
        detail: String,
    },
    /// The far side answered, and refused the request itself.
    ///
    /// Kept apart from [`Answer::Failed`] because a protocol disagreement is not a broken store: a
    /// request at a wire version this build does not speak is a **refusal by a reachable peer**,
    /// and reporting it as unreachable would have a `ServeStale` policy serve stale data forever
    /// against a remote that is up and answering.
    Refused {
        /// Why, in the far side's words.
        detail: String,
    },
    /// The far side could not reach *its* store.
    ///
    /// The third value has to survive the wire. Without this variant an unreachable store one hop
    /// out arrives as an ordinary backend failure, and every [`crate::hybrid::WhenUnreachable`]
    /// policy downstream stops applying to it.
    Unreachable {
        /// Which provider, on the far side.
        provider: String,
        /// What went wrong reaching it.
        detail: String,
    },
}

/// How a request reaches the other side.
///
/// Implemented by the caller. Everything network-shaped — a client, TLS, timeouts, retries, auth —
/// lives in here and nowhere else in this repository.
pub trait Transport {
    /// Sends a request and waits for its answer.
    ///
    /// # Errors
    ///
    /// Any failure to *reach* the other side. Returning `Err` here means **nothing was learned**,
    /// which is why it becomes [`StoreError::Unreachable`] and never an absent instance.
    fn call(&self, request: &Request) -> Result<Answer, String>;

    /// A name for this transport, used in refusals so a person knows which side did not answer.
    fn name(&self) -> String {
        "the remote store".to_owned()
    }
}

/// A [`Store`] whose data is on the other end of a [`Transport`].
#[derive(Debug, Clone)]
pub struct RemoteStore<T> {
    transport: T,
}

impl<T: Transport> RemoteStore<T> {
    /// A store reached through `transport`.
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }

    /// The transport, for a caller that needs to inspect or replace it.
    pub const fn transport(&self) -> &T {
        &self.transport
    }

    /// Sends one request, turning every failure to reach the far side into `Unreachable`.
    fn ask(&self, ask: Ask) -> Result<Answer, StoreError> {
        self.transport
            .call(&Request::new(ask))
            .map_err(|detail| StoreError::Unreachable {
                provider: self.transport.name(),
                detail,
            })
    }
}

/// The answer was not the one this request asks for, which is a protocol disagreement.
fn unexpected(answer: &Answer) -> StoreError {
    StoreError::Backend(format!(
        "the remote answered with {answer:?}, which is not an answer to this request"
    ))
}

/// The answers every request can receive, whatever it asked.
///
/// [`Answer::Refused`] is a [`StoreError::Backend`] and **not** `Unreachable`: the peer answered.
/// [`Answer::Unreachable`] stays unreachable, so the third value survives the hop with the far
/// side's provider name intact rather than being flattened into this one's.
fn common(answer: Answer) -> StoreError {
    match answer {
        Answer::Failed { detail } => StoreError::Backend(detail),
        Answer::Refused { detail } => StoreError::Backend(format!("the remote refused: {detail}")),
        Answer::Unreachable { provider, detail } => StoreError::Unreachable { provider, detail },
        other => unexpected(&other),
    }
}

impl<T: Transport> StateProvider for RemoteStore<T> {
    fn load(&self, entity: &str, id: &str) -> Result<Option<EntityInstance>, StoreError> {
        match self.ask(Ask::Load {
            entity: entity.to_owned(),
            id: id.to_owned(),
        })? {
            Answer::Instance { instance } => instance.map(WireDocument::decode).transpose(),
            other => Err(common(other)),
        }
    }

    fn ids(&self, entity: &str) -> Result<Vec<String>, StoreError> {
        match self.ask(Ask::Ids {
            entity: entity.to_owned(),
        })? {
            Answer::Ids { ids } => Ok(ids),
            other => Err(common(other)),
        }
    }
}

impl<T: Transport> EventProvider for RemoteStore<T> {
    fn events(&self, entity: &str, id: &str) -> Result<Vec<DomainEvent>, StoreError> {
        match self.ask(Ask::Events {
            entity: entity.to_owned(),
            id: id.to_owned(),
        })? {
            Answer::Events { events } => events.into_iter().map(WireDocument::decode).collect(),
            other => Err(common(other)),
        }
    }
}

impl<T: Transport> HistoryProvider for RemoteStore<T> {
    fn records(&self, entity: &str, id: &str) -> Result<Vec<Envelope<DecisionRecord>>, StoreError> {
        match self.ask(Ask::Records {
            entity: entity.to_owned(),
            id: id.to_owned(),
        })? {
            Answer::Documents { documents } => {
                documents.into_iter().map(WireDocument::decode).collect()
            }
            other => Err(common(other)),
        }
    }

    fn observations(&self, entity: &str, id: &str) -> Result<Vec<RecordedObservation>, StoreError> {
        match self.ask(Ask::Observations {
            entity: entity.to_owned(),
            id: id.to_owned(),
        })? {
            Answer::Documents { documents } => {
                documents.into_iter().map(WireDocument::decode).collect()
            }
            other => Err(common(other)),
        }
    }
}

impl<T: Transport> Store for RemoteStore<T> {
    fn commit(&mut self, decision: &Decision, expect: Expect) -> Result<(), StoreError> {
        match self.ask(Ask::Commit {
            decision: WireDocument::encode(decision)?,
            expect: expect.into(),
        })? {
            Answer::Committed => Ok(()),
            Answer::Conflict {
                entity,
                id,
                expected,
                found,
            } => Err(StoreError::RevisionConflict {
                entity,
                id,
                expected: expected.into(),
                found,
            }),
            other => Err(common(other)),
        }
    }

    fn commit_recorded(
        &mut self,
        commit: &RecordedCommit,
        expect: Expect,
    ) -> Result<(), StoreError> {
        commit.validate()?;
        match self.ask(Ask::CommitRecorded {
            commit: WireDocument::encode(commit)?,
            expect: expect.into(),
        })? {
            Answer::Committed => Ok(()),
            Answer::Conflict {
                entity,
                id,
                expected,
                found,
            } => Err(StoreError::RevisionConflict {
                entity,
                id,
                expected: expected.into(),
                found,
            }),
            Answer::RecordConflict { record_id } => Err(StoreError::RecordConflict { record_id }),
            other => Err(common(other)),
        }
    }

    fn observe(&mut self, observation: &RecordedObservation) -> Result<(), StoreError> {
        observation.validate()?;
        match self.ask(Ask::Observe {
            observation: WireDocument::encode(observation)?,
        })? {
            Answer::Committed => Ok(()),
            Answer::Conflict {
                entity,
                id,
                expected,
                found,
            } => Err(StoreError::RevisionConflict {
                entity,
                id,
                expected: expected.into(),
                found,
            }),
            Answer::RecordConflict { record_id } => Err(StoreError::RecordConflict { record_id }),
            other => Err(common(other)),
        }
    }
}

/// Answers a request from a local store: the far side of the wire, wherever it runs.
///
/// A server is this function plus a socket. Keeping it here means both ends of the protocol are
/// implemented once, so they cannot drift apart — and it is what lets the conformance suite run
/// over the wire shape without a network.
///
/// # Errors
///
/// Only a version this build does not know. Every other outcome is an [`Answer`], including a
/// refusal: a conflict is something the store decided, not a failure of the exchange.
pub fn answer(store: &mut dyn RecordedStore, request: &Request) -> Result<Answer, String> {
    if request.version != WIRE_VERSION {
        // A refusal, not a failure of the exchange: the peer answered, and said no by name.
        return Ok(Answer::Refused {
            detail: format!(
                "this build speaks `{WIRE_VERSION}`, not `{}`",
                request.version
            ),
        });
    }

    Ok(match &request.ask {
        Ask::Load { entity, id } => match store.load(entity, id) {
            Ok(instance) => Answer::Instance {
                instance: match instance
                    .map(|instance| WireDocument::encode(&instance))
                    .transpose()
                {
                    Ok(instance) => instance,
                    Err(error) => return Ok(failure(error)),
                },
            },
            Err(error) => failure(error),
        },
        Ask::Events { entity, id } => match store.events(entity, id) {
            Ok(events) => match events
                .iter()
                .map(WireDocument::encode)
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(events) => Answer::Events { events },
                Err(error) => failure(error),
            },
            Err(error) => failure(error),
        },
        Ask::Records { entity, id } => match store.records(entity, id) {
            Ok(records) => match records
                .iter()
                .map(WireDocument::encode)
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(documents) => Answer::Documents { documents },
                Err(error) => failure(error),
            },
            Err(error) => failure(error),
        },
        Ask::Observations { entity, id } => match store.observations(entity, id) {
            Ok(observations) => match observations
                .iter()
                .map(WireDocument::encode)
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(documents) => Answer::Documents { documents },
                Err(error) => failure(error),
            },
            Err(error) => failure(error),
        },
        Ask::Ids { entity } => match store.ids(entity) {
            Ok(ids) => Answer::Ids { ids },
            Err(error) => failure(error),
        },
        Ask::Commit { decision, expect } => {
            let decision: Decision = match decision.clone().decode() {
                Ok(decision) => decision,
                Err(error) => return Ok(failure(error)),
            };
            match store.commit(&decision, (*expect).into()) {
                Ok(()) => Answer::Committed,
                Err(StoreError::RevisionConflict {
                    entity,
                    id,
                    expected,
                    found,
                }) => Answer::Conflict {
                    entity,
                    id,
                    expected: expected.into(),
                    found,
                },
                Err(error) => failure(error),
            }
        }
        Ask::CommitRecorded { commit, expect } => {
            let commit: RecordedCommit = match commit.clone().decode() {
                Ok(commit) => commit,
                Err(error) => return Ok(failure(error)),
            };
            match store.commit_recorded(&commit, (*expect).into()) {
                Ok(()) => Answer::Committed,
                Err(StoreError::RevisionConflict {
                    entity,
                    id,
                    expected,
                    found,
                }) => Answer::Conflict {
                    entity,
                    id,
                    expected: expected.into(),
                    found,
                },
                Err(error) => failure(error),
            }
        }
        Ask::Observe { observation } => {
            let observation: RecordedObservation = match observation.clone().decode() {
                Ok(observation) => observation,
                Err(error) => return Ok(failure(error)),
            };
            match store.observe(&observation) {
                Ok(()) => Answer::Committed,
                Err(StoreError::RevisionConflict {
                    entity,
                    id,
                    expected,
                    found,
                }) => Answer::Conflict {
                    entity,
                    id,
                    expected: expected.into(),
                    found,
                },
                Err(error) => failure(error),
            }
        }
    })
}

/// One store failure, as the wire spells it.
///
/// [`StoreError::Unreachable`] keeps its own variant so the third value survives the hop; every
/// other failure is [`Answer::Failed`].
fn failure(error: StoreError) -> Answer {
    match error {
        StoreError::Unreachable { provider, detail } => Answer::Unreachable { provider, detail },
        StoreError::RecordConflict { record_id } => Answer::RecordConflict { record_id },
        other => Answer::Failed {
            detail: other.to_string(),
        },
    }
}
