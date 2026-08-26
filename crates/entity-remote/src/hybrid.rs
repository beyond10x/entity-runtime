//! Two stores, and a declared rule for when they disagree.
//!
//! A local store and a remote one. The interesting part is not keeping both — it is that *whose
//! copy wins* and *what happens when one is unreachable* are *declared*, in required words, rather
//! than falling out of the order somebody wrote two calls in.
//!
//! # Why every field is required
//!
//! `engineering-protocols` learned this in its ESS wave 2 and wrote it into its own model: a
//! binding states its delivery guarantee and what happens on failure, **both as required words**,
//! never defaulted. A hybrid store is a binding. A default here is a system-wide assumption nobody
//! made on purpose, and the failure mode arrives in production — usually as data loss nobody can
//! date.
//!
//! So [`Policy`] has no `Default`. Constructing one means answering four questions:
//!
//! | question | field |
//! |---|---|
//! | whose copy is the record of truth? | [`Authority`] |
//! | where does a read go first? | [`ReadPath`] |
//! | what happens when the remote does not answer? | [`WhenUnreachable`] |
//! | what happens to a write that lost? | [`OnDivergence`] |
//!
//! # Unreachable stays `Unknown`
//!
//! A remote that did not answer has said nothing about whether anything exists. Serving a stale
//! local copy is a legitimate choice and [`WhenUnreachable::ServeStale`] makes it — but it is a
//! choice somebody typed, and the result says it was stale. What is refused is the shape where
//! silence quietly becomes an answer.

use entity_core::{Decision, DomainEvent, EntityInstance};
use entity_store::{EventProvider, Expect, StateProvider, Store, StoreError};

/// Which side is the record of truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authority {
    /// The remote. The local copy is a cache and may be rebuilt from it.
    Remote,
    /// The local store. The remote is a replica, and a conflict there does not stop the local write.
    Local,
}

/// Where a read goes first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadPath {
    /// Ask the local store; ask the remote only when it holds nothing.
    LocalFirst,
    /// Ask the remote; fall back to local only as [`WhenUnreachable`] permits.
    RemoteFirst,
    /// Ask the remote, and fail if it cannot be reached.
    RemoteOnly,
}

/// What a read does when the remote does not answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhenUnreachable {
    /// Refuse. The caller is told nothing was learned.
    Refuse,
    /// Answer from the local copy, and record that it was stale.
    ///
    /// A real choice, and the reason it is a variant rather than a fallback: somebody typed it, and
    /// [`Read::was_stale`] says so at the point of use.
    ServeStale,
}

/// What happens to a write the authoritative side refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnDivergence {
    /// Refuse the whole write. Neither side moves.
    Refuse,
    /// Write locally and record that the sides have diverged.
    ///
    /// Never silent: [`Hybrid::divergences`] holds every one. A conflict resolved silently is data
    /// loss with good manners.
    RecordDivergence,
}

/// The four answers a hybrid store cannot work without.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    /// Whose copy is the record of truth.
    pub authority: Authority,
    /// Where a read goes first.
    pub read_path: ReadPath,
    /// What a read does when the remote is silent.
    pub when_unreachable: WhenUnreachable,
    /// What happens to a write that lost.
    pub on_divergence: OnDivergence,
}

impl Policy {
    /// A policy. Every field must be given; there is deliberately no default.
    #[must_use]
    pub const fn new(
        authority: Authority,
        read_path: ReadPath,
        when_unreachable: WhenUnreachable,
        on_divergence: OnDivergence,
    ) -> Self {
        Self {
            authority,
            read_path,
            when_unreachable,
            on_divergence,
        }
    }
}

/// A read, and whether it came from a copy that might be behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Read<T> {
    /// What was found.
    pub value: T,
    /// `true` when the remote could not be reached and the local copy answered instead.
    pub was_stale: bool,
}

impl<T> Read<T> {
    /// A read from the authoritative side.
    const fn fresh(value: T) -> Self {
        Self {
            value,
            was_stale: false,
        }
    }

    /// A read served from a copy while the remote was silent.
    const fn stale(value: T) -> Self {
        Self {
            value,
            was_stale: true,
        }
    }

    /// `true` when the answer is known to be current.
    #[must_use]
    pub const fn is_fresh(&self) -> bool {
        !self.was_stale
    }
}

/// One recorded disagreement between the two sides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    /// The entity type.
    pub entity: String,
    /// The identity.
    pub id: String,
    /// The revision written locally.
    pub local_revision: u64,
    /// Why the other side did not take it.
    pub detail: String,
}

/// A local store and a remote one, under a declared policy.
#[derive(Debug)]
pub struct Hybrid<L, R> {
    local: L,
    remote: R,
    policy: Policy,
    divergences: Vec<Divergence>,
}

impl<L: Store, R: Store> Hybrid<L, R> {
    /// A hybrid store. The policy is required and has no default.
    pub const fn new(local: L, remote: R, policy: Policy) -> Self {
        Self {
            local,
            remote,
            policy,
            divergences: Vec::new(),
        }
    }

    /// The policy in force.
    pub const fn policy(&self) -> Policy {
        self.policy
    }

    /// Every divergence recorded so far, oldest first.
    ///
    /// Never empty because something was hidden: [`OnDivergence::RecordDivergence`] is the only way
    /// to get one, and it is a choice somebody typed.
    pub fn divergences(&self) -> &[Divergence] {
        &self.divergences
    }

    /// The local side, for a caller reconciling by hand.
    pub const fn local(&self) -> &L {
        &self.local
    }

    /// The remote side.
    pub const fn remote(&self) -> &R {
        &self.remote
    }

    /// Loads, saying whether the answer came from a copy that might be behind.
    ///
    /// # Errors
    ///
    /// [`StoreError::Unreachable`] when the remote is silent and the policy refuses stale answers,
    /// and whatever either store returns otherwise.
    pub fn load_read(
        &self,
        entity: &str,
        id: &str,
    ) -> Result<Read<Option<EntityInstance>>, StoreError> {
        match self.policy.read_path {
            ReadPath::LocalFirst => match self.local.load(entity, id)? {
                Some(instance) => Ok(Read::fresh(Some(instance))),
                None => self.read_via_remote(entity, id),
            },
            ReadPath::RemoteFirst => self.read_via_remote(entity, id),
            ReadPath::RemoteOnly => self.remote.load(entity, id).map(Read::fresh),
        }
    }

    /// Asks the remote, applying the unreachable policy if it is silent.
    fn read_via_remote(
        &self,
        entity: &str,
        id: &str,
    ) -> Result<Read<Option<EntityInstance>>, StoreError> {
        match self.remote.load(entity, id) {
            Ok(instance) => Ok(Read::fresh(instance)),
            Err(error) if error.is_unreachable() => match self.policy.when_unreachable {
                // Nothing was learned, and the caller is told so rather than handed a `None` that
                // reads exactly like "there is no such thing".
                WhenUnreachable::Refuse => Err(error),
                WhenUnreachable::ServeStale => Ok(Read::stale(self.local.load(entity, id)?)),
            },
            Err(error) => Err(error),
        }
    }

    /// Replays every recorded divergence at the side that has not seen it.
    ///
    /// The catch-up path: a laptop that wrote while the replica was down comes back and the writes
    /// go through. Returns what is *still* outstanding, and keeps it — a reconciliation that
    /// cleared its own list on a partial success would report success and lose the rest.
    ///
    /// # What this deliberately does not do
    ///
    /// It replays; it does not merge. A divergence that comes back as a **conflict** rather than as
    /// unreachable means the other side moved on its own, and no rule here can know whose version
    /// is right. Those stay outstanding, with the conflict recorded, for a person — because the
    /// alternative is a machine picking, and a machine picking is how the wrong version wins
    /// silently.
    ///
    /// # Errors
    ///
    /// Never: an outstanding divergence is a state, not a failure. What could not be replayed is in
    /// the returned slice.
    pub fn catch_up(&mut self) -> usize {
        let outstanding: Vec<Divergence> = std::mem::take(&mut self.divergences);
        let mut still = Vec::new();

        for divergence in outstanding {
            let Some(instance) = self
                .local
                .load(&divergence.entity, &divergence.id)
                .ok()
                .flatten()
            else {
                // The local write is gone, so there is nothing left to replicate. Dropping the
                // record is right here: it describes a write that no longer exists.
                continue;
            };

            // Replayed from what the local store actually holds now, not from the decision as it
            // was — the local side may have moved on since, and replicating a superseded revision
            // would push the replica to a state the authority has already left.
            let decision = Decision {
                instance: instance.clone(),
                events: self
                    .local
                    .events(&divergence.entity, &divergence.id)
                    .unwrap_or_default(),
            };

            let expect = match self.remote.load(&divergence.entity, &divergence.id) {
                Ok(Some(held)) => Expect::Revision(held.revision),
                Ok(None) => Expect::Absent,
                Err(error) => {
                    still.push(Divergence {
                        detail: error.to_string(),
                        ..divergence
                    });
                    continue;
                }
            };

            match self.remote.commit(&decision, expect) {
                Ok(()) => {}
                Err(error) => still.push(Divergence {
                    detail: error.to_string(),
                    ..divergence
                }),
            }
        }

        self.divergences = still;
        self.divergences.len()
    }
}

impl<L: Store, R: Store> StateProvider for Hybrid<L, R> {
    fn load(&self, entity: &str, id: &str) -> Result<Option<EntityInstance>, StoreError> {
        self.load_read(entity, id).map(|read| read.value)
    }
}

impl<L: Store, R: Store> EventProvider for Hybrid<L, R> {
    fn events(&self, entity: &str, id: &str) -> Result<Vec<DomainEvent>, StoreError> {
        match self.policy.read_path {
            ReadPath::RemoteOnly => self.remote.events(entity, id),
            ReadPath::RemoteFirst => match self.remote.events(entity, id) {
                Ok(events) => Ok(events),
                Err(error) if error.is_unreachable() => match self.policy.when_unreachable {
                    WhenUnreachable::Refuse => Err(error),
                    WhenUnreachable::ServeStale => self.local.events(entity, id),
                },
                Err(error) => Err(error),
            },
            ReadPath::LocalFirst => match self.local.events(entity, id) {
                Ok(events) if !events.is_empty() => Ok(events),
                Ok(_) => self.remote.events(entity, id),
                Err(error) => Err(error),
            },
        }
    }
}

impl<L: Store, R: Store> Store for Hybrid<L, R> {
    fn commit(&mut self, decision: &Decision, expect: Expect) -> Result<(), StoreError> {
        let (entity, id) = (
            decision.instance.entity.clone(),
            decision.instance.id.clone(),
        );

        match self.policy.authority {
            // The remote decides. It is written first, and the local copy only follows a write that
            // was actually accepted — so a cache can never hold something the record of truth
            // refused.
            Authority::Remote => {
                self.remote.commit(decision, expect)?;
                self.local.commit(decision, expect)
            }
            // The local store decides. It is written first, and the remote is a replica whose
            // refusal is recorded rather than allowed to undo an accepted local write.
            Authority::Local => {
                self.local.commit(decision, expect)?;
                match self.remote.commit(decision, expect) {
                    Ok(()) => Ok(()),
                    Err(error) => match self.policy.on_divergence {
                        OnDivergence::Refuse => Err(error),
                        OnDivergence::RecordDivergence => {
                            self.divergences.push(Divergence {
                                entity,
                                id,
                                local_revision: decision.instance.revision,
                                detail: error.to_string(),
                            });
                            Ok(())
                        }
                    },
                }
            }
        }
    }
}
