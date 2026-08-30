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

use entity_core::{Decision, DecisionRecord, DomainEvent, EntityInstance};
use entity_store::{
    Envelope, EventProvider, Expect, HistoryProvider, RecordedCommit, RecordedObservation,
    StateProvider, Store, StoreError,
};

/// Which side is the record of truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Authority {
    /// The remote. The local copy is a cache and may be rebuilt from it.
    Remote,
    /// The local store. The remote is a replica, and a conflict there does not stop the local write.
    Local,
}

/// One physical side of a hybrid store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoreSide {
    /// The local provider.
    Local,
    /// The remote provider.
    Remote,
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
    /// Refuse the whole write rather than let one side move without the other.
    ///
    /// Under [`Authority::Local`] this reverses the write order — the replica is asked **first**,
    /// because it is the side that can refuse for a reason the authority does not know about, and
    /// a refusal there must leave the authority untouched.
    ///
    /// # The one case this does not cover, and why it is not claimed to
    ///
    /// If the replica accepts and the authority then refuses, the replica has moved and nothing
    /// here can undo it. That is recorded as a [`Divergence`] and returned as an error, rather
    /// than described as impossible: undoing it needs a two-phase commit and a durable intent log,
    /// which this crate does not have and does not pretend to.
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
    /// The explicit freshness contract.
    pub freshness: Freshness,
}

/// Whether a provider can establish that a read came from the authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// Read directly from the declared authority.
    Current,
    /// Read from a replica that may be behind.
    PotentiallyStale,
}

impl<T> Read<T> {
    /// A read from the authoritative side.
    const fn fresh(value: T) -> Self {
        Self {
            value,
            was_stale: false,
            freshness: Freshness::Current,
        }
    }

    /// A read served from a copy while the remote was silent.
    const fn stale(value: T) -> Self {
        Self {
            value,
            was_stale: true,
            freshness: Freshness::PotentiallyStale,
        }
    }

    /// `true` when the answer is known to be current.
    #[must_use]
    pub const fn is_fresh(&self) -> bool {
        !self.was_stale
    }
}

/// One recorded disagreement between the two sides.
///
/// Data, deliberately: it serialises, so a shell whose process ends after every command — a
/// command-line tool — can write what diverged beside the plan and hand it back to the next
/// process with [`Hybrid::remember`], where `catch_up` finds it. A divergence that lived only as
/// long as the process that recorded it would be a divergence nobody could act on.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Divergence {
    /// The entity type.
    pub entity: String,
    /// The identity.
    pub id: String,
    /// The revision written locally.
    pub local_revision: u64,
    /// The side whose accepted write is the source for catch-up.
    pub source: StoreSide,
    /// The side that still needs the record.
    pub destination: StoreSide,
    /// The durable record identity, when this was a recorded commit.
    pub record_id: Option<String>,
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

    /// Hands back a divergence an earlier process recorded, so [`Hybrid::catch_up`] replays it.
    ///
    /// The same divergence remembered twice is held once: two processes that both wrote the record
    /// down, or one that read its own file back after recording, must not turn one disagreement
    /// into two outstanding ones.
    pub fn remember(&mut self, divergence: Divergence) {
        if !self.divergences.contains(&divergence) {
            self.divergences.push(divergence);
        }
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
                Some(instance) => Ok(match self.policy.authority {
                    Authority::Local => Read::fresh(Some(instance)),
                    Authority::Remote => Read::stale(Some(instance)),
                }),
                None => match self.policy.authority {
                    Authority::Local => Ok(Read::fresh(None)),
                    Authority::Remote => self.read_via_remote(entity, id),
                },
            },
            ReadPath::RemoteFirst => self.read_via_remote(entity, id),
            ReadPath::RemoteOnly => {
                self.remote
                    .load(entity, id)
                    .map(|value| match self.policy.authority {
                        Authority::Remote => Read::fresh(value),
                        Authority::Local => Read::stale(value),
                    })
            }
        }
    }

    /// Asks the remote, applying the unreachable policy if it is silent.
    fn read_via_remote(
        &self,
        entity: &str,
        id: &str,
    ) -> Result<Read<Option<EntityInstance>>, StoreError> {
        match self.remote.load(entity, id) {
            Ok(instance) => Ok(match self.policy.authority {
                Authority::Remote => Read::fresh(instance),
                Authority::Local => Read::stale(instance),
            }),
            Err(error) if error.is_unreachable() => match self.policy.when_unreachable {
                // Nothing was learned, and the caller is told so rather than handed a `None` that
                // reads exactly like "there is no such thing".
                WhenUnreachable::Refuse => Err(error),
                WhenUnreachable::ServeStale => Ok(match self.policy.authority {
                    Authority::Local => Read::fresh(self.local.load(entity, id)?),
                    Authority::Remote => Read::stale(self.local.load(entity, id)?),
                }),
            },
            Err(error) => Err(error),
        }
    }

    /// Replays every recorded divergence at the side that has not seen it.
    ///
    /// The catch-up path: a laptop that wrote while the replica was down comes back and the writes
    /// go through. Returns how many are *still* outstanding, and keeps them — a reconciliation
    /// that cleared its own list on a partial success would report success and lose the rest.
    ///
    /// # What this deliberately does not do
    ///
    /// It replays; it does not merge. A replica that moved on its own is **not** overwritten: if
    /// it holds a revision this store's history does not account for, the divergence stays
    /// outstanding with the conflict recorded, for a person. No rule here can know whose version
    /// is right, and a machine picking is how the wrong version wins silently.
    ///
    /// Nothing is dropped. A local read that fails is a divergence that could not be *examined*,
    /// not one that went away — treating an unreadable local store as "the write is gone" would
    /// discard the only record that it happened.
    ///
    /// # What it compares, and what it therefore cannot catch
    ///
    /// Divergence is judged on the **revision** the replica holds, plus an equality check when the
    /// two are at the same one. A replica that took somebody else's write at a revision this store
    /// has already passed is not detected while it stays behind — detecting that needs the
    /// definition, to fold this store's own history down to that revision and compare, and a store
    /// does not hold definitions. It is caught as soon as the replica reaches this store's
    /// revision, and the write itself is still protected: the replay is committed with
    /// [`Expect::Revision`] of what the replica held, so a replica that moves in between refuses
    /// it and the divergence stays outstanding.
    pub fn catch_up(&mut self) -> usize {
        let outstanding: Vec<Divergence> = std::mem::take(&mut self.divergences);
        let mut still = Vec::new();
        for divergence in outstanding {
            let keep = |detail: String| Divergence {
                entity: divergence.entity.clone(),
                id: divergence.id.clone(),
                local_revision: divergence.local_revision,
                source: divergence.source,
                destination: divergence.destination,
                record_id: divergence.record_id.clone(),
                detail,
            };
            let outcome = match divergence.source {
                StoreSide::Local => reconcile(
                    &self.local,
                    &mut self.remote,
                    &divergence.entity,
                    &divergence.id,
                ),
                StoreSide::Remote => reconcile(
                    &self.remote,
                    &mut self.local,
                    &divergence.entity,
                    &divergence.id,
                ),
            };
            if let Err(detail) = outcome {
                still.push(keep(detail));
            }
        }

        self.divergences = still;
        self.divergences.len()
    }
}

fn reconcile<S: Store, D: Store>(
    source: &S,
    destination: &mut D,
    entity: &str,
    id: &str,
) -> Result<(), String> {
    let instance = source
        .load(entity, id)
        .map_err(|error| format!("the source could not be read: {error}"))?
        .ok_or_else(|| {
            "the source holds no instance; absence cannot prove a recorded divergence vanished"
                .to_owned()
        })?;
    let events = source
        .events(entity, id)
        .map_err(|error| format!("the source history could not be read: {error}"))?;
    let held = destination
        .load(entity, id)
        .map_err(|error| format!("the destination could not be read: {error}"))?;
    let at = held.as_ref().map(|held| held.revision);
    let behind = at.unwrap_or(0);
    if held.as_ref() == Some(&instance) {
        return Ok(());
    }
    if behind >= instance.revision {
        return Err(format!(
            "the destination moved on its own: it holds revision {behind} and the source holds {}; no merge rule exists",
            instance.revision
        ));
    }
    let decision = Decision::legacy_import(
        instance,
        events
            .into_iter()
            .filter(|event| event.revision > behind)
            .collect(),
    );
    destination
        .commit(&decision, at.map_or(Expect::Absent, Expect::Revision))
        .map_err(|error| error.to_string())
}

impl<L: Store, R: Store> StateProvider for Hybrid<L, R> {
    /// # Absent means absent, even when the answer came from a stale copy
    ///
    /// [`Read`] carries `was_stale`; this trait has nowhere to put it. So a stale answer that
    /// found **nothing** is not returned as `Ok(None)` — nothing was learned about whether the
    /// instance exists, and `Ok(None)` is the one thing an unreachable store must never say. It
    /// becomes [`StoreError::Unreachable`], which is what a caller with no `was_stale` field can
    /// still act on correctly. A stale answer that found a value is returned: serving it is what
    /// the policy asked for, and it is a fact about the data rather than about the network.
    ///
    /// Callers that need to know staleness while still getting the value use
    /// [`Hybrid::load_read`].
    fn load(&self, entity: &str, id: &str) -> Result<Option<EntityInstance>, StoreError> {
        let read = self.load_read(entity, id)?;
        match (read.was_stale, read.value) {
            (true, None) => Err(StoreError::Unreachable {
                provider: "hybrid".to_owned(), detail: "the authority did not answer and the local copy holds nothing, so whether this instance exists is unknown"
                    .to_owned(),
            }),
            (_, value) => Ok(value),
        }
    }

    /// # Through the read path, and an unreachable authority is `Unreachable`, never an empty list
    ///
    /// The same shape as [`EventProvider::events`] on this type: `RemoteOnly` asks the remote and
    /// fails as it fails; `RemoteFirst` asks the remote and, when it cannot be reached, does what
    /// [`WhenUnreachable`] says — refuse, or list the local copy; `LocalFirst` lists the local copy
    /// and asks the remote only when it holds nothing. What is refused is the shape where a store
    /// that could not be asked reads as a store that holds nothing: a shell hydrating from an
    /// empty list would rebuild an empty process and call it current.
    fn ids(&self, entity: &str) -> Result<Vec<String>, StoreError> {
        match self.policy.read_path {
            ReadPath::RemoteOnly => self.remote.ids(entity),
            ReadPath::RemoteFirst => match self.remote.ids(entity) {
                Ok(ids) => Ok(ids),
                Err(error) if error.is_unreachable() => match self.policy.when_unreachable {
                    WhenUnreachable::Refuse => Err(error),
                    WhenUnreachable::ServeStale => self.local.ids(entity),
                },
                Err(error) => Err(error),
            },
            ReadPath::LocalFirst => match self.local.ids(entity) {
                Ok(ids) if !ids.is_empty() => Ok(ids),
                Ok(ids) => match self.policy.authority {
                    Authority::Local => Ok(ids),
                    Authority::Remote => self.remote.ids(entity),
                },
                Err(error) => Err(error),
            },
        }
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
                Ok(events) => match self.policy.authority {
                    Authority::Local => Ok(events),
                    Authority::Remote => self.remote.events(entity, id),
                },
                Err(error) => Err(error),
            },
        }
    }
}

impl<L, R> HistoryProvider for Hybrid<L, R>
where
    L: Store + HistoryProvider,
    R: Store + HistoryProvider,
{
    fn records(&self, entity: &str, id: &str) -> Result<Vec<Envelope<DecisionRecord>>, StoreError> {
        match self.policy.authority {
            Authority::Local => self.local.records(entity, id),
            Authority::Remote => self.remote.records(entity, id),
        }
    }

    fn observations(&self, entity: &str, id: &str) -> Result<Vec<RecordedObservation>, StoreError> {
        match self.policy.authority {
            Authority::Local => self.local.observations(entity, id),
            Authority::Remote => self.remote.observations(entity, id),
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
                match self.local.commit(decision, expect) {
                    Ok(()) => Ok(()),
                    // The mirror of the `Authority::Local` case, and it was missed when that one
                    // was fixed: the authority took the write and the **local copy** refused it —
                    // a full disk is enough. Returning the error alone left the two sides
                    // disagreeing with nothing recorded, `divergences()` empty and `catch_up()` a
                    // no-op, while every later write computed its expectation from the stale local
                    // revision and was refused by the authority for ever.
                    //
                    // Recorded rather than described as impossible: nothing here can undo the
                    // authority's write, and a divergence a person can see is the honest end of it.
                    Err(error) => {
                        self.divergences.push(Divergence {
                            entity,
                            id,
                            local_revision: decision.instance.revision,
                            source: StoreSide::Remote,
                            destination: StoreSide::Local,
                            record_id: None,
                            detail: format!(
                                "the authority accepted revision {} and the local copy refused it: \
                                 {error}", decision.instance.revision
                            ),
                        });
                        Err(error)
                    }
                }
            }
            // The local store decides, and what "decides" means depends on what was declared for
            // a divergence.
            Authority::Local => match self.policy.on_divergence {
                // `Refuse` promises neither side moves. That promise cannot be kept by writing the
                // local store first and asking the replica afterwards: a replica that refuses
                // leaves an accepted local write standing, unreplicated, with the caller told the
                // write failed. So under `Refuse` the **replica is asked first** — the side that
                // can refuse for a reason the authority does not know about.
                OnDivergence::Refuse => {
                    self.remote.commit(decision, expect)?;
                    match self.local.commit(decision, expect) {
                        Ok(()) => Ok(()),
                        // The residual case, and the reason this is not two-phase commit: the
                        // replica took the write and the authority then refused it. Nothing here
                        // can undo the replica, so the fact is **recorded** rather than claimed
                        // impossible — a divergence in the other direction, which `catch_up` will
                        // report as a conflict for a person.
                        Err(error) => {
                            self.divergences.push(Divergence {
                                entity, id, local_revision: decision.instance.revision,
                                source: StoreSide::Remote,
                                destination: StoreSide::Local,
                                record_id: None,
                                detail: format!(
                                    "the replica accepted revision {} and this store refused it:                                      {error}", decision.instance.revision
                                ),
                            });
                            Err(error)
                        }
                    }
                }
                // The replica is a replica: it is written second and its refusal is recorded
                // rather than allowed to undo an accepted authority write.
                OnDivergence::RecordDivergence => {
                    self.local.commit(decision, expect)?;
                    match self.remote.commit(decision, expect) {
                        Ok(()) => Ok(()),
                        Err(error) => {
                            self.divergences.push(Divergence {
                                entity,
                                id,
                                local_revision: decision.instance.revision,
                                source: StoreSide::Local,
                                destination: StoreSide::Remote,
                                record_id: None,
                                detail: error.to_string(),
                            });
                            Ok(())
                        }
                    }
                }
            },
        }
    }

    fn commit_recorded(
        &mut self,
        commit: &RecordedCommit,
        expect: Expect,
    ) -> Result<(), StoreError> {
        commit.validate()?;
        let entity = commit.instance.entity.clone();
        let id = commit.instance.id.clone();
        let revision = commit.instance.revision;
        let record_id = Some(commit.envelope.record_id.clone());
        match self.policy.authority {
            Authority::Remote => {
                self.remote.commit_recorded(commit, expect)?;
                match self.local.commit_recorded(commit, expect) {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        self.divergences.push(Divergence {
                            entity,
                            id,
                            local_revision: revision,
                            source: StoreSide::Remote,
                            destination: StoreSide::Local,
                            record_id,
                            detail: format!(
                                "the authority accepted record at revision {revision} and the local copy refused it: {error}"
                            ),
                        });
                        Err(error)
                    }
                }
            }
            Authority::Local => match self.policy.on_divergence {
                OnDivergence::Refuse => {
                    self.remote.commit_recorded(commit, expect)?;
                    match self.local.commit_recorded(commit, expect) {
                        Ok(()) => Ok(()),
                        Err(error) => {
                            self.divergences.push(Divergence {
                                entity,
                                id,
                                local_revision: revision,
                                source: StoreSide::Remote,
                                destination: StoreSide::Local,
                                record_id,
                                detail: format!(
                                    "the replica accepted record at revision {revision} and the authority refused it: {error}"
                                ),
                            });
                            Err(error)
                        }
                    }
                }
                OnDivergence::RecordDivergence => {
                    self.local.commit_recorded(commit, expect)?;
                    match self.remote.commit_recorded(commit, expect) {
                        Ok(()) => Ok(()),
                        Err(error) => {
                            self.divergences.push(Divergence {
                                entity,
                                id,
                                local_revision: revision,
                                source: StoreSide::Local,
                                destination: StoreSide::Remote,
                                record_id,
                                detail: error.to_string(),
                            });
                            Ok(())
                        }
                    }
                }
            },
        }
    }

    fn observe(&mut self, observation: &RecordedObservation) -> Result<(), StoreError> {
        observation.validate()?;
        let divergence = |source, destination, detail: String| Divergence {
            entity: observation.entity.clone(),
            id: observation.id.clone(),
            local_revision: observation.revision,
            source,
            destination,
            record_id: Some(observation.envelope.record_id.clone()),
            detail,
        };
        match self.policy.authority {
            Authority::Remote => {
                self.remote.observe(observation)?;
                match self.local.observe(observation) {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        self.divergences.push(divergence(
                            StoreSide::Remote,
                            StoreSide::Local,
                            error.to_string(),
                        ));
                        Err(error)
                    }
                }
            }
            Authority::Local => match self.policy.on_divergence {
                OnDivergence::Refuse => {
                    self.remote.observe(observation)?;
                    match self.local.observe(observation) {
                        Ok(()) => Ok(()),
                        Err(error) => {
                            self.divergences.push(divergence(
                                StoreSide::Remote,
                                StoreSide::Local,
                                error.to_string(),
                            ));
                            Err(error)
                        }
                    }
                }
                OnDivergence::RecordDivergence => {
                    self.local.observe(observation)?;
                    match self.remote.observe(observation) {
                        Ok(()) => Ok(()),
                        Err(error) => {
                            self.divergences.push(divergence(
                                StoreSide::Local,
                                StoreSide::Remote,
                                error.to_string(),
                            ));
                            Ok(())
                        }
                    }
                }
            },
        }
    }
}
