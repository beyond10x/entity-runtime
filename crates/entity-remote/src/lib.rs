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
//! Both sides speak `entity.store/1`. A request naming a version this build does not know is
//! refused by name rather than parsed as much as possible — a partial read of a protocol nobody
//! agreed on is how two deployments come to disagree quietly.

use entity_core::{Decision, DomainEvent, EntityInstance};
use entity_store::{EventProvider, Expect, StateProvider, Store, StoreError};
use serde::{Deserialize, Serialize};

pub mod hybrid;
pub mod loopback;

pub use hybrid::{
    Authority, Divergence, Hybrid, OnDivergence, Policy, Read, ReadPath, WhenUnreachable,
};
pub use loopback::LoopbackTransport;

/// The wire format both sides speak.
pub const WIRE_VERSION: &str = "entity.store/1";

/// What one side asks the other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// The three things a store is ever asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "ask", rename_all = "snake_case", deny_unknown_fields)]
pub enum Ask {
    /// The instance under this identity, if any.
    Load {
        /// The entity type.
        entity: String,
        /// The identity.
        id: String,
    },
    /// Every event for this identity, oldest first.
    Events {
        /// The entity type.
        entity: String,
        /// The identity.
        id: String,
    },
    /// Write a decision, if the store holds what was expected.
    Commit {
        /// The decision to write.
        decision: Box<Decision>,
        /// What the writer expected to find.
        expect: Expectation,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case", deny_unknown_fields)]
pub enum Answer {
    /// The instance, or `None` when nothing is stored.
    Instance {
        /// What was found.
        instance: Option<Box<EntityInstance>>,
    },
    /// The events, oldest first.
    Events {
        /// What was found.
        events: Vec<DomainEvent>,
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
    /// The far side failed for a reason of its own.
    Failed {
        /// What went wrong.
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

impl<T: Transport> StateProvider for RemoteStore<T> {
    fn load(&self, entity: &str, id: &str) -> Result<Option<EntityInstance>, StoreError> {
        match self.ask(Ask::Load {
            entity: entity.to_owned(),
            id: id.to_owned(),
        })? {
            Answer::Instance { instance } => Ok(instance.map(|boxed| *boxed)),
            Answer::Failed { detail } => Err(StoreError::Backend(detail)),
            other => Err(unexpected(&other)),
        }
    }
}

impl<T: Transport> EventProvider for RemoteStore<T> {
    fn events(&self, entity: &str, id: &str) -> Result<Vec<DomainEvent>, StoreError> {
        match self.ask(Ask::Events {
            entity: entity.to_owned(),
            id: id.to_owned(),
        })? {
            Answer::Events { events } => Ok(events),
            Answer::Failed { detail } => Err(StoreError::Backend(detail)),
            other => Err(unexpected(&other)),
        }
    }
}

impl<T: Transport> Store for RemoteStore<T> {
    fn commit(&mut self, decision: &Decision, expect: Expect) -> Result<(), StoreError> {
        match self.ask(Ask::Commit {
            decision: Box::new(decision.clone()),
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
            Answer::Failed { detail } => Err(StoreError::Backend(detail)),
            other => Err(unexpected(&other)),
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
pub fn answer(store: &mut dyn Store, request: &Request) -> Result<Answer, String> {
    if request.version != WIRE_VERSION {
        return Err(format!(
            "this build speaks `{WIRE_VERSION}`, not `{}`",
            request.version
        ));
    }

    Ok(match &request.ask {
        Ask::Load { entity, id } => match store.load(entity, id) {
            Ok(instance) => Answer::Instance {
                instance: instance.map(Box::new),
            },
            Err(error) => Answer::Failed {
                detail: error.to_string(),
            },
        },
        Ask::Events { entity, id } => match store.events(entity, id) {
            Ok(events) => Answer::Events { events },
            Err(error) => Answer::Failed {
                detail: error.to_string(),
            },
        },
        Ask::Commit { decision, expect } => match store.commit(decision, (*expect).into()) {
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
            Err(error) => Answer::Failed {
                detail: error.to_string(),
            },
        },
    })
}
