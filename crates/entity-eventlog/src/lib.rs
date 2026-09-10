//! Complete entity history over caller-selected Eventlog storage.
//!
//! Each record reserves its global identity and appends its subject history in one atomic group.
//! The resulting instance is derived by replay; observations consume physical log positions but
//! never entity revisions. Existing ER providers and their persisted layouts are unaffected.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use entity_core::{DecisionRecord, DomainEvent, EntityInstance};
use entity_store::{
    Envelope, Expect, RecordedCommit, RecordedObservation, StoreError,
    asynchronous::{
        AsyncAtomicRecordedStore, AsyncRecordedStore, AtomicRecordedCommit, StoreFuture,
    },
};
use eventlog_core::{
    AppendGroup, AtomicEventStore, CommandMeta, EventLogError, Expected, NewEvent, RecordedEvent,
    StreamAppend, StreamId, TenantId, request_hash,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

mod wire;
use wire::{IdentityClaim, Payload, StoredRecord, Subject, decode, invalid};

/// A tenant and namespace scoped ER store over a caller-owned Eventlog provider.
///
/// Record ids are global within this exact tenant and namespace. Independent namespaces share
/// no ER identity. The host supplies opaque Eventlog subject/actor attribution; original ER
/// recording metadata is preserved unchanged inside each record body.
pub struct EventlogStore<S: ?Sized> {
    store: Arc<S>,
    tenant: TenantId,
    history_type: String,
    identity_type: String,
    subject: String,
    actor: String,
    cache: BTreeMap<(String, String), (Option<eventlog_core::SnapshotGeneration>, Subject)>,
    cache_subjects: usize,
    cache_bytes: usize,
}

impl<S: AtomicEventStore + ?Sized> EventlogStore<S> {
    /// Selects storage and authority without opening a path, choosing credentials or reading time.
    ///
    /// # Errors
    /// Blank namespaces or invalid opaque Eventlog attribution identifiers.
    pub fn new(
        store: Arc<S>,
        tenant: TenantId,
        namespace: &str,
        subject: &str,
        actor: &str,
    ) -> Result<Self, StoreError> {
        if namespace.trim().is_empty() {
            return Err(invalid("namespace must not be empty"));
        }
        eventlog_core::validate_identity("subject", subject).map_err(backend)?;
        eventlog_core::validate_identity("actor", actor).map_err(backend)?;
        let namespace = digest(&namespace)?;
        Ok(Self {
            store,
            tenant,
            history_type: format!("er-history-v1-{namespace}"),
            identity_type: format!("er-record-v1-{namespace}"),
            subject: subject.into(),
            actor: actor.into(),
            cache: BTreeMap::new(),
            cache_subjects: 64,
            cache_bytes: 16 * 1024 * 1024,
        })
    }

    /// Bounds retained verified prefixes by subject count and total encoded record-body bytes.
    ///
    /// Defaults are 64 subjects and 16 MiB of encoded bodies. Either zero disables retention.
    /// This is a cache budget, not an exact heap-size limit: runtime structures add overhead and
    /// caller-retained proof handles live independently. Oversized histories remain readable but
    /// are not retained. Changing the budget discards cached prefixes, never durable history.
    #[must_use]
    pub fn with_cache_budget(mut self, subjects: usize, encoded_bytes: usize) -> Self {
        self.cache.clear();
        self.cache_subjects = subjects;
        self.cache_bytes = encoded_bytes;
        self
    }

    fn history_stream(&self, entity: &str, id: &str) -> Result<StreamId, StoreError> {
        StreamId::new(
            self.tenant.clone(),
            self.history_type.clone(),
            digest(&(entity, id))?,
        )
        .map_err(backend)
    }

    fn identity_stream(&self, record_id: &str) -> Result<StreamId, StoreError> {
        StreamId::new(
            self.tenant.clone(),
            self.identity_type.clone(),
            digest(&record_id)?,
        )
        .map_err(backend)
    }

    async fn read_subject(&mut self, entity: &str, id: &str) -> Result<Subject, StoreError> {
        let stream = self.history_stream(entity, id)?;
        let generation = self
            .store
            .snapshot_generation(&stream)
            .await
            .map_err(backend)?;
        let head = self
            .store
            .stream_version(&stream)
            .await
            .map_err(backend)?
            .unwrap_or(0);
        let key = (entity.to_owned(), id.to_owned());
        let mut subject = match self.cache.remove(&key) {
            Some((previous, cached)) if generation.is_some() && previous == generation => {
                if cached.head > head {
                    return Err(invalid("history shrank without changing its generation"));
                }
                cached
            }
            _ => Subject::new(entity, id),
        };
        while subject.head < head {
            let previous_head = subject.head;
            let page = self
                .store
                .read_stream(&stream, subject.head, eventlog_core::MAX_READ_LIMIT)
                .await
                .map_err(backend)?;
            if page.events.is_empty() {
                return Err(invalid("history ended before its observed head"));
            }
            for event in page.events {
                if event.version > head {
                    break;
                }
                if event.stream().map_err(backend)? != stream || event.version != subject.head + 1 {
                    return Err(invalid(
                        "history has a wrong stream or noncontiguous position",
                    ));
                }
                let record = decode(&event)?;
                if record.payload.subject() != (entity, id) {
                    return Err(invalid("record belongs to another subject"));
                }
                let claim = self
                    .identity_claim(&record.payload)
                    .await?
                    .ok_or_else(|| invalid("record has no atomic identity claim"))?;
                if claim.stream != stream || claim.version != event.version {
                    return Err(invalid("identity claim points to another record position"));
                }
                subject.fold(record.payload)?;
                subject.encoded_bytes = subject
                    .encoded_bytes
                    .saturating_add(serde_json::to_vec(&event.data).map_err(invalid)?.len());
                subject.head = event.version;
            }
            if subject.head == previous_head {
                return Err(invalid("history reader did not advance"));
            }
        }
        if self
            .store
            .snapshot_generation(&stream)
            .await
            .map_err(backend)?
            != generation
        {
            return Err(invalid("history generation changed while reading"));
        }
        if generation.is_some()
            && self.cache_subjects > 0
            && self.cache_bytes > 0
            && subject.encoded_bytes <= self.cache_bytes
        {
            while self.cache.len() >= self.cache_subjects
                || self
                    .cache
                    .values()
                    .fold(subject.encoded_bytes, |total, (_, entry)| {
                        total.saturating_add(entry.encoded_bytes)
                    })
                    > self.cache_bytes
            {
                self.cache.pop_first();
            }
            self.cache.insert(key, (generation, subject.clone()));
        }
        Ok(subject)
    }

    // Identity streams are transactional reservations, not a second copy of the record body.
    async fn identity_claim(&self, payload: &Payload) -> Result<Option<IdentityClaim>, StoreError> {
        let stream = self.identity_stream(payload.record_id())?;
        let slice = self
            .store
            .read_stream(&stream, 0, 2)
            .await
            .map_err(backend)?;
        if slice.events.is_empty() {
            return Ok(None);
        }
        if slice.events.len() != 1 || !slice.end_of_stream {
            return Err(invalid("record identity has multiple reservations"));
        }
        let event = &slice.events[0];
        if event.is_redacted()
            || event.stream().map_err(backend)? != stream
            || event.version != 1
            || event.name != "entity.record-identity"
            || event.schema_version != 1
        {
            return Err(invalid(
                "record identity reservation is invalid or redacted",
            ));
        }
        let claim: IdentityClaim = serde_json::from_value(event.data.clone()).map_err(invalid)?;
        if claim.digest != digest(payload)? {
            return Err(StoreError::RecordConflict {
                record_id: payload.record_id().into(),
            });
        }
        let (entity, id) = payload.subject();
        if claim.stream != self.history_stream(entity, id)? || claim.version == 0 {
            return Err(invalid("record identity points to the wrong subject"));
        }
        Ok(Some(claim))
    }

    async fn recorded(&mut self, payload: &Payload) -> Result<bool, StoreError> {
        let Some(claim) = self.identity_claim(payload).await? else {
            return Ok(false);
        };
        let (entity, id) = payload.subject();
        let recorded = self
            .store
            .read_stream(&claim.stream, claim.version - 1, 1)
            .await
            .map_err(backend)?;
        let held = recorded
            .events
            .first()
            .ok_or_else(|| invalid("reserved record body is missing"))?;
        if held.version != claim.version
            || held.stream().map_err(backend)? != claim.stream
            || decode(held)?.payload != *payload
        {
            return Err(invalid(
                "reserved record body differs from its identity claim",
            ));
        }
        // A claim proves identity, not a valid decision chain. Refuse corrupted retained history.
        self.read_subject(entity, id).await?;
        Ok(true)
    }

    async fn write(&mut self, entries: &[(Payload, Expect)]) -> Result<(), StoreError> {
        if entries.is_empty() {
            return Ok(());
        }
        let mut subjects = BTreeMap::new();
        let mut identities = BTreeMap::new();
        let mut appends = Vec::new();
        let mut pending = Vec::new();
        for (payload, expect) in entries {
            payload.validate()?;
            if let Some(previous) = identities.insert(payload.record_id(), payload) {
                if previous != payload {
                    return Err(StoreError::RecordConflict {
                        record_id: payload.record_id().into(),
                    });
                }
                continue;
            }
            if self.recorded(payload).await? {
                continue;
            }
            let (entity, id) = payload.subject();
            let key = (entity.to_owned(), id.to_owned());
            if !subjects.contains_key(&key) {
                subjects.insert(key.clone(), self.read_subject(entity, id).await?);
            }
            let subject = subjects.get_mut(&key).expect("subject was loaded");
            entity_store::check(entity, id, *expect, subject.instance().map(|i| i.revision))?;
            subject.fold(payload.clone())?;
            let stream = self.history_stream(entity, id)?;
            let version = subject
                .head
                .checked_add(1)
                .ok_or_else(|| invalid("physical position overflow"))?;
            let claim = IdentityClaim {
                digest: digest(payload)?,
                stream: stream.clone(),
                version,
            };
            appends.push(StreamAppend {
                stream: self.identity_stream(payload.record_id())?,
                expected: Expected::NoStream,
                events: vec![
                    NewEvent::new(
                        "entity.record-identity",
                        1,
                        serde_json::to_value(claim).map_err(invalid)?,
                    )
                    .map_err(backend)?,
                ],
            });
            appends.push(StreamAppend {
                stream,
                expected: if subject.head == 0 {
                    Expected::NoStream
                } else {
                    Expected::Exact(subject.head)
                },
                events: vec![
                    NewEvent::new(
                        "entity.record",
                        1,
                        serde_json::to_value(StoredRecord::new(payload.clone()))
                            .map_err(invalid)?,
                    )
                    .map_err(backend)?,
                ],
            });
            subject.head = version;
            pending.push(payload);
        }
        if appends.is_empty() {
            return Ok(());
        }
        // Bind group identity to the complete caller request, never to a newly observed head.
        let request: Vec<_> = entries.iter().map(|(payload, expect)| json!({"record": payload, "expect": match expect { Expect::Absent => Value::Null, Expect::Revision(v) => json!(v) }})).collect();
        let hash = digest(&request)?;
        let timestamp = entries[0].0.recorded_at();
        let meta = CommandMeta {
            idempotency_key: format!("{}-{hash}", self.identity_type),
            request_hash: hash.clone(),
            subject: self.subject.clone(),
            actor: self.actor.clone(),
            request_id: hash.clone(),
            trace_id: hash,
            causation_id: None,
            causation_depth: 0,
            occurred_at: OffsetDateTime::parse(timestamp, &Rfc3339).map_err(invalid)?,
            claim: None,
        };
        let group = AppendGroup {
            tenant: self.tenant.clone(),
            appends,
            meta,
        };
        match self.store.append_group(&group).await {
            Ok(_) => Ok(()),
            Err(
                error
                @ (EventLogError::Conflict { .. } | EventLogError::IdempotencyMismatch { .. }),
            ) => {
                // Another handle may have committed the identical records while this call awaited.
                // Resolve identity read-only; never retry a mutation with a new identity.
                let mut all_recorded = true;
                for payload in pending {
                    all_recorded &= self.recorded(payload).await?;
                }
                if all_recorded {
                    return Ok(());
                }
                for (payload, expect) in entries {
                    let (entity, id) = payload.subject();
                    let current = self.read_subject(entity, id).await?;
                    entity_store::check(
                        entity,
                        id,
                        *expect,
                        current.instance().map(|i| i.revision),
                    )?;
                }
                Err(backend(error))
            }
            Err(error) => Err(backend(error)),
        }
    }
}

impl<S: AtomicEventStore + ?Sized> AsyncRecordedStore for EventlogStore<S> {
    fn verified_history<'a>(
        &'a mut self,
        entity: &'a str,
        id: &'a str,
    ) -> StoreFuture<'a, Arc<entity_store::VerifiedHistory>> {
        Box::pin(async move { Ok(self.read_subject(entity, id).await?.history) })
    }
    fn load<'a>(
        &'a mut self,
        entity: &'a str,
        id: &'a str,
    ) -> StoreFuture<'a, Option<EntityInstance>> {
        Box::pin(async move { Ok(self.read_subject(entity, id).await?.instance().cloned()) })
    }
    fn ids<'a>(&'a mut self, entity: &'a str) -> StoreFuture<'a, Vec<String>> {
        Box::pin(async move {
            let mut ids = BTreeSet::new();
            let mut cursor: Option<String> = None;
            loop {
                let streams = self
                    .store
                    .list_streams(
                        &self.tenant,
                        &self.history_type,
                        cursor.as_deref(),
                        eventlog_core::MAX_READ_LIMIT,
                    )
                    .await
                    .map_err(backend)?;
                for stream in &streams {
                    if stream.tenant() != &self.tenant
                        || stream.stream_type() != self.history_type
                        || cursor
                            .as_deref()
                            .is_some_and(|after| stream.stream_id() <= after)
                    {
                        return Err(invalid(
                            "inventory has a wrong scope or non-advancing identity",
                        ));
                    }
                    let first = self
                        .store
                        .read_stream(stream, 0, 1)
                        .await
                        .map_err(backend)?;
                    let event = first
                        .events
                        .first()
                        .ok_or_else(|| invalid("enumerated history has no creation record"))?;
                    let record = decode(event)?;
                    let (kind, id) = record.payload.subject();
                    if event.version != 1
                        || event.stream().map_err(backend)? != *stream
                        || *stream != self.history_stream(kind, id)?
                    {
                        return Err(invalid("enumerated record has the wrong subject stream"));
                    }
                    if kind == entity {
                        self.read_subject(kind, id).await?;
                        ids.insert(id.to_owned());
                    }
                    cursor = Some(stream.stream_id().into());
                }
                if streams.len() < eventlog_core::MAX_READ_LIMIT {
                    break;
                }
            }
            Ok(ids.into_iter().collect())
        })
    }
    fn events<'a>(&'a mut self, entity: &'a str, id: &'a str) -> StoreFuture<'a, Vec<DomainEvent>> {
        Box::pin(async move {
            Ok(self
                .read_subject(entity, id)
                .await?
                .history
                .records()
                .iter()
                .flat_map(|r| r.record.events.iter().cloned())
                .collect())
        })
    }
    fn records<'a>(
        &'a mut self,
        entity: &'a str,
        id: &'a str,
    ) -> StoreFuture<'a, Vec<Envelope<DecisionRecord>>> {
        Box::pin(async move {
            Ok(self
                .read_subject(entity, id)
                .await?
                .history
                .records()
                .to_vec())
        })
    }
    fn observations<'a>(
        &'a mut self,
        entity: &'a str,
        id: &'a str,
    ) -> StoreFuture<'a, Vec<RecordedObservation>> {
        Box::pin(async move {
            Ok(self
                .read_subject(entity, id)
                .await?
                .observations
                .as_ref()
                .clone())
        })
    }
    fn commit_recorded<'a>(
        &'a mut self,
        commit: &'a RecordedCommit,
        expect: Expect,
    ) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            self.write(&[(Payload::Decision(Box::new(commit.clone())), expect)])
                .await
        })
    }
    fn observe<'a>(&'a mut self, observation: &'a RecordedObservation) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            self.write(&[(
                Payload::Observation(observation.clone()),
                Expect::Revision(observation.revision),
            )])
            .await
        })
    }
}

impl<S: AtomicEventStore + ?Sized> AsyncAtomicRecordedStore for EventlogStore<S> {
    fn commit_recorded_batch<'a>(
        &'a mut self,
        commits: &'a [AtomicRecordedCommit],
    ) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            let entries: Vec<_> = commits
                .iter()
                .map(|entry| {
                    (
                        Payload::Decision(Box::new(entry.commit.clone())),
                        entry.expect,
                    )
                })
                .collect();
            self.write(&entries).await
        })
    }
}

fn digest(value: &impl Serialize) -> Result<String, StoreError> {
    // Persisted identity must not depend on Rust struct field declaration order.
    request_hash(&serde_json::to_value(value).map_err(invalid)?).map_err(backend)
}

fn backend(error: EventLogError) -> StoreError {
    match error {
        EventLogError::UnknownCommit | EventLogError::Closed | EventLogError::Deadline { .. } => {
            StoreError::Unreachable {
                provider: "eventlog".into(),
                detail: error.to_string(),
            }
        }
        other => invalid(other),
    }
}
