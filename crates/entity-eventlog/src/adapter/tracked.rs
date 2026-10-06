//! Provider-attested reuse of a completely verified observation.
use super::*;
use eventlog_core::{CaptureCheckpoint, TenantCaptureDelta, TenantCaptureUpdate};

/// Integrity boundary for reads through one live provider handle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CapturePolicy {
    /// Obtain fresh content for every complete read, including providers without change tracking.
    #[default]
    FullVerification,
    /// Reuse only provider-attested unchanged content or an exactly verified append suffix.
    ///
    /// Initial open is always verified completely. SQLite tracks SQL writes through every
    /// connection; raw file edits bypassing SQLite are outside this live-handle guarantee.
    ProviderTracked,
}

impl CapturePolicy {
    /// The complete transient policy domain, in specification declaration order.
    pub const ALL: [Self; 2] = [Self::FullVerification, Self::ProviderTracked];
}

#[derive(Default)]
pub(super) struct Cache {
    generation: u64,
    held: Option<Held>,
}

struct Held {
    checkpoint: Option<CaptureCheckpoint>,
    model: Arc<CapturedModel>,
    /// Bound blobs an advance may read again: wrappers, requests and the rest. Not the record and
    /// batch blobs of committed records, whose bytes the model holds; which digests are bound at
    /// all is `model.held.digests`.
    blobs: BTreeMap<String, Vec<u8>>,
    rows: Vec<BTreeMap<String, Value>>,
    last_position: u64,
}

impl EventlogRecordedStore {
    pub(super) async fn tracked_model(&self) -> Result<Arc<CapturedModel>, AsyncStoreError> {
        loop {
            let (generation, checkpoint) = {
                let cache = self
                    .tracked
                    .lock()
                    .map_err(|_| integrity("tracked cache poisoned"))?;
                (
                    cache.generation,
                    cache.held.as_ref().and_then(|h| h.checkpoint.clone()),
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
                        mut capture,
                        checkpoint,
                    } => {
                        self.native_captures.fetch_add(1, Ordering::Relaxed);
                        if capture.tenant != self.tenant
                            || capture.stream_identity != self.authority.stream_identity
                        {
                            return Err(integrity("native capture substituted tenant generation"));
                        }
                        // The model takes the record and batch blobs; what this handle keeps of
                        // the capture is every other blob, which is all an advance reads again.
                        let model = build_model_owning(&self.authority, &mut capture)?;
                        if model.binding.is_none() {
                            return Err(integrity("the tenant has no authoritative binding"));
                        }
                        self.model_builds.fetch_add(1, Ordering::Relaxed);
                        self.records_decoded
                            .fetch_add(model.decoded, Ordering::Relaxed);
                        *self
                            .held
                            .lock()
                            .map_err(|_| integrity("read bound lock poisoned"))? =
                            model.held.clone();
                        Some(Held {
                            checkpoint,
                            model: Arc::new(model),
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
                        })
                    }
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
        let read = self.scoped_model(&ReadScope::subject(subject)).await?;
        model_state(&read.model, subject)
    }

    #[cfg(feature = "sync-bridge")]
    pub(crate) async fn scoped_record(
        &self,
        id: &str,
    ) -> Result<Option<RecordLookup>, AsyncStoreError> {
        let read = self.scoped_model(&ReadScope::record(id)).await?;
        Ok(read.model.records.get(id).map(ModelLookup::to_public))
    }

    #[cfg(feature = "sync-bridge")]
    pub(crate) async fn scoped_batch(
        &self,
        key: &BatchKey,
    ) -> Result<Option<StoredBatch>, AsyncStoreError> {
        let read = self.scoped_model(&ReadScope::batch(key)).await?;
        Ok(read.model.batches.get(key).map(ModelBatch::to_public))
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
