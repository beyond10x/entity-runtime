//! Provider-attested reuse of a completely verified observation.
use super::*;
use eventlog_core::{CaptureCheckpoint, CaptureUsage, TenantCaptureDelta, TenantCaptureUpdate};

/// Integrity boundary for reads through one live provider handle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CapturePolicy {
    /// Obtain fresh content for every complete read, including providers without change tracking.
    ///
    /// An open verifies the whole authority and never reads or writes an open checkpoint.
    #[default]
    FullVerification,
    /// Reuse only provider-attested unchanged content or an exactly verified append suffix.
    ///
    /// An open verifies the whole authority, unless the store's owner enabled durable open
    /// checkpoints: then it verifies the persisted checkpoint of the last completely verified
    /// observation, the provider's proof that only its acknowledged appends followed it, and those
    /// appends. SQLite tracks SQL writes through every connection, live or between opens; raw
    /// file edits bypassing SQLite are outside that guarantee, and only a `FullVerification` open
    /// or a complete read sees them.
    ProviderTracked,
}

impl CapturePolicy {
    /// The complete transient policy domain, in specification declaration order.
    pub const ALL: [Self; 2] = [Self::FullVerification, Self::ProviderTracked];
}

/// How a store handle's open verified its authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenVerification {
    /// Every event, bound blob and index row of the tenant was captured and verified.
    Complete,
    /// The persisted open checkpoint was verified, and the provider proved that nothing changed
    /// the tenant's captured material since the observation it names.
    Checkpoint,
    /// The persisted open checkpoint was verified, and then every event appended after it.
    Suffix {
        /// Events in the verified suffix.
        events: u64,
    },
}

#[derive(Default)]
pub(super) struct Cache {
    generation: u64,
    held: Option<Held>,
    /// A handle opened from a checkpoint, until a complete read verifies the whole authority.
    bounded: Option<Bounded>,
    /// The highest tenant position this handle has verified. History is append-only, so a
    /// complete capture whose head is behind it is a store that lost its tail under this handle.
    floor: u64,
    /// Moves whenever what the handle answers from moves: a verified suffix, a complete
    /// verification, an installed or forgotten observation; never on a provider `Unchanged`. Reads
    /// of the provider's rows made between two continuations that leave it in place read the rows
    /// the provider attested unchanged between them.
    serial: u64,
}

/// A handle with no model: what it answers comes from the provider's index rows, which the last
/// complete verification held to the events and every verified suffix since kept equal to them,
/// and which the provider's continuity proof shows nothing else has written.
#[derive(Clone)]
pub(super) struct Bounded {
    pub(super) checkpoint: CaptureCheckpoint,
    pub(super) last_position: u64,
    pub(super) binding: PhysicalRef,
    /// The usage the provider bound to the observation, which a suffix after it must add to
    /// exactly. Not the handle's read-bound totals, which its own writes advance before the
    /// provider reports them.
    pub(super) usage: CaptureUsage,
}

/// What a `ProviderTracked` handle answers a read from once it has continued its observation.
pub(super) enum Tracked {
    /// The provider's verified rows: the handle has no model. Carries the serial the continuation
    /// left, which [`EventlogRecordedStore::unchanged_since`] compares after the rows are read.
    Rows(u64),
    /// The whole verified model.
    Whole(Arc<CapturedModel>),
}

/// The observation a handle would persist as its open checkpoint.
pub(super) struct Observation {
    pub(super) checkpoint: CaptureCheckpoint,
    pub(super) position: u64,
    pub(super) binding: PhysicalRef,
}

pub(super) struct Held {
    checkpoint: Option<CaptureCheckpoint>,
    pub(super) model: Arc<CapturedModel>,
    /// Bound blobs an advance may read again: wrappers, requests and the rest. Not the record and
    /// batch blobs of committed records, whose bytes the model holds; which digests are bound at
    /// all is `model.held.digests`.
    blobs: BTreeMap<String, Vec<u8>>,
    rows: Vec<BTreeMap<String, Value>>,
    last_position: u64,
}

impl EventlogRecordedStore {
    /// The whole verified model: the held one continued by the provider, or a complete
    /// verification. A handle opened from a checkpoint takes a complete one here.
    pub(super) async fn tracked_model(&self) -> Result<Arc<CapturedModel>, AsyncStoreError> {
        let result = self.tracked_model_once().await;
        if let Err(error) = &result {
            self.note_refusal(error);
        }
        result
    }

    /// Continues this handle's observation and says what the next read is answered from.
    ///
    /// A bounded handle asks the provider to continue from its checkpoint: `Unchanged` leaves its
    /// rows verified; an `AppendDelta` is verified against the rows it changes before it is
    /// accepted; `Complete`, or a suffix that cannot be verified that way, makes it a whole-model
    /// handle through a complete verification. Any other handle continues its whole model.
    pub(super) async fn continue_tracked(&self) -> Result<Tracked, AsyncStoreError> {
        let result = self.continue_tracked_once().await;
        if let Err(error) = &result {
            self.note_refusal(error);
        }
        result
    }

    async fn continue_tracked_once(&self) -> Result<Tracked, AsyncStoreError> {
        loop {
            let (generation, bounded, floor) = {
                let cache = self
                    .tracked
                    .lock()
                    .map_err(|_| integrity("tracked cache poisoned"))?;
                match &cache.bounded {
                    Some(bounded) => (cache.generation, bounded.clone(), cache.floor),
                    None => break,
                }
            };
            let update = self
                .backend
                .capture_tenant_since(
                    &self.tenant,
                    projection_specs(),
                    self.limits,
                    Some(&bounded.checkpoint),
                )
                .await
                .map_err(map_capture)?;
            let suffix = match &update {
                TenantCaptureUpdate::AppendDelta { delta, .. } => Some(
                    self.verify_suffix(bounded.usage, bounded.last_position, delta)
                        .await,
                ),
                _ => None,
            };
            let mut cache = self
                .tracked
                .lock()
                .map_err(|_| integrity("tracked cache poisoned"))?;
            if cache.generation != generation {
                continue;
            }
            let next_generation = generation
                .checked_add(1)
                .ok_or_else(|| integrity("tracked cache generation exhausted"))?;
            match (update, suffix) {
                (
                    TenantCaptureUpdate::Complete {
                        capture,
                        checkpoint,
                    },
                    _,
                ) => {
                    let held = self.verify_complete(capture, checkpoint, floor)?;
                    let model = Arc::clone(&held.model);
                    cache.floor = cache.floor.max(held.last_position);
                    cache.held = Some(held);
                    cache.bounded = None;
                    cache.generation = next_generation;
                    cache.serial = cache.serial.wrapping_add(1);
                    return Ok(Tracked::Whole(model));
                }
                (TenantCaptureUpdate::Unchanged { checkpoint }, _) => {
                    cache.bounded = Some(Bounded {
                        checkpoint,
                        ..bounded
                    });
                    cache.generation = next_generation;
                    return Ok(Tracked::Rows(cache.serial));
                }
                (TenantCaptureUpdate::AppendDelta { checkpoint, delta }, Some(Ok(suffix))) => {
                    self.install_suffix(&delta, &suffix)?;
                    cache.floor = cache.floor.max(suffix.last_position);
                    cache.bounded = Some(Bounded {
                        checkpoint,
                        last_position: suffix.last_position,
                        binding: bounded.binding,
                        usage: suffix.usage,
                    });
                    cache.generation = next_generation;
                    cache.serial = cache.serial.wrapping_add(1);
                    return Ok(Tracked::Rows(cache.serial));
                }
                (TenantCaptureUpdate::AppendDelta { .. }, _) => {
                    // A suffix the rows cannot verify is not the answer: a complete verification
                    // is, and it words any refusal exactly as an open would.
                    cache.bounded = None;
                    cache.generation = next_generation;
                    cache.serial = cache.serial.wrapping_add(1);
                    break;
                }
            }
        }
        Ok(Tracked::Whole(self.tracked_model().await?))
    }

    /// A complete capture verified into a whole model, as an open without a checkpoint does.
    pub(super) fn verify_complete(
        &self,
        mut capture: TenantCapture,
        checkpoint: Option<CaptureCheckpoint>,
        floor: u64,
    ) -> Result<Held, AsyncStoreError> {
        self.native_captures.fetch_add(1, Ordering::Relaxed);
        if capture.tenant != self.tenant
            || capture.stream_identity != self.authority.stream_identity
        {
            return Err(integrity("native capture substituted tenant generation"));
        }
        // The model takes the record and batch blobs; what this handle keeps of the capture is
        // every other blob, which is all an advance reads again.
        let model = build_model_owning(&self.authority, &mut capture)?;
        if model.binding.is_none() {
            return Err(integrity("the tenant has no authoritative binding"));
        }
        self.model_builds.fetch_add(1, Ordering::Relaxed);
        self.records_decoded
            .fetch_add(model.decoded, Ordering::Relaxed);
        let last_position = capture.events.last().map_or(0, |e| e.global_seq);
        if last_position < floor {
            return Err(integrity(format!(
                "the provider's head {last_position} is behind position {floor} this handle already verified"
            )));
        }
        *self
            .held
            .lock()
            .map_err(|_| integrity("read bound lock poisoned"))? = model.held.clone();
        self.refused.store(false, Ordering::Relaxed);
        Ok(Held {
            checkpoint,
            model: Arc::new(model),
            last_position,
            blobs: capture
                .blobs
                .into_iter()
                .map(|b| (b.digest, b.bytes))
                .collect(),
            rows: capture
                .projections
                .into_iter()
                .map(|p| p.rows.into_iter().collect())
                .collect(),
        })
    }

    /// Installs a verified whole model as this handle's observation.
    pub(super) fn install_held(&self, held: Held) -> Result<(), AsyncStoreError> {
        let mut cache = self
            .tracked
            .lock()
            .map_err(|_| integrity("tracked cache poisoned"))?;
        cache.floor = cache.floor.max(held.last_position);
        cache.held = Some(held);
        cache.bounded = None;
        cache.generation = cache
            .generation
            .checked_add(1)
            .ok_or_else(|| integrity("tracked cache generation exhausted"))?;
        cache.serial = cache.serial.wrapping_add(1);
        Ok(())
    }

    /// Installs a verified checkpoint, or a checkpoint and its verified suffix, as this handle's
    /// observation: a handle with no model.
    pub(super) fn install_bounded(&self, bounded: Bounded) -> Result<(), AsyncStoreError> {
        let mut cache = self
            .tracked
            .lock()
            .map_err(|_| integrity("tracked cache poisoned"))?;
        cache.floor = cache.floor.max(bounded.last_position);
        cache.held = None;
        cache.bounded = Some(bounded);
        cache.generation = cache
            .generation
            .checked_add(1)
            .ok_or_else(|| integrity("tracked cache generation exhausted"))?;
        cache.serial = cache.serial.wrapping_add(1);
        Ok(())
    }

    /// Forgets the held observation, so the next read verifies completely; what this handle has
    /// verified it keeps as its floor.
    #[cfg(feature = "sync-bridge")]
    pub(super) fn forget_observation(&self) -> Result<(), AsyncStoreError> {
        let mut cache = self
            .tracked
            .lock()
            .map_err(|_| integrity("tracked cache poisoned"))?;
        cache.held = None;
        cache.bounded = None;
        cache.generation = cache
            .generation
            .checked_add(1)
            .ok_or_else(|| integrity("tracked cache generation exhausted"))?;
        cache.serial = cache.serial.wrapping_add(1);
        Ok(())
    }

    /// The observation this handle would persist, if it holds one a later open may start from.
    ///
    /// A whole model that holds a forked or lineage subject is never persisted: the bounded
    /// reads and the suffix verifier answer only linear histories.
    pub(super) fn observation(&self) -> Result<Option<Observation>, AsyncStoreError> {
        let cache = self
            .tracked
            .lock()
            .map_err(|_| integrity("tracked cache poisoned"))?;
        if let Some(held) = &cache.held {
            let (Some(checkpoint), Some(binding)) =
                (held.checkpoint.clone(), held.model.binding.clone())
            else {
                return Ok(None);
            };
            if !held.model.forked.is_empty() || !held.model.lineage_subjects.is_empty() {
                return Ok(None);
            }
            return Ok(Some(Observation {
                checkpoint,
                position: held.last_position,
                binding,
            }));
        }
        Ok(cache.bounded.as_ref().map(|bounded| Observation {
            checkpoint: bounded.checkpoint.clone(),
            position: bounded.last_position,
            binding: bounded.binding.clone(),
        }))
    }

    /// The highest tenant position this handle has verified.
    pub(super) fn verified_position(&self) -> Result<u64, AsyncStoreError> {
        Ok(self
            .tracked
            .lock()
            .map_err(|_| integrity("tracked cache poisoned"))?
            .floor)
    }

    async fn tracked_model_once(&self) -> Result<Arc<CapturedModel>, AsyncStoreError> {
        loop {
            let (generation, checkpoint, floor) = {
                let cache = self
                    .tracked
                    .lock()
                    .map_err(|_| integrity("tracked cache poisoned"))?;
                (
                    cache.generation,
                    // A bounded handle has no model to continue: it asks for complete content.
                    cache.held.as_ref().and_then(|h| h.checkpoint.clone()),
                    cache.floor,
                )
            };
            let update = self
                .backend
                .capture_tenant_since(
                    &self.tenant,
                    projection_specs(),
                    self.limits,
                    checkpoint.as_ref(),
                )
                .await
                .map_err(map_capture)?;
            let answer = {
                let mut cache = self
                    .tracked
                    .lock()
                    .map_err(|_| integrity("tracked cache poisoned"))?;
                if cache.generation != generation {
                    continue;
                }
                let next_generation = generation
                    .checked_add(1)
                    .ok_or_else(|| integrity("tracked cache generation exhausted"))?;
                let held = match update {
                    TenantCaptureUpdate::Complete {
                        capture,
                        checkpoint,
                    } => Some(self.verify_complete(capture, checkpoint, floor)?),
                    TenantCaptureUpdate::Unchanged { checkpoint } => {
                        let mut held = cache
                            .held
                            .take()
                            .ok_or_else(|| integrity("unchanged capture has no verified base"))?;
                        if held.checkpoint.is_none() {
                            return Err(integrity("unchanged capture has no provider checkpoint"));
                        }
                        held.checkpoint = Some(checkpoint);
                        Some(held)
                    }
                    TenantCaptureUpdate::AppendDelta { checkpoint, delta } => {
                        let added_digests: Vec<_> =
                            delta.blobs.iter().map(|blob| blob.digest.clone()).collect();
                        let held = cache
                            .held
                            .take()
                            .ok_or_else(|| integrity("capture delta has no verified base"))?;
                        // A concurrent reader keeps its exact old model. Rebuild rather than
                        // copying the whole authority merely to advance one record.
                        advance(&self.authority, held, delta, checkpoint)
                            .ok()
                            .inspect(|held| {
                                if let Ok(mut bounds) = self.held.lock() {
                                    bounds.digests.extend(added_digests);
                                }
                                self.model_advances.fetch_add(1, Ordering::Relaxed);
                                self.records_decoded
                                    .fetch_add(held.model.decoded, Ordering::Relaxed);
                            })
                    }
                };
                cache.generation = next_generation;
                cache.serial = cache.serial.wrapping_add(1);
                if let Some(verified) = &held {
                    cache.bounded = None;
                    cache.floor = cache.floor.max(verified.last_position);
                }
                cache.held = held;
                if let Some(verified) = &cache.held {
                    // Publish accounting under the same generation lock as its model. A slower
                    // caller must not overwrite a newer observation's totals after releasing it.
                    let mut bounds = self
                        .held
                        .lock()
                        .map_err(|_| integrity("read bound lock poisoned"))?;
                    bounds.events = verified.model.held.events;
                    bounds.blobs = verified.model.held.blobs;
                    bounds.rows = verified.model.held.rows;
                }
                cache.held.as_ref().map(|h| Arc::clone(&h.model))
            };
            if let Some(model) = answer {
                return Ok(model);
            }
            // Unsupported suffixes and refused tentative advances have no installed checkpoint.
            // The next iteration asks for complete content and preserves the full verifier's error.
        }
    }

    #[cfg(feature = "sync-bridge")]
    pub(crate) async fn scoped_histories(
        &self,
        subjects: &[Subject],
    ) -> Result<Vec<SubjectHistory>, AsyncStoreError> {
        let mut scope = ReadScope::default();
        for subject in subjects {
            subject.validate()?;
            scope.subjects.insert(subject.clone());
        }
        let read = self.scoped_model(&scope).await?;
        Ok(subjects
            .iter()
            .map(|subject| model_history(&read.model, subject))
            .collect())
    }

    #[cfg(feature = "sync-bridge")]
    pub(crate) async fn scoped_state(
        &self,
        subject: &Subject,
    ) -> Result<Option<EntityInstance>, AsyncStoreError> {
        if self.policy == CapturePolicy::ProviderTracked {
            return self.tracked_state(subject).await;
        }
        let read = self.scoped_model(&ReadScope::subject(subject)).await?;
        model_state(&read.model, subject)
    }

    #[cfg(feature = "sync-bridge")]
    pub(crate) async fn scoped_record(
        &self,
        id: &str,
    ) -> Result<Option<RecordLookup>, AsyncStoreError> {
        if self.policy == CapturePolicy::ProviderTracked {
            return self.tracked_record(id).await;
        }
        let read = self.scoped_model(&ReadScope::record(id)).await?;
        Ok(read.model.records.get(id).map(ModelLookup::to_public))
    }

    #[cfg(feature = "sync-bridge")]
    pub(crate) async fn scoped_batch(
        &self,
        key: &BatchKey,
    ) -> Result<Option<StoredBatch>, AsyncStoreError> {
        if self.policy == CapturePolicy::ProviderTracked {
            return self.tracked_batch(key).await;
        }
        let read = self.scoped_model(&ReadScope::batch(key)).await?;
        Ok(read.model.batches.get(key).map(ModelBatch::to_public))
    }

    /// A subject's state: from the whole model, or from its row on a bounded handle.
    pub(super) async fn tracked_state(
        &self,
        subject: &Subject,
    ) -> Result<Option<EntityInstance>, AsyncStoreError> {
        let result = match self.tracked_point(Point::State(subject)).await {
            Ok(Answer::State(state)) => Ok(state),
            Ok(_) => Err(integrity("a state read answered another read")),
            Err(error) => Err(error),
        };
        self.noted(result)
    }

    /// A record lookup: from the whole model, or from its row on a bounded handle.
    pub(super) async fn tracked_record(
        &self,
        id: &str,
    ) -> Result<Option<RecordLookup>, AsyncStoreError> {
        let result = match self.tracked_point(Point::Record(id)).await {
            Ok(Answer::Record(record)) => Ok(*record),
            Ok(_) => Err(integrity("a record lookup answered another read")),
            Err(error) => Err(error),
        };
        self.noted(result)
    }

    /// A batch lookup: from the whole model, or from its row on a bounded handle.
    pub(super) async fn tracked_batch(
        &self,
        key: &BatchKey,
    ) -> Result<Option<StoredBatch>, AsyncStoreError> {
        let result = match self.tracked_point(Point::Batch(key)).await {
            Ok(Answer::Batch(batch)) => Ok(batch),
            Ok(_) => Err(integrity("a batch lookup answered another read")),
            Err(error) => Err(error),
        };
        self.noted(result)
    }

    /// A subject's history: from the whole model, or read per entity on a bounded handle, as a
    /// `FullVerification` handle reads it, confirmed as [`Self::scoped_model`] confirms it.
    pub(super) async fn tracked_history(
        &self,
        subject: &Subject,
    ) -> Result<SubjectHistory, AsyncStoreError> {
        let read = self.scoped_model(&ReadScope::subject(subject)).await?;
        Ok(model_history(&read.model, subject))
    }

    /// One point read on a `ProviderTracked` handle.
    ///
    /// A bounded handle reads the row and the blobs it names after the provider's continuity
    /// answer, in transactions of their own: the Eventlog port answers continuity and reads rows
    /// in separate calls. So the answer is served only if the provider, asked again afterwards,
    /// still leaves the observation in place: then nothing reached the tenant between the two
    /// answers, and the rows read are the rows the last verification held. Otherwise the read is
    /// taken again from the continued observation. After [`CONFIRMATIONS`] attempts that each
    /// overlapped a write, one complete verification answers it from the whole model.
    async fn tracked_point(&self, read: Point<'_>) -> Result<Answer, AsyncStoreError> {
        for _ in 0..CONFIRMATIONS {
            let serial = match self.continue_tracked().await? {
                Tracked::Whole(model) => return read.answered_by(&model),
                Tracked::Rows(serial) => serial,
            };
            let answer = match read {
                Point::State(subject) => self.rows_state(subject).await.map(Answer::State),
                Point::Record(id) => self
                    .rows_record(id)
                    .await
                    .map(|record| Answer::Record(Box::new(record))),
                Point::Batch(key) => self.rows_batch(key).await.map(Answer::Batch),
            };
            // A refusal is confirmed too: one a concurrent write caused is taken again, and one
            // the provider still attests nothing changed for is the store's answer.
            if self.unchanged_since(serial).await? {
                return answer;
            }
        }
        let model = self.tracked_model().await?;
        read.answered_by(&model)
    }

    /// Whether the provider, asked now, leaves in place the observation `serial` named: nothing
    /// reached the tenant's captured material since the continuation that returned it.
    pub(super) async fn unchanged_since(&self, serial: u64) -> Result<bool, AsyncStoreError> {
        Ok(matches!(
            self.continue_tracked().await?,
            Tracked::Rows(now) if now == serial
        ))
    }
}

/// How many times a bounded read is taken again when a write overlapped it before one complete
/// verification answers it instead.
pub(super) const CONFIRMATIONS: usize = 3;

/// A point read a `ProviderTracked` handle answers from its model or from the provider's rows.
#[derive(Clone, Copy)]
enum Point<'a> {
    State(&'a Subject),
    Record(&'a str),
    Batch(&'a BatchKey),
}

enum Answer {
    State(Option<EntityInstance>),
    /// Boxed: a record lookup is far larger than the other answers.
    Record(Box<Option<RecordLookup>>),
    Batch(Option<StoredBatch>),
}

impl Point<'_> {
    fn answered_by(self, model: &CapturedModel) -> Result<Answer, AsyncStoreError> {
        Ok(match self {
            Self::State(subject) => Answer::State(model_state(model, subject)?),
            Self::Record(id) => {
                Answer::Record(Box::new(model.records.get(id).map(ModelLookup::to_public)))
            }
            Self::Batch(key) => Answer::Batch(model.batches.get(key).map(ModelBatch::to_public)),
        })
    }
}

fn advance(
    authority: &Authority,
    held: Held,
    delta: TenantCaptureDelta,
    checkpoint: CaptureCheckpoint,
) -> Result<Held, AsyncStoreError> {
    if held.checkpoint.is_none()
        || delta.tenant.as_str() != authority.tenant
        || delta.stream_identity != authority.stream_identity
        || delta
            .events
            .iter()
            .any(|event| event.name != "er.recorded_entry" || event.digest.is_some())
    {
        return Err(integrity("capture suffix requires complete verification"));
    }
    let Held {
        model,
        mut blobs,
        mut rows,
        mut last_position,
        ..
    } = held;
    let mut model = Arc::try_unwrap(model)
        .map_err(|_| integrity("verified model is held by another reader"))?;
    let mut new_blobs = BTreeSet::new();
    for blob in delta.blobs {
        // Every bound digest, not only the retained blobs: the record and batch blobs the model
        // holds are no longer retained here, and a suffix rebinding one is still a replacement.
        if !new_blobs.insert(blob.digest.clone())
            || model.held.digests.contains(&blob.digest)
            || blobs.insert(blob.digest.clone(), blob.bytes).is_some()
        {
            return Err(integrity("capture suffix replaces a bound blob"));
        }
        model.held.digests.insert(blob.digest);
    }
    // Borrow only the suffix's references. Building a map of every retained blob is itself a
    // whole-store scan, even when no bytes are decoded.
    let mut wanted = BTreeSet::new();
    // The suffix's record and batch blobs, which the advanced model copies and this handle then
    // stops retaining. A later suffix naming one is refused here and verified by a whole build.
    let mut held_by_model = Vec::new();
    for event in &delta.events {
        if event.global_seq <= last_position {
            return Err(integrity("capture suffix does not advance tenant position"));
        }
        last_position = event.global_seq;
        let digest = reference_digest(event)?;
        let wrapper = decode_entry(
            blobs
                .get(digest)
                .ok_or_else(|| integrity("suffix wrapper blob is missing"))?,
        )?;
        held_by_model.extend([wrapper.record_blob.clone(), wrapper.batch_blob.clone()]);
        wanted.extend([
            digest.to_owned(),
            wrapper.record_blob,
            wrapper.request_blob,
            wrapper.batch_blob,
        ]);
    }
    let borrowed: BTreeMap<&str, &[u8]> = wanted
        .iter()
        .map(|digest| {
            blobs
                .get(digest)
                .map(|bytes| (digest.as_str(), bytes.as_slice()))
                .ok_or_else(|| integrity("capture suffix references a missing blob"))
        })
        .collect::<Result<_, _>>()?;
    let mut bound = BoundBlobs::new(&borrowed);
    let mut pending = Vec::new();
    model.decoded = 0;
    for event in &delta.events {
        admit_event(authority, event, &mut bound, &mut model, &mut pending)?;
    }
    let mut starts = BTreeMap::new();
    let mut keys = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for record in &pending {
        let subject = record.entry.subject();
        if model.lineage_subjects.contains(&subject)
            || model
                .histories
                .get(&subject)
                .is_some_and(|h| !matches!(h.origin, HistoryOrigin::Genesis))
        {
            return Err(integrity(
                "nonlinear or imported suffix needs complete verification",
            ));
        }
        starts
            .entry(subject.clone())
            .or_insert_with(|| model.histories.get(&subject).map_or(0, |h| h.records.len()));
        keys.insert(BatchKey::from(record.wrapper.batch_key.clone()));
        ids.insert(record.entry.record_id().to_owned());
    }
    insert_committed(pending, &mut bound, &mut model)?;
    for (subject, start) in &starts {
        let history = model
            .histories
            .get_mut(subject)
            .ok_or_else(|| integrity("suffix subject missing"))?;
        // Batches are admitted as whole groups; a coalesced delta can contain several keys in
        // another lexical order. Sort only their new suffix, never the verified prefix.
        history.records[*start..].sort_by_key(|record| record.position.store);
        let mut previous = start.checked_sub(1).map(|i| history.records[i].position);
        let mut state = model.terminals.get(subject).cloned();
        for record in &history.records[*start..] {
            if previous.is_some_and(|p| {
                p.store >= record.position.store || p.subject >= record.position.subject
            }) {
                return Err(corrupt(
                    subject,
                    "suffix positions do not follow verified history",
                ));
            }
            // The remaining receipt fields are constructed from this exact entry and position
            // by insert_committed. Its batch key still needs the full history verifier's checks.
            record.receipt.batch_key.validate().map_err(|error| {
                corrupt(
                    subject,
                    format!("record receipt carries an invalid key: {error}"),
                )
            })?;
            if let BatchKey::SingleRecord(id) = &record.receipt.batch_key
                && (id != &record.receipt.record_id || record.receipt.member_index != 0)
            {
                return Err(corrupt(
                    subject,
                    "single-record receipt does not reproduce its record identity and zero index",
                ));
            }
            state = validate_entry_against_state(&record.entry, record.expect, state.as_ref())?;
            previous = Some(record.position);
        }
        model.terminals.insert(
            subject.clone(),
            state.ok_or_else(|| corrupt(subject, "suffix has no state-producing record"))?,
        );
    }
    // Reuse the existing exact row renderer against a compact model holding only changed
    // coordinates. Old state-source metadata is included without cloning the old history, and
    // every record it names is the model's own allocation, shared rather than copied.
    let mut touched = CapturedModel::default();
    for id in &ids {
        touched
            .records
            .insert(id.clone(), model.records[id].clone());
        touched
            .record_physical
            .insert(id.clone(), model.record_physical[id].clone());
        touched
            .record_blob_digests
            .insert(id.clone(), model.record_blob_digests[id].clone());
    }
    for key in &keys {
        touched
            .batches
            .insert(key.clone(), model.batches[key].clone());
        touched
            .batch_blob_digests
            .insert(key.clone(), model.batch_blob_digests[key].clone());
    }
    for subject in starts.keys() {
        let history = &model.histories[subject];
        let mut records = Vec::new();
        if let Some(id) = model.state_records.get(subject) {
            let ModelLookup::Committed(record) = &model.records[id] else {
                return Err(integrity("linear state source is not committed"));
            };
            records.push(Arc::clone(record));
            touched
                .record_blob_digests
                .insert(id.clone(), model.record_blob_digests[id].clone());
        }
        if let Some(last) = history.records.last() {
            if records
                .last()
                .is_none_or(|record| record.entry.record_id() != last.entry.record_id())
            {
                records.push(Arc::clone(last));
            }
            touched.record_physical.insert(
                last.entry.record_id().to_owned(),
                model.record_physical[last.entry.record_id()].clone(),
            );
        }
        touched.histories.insert(
            subject.clone(),
            ModelHistory {
                records,
                ..ModelHistory::genesis(subject.clone())
            },
        );
        touched
            .terminals
            .insert(subject.clone(), model.terminals[subject].clone());
    }
    let expected = expected_projection_rows(authority, &touched)?;
    let specs = projection_specs();
    let mut row_count = model.held.rows;
    if delta.projections.len() != specs.len() || rows.len() != specs.len() {
        return Err(integrity("capture suffix projection set differs"));
    }
    for (index, changes) in delta.projections.into_iter().enumerate() {
        if changes.specification != specs[index] {
            return Err(integrity("capture suffix projection specification differs"));
        }
        let mut seen = BTreeSet::new();
        for change in changes.rows {
            if !seen.insert(change.key.clone())
                || rows[index].get(&change.key) != change.before.as_ref()
            {
                return Err(integrity("capture suffix projection before-value differs"));
            }
            let wanted = expected[index]
                .get(&change.key)
                .or_else(|| rows[index].get(&change.key));
            if wanted != change.after.as_ref() {
                return Err(integrity(
                    "capture suffix projection differs from authoritative events",
                ));
            }
            row_count = match (change.before.is_some(), change.after.is_some()) {
                (false, true) => row_count.checked_add(1),
                (true, false) => row_count.checked_sub(1),
                _ => Some(row_count),
            }
            .ok_or_else(|| integrity("capture suffix row accounting overflow"))?;
            match change.after {
                Some(value) => {
                    rows[index].insert(change.key, value);
                }
                None => {
                    rows[index].remove(&change.key);
                }
            }
        }
        for (key, value) in &expected[index] {
            if rows[index].get(key) != Some(value) {
                return Err(integrity(
                    "capture suffix omitted an authoritative projection row",
                ));
            }
        }
    }
    let usage = delta.resulting_usage;
    if usage.events != model.held.events + delta.events.len() as u64
        || usage.blobs != model.held.blobs + new_blobs.len() as u64
        || usage.projection_rows != row_count
    {
        return Err(integrity(
            "capture suffix usage differs from admitted additions",
        ));
    }
    model.held.events = usage.events;
    model.held.blobs = usage.blobs;
    model.held.rows = usage.projection_rows;
    for digest in &held_by_model {
        blobs.remove(digest);
    }
    Ok(Held {
        checkpoint: Some(checkpoint),
        model: Arc::new(model),
        blobs,
        rows,
        last_position,
    })
}

#[cfg(all(test, feature = "sqlite", feature = "sync-bridge"))]
mod review_tests;

#[cfg(all(test, feature = "sqlite", feature = "sync-bridge"))]
mod tests {
    use super::*;
    use entity_executor::{CreateRequest, ExecuteRequest};
    use entity_store::Recording;
    use eventlog_core::{CaptureUsage, CapturedProjectionDelta, CapturedRowChange};

    const LIMITS: CaptureLimits = CaptureLimits {
        max_events: 128,
        max_blobs: 1024,
        max_projection_rows: 1024,
        max_payload_bytes: 16 * 1024 * 1024,
    };
    fn context(label: &str) -> EventlogOperationContext {
        EventlogOperationContext {
            subject: "tracked-test".into(),
            actor: "entity-eventlog-test".into(),
            request_id: label.into(),
            trace_id: label.into(),
            causation_id: None,
            causation_depth: 0,
            occurred_at: OffsetDateTime::UNIX_EPOCH,
        }
    }
    fn registry() -> Registry {
        let mut registry = Registry::new();
        registry.register(serde_json::from_value(json!({"entity":"ticket","version":1,"schema":{"fields":{"title":{"type":"string","required":true}}},"lifecycle":{"initial":"Open","states":["Open"]},"operations":{"Touch":{"transitions":[{"from":"Open","to":"Open"}],"arguments":{"fields":{}},"emits":[]}}})).unwrap()).unwrap();
        registry
    }
    fn recording(id: &str) -> Recording {
        Recording {
            record_id: id.into(),
            recorded_at: "2026-10-03T00:00:00Z".into(),
            correlation: None,
            causation: None,
            actor: None,
        }
    }
    async fn fixture(
        policy: CapturePolicy,
    ) -> (
        Arc<eventlog_sqlite::SqliteEventStore>,
        EventlogRecordedStore,
        Registry,
    ) {
        let backend = Arc::new(
            eventlog_sqlite::SqliteEventStore::in_memory("tracked_test")
                .await
                .unwrap(),
        );
        let tenant = TenantId::new("tracked-test").unwrap();
        let authority = Authority {
            logical_scope: "tracked-scope".into(),
            tenant: tenant.as_str().into(),
            stream_identity: backend.stream_identity(&tenant).await.unwrap(),
        };
        let projector = Arc::new(crate::ErRecordedProjector::new());
        backend.create_projections(projector.clone()).await.unwrap();
        backend.attach_inline_existing(projector).await.unwrap();
        EventlogBindingProvisioner::new(backend.clone(), LIMITS)
            .provision_binding(authority.clone(), context("binding"))
            .await
            .unwrap();
        let store =
            EventlogRecordedStore::open_with_policy(backend.clone(), authority, LIMITS, policy)
                .await
                .unwrap();
        (backend, store, registry())
    }
    async fn append(store: &EventlogRecordedStore, registry: &Registry, revision: u64) {
        let id = format!("record-{revision}");
        let subject = Subject::new("ticket", "clock").unwrap();
        let action = if revision == 0 {
            BatchAction::Create(CreateRequest {
                subject,
                definition_version: 1,
                fields: json!({"title":"clock"}),
                recording: recording(&id),
            })
        } else {
            BatchAction::Execute(ExecuteRequest {
                subject,
                expected_revision: revision,
                operation: "Touch".into(),
                arguments: json!({}),
                fulfillments: BTreeMap::new(),
                recording: recording(&id),
            })
        };
        store
            .operation(context(&id))
            .execute_batch(registry, BatchKey::Named(id), vec![action])
            .await
            .unwrap();
    }
    fn held(authority: &Authority, capture: TenantCapture) -> Held {
        Held {
            checkpoint: Some(CaptureCheckpoint::new(())),
            model: Arc::new(build_model(authority, &capture).unwrap()),
            last_position: capture.events.last().map_or(0, |e| e.global_seq),
            blobs: capture
                .blobs
                .into_iter()
                .map(|b| (b.digest, b.bytes))
                .collect(),
            rows: capture
                .projections
                .into_iter()
                .map(|p| p.rows.into_iter().collect())
                .collect(),
        }
    }
    fn difference(before: &TenantCapture, after: &TenantCapture) -> TenantCaptureDelta {
        let old: BTreeSet<_> = before.blobs.iter().map(|b| b.digest.as_str()).collect();
        TenantCaptureDelta {
            tenant: after.tenant.clone(),
            stream_identity: after.stream_identity.clone(),
            events: after.events[before.events.len()..].to_vec(),
            blobs: after
                .blobs
                .iter()
                .filter(|b| !old.contains(b.digest.as_str()))
                .cloned()
                .collect(),
            projections: before
                .projections
                .iter()
                .zip(&after.projections)
                .map(|(before, after)| {
                    let old: BTreeMap<_, _> = before.rows.iter().cloned().collect();
                    let new: BTreeMap<_, _> = after.rows.iter().cloned().collect();
                    let keys: BTreeSet<_> = old.keys().chain(new.keys()).cloned().collect();
                    CapturedProjectionDelta {
                        specification: after.specification,
                        rows: keys
                            .into_iter()
                            .filter(|k| old.get(k) != new.get(k))
                            .map(|key| CapturedRowChange {
                                before: old.get(&key).cloned(),
                                after: new.get(&key).cloned(),
                                key,
                            })
                            .collect(),
                    }
                })
                .collect(),
            resulting_usage: CaptureUsage {
                events: after.events.len() as u64,
                blobs: after.blobs.len() as u64,
                projection_rows: projection_rows(after),
                payload_bytes: 0,
            },
        }
    }
    #[test]
    fn tracked_unchanged_reads_and_appends_do_not_recapture_verified_prefixes() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(async {
                let (backend, store, registry) = fixture(CapturePolicy::ProviderTracked).await;
                let before = store.calls();
                let subject = Subject::new("ticket", "clock").unwrap();
                assert_eq!(store.load(&subject).await.unwrap(), None);
                assert_eq!(
                    store.calls().captures,
                    before.captures,
                    "unchanged provider proof must avoid another capture"
                );
                for revision in 0..4 {
                    append(&store, &registry, revision).await;
                    let complete = backend
                        .capture_tenant(&store.tenant, projection_specs(), LIMITS)
                        .await
                        .unwrap();
                    let full = build_model(&store.authority, &complete).unwrap();
                    assert_eq!(
                        store.load(&subject).await.unwrap(),
                        full.terminals.get(&subject).cloned()
                    );
                    assert_eq!(
                        store.history(&subject).await.unwrap(),
                        full.histories[&subject]
                    );
                }
                assert_eq!(
                    store.calls().captures,
                    before.captures,
                    "acknowledged linear suffixes must not reread old content"
                );
                assert_eq!(store.calls().model_advances - before.model_advances, 4);
            });
    }
    #[test]
    fn delta_verification_matches_full_models_and_refuses_missing_or_forged_coordinates() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(async {
                let (backend, store, registry) = fixture(CapturePolicy::FullVerification).await;
                append(&store, &registry, 0).await;
                let before = backend
                    .capture_tenant(&store.tenant, projection_specs(), LIMITS)
                    .await
                    .unwrap();
                append(&store, &registry, 1).await;
                append(&store, &registry, 2).await;
                let after = backend
                    .capture_tenant(&store.tenant, projection_specs(), LIMITS)
                    .await
                    .unwrap();
                let full = build_model(&store.authority, &after).unwrap();
                let advanced = advance(
                    &store.authority,
                    held(&store.authority, before.clone()),
                    difference(&before, &after),
                    CaptureCheckpoint::new(()),
                )
                .unwrap();
                assert_eq!(advanced.model.histories, full.histories);
                assert_eq!(advanced.model.terminals, full.terminals);
                assert_eq!(advanced.model.records, full.records);
                assert_eq!(advanced.model.batches, full.batches);
                for mutation in 0..6 {
                    let mut delta = difference(&before, &after);
                    match mutation {
                        0 => {
                            delta.projections[1].rows.pop();
                        }
                        1 => {
                            delta.projections[2].rows.pop();
                        }
                        2 => {
                            delta.projections[3].rows.pop();
                        }
                        3 => {
                            delta.projections[3].rows[0].before = None;
                        }
                        4 => {
                            delta.events[0].global_seq = before.events.last().unwrap().global_seq;
                        }
                        _ => {
                            delta.projections[3].rows[0].after = Some(json!(["forged"]));
                        }
                    }
                    let result = advance(
                        &store.authority,
                        held(&store.authority, before.clone()),
                        delta,
                        CaptureCheckpoint::new(()),
                    );
                    assert!(
                        matches!(
                            result,
                            Err(AsyncStoreError::ProviderIntegrity { .. })
                                | Err(AsyncStoreError::CorruptHistory { .. })
                        ),
                        "mutation {mutation} must be refused"
                    );
                }
            });
    }

    /// What a tracked handle retains: every bound blob except the record and batch blobs whose
    /// bytes its model holds. Returns how many of those the model holds.
    fn retained_beside(label: &str, store: &EventlogRecordedStore) -> usize {
        let cache = store.tracked.lock().expect("tracked cache");
        let held = cache.held.as_ref().expect("a verified observation");
        let model = &held.model;
        let mut in_model: BTreeSet<&String> = model.batch_blob_digests.values().collect();
        for (record_id, lookup) in &model.records {
            if let ModelLookup::Committed(_) = lookup {
                in_model.insert(&model.record_blob_digests[record_id].record);
            }
        }
        let expected: BTreeSet<&String> = model
            .held
            .digests
            .iter()
            .filter(|digest| !in_model.contains(digest))
            .collect();
        assert_eq!(
            held.blobs.keys().collect::<BTreeSet<_>>(),
            expected,
            "{label}: the handle retains a blob its model holds, or lost one it may read again"
        );
        in_model.len()
    }

    /// A tracked handle keeps one copy of every record and batch blob, the model's (issue 59).
    ///
    /// After advances and after a cold open of the same store, it retains every bound blob except
    /// the ones its model holds. A suffix binding one of those again is still refused as the
    /// replacement of a bound blob: the handle no longer retains the bytes, but it still knows the
    /// digest.
    #[test]
    fn a_tracked_handle_retains_no_blob_its_model_holds_and_refuses_one_bound_again() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(async {
                let (backend, store, registry) = fixture(CapturePolicy::ProviderTracked).await;
                append(&store, &registry, 0).await;
                append(&store, &registry, 1).await;
                store.tracked_model().await.unwrap();
                assert!(store.calls().model_advances > 0, "the appends advanced");
                assert_eq!(retained_beside("advanced", &store), 4);
                let cold = EventlogRecordedStore::open_with_policy(
                    backend.clone(),
                    store.authority.clone(),
                    LIMITS,
                    CapturePolicy::ProviderTracked,
                )
                .await
                .unwrap();
                assert_eq!(retained_beside("cold", &cold), 4);
                let held = cold.tracked.lock().unwrap().held.take().unwrap();
                let ModelLookup::Committed(record) = &held.model.records["record-0"] else {
                    panic!("record-0 is committed")
                };
                let rebound = held.model.record_blob_digests[record.entry.record_id()]
                    .record
                    .clone();
                let delta = TenantCaptureDelta {
                    tenant: cold.tenant.clone(),
                    stream_identity: cold.authority.stream_identity.clone(),
                    events: Vec::new(),
                    blobs: vec![eventlog_core::CapturedBlob {
                        digest: rebound,
                        bytes: b"bound again".to_vec(),
                    }],
                    projections: Vec::new(),
                    resulting_usage: CaptureUsage {
                        events: 0,
                        blobs: 0,
                        projection_rows: 0,
                        payload_bytes: 0,
                    },
                };
                let refused = advance(&cold.authority, held, delta, CaptureCheckpoint::new(()))
                    .err()
                    .expect("a record blob bound again is refused");
                assert!(
                    matches!(
                        &refused,
                        AsyncStoreError::ProviderIntegrity { detail, .. }
                            if detail == "capture suffix replaces a bound blob"
                    ),
                    "a record blob bound again is not refused as a replacement: {refused:?}"
                );
            });
    }

    #[test]
    fn held_reader_keeps_its_old_model_while_a_refresh_rebuilds_safely() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(async {
                let (_, store, registry) = fixture(CapturePolicy::ProviderTracked).await;
                append(&store, &registry, 0).await;
                let subject = Subject::new("ticket", "clock").unwrap();
                let pinned = store.tracked_model().await.unwrap();
                let before = store.calls();
                append(&store, &registry, 1).await;
                assert_eq!(pinned.terminals[&subject].revision, 1);
                assert_eq!(store.load(&subject).await.unwrap().unwrap().revision, 2);
                assert_eq!(store.calls().captures, before.captures + 1);
                drop(pinned);
                let before = store.calls();
                append(&store, &registry, 2).await;
                assert_eq!(store.calls().captures, before.captures);
                assert_eq!(store.calls().model_advances, before.model_advances + 1);
            });
    }

    #[test]
    fn unjournaled_provider_mutation_invalidates_the_model_before_reuse() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(async {
                let (backend, store, registry) = fixture(CapturePolicy::ProviderTracked).await;
                append(&store, &registry, 0).await;
                let subject = Subject::new("ticket", "clock").unwrap();
                let before = store.calls();
                let bytes = b"unreferenced-content";
                let digest = framed_key("er.test-orphan/1", bytes).unwrap();
                backend
                    .put_blob(&store.tenant, &digest, bytes)
                    .await
                    .unwrap();
                assert_eq!(store.load(&subject).await.unwrap().unwrap().revision, 1);
                assert_eq!(store.calls().captures, before.captures + 1);
            });
    }

    #[test]
    fn one_scoped_history_read_preserves_order_duplicates_and_absence() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(async {
                let (_, store, registry) = fixture(CapturePolicy::ProviderTracked).await;
                append(&store, &registry, 0).await;
                let subject = Subject::new("ticket", "clock").unwrap();
                let absent = Subject::new("ticket", "absent").unwrap();
                let before = store.calls();
                let histories = store
                    .scoped_histories(&[subject.clone(), absent.clone(), subject.clone()])
                    .await
                    .unwrap();
                assert_eq!(histories[0], histories[2]);
                assert_eq!(histories[0].records.len(), 1);
                assert_eq!(
                    histories[1],
                    SubjectHistory {
                        subject: absent,
                        origin: HistoryOrigin::Genesis,
                        records: Vec::new()
                    }
                );
                assert_eq!(store.calls().captures, before.captures);
            });
    }
}
