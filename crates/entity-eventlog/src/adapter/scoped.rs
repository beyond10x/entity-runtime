//! Per-entity reads: the model of only the subjects one command names.
//!
//! A complete tenant capture reads and verifies every event, blob and index row of the authority,
//! so a command that reads through one costs the whole store however little it touches. A command
//! needs far less: the state and history of the subjects it decides on, whether its batch key and
//! record ids were already used, and the binding that says the tenant is this authority. This
//! module reads exactly that — the binding stream, the streams of those subjects, the blobs their
//! events bind, and the index rows for the keys the command names — and verifies it with the same
//! code a complete capture is verified with.
//!
//! Two things are read beyond the named subjects, because the model cannot be verified without
//! them. A subject whose index row says a named record id or batch key is its own is read too,
//! so an id already used elsewhere is found and refused rather than recorded twice. And every
//! subject that shares a batch with a record read here is read, because a batch is verified whole
//! or not at all.
//!
//! What a per-entity read does not verify is an index row it did not ask for. A complete capture
//! holds every row to the events; this holds the rows for the keys the command names, which are
//! the rows the append guard later decides on under the provider's lock.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::Ordering;

use entity_store::asynchronous::{AppendRequest, AsyncStoreError, BatchKey, Subject};
use eventlog_core::{
    CapturedBlob, MAX_READ_LIMIT, Read, ReadResult, RecordedEvent, StreamId, TenantCapture,
};
use serde_json::Value;

use super::{
    CapturedModel, Decoded, EventlogRecordedStore, build_model_events_remembering,
    expected_projection_rows, integrity, map_read_error, reference_digest,
};
use crate::{
    encoding::{
        EvidenceWire, RecordedEntryWrapper, SubjectWire, decode_anchor, decode_batch, decode_entry,
    },
    projection::{
        batch_key as physical_batch_key, batch_spec, binding_spec, record_key, record_spec,
        subject_key, subject_spec, subject_stream_id,
    },
};

/// How many times a per-entity read is taken again when what it read does not verify.
///
/// A complete capture is one observation. A per-entity read is several provider calls, so a
/// writer committing between two of them can leave an index row that names a record the stream
/// read before it did not yet hold. That disagreement is usually gone on the next read. After the
/// last attempt one complete capture decides: it is one observation, so it refuses damage and
/// answers contention for what it is.
const ATTEMPTS: usize = 3;

/// The subjects, record ids and batch keys one command's reads are about.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ReadScope {
    pub(super) subjects: BTreeSet<Subject>,
    pub(super) records: BTreeSet<String>,
    pub(super) batches: BTreeSet<BatchKey>,
}

impl ReadScope {
    pub(super) fn subject(subject: &Subject) -> Self {
        Self {
            subjects: BTreeSet::from([subject.clone()]),
            ..Self::default()
        }
    }

    pub(super) fn record(record_id: &str) -> Self {
        Self {
            records: BTreeSet::from([record_id.to_owned()]),
            ..Self::default()
        }
    }

    pub(super) fn batch(key: &BatchKey) -> Self {
        Self {
            batches: BTreeSet::from([key.clone()]),
            ..Self::default()
        }
    }

    /// Everything one append request decides on: its key, its members' record ids and subjects.
    pub(super) fn of_request(key: &BatchKey, request: &AppendRequest) -> Self {
        Self {
            subjects: request
                .members
                .iter()
                .map(|member| member.entry.subject())
                .collect(),
            records: request
                .members
                .iter()
                .map(|member| member.entry.record_id().to_owned())
                .collect(),
            batches: BTreeSet::from([key.clone()]),
        }
    }

    #[cfg(feature = "sync-bridge")]
    pub(super) fn extend(&mut self, other: &Self) {
        self.subjects.extend(other.subjects.iter().cloned());
        self.records.extend(other.records.iter().cloned());
        self.batches.extend(other.batches.iter().cloned());
    }

    #[cfg(feature = "sync-bridge")]
    fn is_within(&self, covered: &Self) -> bool {
        self.subjects.is_subset(&covered.subjects)
            && self.records.is_subset(&covered.records)
            && self.batches.is_subset(&covered.batches)
    }
}

/// A verified model of what one scope reads, and what it can answer.
pub(super) struct ScopedModel {
    /// The scope asked for, with every subject whose stream was actually read. Only the executor's
    /// batch reads, which share one read across a command, ask what a read covers.
    #[cfg(feature = "sync-bridge")]
    pub(super) covered: ReadScope,
    pub(super) model: CapturedModel,
}

#[cfg(feature = "sync-bridge")]
impl ScopedModel {
    /// Whether every read `scope` names is answered by this model rather than by its absence.
    pub(super) fn covers(&self, scope: &ReadScope) -> bool {
        scope.is_within(&self.covered)
    }
}

/// What a blob is, so the reads that discover the blobs it names know how to decode it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Role {
    Leaf,
    Entry,
    Batch,
    Anchor,
}

impl EventlogRecordedStore {
    /// A verified model of the subjects, record ids and batch keys `scope` names, read from their
    /// own streams and index rows rather than from a capture of the whole tenant.
    ///
    /// # Errors
    /// A provider stream identity other than the bound one, a provider failure, or anything the
    /// read does not verify.
    pub(super) async fn scoped_model(
        &self,
        scope: &ReadScope,
    ) -> Result<ScopedModel, AsyncStoreError> {
        for _ in 0..ATTEMPTS {
            self.scoped_reads.fetch_add(1, Ordering::Relaxed);
            match self.scoped_once(scope).await {
                Ok(scoped) => return Ok(scoped),
                Err(Retry::Final(error)) => return Err(error),
                Err(Retry::Again) => {}
            }
        }
        // Every attempt disagreed with itself. That is what a writer committing between the
        // separate calls of each attempt looks like, and it is also what damage looks like; only
        // one consistent observation can tell them apart. A complete capture is one, so it decides:
        // contention then ends as the revision conflict or success it is, and damage is refused
        // from the capture exactly as a complete read refuses it.
        let complete = self.capture_model().await?;
        Ok(ScopedModel {
            #[cfg(feature = "sync-bridge")]
            covered: ReadScope {
                subjects: scope
                    .subjects
                    .iter()
                    .chain(complete.histories.keys())
                    .cloned()
                    .collect(),
                ..scope.clone()
            },
            model: (*complete).clone(),
        })
    }

    async fn scoped_once(&self, scope: &ReadScope) -> Result<ScopedModel, Retry> {
        // The binding row is read first, and the identity only once it exists. The Eventlog port has
        // no non-minting identity call: `stream_identity` mints one for a tenant that has none, so
        // asking it about a tenant that was forgotten would re-create that tenant. A bound tenant
        // already has its identity, so behind a present binding the call only reads it.
        let binding_row = self.row(0, "singleton").await?;
        if binding_row.is_none() {
            return Err(Retry::Final(integrity(
                "the tenant has no authoritative binding",
            )));
        }
        // The identity is checked before any subject is read, as a capture's is: a provider
        // answering for another generation of this tenant is refused, not read.
        let identity = self
            .backend
            .stream_identity(&self.tenant)
            .await
            .map_err(|error| Retry::Final(map_read_error(error)))?;
        if identity != self.authority.stream_identity {
            return Err(Retry::Final(integrity(
                "provider stream identity substituted tenant generation",
            )));
        }

        let mut rows: Vec<(usize, String, Option<Value>)> = Vec::new();
        let mut subjects = scope.subjects.clone();
        rows.push((0, "singleton".to_owned(), binding_row));
        for key in &scope.batches {
            let index = physical_batch_key(&self.authority, key).map_err(Retry::Final)?;
            let row = self.row(2, &index).await?;
            if let Some(row) = &row {
                subjects.extend(row_subjects(row, "/1/members", "/subject")?);
            }
            rows.push((2, index, row));
        }
        for record_id in &scope.records {
            let index = record_key(&self.authority, record_id).map_err(Retry::Final)?;
            let row = self.row(1, &index).await?;
            if let Some(row) = &row {
                subjects.extend(row_subjects(row, "", "/1/entry/subject")?);
            }
            rows.push((1, index, row));
        }

        let binding = self.stream("er.binding", "singleton")?;
        let mut events = self.read_streams(vec![binding]).await?;
        let mut read: BTreeSet<Subject> = BTreeSet::new();
        let mut blobs: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        let mut absent: BTreeSet<String> = BTreeSet::new();
        let mut pending: Vec<(String, Role)> = event_blobs(&events);
        loop {
            let unread: Vec<Subject> = subjects.difference(&read).cloned().collect();
            if unread.is_empty() && pending.is_empty() {
                break;
            }
            if !unread.is_empty() {
                let mut streams = Vec::with_capacity(unread.len());
                for subject in &unread {
                    let id = subject_stream_id(&self.authority, subject).map_err(Retry::Final)?;
                    streams.push(self.stream("er.subject", &id)?);
                }
                let found = self.read_streams(streams).await?;
                pending.extend(event_blobs(&found));
                events.extend(found);
                read.extend(unread);
            }
            // Each blob once per round in each role: every member of a group names the group's
            // blob, and a round that reaches several of them would otherwise read and decode it
            // once for each.
            let mut named: BTreeSet<(String, Role)> = BTreeSet::new();
            let wanted: Vec<(String, Role)> = std::mem::take(&mut pending)
                .into_iter()
                .filter(|(digest, role)| {
                    !blobs.contains_key(digest)
                        && !absent.contains(digest)
                        && named.insert((digest.clone(), *role))
                })
                .collect();
            if wanted.is_empty() {
                continue;
            }
            let reads: Vec<Read> = wanted
                .iter()
                .map(|(digest, _)| Read::Blob {
                    tenant: self.tenant.clone(),
                    digest: digest.clone(),
                })
                .collect();
            let results = self
                .backend
                .read_many(&reads)
                .await
                .map_err(|error| Retry::Final(map_read_error(error)))?;
            if results.len() != reads.len() {
                return Err(Retry::Final(integrity(
                    "provider answered a read batch with another number of results",
                )));
            }
            for ((digest, role), result) in wanted.into_iter().zip(results) {
                let ReadResult::Blob(bytes) = result else {
                    return Err(Retry::Final(integrity(
                        "provider answered a blob read with a stream",
                    )));
                };
                let Some(bytes) = bytes else {
                    // Left for the build to refuse, which names a missing reference the way a
                    // capture that lacks it does.
                    absent.insert(digest);
                    continue;
                };
                // What a blob names is read to know what else to read. It is verified by the
                // build below; a blob that does not decode here is refused there.
                let (named, members) = self.discover(&digest, role, &bytes);
                pending.extend(named);
                subjects.extend(members);
                blobs.insert(digest, bytes);
            }
        }
        for subject in &read {
            let index = subject_key(&self.authority, subject).map_err(Retry::Final)?;
            rows.push((3, index.clone(), self.row(3, &index).await?));
        }

        events.sort_by_key(|event| event.global_seq);
        let capture = TenantCapture {
            tenant: self.tenant.clone(),
            stream_identity: identity,
            events,
            blobs: blobs
                .into_iter()
                .map(|(digest, bytes)| CapturedBlob { digest, bytes })
                .collect(),
            projections: Vec::new(),
        };
        let model = {
            let mut memory = self.memory.lock().ok();
            build_model_events_remembering(&self.authority, &capture, 0, memory.as_deref_mut())
        }
        .map_err(|_| Retry::Again)?;
        self.records_decoded
            .fetch_add(model.decoded, Ordering::Relaxed);
        if model.binding.is_none() {
            return Err(Retry::Again);
        }
        let expected =
            expected_projection_rows(&self.authority, &model).map_err(|_| Retry::Again)?;
        for (index, key, row) in &rows {
            if expected[*index].get(key) != row.as_ref() {
                return Err(Retry::Again);
            }
        }
        Ok(ScopedModel {
            #[cfg(feature = "sync-bridge")]
            covered: ReadScope {
                subjects: read,
                ..scope.clone()
            },
            model,
        })
    }

    /// What a blob names, answered from this handle's decode of exactly these bytes when it has
    /// one. The bytes are compared with the ones it verified, not trusted from the digest: what a
    /// per-entity read discovers has not been verified yet, and is verified by the build.
    fn discover(
        &self,
        digest: &str,
        role: Role,
        bytes: &[u8],
    ) -> (Vec<(String, Role)>, Vec<Subject>) {
        let remembered = self
            .memory
            .lock()
            .ok()
            .and_then(|memory| memory.decoded(digest, bytes).cloned());
        match (role, remembered) {
            (Role::Entry, Some(Decoded::Entry(wrapper))) => (named_by_entry(*wrapper), Vec::new()),
            (Role::Batch, Some(Decoded::BatchSubjects(subjects))) => (Vec::new(), subjects),
            _ => discover(role, bytes),
        }
    }

    fn stream(&self, stream_type: &str, stream_id: &str) -> Result<StreamId, Retry> {
        StreamId::new(self.tenant.clone(), stream_type, stream_id)
            .map_err(|error| Retry::Final(super::input_eventlog(error)))
    }

    async fn row(&self, index: usize, key: &str) -> Result<Option<Value>, Retry> {
        let spec = [binding_spec(), record_spec(), batch_spec(), subject_spec()][index];
        self.backend
            .projection_get(spec, &self.tenant, key)
            .await
            .map_err(|error| Retry::Final(map_read_error(error)))
    }

    /// Every event of `streams`, read forward to its end in batches the provider answers together.
    async fn read_streams(&self, streams: Vec<StreamId>) -> Result<Vec<RecordedEvent>, Retry> {
        let mut events = Vec::new();
        let mut cursors: Vec<(StreamId, u64)> =
            streams.into_iter().map(|stream| (stream, 0)).collect();
        while !cursors.is_empty() {
            let reads: Vec<Read> = cursors
                .iter()
                .map(|(stream, after)| Read::Stream {
                    stream: stream.clone(),
                    after_version: *after,
                    limit: MAX_READ_LIMIT,
                })
                .collect();
            let results = self
                .backend
                .read_many(&reads)
                .await
                .map_err(|error| Retry::Final(map_read_error(error)))?;
            if results.len() != reads.len() {
                return Err(Retry::Final(integrity(
                    "provider answered a read batch with another number of results",
                )));
            }
            let mut next = Vec::new();
            for ((stream, after), result) in cursors.into_iter().zip(results) {
                let ReadResult::Stream(slice) = result else {
                    return Err(Retry::Final(integrity(
                        "provider answered a stream read with a blob",
                    )));
                };
                if slice.events.iter().any(|event| {
                    event.tenant != self.tenant
                        || event.stream_type != stream.stream_type()
                        || event.stream_id != stream.stream_id()
                }) {
                    return Err(Retry::Final(integrity(
                        "provider answered a stream read with another stream's events",
                    )));
                }
                let advanced = !slice.end_of_stream;
                if advanced && slice.next_version <= after {
                    return Err(Retry::Final(integrity(
                        "provider stream read does not advance",
                    )));
                }
                events.extend(slice.events);
                if advanced {
                    next.push((stream, slice.next_version));
                }
            }
            cursors = next;
        }
        Ok(events)
    }
}

/// Whether a failed per-entity read is worth taking again.
enum Retry {
    /// Refused for a reason another read cannot change.
    Final(AsyncStoreError),
    /// What was read did not verify, which a writer committing mid-read can also cause. The
    /// reason is not kept: after the last attempt a complete capture decides, and words, it.
    Again,
}

/// The blob each event references, and what kind of blob it is.
fn event_blobs(events: &[RecordedEvent]) -> Vec<(String, Role)> {
    events
        .iter()
        .filter_map(|event| {
            let role = match event.name.as_str() {
                "er.recorded_entry" => Role::Entry,
                "er.import_anchor" => Role::Anchor,
                _ => Role::Leaf,
            };
            reference_digest(event)
                .ok()
                .map(|digest| (digest.to_owned(), role))
        })
        .collect()
}

/// The blobs one blob names, and the subjects of the batch it is, if it is one.
fn discover(role: Role, bytes: &[u8]) -> (Vec<(String, Role)>, Vec<Subject>) {
    match role {
        Role::Leaf => (Vec::new(), Vec::new()),
        Role::Entry => decode_entry(bytes).map_or_else(
            |_| (Vec::new(), Vec::new()),
            |wrapper| (named_by_entry(wrapper), Vec::new()),
        ),
        Role::Batch => decode_batch(bytes).map_or_else(
            |_| (Vec::new(), Vec::new()),
            |(_, members)| {
                (
                    Vec::new(),
                    members
                        .iter()
                        .map(|member| member.entry.subject())
                        .collect(),
                )
            },
        ),
        Role::Anchor => decode_anchor(bytes).map_or_else(
            |_| (Vec::new(), Vec::new()),
            |wrapper| {
                (
                    wrapper
                        .evidence
                        .iter()
                        .filter_map(|evidence| match evidence {
                            EvidenceWire::Envelope { record_blob, .. } => {
                                Some((record_blob.clone(), Role::Leaf))
                            }
                            _ => None,
                        })
                        .collect(),
                    Vec::new(),
                )
            },
        ),
    }
}

/// The blobs an entry wrapper names, and what each of them is.
fn named_by_entry(wrapper: RecordedEntryWrapper) -> Vec<(String, Role)> {
    vec![
        (wrapper.record_blob, Role::Leaf),
        (wrapper.request_blob, Role::Leaf),
        (wrapper.batch_blob, Role::Batch),
    ]
}

/// The subjects an index row names: the one at `subject` in each element of the array at `list`,
/// or the one at `subject` in the row itself when `list` is empty.
fn row_subjects(row: &Value, list: &str, subject: &str) -> Result<Vec<Subject>, Retry> {
    let malformed = || Retry::Again;
    let items: Vec<&Value> = if list.is_empty() {
        vec![row]
    } else {
        row.pointer(list)
            .and_then(Value::as_array)
            .ok_or_else(malformed)?
            .iter()
            .collect()
    };
    items
        .into_iter()
        .map(|item| {
            let wire: SubjectWire = item
                .pointer(subject)
                .cloned()
                .and_then(|value| serde_json::from_value(value).ok())
                .ok_or_else(malformed)?;
            Ok(Subject::from(wire))
        })
        .collect()
}
