//! Keeping what the kernel decided.
//!
//! `entity-core` decides and holds nothing: it takes an instance, an operation and arguments, and
//! returns a [`Decision`]. **Everything that keeps one is here** — which is R-82, and it is a
//! boundary rather than a layering preference. The kernel's purity scan refuses the tokens for
//! filesystem, network, clock and randomness, so a provider could not live there even if somebody
//! wanted it to.
//!
//! ```text
//!   definition + instance + operation  ──kernel──▶  Decision { instance, events }
//!                                                        │
//!                                                        ▼
//!                                            Store::commit(decision, expect)
//!                                        state written · events appended · together
//! ```
//!
//! # Why `commit` and not `put` then `append`
//!
//! R-80 says the shell persists the instance **and** appends the events **together**. Two calls a
//! caller is trusted to make in order is not "together": the failure it permits is a state that
//! moved with no event explaining it, and every projection, audit and replay downstream is then
//! quietly wrong in a way nothing detects. [`Store::commit`] is the operation providers implement,
//! and the two halves are its parts rather than its API.
//!
//! # Optimistic concurrency, and what it buys a person
//!
//! Every write says what it expected to find ([`Expect`]). Two people acting on the same version of
//! something is not rare — it is the normal shape of a team — and the choice is between the second
//! write silently overwriting the first and the second write being told. This refuses
//! ([`StoreError::RevisionConflict`]), because a lost update is invisible at the moment it happens
//! and expensive whenever it is finally noticed.
//!
//! Revisions are the kernel's (R-44): `1` after creation, `+1` per operation. Nothing here invents
//! one.
//!
//! # What is deliberately not here yet
//!
//! Search and blob providers. Enumeration is here — [`StateProvider::ids`] says what a store holds
//! for one entity type, sorted, and nothing more: no filter, no page, no query. An enumeration is
//! the primitive a projection or a search index folds from, and the fold is the shell's (R-98).
//! Each of the rest is its own story; naming them here is what stops this crate growing them by
//! accident.

#![doc(html_root_url = "https://github.com/beyond10x/entity-runtime")]

use std::fmt;

use entity_core::{Decision, DomainEvent, EntityInstance};

pub mod conformance;
pub mod envelope;
pub mod file;
pub mod memory;
pub mod projection;

pub use envelope::{derived_id, Envelope, Recording};
pub use file::FileStore;
pub use memory::MemoryStore;
pub use projection::{project, Grouping, Projections};

/// What a write expects to find where it is about to write.
///
/// Named rather than an `Option<u64>`, because *absent* and *at revision zero* are different claims
/// and one of them is not a revision at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect {
    /// Nothing is stored under this identity yet. What a creation expects.
    Absent,
    /// Exactly this revision is stored. What every later operation expects.
    Revision(u64),
}

impl fmt::Display for Expect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absent => f.write_str("absent"),
            Self::Revision(revision) => write!(f, "revision {revision}"),
        }
    }
}

/// Why a store refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// The store held something other than what the write expected.
    ///
    /// The ordinary cause is that somebody else got there first, which is why the message says what
    /// was expected *and* what was found: a caller that can see both can re-read and retry, and one
    /// that can see neither can only give up or clobber.
    RevisionConflict {
        /// The entity type.
        entity: String,
        /// The instance identity.
        id: String,
        /// What the write expected.
        expected: Expect,
        /// What was actually there. `None` when nothing was.
        found: Option<u64>,
    },
    /// The provider could not be reached at all.
    ///
    /// Kept apart from every other failure because it is the one that must **not** read as
    /// *absent*. A remote that did not answer has told you nothing about whether the instance
    /// exists, and a caller that treated silence as "no such thing" would create a duplicate, or
    /// report a customer's record as missing because a switch was rebooting.
    ///
    /// This is the store's spelling of `Unknown`: the third answer, kept distinct from both yes and
    /// no, exactly as the condition language keeps it.
    Unreachable {
        /// Which provider.
        provider: String,
        /// What went wrong reaching it.
        detail: String,
    },
    /// The provider itself failed: a disk, a socket, a driver.
    ///
    /// Kept apart from a conflict on purpose. A conflict is the system working — two people acted
    /// at once — and is retried by re-reading; a backend failure is the system broken, and retrying
    /// it in a loop is how an outage becomes a longer outage.
    Backend(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RevisionConflict {
                entity,
                id,
                expected,
                found,
            } => {
                write!(f, "{entity} {id}: expected {expected}, found ")?;
                match found {
                    Some(revision) => write!(f, "revision {revision}"),
                    None => f.write_str("nothing"),
                }
            }
            Self::Unreachable { provider, detail } => {
                write!(f, "{provider} could not be reached: {detail}")
            }
            Self::Backend(detail) => write!(f, "the store failed: {detail}"),
        }
    }
}

impl std::error::Error for StoreError {}

/// Reading and writing an instance's current state.
pub trait StateProvider {
    /// The instance stored under this identity, if there is one.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the provider itself fails. A missing instance is `Ok(None)`:
    /// not being there is an answer, not a failure.
    fn load(&self, entity: &str, id: &str) -> Result<Option<EntityInstance>, StoreError>;

    /// The revision currently stored, if any.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the provider itself fails.
    fn revision_of(&self, entity: &str, id: &str) -> Result<Option<u64>, StoreError> {
        Ok(self.load(entity, id)?.map(|instance| instance.revision))
    }

    /// Every identity the store holds for `entity`, sorted, so two calls and two providers agree
    /// byte for byte.
    ///
    /// This is what lets a shell open a store it did not write and rebuild from it. Every other
    /// question here needs an `(entity, id)` the caller already knows; a process hydrating from a
    /// populated store has no id to ask with — which is why the first adopter's SQLite backend
    /// refused any row it had not written itself and told people to point it at an empty database.
    ///
    /// Required rather than defaulted: a default that returned an empty list would let a provider
    /// claim to hold nothing while holding everything, and a store answering *nothing* about a
    /// question it cannot answer is the one thing this crate refuses everywhere else.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the provider itself fails, and [`StoreError::Unreachable`] when
    /// it could not be asked. A type nobody stored under is `Ok(vec![])`: not being there is an
    /// answer, not a failure.
    fn ids(&self, entity: &str) -> Result<Vec<String>, StoreError>;
}

/// Reading and appending an instance's events.
pub trait EventProvider {
    /// Every event recorded for this instance, oldest first.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the provider itself fails.
    fn events(&self, entity: &str, id: &str) -> Result<Vec<DomainEvent>, StoreError>;
}

/// A provider that keeps state and events, and writes both together.
pub trait Store: StateProvider + EventProvider {
    /// Writes a decision: the instance and its events, as one step.
    ///
    /// `expect` is checked **before** anything is written, so a refusal changes nothing — the same
    /// guarantee the kernel gives for a refused operation (R-04), continued across the boundary
    /// where it would otherwise be lost.
    ///
    /// # Errors
    ///
    /// [`StoreError::RevisionConflict`] when the store holds something other than `expect`, and
    /// [`StoreError::Backend`] when the provider itself fails.
    fn commit(&mut self, decision: &Decision, expect: Expect) -> Result<(), StoreError>;
}

impl StoreError {
    /// `true` when the provider could not be reached, so nothing was learned either way.
    ///
    /// The question a caller must be able to ask before deciding anything: *did the store say no,
    /// or did it not answer?* Collapsing the two is how a sync tool loses data.
    #[must_use]
    pub fn is_unreachable(&self) -> bool {
        matches!(self, Self::Unreachable { .. })
    }
}

/// Whether what is stored matches what a write expected.
///
/// Free rather than a method, so every provider decides conflicts the same way: a rule implemented
/// once cannot disagree with itself, and this is the rule the whole crate exists to enforce.
///
/// # Errors
///
/// [`StoreError::RevisionConflict`] when they do not match.
pub fn check(
    entity: &str,
    id: &str,
    expected: Expect,
    found: Option<u64>,
) -> Result<(), StoreError> {
    let agrees = match (expected, found) {
        (Expect::Absent, None) => true,
        (Expect::Revision(want), Some(have)) => want == have,
        _ => false,
    };
    if agrees {
        return Ok(());
    }
    Err(StoreError::RevisionConflict {
        entity: entity.to_owned(),
        id: id.to_owned(),
        expected,
        found,
    })
}
