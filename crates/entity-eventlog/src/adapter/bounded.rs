//! A `ProviderTracked` handle opened from a persisted checkpoint, which holds no whole model.
//!
//! What the open no longer does, and where each part of it happens instead
//! (`docs/design/recorded-open-checkpoint-v0.1.md` § *No silent unverified read*):
//!
//! - **every event, blob and index row held to the events**: by the last complete verification,
//!   in the process that wrote the checkpoint, and by every suffix verified since. The provider's
//!   durable continuity proof is what says nothing else wrote the tenant's captured material
//!   since; an edit through any SQL connection ends it, and the open is then complete.
//! - **the suffix after the checkpoint**: verified here, from the rows it changes and the blobs
//!   it binds, by [`EventlogRecordedStore::verify_suffix`].
//! - **the binding and the stream identity**: the binding row is read and held to the checkpoint
//!   on every bounded open; the provider holds a restored checkpoint to the stored identity.
//! - **each answer**: a state, record or batch is built from the row the verifications above
//!   held, and every blob it reads is held to its digest on the way. The row is read after the
//!   provider's continuity answer, so the answer is served only if the provider, asked again
//!   afterwards, still answers that nothing changed (`tracked.rs`, `tracked_point`). A history is
//!   read per entity, verified as a `FullVerification` handle verifies it and confirmed the same
//!   way; a complete read is a complete verification. A raw edit of an index row that bypasses
//!   SQLite is not caught here: only a `FullVerification` open or a complete read sees it.

use eventlog_core::{
    CaptureUsage, CapturedRowChange, DurableCaptureCheckpoint, Read, ReadResult,
    TenantCaptureDelta, TenantCaptureUpdate,
};
use serde::de::DeserializeOwned;

use super::checkpoint::Record;
use super::tracked::{Bounded, OpenVerification};
use super::*;
use crate::encoding::{BatchKeyWire, SubjectWire};

/// What a verified suffix advanced a handle to.
pub(super) struct Suffix {
    pub(super) last_position: u64,
    pub(super) usage: CaptureUsage,
    pub(super) events: u64,
    decoded: usize,
}

/// What a bounded open settled on.
pub(super) enum Opened {
    /// The handle starts from the checkpoint, with no model.
    Bounded(OpenVerification),
    /// The provider answered with complete content, which was verified whole.
    Whole(Arc<CapturedModel>),
    /// The checkpoint cannot be continued; the open verifies completely instead.
    Fallback,
}

/// What a subject row says, with the state its state source holds.
struct SubjectRow {
    origin: Value,
    state_source: Value,
    head: PhysicalRef,
    state: EntityInstance,
}

fn row_error(error: EventLogError) -> AsyncStoreError {
    integrity(format!("a verified index row is malformed: {error}"))
}

fn field<T: DeserializeOwned>(
    body: &serde_json::Map<String, Value>,
    name: &str,
) -> Result<T, AsyncStoreError> {
    body.get(name)
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .ok_or_else(|| integrity(format!("a verified index row has no valid {name}")))
}

fn text<'a>(
    body: &'a serde_json::Map<String, Value>,
    name: &str,
) -> Result<&'a str, AsyncStoreError> {
    body.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| integrity(format!("a verified index row has no valid {name}")))
}

impl EventlogRecordedStore {
    /// Holds the provider's usage as this handle's totals, knowing none of its bound digests.
    fn hold_usage(&self, usage: CaptureUsage) -> Result<(), AsyncStoreError> {
        *self
            .held
            .lock()
            .map_err(|_| integrity("read bound lock poisoned"))? = CaptureHeld {
            events: usage.events,
            blobs: usage.blobs,
            rows: usage.projection_rows,
            digests: BTreeSet::new(),
            partial: true,
        };
        Ok(())
    }

    /// Accepts a verified suffix: the provider's resulting usage replaces what this handle
    /// counted, and the blobs the suffix bound are known to be bound.
    pub(super) fn install_suffix(
        &self,
        delta: &TenantCaptureDelta,
        suffix: &Suffix,
    ) -> Result<(), AsyncStoreError> {
        let mut held = self
            .held
            .lock()
            .map_err(|_| integrity("read bound lock poisoned"))?;
        held.events = suffix.usage.events;
        held.blobs = suffix.usage.blobs;
        held.rows = suffix.usage.projection_rows;
        held.digests
            .extend(delta.blobs.iter().map(|blob| blob.digest.clone()));
        self.model_advances.fetch_add(1, Ordering::Relaxed);
        self.records_decoded
            .fetch_add(suffix.decoded, Ordering::Relaxed);
        Ok(())
    }

    /// Opens from a valid checkpoint: the binding row, the provider's continuation from the
    /// restored checkpoint, and the suffix after it are verified; nothing before it is read.
    pub(super) async fn open_from(&self, record: &Record) -> Result<Opened, AsyncStoreError> {
        if self.binding_ref().await? != Some(record.binding.clone()) {
            return Ok(Opened::Fallback);
        }
        let durable = DurableCaptureCheckpoint::from_bytes(record.provider.clone());
        let Some(restored) = self.backend.restore_checkpoint(&durable) else {
            return Ok(Opened::Fallback);
        };
        let Some(base) = self.backend.checkpoint_usage(&restored) else {
            return Ok(Opened::Fallback);
        };
        let update = self
            .backend
            .capture_tenant_since(
                &self.tenant,
                projection_specs(),
                self.limits,
                Some(&restored),
            )
            .await
            .map_err(map_capture)?;
        match update {
            TenantCaptureUpdate::Unchanged { checkpoint } => {
                let usage = self.backend.checkpoint_usage(&checkpoint).unwrap_or(base);
                self.hold_usage(usage)?;
                self.install_bounded(Bounded {
                    checkpoint,
                    last_position: record.position,
                    binding: record.binding.clone(),
                    usage,
                })?;
                Ok(Opened::Bounded(OpenVerification::Checkpoint))
            }
            TenantCaptureUpdate::AppendDelta { checkpoint, delta } => {
                let Ok(suffix) = self.verify_suffix(base, record.position, &delta).await else {
                    // The complete verification decides, and words any refusal.
                    return Ok(Opened::Fallback);
                };
                self.hold_usage(base)?;
                self.install_suffix(&delta, &suffix)?;
                self.install_bounded(Bounded {
                    checkpoint,
                    last_position: suffix.last_position,
                    binding: record.binding.clone(),
                    usage: suffix.usage,
                })?;
                Ok(Opened::Bounded(OpenVerification::Suffix {
                    events: suffix.events,
                }))
            }
            TenantCaptureUpdate::Complete {
                capture,
                checkpoint,
            } => {
                let held = self.verify_complete(capture, checkpoint, 0)?;
                let model = Arc::clone(&held.model);
                self.install_held(held)?;
                Ok(Opened::Whole(model))
            }
        }
    }

    /// The binding event the tenant's binding row names, when it names this authority.
    async fn binding_ref(&self) -> Result<Option<PhysicalRef>, AsyncStoreError> {
        let Some(row) = self
            .backend
            .projection_get(binding_spec(), &self.tenant, "singleton")
            .await
            .map_err(map_read_error)?
        else {
            return Ok(None);
        };
        let Ok(body) = tagged_body(&row, "er.eventlog.binding-index/1") else {
            return Ok(None);
        };
        let authority: Option<Authority> = body
            .get("authority")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok());
        if authority.as_ref() != Some(&self.authority) {
            return Ok(None);
        }
        Ok(body
            .get("physical")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok()))
    }

    /// The bytes bound under `digest`, held to it in `domain`.
    async fn bound_blob(&self, digest: &str, domain: &str) -> Result<Vec<u8>, AsyncStoreError> {
        let bytes = self
            .backend
            .get_blob(&self.tenant, digest)
            .await
            .map_err(map_read_error)?
            .ok_or_else(|| {
                integrity("a verified index row names a blob the provider does not hold")
            })?;
        verify_digest(domain, digest, &bytes)?;
        Ok(bytes)
    }

    /// One index row, read from the provider.
    async fn index_row(
        &self,
        spec: &eventlog_core::ProjectionSpec,
        key: &str,
    ) -> Result<Option<Value>, AsyncStoreError> {
        self.backend
            .projection_get(spec, &self.tenant, key)
            .await
            .map_err(map_read_error)
    }

    /// What one subject row says, with the state its state source holds, read and held to its
    /// digest.
    async fn subject_row(
        &self,
        row: &Value,
        subject: &Subject,
    ) -> Result<SubjectRow, AsyncStoreError> {
        let body = tagged_body(row, "er.eventlog.subject-index/1").map_err(row_error)?;
        let authority: Authority = field(body, "authority")?;
        let named: SubjectWire = field(body, "subject")?;
        if authority != self.authority || Subject::from(named) != *subject {
            return Err(integrity(
                "a subject row names another authority or subject",
            ));
        }
        let revision: u64 = field(body, "revision")?;
        let head: PhysicalRef = field(body, "physical_head")?;
        let origin = body
            .get("origin")
            .cloned()
            .ok_or_else(|| integrity("a subject row has no origin"))?;
        let state_source = body
            .get("state_source")
            .cloned()
            .ok_or_else(|| integrity("a subject row has no state source"))?;
        let source = state_source
            .as_object()
            .ok_or_else(|| integrity("a subject row's state source is malformed"))?;
        let state = match source.get("kind").and_then(Value::as_str) {
            Some("decision") => {
                let bytes = self
                    .bound_blob(text(source, "record_blob")?, RECORD_BLOB_DOMAIN)
                    .await?;
                match decode_record(&bytes)? {
                    entity_store::asynchronous::RecordedEntry::Decision(commit) => commit.instance,
                    entity_store::asynchronous::RecordedEntry::Observation(_) => {
                        return Err(integrity("a subject's state source is not a decision"));
                    }
                }
            }
            Some("anchor") => {
                let bytes = self
                    .bound_blob(text(source, "anchor_blob")?, ANCHOR_BLOB_DOMAIN)
                    .await?;
                let anchor = decode_anchor(&bytes)?;
                require_authority(&self.authority, &anchor.authority)?;
                if Subject::from(anchor.subject) != *subject {
                    return Err(integrity("a subject's anchor names another subject"));
                }
                anchor.instance
            }
            _ => return Err(integrity("a subject row has an unknown state source")),
        };
        if state.entity != subject.entity || state.id != subject.id || state.revision != revision {
            return Err(integrity(
                "a subject row's state source is not the state it names",
            ));
        }
        Ok(SubjectRow {
            origin,
            state_source,
            head,
            state,
        })
    }

    /// A subject's state, from its row and the record or anchor blob its state comes from.
    pub(super) async fn rows_state(
        &self,
        subject: &Subject,
    ) -> Result<Option<EntityInstance>, AsyncStoreError> {
        subject.validate()?;
        let key = subject_key(&self.authority, subject)?;
        let Some(row) = self.index_row(subject_spec(), &key).await? else {
            return Ok(None);
        };
        Ok(Some(self.subject_row(&row, subject).await?.state))
    }

    /// One committed member as a stored record, from the batch blob that holds it and the row
    /// coordinates that place it. The row's record and request digests must be the digests of
    /// exactly the member's canonical bytes, so the record blob itself need not be read.
    fn stored_member(
        &self,
        key: &BatchKey,
        member_index: u64,
        member: AppendMember,
        record_blob: &str,
        request_blob: &str,
        physical: &PhysicalRef,
    ) -> Result<StoredRecord, AsyncStoreError> {
        let subject = member.entry.subject();
        let record_bytes = record_comparison_bytes(&member.entry)?;
        if framed_key(RECORD_BLOB_DOMAIN, &record_bytes)? != record_blob
            || framed_key(REQUEST_BLOB_DOMAIN, &member.request_bytes)? != request_blob
            || physical.stream_id != subject_stream_id(&self.authority, &subject)?
        {
            return Err(integrity(
                "a verified index row and the batch blob it names disagree",
            ));
        }
        let position = RecordPosition {
            subject: physical.stream_version,
            store: physical.global_seq,
        };
        let receipt = RecordReceipt {
            record_id: member.entry.record_id().to_owned(),
            subject,
            kind: member.entry.kind(),
            revision: member.entry.revision(),
            position,
            batch_key: key.clone(),
            member_index,
        };
        Ok(StoredRecord {
            entry: member.entry,
            position,
            receipt,
            expect: member.expect,
            request_bytes: member.request_bytes,
            lineage: None,
            record_bytes,
        })
    }

    /// The batch blob `digest` names, held to it and decoded, refusing one of another key.
    async fn batch_members(
        &self,
        digest: &str,
        key: &BatchKey,
    ) -> Result<(Vec<u8>, Vec<AppendMember>), AsyncStoreError> {
        let bytes = self.bound_blob(digest, BATCH_BLOB_DOMAIN).await?;
        let (decoded, members) = decode_batch(&bytes)?;
        if decoded != *key {
            return Err(integrity("a verified index row names another batch's blob"));
        }
        Ok((bytes, members))
    }

    /// A record lookup, from its row and its batch's blob. An imported record is read per entity.
    pub(super) async fn rows_record(
        &self,
        id: &str,
    ) -> Result<Option<RecordLookup>, AsyncStoreError> {
        let key = record_key(&self.authority, id)?;
        let Some(row) = self.index_row(record_spec(), &key).await? else {
            return Ok(None);
        };
        let body = tagged_body(&row, "er.eventlog.record-index/1").map_err(row_error)?;
        let entry = body
            .get("entry")
            .and_then(Value::as_object)
            .ok_or_else(|| integrity("a record row has no entry"))?;
        match entry.get("kind").and_then(Value::as_str) {
            Some("committed") => {}
            Some("imported") => {
                let read = self.scoped_model(&ReadScope::record(id)).await?;
                return Ok(read.model.records.get(id).map(ModelLookup::to_public));
            }
            _ => return Err(integrity("a record row has an unknown kind")),
        }
        let authority: Authority = field(entry, "authority")?;
        let named: SubjectWire = field(entry, "subject")?;
        let batch_key: BatchKey = field::<BatchKeyWire>(entry, "batch_key")?.into();
        let member_index: u64 = field(entry, "member_index")?;
        let revision: u64 = field(entry, "revision")?;
        let physical: PhysicalRef = field(entry, "physical")?;
        if authority != self.authority || text(entry, "record_id")? != id {
            return Err(integrity("a record row names another authority or record"));
        }
        let (_, mut members) = self
            .batch_members(text(entry, "batch_blob")?, &batch_key)
            .await?;
        let index = usize::try_from(member_index)
            .ok()
            .filter(|index| *index < members.len())
            .ok_or_else(|| integrity("a record row's member index exceeds its batch"))?;
        let member = members.swap_remove(index);
        if member.entry.record_id() != id
            || member.entry.subject() != Subject::from(named)
            || member.entry.revision() != revision
        {
            return Err(integrity("a record row and its batch member disagree"));
        }
        Ok(Some(RecordLookup::Committed(self.stored_member(
            &batch_key,
            member_index,
            member,
            text(entry, "record_blob")?,
            text(entry, "request_blob")?,
            &physical,
        )?)))
    }

    /// A batch lookup, from its row and its blob.
    pub(super) async fn rows_batch(
        &self,
        key: &BatchKey,
    ) -> Result<Option<StoredBatch>, AsyncStoreError> {
        key.validate()?;
        let row_key = physical_batch_key(&self.authority, key)?;
        let Some(row) = self.index_row(batch_spec(), &row_key).await? else {
            return Ok(None);
        };
        let body = tagged_body(&row, "er.eventlog.batch-index/1").map_err(row_error)?;
        let authority: Authority = field(body, "authority")?;
        let named: BatchKey = field::<BatchKeyWire>(body, "batch_key")?.into();
        if authority != self.authority || named != *key {
            return Err(integrity("a batch row names another authority or key"));
        }
        let rows = body
            .get("members")
            .and_then(Value::as_array)
            .ok_or_else(|| integrity("a batch row has no members"))?;
        let (comparison_bytes, members) =
            self.batch_members(text(body, "batch_blob")?, key).await?;
        if rows.len() != members.len() || members.is_empty() {
            return Err(integrity("a batch row and its blob have different members"));
        }
        let mut records = Vec::with_capacity(members.len());
        let mut prior_position = 0;
        for (index, (member, row)) in members.into_iter().zip(rows).enumerate() {
            let row = row
                .as_object()
                .ok_or_else(|| integrity("a batch row member is malformed"))?;
            let member_index =
                u64::try_from(index).map_err(|_| AsyncStoreError::PositionExhausted {
                    domain: "batch member".into(),
                })?;
            let physical: PhysicalRef = field(row, "physical")?;
            if field::<u64>(row, "member_index")? != member_index
                || text(row, "record_id")? != member.entry.record_id()
                || physical.global_seq <= prior_position
            {
                return Err(integrity("a batch row member and its blob disagree"));
            }
            prior_position = physical.global_seq;
            records.push(self.stored_member(
                key,
                member_index,
                member,
                text(row, "record_blob")?,
                text(row, "request_blob")?,
                &physical,
            )?);
        }
        let receipt = match key {
            BatchKey::SingleRecord(_) => CommitReceipt::Single(records[0].receipt.clone()),
            BatchKey::Named(_) => CommitReceipt::Batch(BatchReceipt {
                key: key.clone(),
                members: records
                    .iter()
                    .map(|record| record.receipt.clone())
                    .collect(),
            }),
        };
        Ok(Some(StoredBatch {
            key: key.clone(),
            records,
            comparison_bytes,
            receipt,
        }))
    }

    /// Verifies a provider's append suffix without a whole model.
    ///
    /// The prior state of each subject the suffix touches is the state its subject row names
    /// before the suffix, as the provider journaled it; the record ids and batch keys it adds must
    /// have had no row. Each new record is then held as a whole-model advance holds it: every
    /// wrapper, record, request and batch blob to its digest and to each other, positions after
    /// the subject's physical head, the record against the prior state, its receipt, and every row
    /// the suffix changes equal to the rendering of the new records, with none omitted and none
    /// the records do not explain. The usage the provider reports must be the prior usage plus
    /// exactly what the suffix added.
    ///
    /// A suffix holding anything but recorded entries, or touching a subject whose history was
    /// imported, is refused here: the caller verifies completely instead.
    pub(super) async fn verify_suffix(
        &self,
        base: CaptureUsage,
        from: u64,
        delta: &TenantCaptureDelta,
    ) -> Result<Suffix, AsyncStoreError> {
        if delta.tenant != self.tenant
            || delta.stream_identity != self.authority.stream_identity
            || delta
                .events
                .iter()
                .any(|event| event.name != "er.recorded_entry" || event.digest.is_some())
        {
            return Err(integrity("capture suffix requires complete verification"));
        }
        let specs = projection_specs();
        if delta.projections.len() != specs.len()
            || delta
                .projections
                .iter()
                .zip(specs)
                .any(|(changes, spec)| changes.specification != *spec)
        {
            return Err(integrity("capture suffix projection set differs"));
        }
        let mut last_position = from;
        for event in &delta.events {
            if event.global_seq <= last_position {
                return Err(integrity("capture suffix does not advance tenant position"));
            }
            last_position = event.global_seq;
        }
        let mut blobs: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for blob in &delta.blobs {
            if blobs
                .insert(blob.digest.clone(), blob.bytes.clone())
                .is_some()
            {
                return Err(integrity("capture suffix replaces a bound blob"));
            }
        }
        let added_blobs = blobs.len() as u64;
        // What each wrapper names. A blob the suffix does not bind was bound before it, so it is
        // read from the provider; every one is held to its digest when it is admitted below.
        let mut wanted = BTreeSet::new();
        for event in &delta.events {
            let digest = reference_digest(event)?;
            wanted.insert(digest.to_owned());
            if !blobs.contains_key(digest) {
                self.fetch(&mut blobs, std::slice::from_ref(&digest.to_owned()))
                    .await?;
            }
            let wrapper = decode_entry(&blobs[digest])?;
            wanted.extend([
                wrapper.record_blob,
                wrapper.request_blob,
                wrapper.batch_blob,
            ]);
        }
        let missing: Vec<String> = wanted
            .iter()
            .filter(|digest| !blobs.contains_key(*digest))
            .cloned()
            .collect();
        self.fetch(&mut blobs, &missing).await?;
        let borrowed: BTreeMap<&str, &[u8]> = blobs
            .iter()
            .map(|(digest, bytes)| (digest.as_str(), bytes.as_slice()))
            .collect();
        let mut bound = BoundBlobs::new(&borrowed);
        let mut compact = CapturedModel::default();
        let mut pending = Vec::new();
        for event in &delta.events {
            admit_event(
                &self.authority,
                event,
                &mut bound,
                &mut compact,
                &mut pending,
            )?;
        }
        insert_committed(pending, &mut bound, &mut compact)?;
        let histories = std::mem::take(&mut compact.histories);
        // Record and batch rows of exactly the new records; no binding row and no subject row.
        let rendered = expected_projection_rows(&self.authority, &compact)?;
        let changes: Vec<BTreeMap<&str, &CapturedRowChange>> = delta
            .projections
            .iter()
            .map(|projection| {
                let mut keyed = BTreeMap::new();
                for change in &projection.rows {
                    if keyed.insert(change.key.as_str(), change).is_some() {
                        return Err(integrity("capture suffix projection before-value differs"));
                    }
                }
                Ok(keyed)
            })
            .collect::<Result<_, _>>()?;
        let mut subjects = BTreeMap::new();
        for (subject, history) in histories {
            let key = subject_key(&self.authority, &subject)?;
            let change = changes[3].get(key.as_str()).ok_or_else(|| {
                integrity("capture suffix omitted an authoritative projection row")
            })?;
            let prior = match &change.before {
                Some(row) => Some(self.subject_row(row, &subject).await?),
                None => None,
            };
            if prior
                .as_ref()
                .is_some_and(|prior| prior.origin != json!({"kind":"genesis"}))
            {
                return Err(integrity(
                    "nonlinear or imported suffix needs complete verification",
                ));
            }
            let mut records = history.records;
            records.sort_by_key(|record| record.position.store);
            let mut previous = prior.as_ref().map(|prior| RecordPosition {
                subject: prior.head.stream_version,
                store: prior.head.global_seq,
            });
            let mut state = prior.as_ref().map(|prior| prior.state.clone());
            for record in &records {
                if previous.is_some_and(|p| {
                    p.store >= record.position.store || p.subject >= record.position.subject
                }) {
                    return Err(corrupt(
                        &subject,
                        "suffix positions do not follow verified history",
                    ));
                }
                record.receipt.batch_key.validate().map_err(|error| {
                    corrupt(
                        &subject,
                        format!("record receipt carries an invalid key: {error}"),
                    )
                })?;
                if let BatchKey::SingleRecord(id) = &record.receipt.batch_key
                    && (id != &record.receipt.record_id || record.receipt.member_index != 0)
                {
                    return Err(corrupt(
                        &subject,
                        "single-record receipt does not reproduce its record identity and zero index",
                    ));
                }
                state = validate_entry_against_state(&record.entry, record.expect, state.as_ref())?;
                previous = Some(record.position);
            }
            let terminal =
                state.ok_or_else(|| corrupt(&subject, "suffix has no state-producing record"))?;
            let state_source = match records.iter().rev().find(|record| {
                matches!(
                    record.entry,
                    entity_store::asynchronous::RecordedEntry::Decision(_)
                )
            }) {
                Some(decision) => json!({
                    "kind": "decision",
                    "record_blob": compact
                        .record_blob_digests
                        .get(decision.entry.record_id())
                        .ok_or_else(|| integrity("suffix record has no admitted record blob"))?
                        .record,
                }),
                None => prior
                    .as_ref()
                    .map(|prior| prior.state_source.clone())
                    .ok_or_else(|| corrupt(&subject, "suffix has no state-producing record"))?,
            };
            let last = records
                .last()
                .ok_or_else(|| integrity("suffix subject has no record"))?;
            let head = compact
                .record_physical
                .get(last.entry.record_id())
                .ok_or_else(|| integrity("suffix record has no physical reference"))?;
            subjects.insert(
                key,
                json!(["er.eventlog.subject-index/1", {
                    "authority": self.authority, "subject": SubjectWire::from(&subject),
                    "origin": prior.map_or_else(|| json!({"kind":"genesis"}), |prior| prior.origin),
                    "revision": terminal.revision, "state_source": state_source,
                    "physical_head": head,
                }]),
            );
        }
        let no_rows = BTreeMap::new();
        let expected = [&no_rows, &rendered[1], &rendered[2], &subjects];
        let mut rows = base.projection_rows;
        for (index, keyed) in changes.iter().enumerate() {
            for (key, change) in keyed {
                let wanted = expected[index].get(*key).ok_or_else(|| {
                    integrity("capture suffix changes a row its events do not explain")
                })?;
                if change.after.as_ref() != Some(wanted) {
                    return Err(integrity(
                        "capture suffix projection differs from authoritative events",
                    ));
                }
                if change.before.is_some() {
                    if index != 3 {
                        // A record id or batch key the suffix adds was already used.
                        return Err(integrity("capture suffix projection before-value differs"));
                    }
                } else {
                    rows = rows
                        .checked_add(1)
                        .ok_or_else(|| integrity("capture suffix row accounting overflow"))?;
                }
            }
            if expected[index]
                .keys()
                .any(|key| !keyed.contains_key(key.as_str()))
            {
                return Err(integrity(
                    "capture suffix omitted an authoritative projection row",
                ));
            }
        }
        let events = delta.events.len() as u64;
        let usage = delta.resulting_usage;
        if Some(usage.events) != base.events.checked_add(events)
            || Some(usage.blobs) != base.blobs.checked_add(added_blobs)
            || usage.projection_rows != rows
        {
            return Err(integrity(
                "capture suffix usage differs from admitted additions",
            ));
        }
        Ok(Suffix {
            last_position,
            usage,
            events,
            decoded: compact.decoded,
        })
    }

    /// Reads blobs bound before a suffix from the provider, refusing one it does not hold.
    async fn fetch(
        &self,
        blobs: &mut BTreeMap<String, Vec<u8>>,
        digests: &[String],
    ) -> Result<(), AsyncStoreError> {
        if digests.is_empty() {
            return Ok(());
        }
        let reads: Vec<Read> = digests
            .iter()
            .map(|digest| Read::Blob {
                tenant: self.tenant.clone(),
                digest: digest.clone(),
            })
            .collect();
        let results = self
            .backend
            .read_many(&reads)
            .await
            .map_err(map_read_error)?;
        if results.len() != reads.len() {
            return Err(integrity(
                "provider answered a read batch with another number of results",
            ));
        }
        for (digest, result) in digests.iter().zip(results) {
            let ReadResult::Blob(Some(bytes)) = result else {
                return Err(integrity("capture suffix references a missing blob"));
            };
            blobs.insert(digest.clone(), bytes);
        }
        Ok(())
    }
}

#[cfg(all(test, feature = "sqlite", feature = "sync-bridge"))]
mod tests;
