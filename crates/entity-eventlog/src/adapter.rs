use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use entity_core::EntityInstance;
#[cfg(feature = "sync-bridge")]
use entity_core::Registry;
#[cfg(feature = "sync-bridge")]
use entity_executor::{BatchAction, ExecutionError, Executor};
use entity_store::{
    Expect,
    asynchronous::{
        AppendOutcome, AppendRequest, AsyncRecordedReader, AsyncRecordedWriter, AsyncStateReader,
        AsyncStoreError, BatchKey, BatchReceipt, BoxFuture, CommitReceipt, CompleteStoreSnapshot,
        HistoryOrigin, RecordLookup, RecordPosition, RecordReceipt, StoreCoverage, StoredBatch,
        StoredRecord, Subject, SubjectAssurance, SubjectHistory, SubjectSnapshot, WriteFailure,
        batch_comparison_bytes, original_request_comparison_bytes, record_comparison_bytes,
        validate_entry_against_state, verify_subject_history, verify_subject_history_extension,
    },
};
use eventlog_core::{
    AppendGroup, AtomicEventStore, CaptureError, CaptureLimits, CommandMeta,
    ConsistentTenantCapture, EventLogError, EventStore, Expected, Guard, InlineProjectionAdmin,
    NewEvent, ProjectionStore, RecordedEvent, StreamAppend, StreamId, TenantCapture, TenantId,
    UNAVAILABLE,
};
use serde_json::{Value, json};
use time::OffsetDateTime;

use crate::{
    encoding::{
        ANCHOR_BLOB_DOMAIN, Authority, BATCH_BLOB_DOMAIN, BINDING_BLOB_DOMAIN, ENTRY_BLOB_DOMAIN,
        EvidenceWire, PhysicalRef, RECORD_BLOB_DOMAIN, REQUEST_BLOB_DOMAIN, RecordedEntryWrapper,
        SubjectWire, anchor_from_history, decode_anchor, decode_batch, decode_binding,
        decode_entry, decode_record, encode_anchor, encode_binding, encode_entry,
        encode_source_anchor, history_from_anchor, key_for_value,
    },
    projection::{
        PROJECTOR_NAME, batch_key as physical_batch_key, batch_spec, binding_spec, physical,
        projection_specs, record_key, record_spec, subject_key, subject_spec, subject_stream_id,
        tagged_body,
    },
};

#[cfg(test)]
thread_local! {
    /// Blob bytes this file has put through SHA-256 on the current thread.
    ///
    /// Charged by the only function in this file that can reach the hash, not beside it: a change
    /// that stops hashing a blob stops calling [`framed_key`] and therefore stops charging, and a
    /// change that keeps hashing cannot avoid the charge without importing
    /// `crate::encoding::framed_key` again by name. That is why the direct import was removed.
    static HASHED_BYTES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// This file's only path to [`crate::encoding::framed_key`].
fn framed_key(domain: &str, bytes: &[u8]) -> Result<String, AsyncStoreError> {
    #[cfg(test)]
    HASHED_BYTES.with(|charged| charged.set(charged.get().saturating_add(bytes.len() as u64)));
    crate::encoding::framed_key(domain, bytes)
}

/// Eventlog capabilities required by the adapter, available as one object-safe backend.
pub trait EventlogBackend:
    EventStore + AtomicEventStore + ConsistentTenantCapture + InlineProjectionAdmin
{
}

#[cfg(all(test, feature = "sqlite", feature = "sync-bridge"))]
mod batch_read_tests {
    use super::*;
    use entity_executor::{CreateRequest, ExecuteRequest};
    use entity_store::Recording;

    const LIMITS: CaptureLimits = CaptureLimits {
        max_events: 128,
        max_blobs: 512,
        max_projection_rows: 512,
        max_payload_bytes: 4 * 1024 * 1024,
    };

    fn context(label: &str) -> EventlogOperationContext {
        EventlogOperationContext {
            subject: "batch-read-test".into(),
            actor: "entity-eventlog-test".into(),
            request_id: format!("request-{label}"),
            trace_id: format!("trace-{label}"),
            causation_id: None,
            causation_depth: 0,
            occurred_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn registry() -> Registry {
        let definition = serde_json::from_value(json!({
            "entity": "ticket",
            "version": 1,
            "schema": { "fields": { "title": { "type": "string", "required": true } } },
            "lifecycle": { "initial": "open", "states": ["open"] },
            "operations": {
                "touch": {
                    "transitions": [{ "from": "open", "to": "open" }],
                    "arguments": { "fields": {} },
                    "emits": []
                }
            }
        }))
        .expect("definition parses");
        let mut registry = Registry::new();
        registry.register(definition).expect("definition validates");
        registry
    }

    fn recording(id: &str) -> Recording {
        Recording {
            record_id: id.into(),
            recorded_at: "2026-09-18T00:00:00Z".into(),
            correlation: None,
            causation: None,
            actor: None,
        }
    }

    fn create(id: &str, record_id: &str) -> BatchAction {
        BatchAction::Create(CreateRequest {
            subject: Subject::new("ticket", id).expect("subject"),
            definition_version: 1,
            fields: json!({"title": id}),
            recording: recording(record_id),
        })
    }

    fn touch(id: &str, record_id: &str) -> BatchAction {
        BatchAction::Execute(ExecuteRequest {
            subject: Subject::new("ticket", id).expect("subject"),
            expected_revision: 1,
            operation: "touch".into(),
            arguments: json!({}),
            fulfillments: BTreeMap::new(),
            recording: recording(record_id),
        })
    }

    async fn store() -> EventlogRecordedStore {
        let backend = Arc::new(
            eventlog_sqlite::SqliteEventStore::in_memory("batch_read")
                .await
                .expect("SQLite provider"),
        );
        let tenant = TenantId::new("batch-read-test").expect("tenant");
        let stream_identity = backend.stream_identity(&tenant).await.expect("generation");
        let projector = Arc::new(crate::ErRecordedProjector::new());
        backend
            .create_projections(projector.clone())
            .await
            .expect("projection admission");
        backend
            .attach_inline_existing(projector)
            .await
            .expect("projection attachment");
        let authority = Authority {
            logical_scope: "batch-read-scope".into(),
            tenant: tenant.as_str().into(),
            stream_identity,
        };
        let erased: Arc<dyn EventlogBackend> = backend;
        EventlogBindingProvisioner::new(erased.clone(), LIMITS)
            .provision_binding(authority.clone(), context("binding"))
            .await
            .expect("binding");
        EventlogRecordedStore::open(erased, authority, LIMITS)
            .await
            .expect("bound store")
    }

    fn captures(store: &EventlogRecordedStore) -> usize {
        store.native_captures.load(Ordering::Relaxed)
    }

    #[tokio::test]
    async fn one_native_capture_serves_each_preflight_and_is_never_reused_across_calls() {
        let store = store().await;
        let registry = registry();
        let first_key = BatchKey::Named("first".into());
        let first: Vec<_> = (0..4)
            .map(|i| create(&format!("first-{i}"), &format!("first-record-{i}")))
            .collect();
        let before = captures(&store);
        assert_eq!(
            store
                .operation(context("empty"))
                .execute_batch(&registry, BatchKey::Named("".into()), Vec::new())
                .await
                .expect("empty batch"),
            AppendOutcome::Empty
        );
        assert_eq!(captures(&store), before, "empty batches do no IO");

        let committed = store
            .operation(context("first"))
            .execute_batch(&registry, first_key.clone(), first.clone())
            .await
            .expect("four authored decisions commit");
        assert_eq!(
            captures(&store) - before,
            3,
            "one preflight, one append recovery whose capture also supplies the heads, one \
             postcommit verification"
        );
        let before = captures(&store);
        let replay = store
            .operation(context("first-replay"))
            .execute_batch(&registry, first_key, first)
            .await
            .expect("exact replay");
        assert!(replay.replayed());
        assert_eq!(replay.receipt(), committed.receipt());
        assert_eq!(
            captures(&store) - before,
            1,
            "replay verifies one fresh authority"
        );

        let second: Vec<_> = (0..4)
            .map(|i| create(&format!("second-{i}"), &format!("second-record-{i}")))
            .collect();
        let before = captures(&store);
        store
            .operation(context("second"))
            .execute_batch(&registry, BatchKey::Named("second".into()), second)
            .await
            .expect("second batch commits");
        assert_eq!(
            captures(&store) - before,
            3,
            "second call takes its own preflight"
        );

        let pending = touch("later", "later-touch");
        let before = captures(&store);
        assert!(
            store
                .operation(context("refused"))
                .execute_batch(
                    &registry,
                    BatchKey::SingleRecord("later-touch".into()),
                    vec![pending.clone()]
                )
                .await
                .expect_err("missing predecessor refuses")
                .is_revision_conflict()
        );
        assert_eq!(captures(&store) - before, 1);
        store
            .operation(context("later-create"))
            .execute_batch(
                &registry,
                BatchKey::SingleRecord("later-create".into()),
                vec![create("later", "later-create")],
            )
            .await
            .expect("authority changes after refusal");
        let before = captures(&store);
        store
            .operation(context("later-touch"))
            .execute_batch(
                &registry,
                BatchKey::SingleRecord("later-touch".into()),
                vec![pending],
            )
            .await
            .expect("fresh call sees created subject");
        assert_eq!(captures(&store) - before, 3);
    }

    #[tokio::test]
    async fn stale_preflight_is_guarded_and_replayed_append_recovers_from_fresh_capture() {
        let store = store().await;
        let registry = registry();
        let stale = BatchReadStore::new(store.operation(context("stale")));
        stale
            .lookup_batch(&BatchKey::Named("stale".into()))
            .await
            .expect("preflight");
        let winner = store
            .operation(context("winner"))
            .execute_batch(
                &registry,
                BatchKey::SingleRecord("winner".into()),
                vec![create("same", "winner")],
            )
            .await
            .expect("winner commits");
        assert!(!winner.replayed());
        let conflict = Executor::new(&registry, &stale)
            .batch(
                BatchKey::SingleRecord("stale".into()),
                vec![create("same", "stale")],
            )
            .await
            .expect_err("stale predecessor must not append");
        assert!(conflict.is_revision_conflict(), "{conflict:?}");
        assert!(
            store
                .lookup_record("stale")
                .await
                .expect("fresh lookup")
                .is_none()
        );

        let action = create("replayed", "replayed");
        let replay_key = BatchKey::SingleRecord("replayed".into());
        let pending = BatchReadStore::new(store.operation(context("pending")));
        pending
            .lookup_batch(&replay_key)
            .await
            .expect("stale empty preflight");
        let committed = store
            .operation(context("commit-replayed"))
            .execute_batch(&registry, replay_key.clone(), vec![action.clone()])
            .await
            .expect("other caller commits exact action");
        let before = captures(&store);
        let replay = Executor::new(&registry, &pending)
            .batch(replay_key, vec![action])
            .await
            .expect("append replay recovers");
        assert!(replay.replayed());
        assert_eq!(replay.receipt(), committed.receipt());
        assert_eq!(
            captures(&store) - before,
            2,
            "append and executor recovery use fresh captures"
        );
    }

    #[tokio::test]
    async fn lost_append_reply_recovers_with_a_fresh_verified_capture() {
        let store = store().await;
        let registry = registry();
        let key = BatchKey::Named("lost-reply".into());
        let mut operation = BatchReadStore::new(store.operation(context("lost-reply")));
        operation.return_uncertain_after_commit = true;
        let before = captures(&store);
        let recovered = Executor::new(&registry, &operation)
            .batch(key.clone(), vec![create("lost-reply", "lost-reply-record")])
            .await
            .expect("uncertain append recovers from committed authority");
        assert!(recovered.replayed());
        assert_eq!(
            captures(&store) - before,
            4,
            "preflight, guarded write path, postcommit check and fresh recovery"
        );
        let committed = store
            .lookup_batch(&key)
            .await
            .expect("fresh authoritative lookup")
            .expect("committed batch");
        assert_eq!(recovered.receipt(), Some(&committed.receipt));
    }

    fn spent(before: StoreCalls, after: StoreCalls) -> StoreCalls {
        StoreCalls {
            captures: after.captures - before.captures,
            model_builds: after.model_builds - before.model_builds,
            model_advances: after.model_advances - before.model_advances,
            records_decoded: after.records_decoded - before.records_decoded,
        }
    }

    async fn seed(store: &EventlogRecordedStore, registry: &Registry, count: usize) {
        let actions: Vec<_> = (0..count)
            .map(|i| create(&format!("seed-{i}"), &format!("seed-record-{i}")))
            .collect();
        store
            .operation(context("seed"))
            .execute_batch(registry, BatchKey::Named("seed".into()), actions)
            .await
            .expect("seed batch commits");
    }

    /// A read of an authority whose head has not moved is answered from the model this handle
    /// already verified. Each read still takes its own capture — that is what proves the head has
    /// not moved, and what a tampered capture is refused from — but a capture that is exactly the
    /// verified one is not decoded, hashed and replayed a second time.
    #[tokio::test]
    async fn reads_of_an_unmoved_head_reuse_the_model_the_handle_already_verified() {
        let store = store().await;
        let registry = registry();
        seed(&store, &registry, 3).await;
        let subject = Subject::new("ticket", "seed-0").expect("subject");
        let before = store.calls();
        assert!(store.load(&subject).await.expect("load").is_some());
        assert!(
            store
                .lookup_record("seed-record-1")
                .await
                .expect("lookup")
                .is_some()
        );
        assert!(
            store
                .lookup_batch(&BatchKey::Named("seed".into()))
                .await
                .expect("batch")
                .is_some()
        );
        assert_eq!(
            store
                .history(&subject)
                .await
                .expect("history")
                .records
                .len(),
            1
        );
        assert_eq!(
            store
                .complete_snapshot("batch-read-scope")
                .await
                .expect("snapshot")
                .histories
                .len(),
            3
        );
        assert_eq!(
            spent(before, store.calls()),
            StoreCalls {
                captures: 5,
                model_builds: 0,
                model_advances: 0,
                records_decoded: 0,
            },
            "five reads of one unmoved head: five captures, and not one model rebuilt"
        );
    }

    /// A decision this handle commits advances the verified model by the records it appended,
    /// and nothing else is decoded again — the bound is one record per decision, not the store.
    #[tokio::test]
    async fn a_decision_this_handle_commits_advances_its_model_by_its_own_records_only() {
        let store = store().await;
        let registry = registry();
        seed(&store, &registry, 4).await;
        for index in 0..3 {
            let before = store.calls();
            store
                .operation(context(&format!("touch-{index}")))
                .execute_batch(
                    &registry,
                    BatchKey::SingleRecord(format!("touch-{index}")),
                    vec![touch(&format!("seed-{index}"), &format!("touch-{index}"))],
                )
                .await
                .expect("the decision commits");
            assert_eq!(
                spent(before, store.calls()),
                StoreCalls {
                    captures: 3,
                    model_builds: 0,
                    model_advances: 1,
                    records_decoded: 1,
                },
                "decision {index}: preflight, append recovery and post-commit captures, one \
                 advance by the one record it appended"
            );
        }
        // What the advanced handle answers is what a handle that built everything answers.
        let fresh = EventlogRecordedStore::open(
            Arc::clone(&store.backend),
            store.authority.clone(),
            LIMITS,
        )
        .await
        .expect("a second handle opens");
        assert_eq!(fresh.calls().model_builds, 1, "open builds the whole model");
        assert_eq!(
            store
                .complete_snapshot("batch-read-scope")
                .await
                .expect("advanced snapshot"),
            fresh
                .complete_snapshot("batch-read-scope")
                .await
                .expect("whole-build snapshot")
        );
        for index in 0..3 {
            let key = BatchKey::SingleRecord(format!("touch-{index}"));
            assert_eq!(
                store.lookup_batch(&key).await.expect("advanced batch"),
                fresh.lookup_batch(&key).await.expect("whole-build batch")
            );
        }
    }

    /// A head this handle did not produce is never answered from this handle's model: the next
    /// call builds the whole model again and sees what the other writer committed.
    #[tokio::test]
    async fn a_head_another_writer_moved_forces_a_whole_build_that_sees_it() {
        let ours = store().await;
        let theirs =
            EventlogRecordedStore::open(Arc::clone(&ours.backend), ours.authority.clone(), LIMITS)
                .await
                .expect("the other writer's handle opens");
        let registry = registry();
        let foreign = Subject::new("ticket", "foreign").expect("subject");
        assert!(ours.load(&foreign).await.expect("load").is_none());
        let before = ours.calls();
        assert!(ours.load(&foreign).await.expect("load").is_none());
        assert_eq!(
            spent(before, ours.calls()).model_builds,
            0,
            "an unmoved head is not rebuilt"
        );

        theirs
            .operation(context("foreign-create"))
            .execute_batch(
                &registry,
                BatchKey::SingleRecord("foreign-create".into()),
                vec![create("foreign", "foreign-create")],
            )
            .await
            .expect("the other writer commits");
        let before = ours.calls();
        let seen = ours
            .load(&foreign)
            .await
            .expect("load")
            .expect("the other writer's subject is seen");
        assert_eq!(seen.revision, 1);
        assert_eq!(
            spent(before, ours.calls()),
            StoreCalls {
                captures: 1,
                model_builds: 1,
                model_advances: 0,
                records_decoded: 1,
            },
            "a foreign head is a whole build, never an advance"
        );

        // Our next decision is decided on the other writer's state, and theirs then sees ours.
        ours.operation(context("foreign-touch"))
            .execute_batch(
                &registry,
                BatchKey::SingleRecord("foreign-touch".into()),
                vec![touch("foreign", "foreign-touch")],
            )
            .await
            .expect("a decision on the foreign subject commits");
        let before = theirs.calls();
        assert_eq!(
            theirs
                .load(&foreign)
                .await
                .expect("load")
                .expect("subject")
                .revision,
            2
        );
        assert_eq!(spent(before, theirs.calls()).model_builds, 1);
    }

    /// A definition of `fields` optional fields, so every decision record carries a large one.
    fn large_registry(fields: usize) -> Registry {
        let mut schema = serde_json::Map::new();
        schema.insert(
            "title".into(),
            json!({ "type": "string", "required": true }),
        );
        for index in 0..fields {
            schema.insert(
                format!("field_{index:06}_carrying_a_long_declared_name"),
                json!({ "type": "string", "max_length": 64 }),
            );
        }
        let definition = serde_json::from_value(json!({
            "entity": "ticket",
            "version": 1,
            "schema": { "fields": schema },
            "lifecycle": { "initial": "open", "states": ["open"] },
            "operations": {
                "touch": {
                    "transitions": [{ "from": "open", "to": "open" }],
                    "arguments": { "fields": {} },
                    "emits": []
                }
            }
        }))
        .expect("large definition parses");
        let mut registry = Registry::new();
        registry
            .register(definition)
            .expect("large definition validates");
        registry
    }

    fn measured_number(name: &str, default: usize) -> usize {
        std::env::var(name).map_or(default, |value| value.parse().expect("a count"))
    }

    /// The measurement for the model-cost unit: N sequential decisions on a seeded store whose
    /// every record carries a large definition. Asks for nothing unless
    /// `ENTITY_EVENTLOG_DECISION_RECORDS` is set.
    #[tokio::test]
    async fn sequential_decisions_measurement() {
        let Ok(records) = std::env::var("ENTITY_EVENTLOG_DECISION_RECORDS") else {
            return;
        };
        let records: usize = records.parse().expect("record count");
        let fields = measured_number("ENTITY_EVENTLOG_DECISION_FIELDS", 4_000);
        let decisions = measured_number("ENTITY_EVENTLOG_DECISIONS", 5);
        let chunk = measured_number("ENTITY_EVENTLOG_DECISION_SEED_BATCH", 100);
        let limits = CaptureLimits {
            max_events: 1_000_000,
            max_blobs: 1_000_000,
            max_projection_rows: 1_000_000,
            max_payload_bytes: 1 << 36,
        };
        let registry = large_registry(fields);
        let backend = Arc::new(
            eventlog_sqlite::SqliteEventStore::in_memory("decision_measurement")
                .await
                .expect("SQLite provider"),
        );
        let tenant = TenantId::new("decision-measurement").expect("tenant");
        let stream_identity = backend.stream_identity(&tenant).await.expect("generation");
        let projector = Arc::new(crate::ErRecordedProjector::new());
        backend
            .create_projections(projector.clone())
            .await
            .expect("projection admission");
        backend
            .attach_inline_existing(projector)
            .await
            .expect("projection attachment");
        let authority = Authority {
            logical_scope: "decision-measurement-scope".into(),
            tenant: tenant.as_str().into(),
            stream_identity,
        };
        let erased: Arc<dyn EventlogBackend> = backend;
        EventlogBindingProvisioner::new(erased.clone(), limits)
            .provision_binding(authority.clone(), context("binding"))
            .await
            .expect("binding");
        let store = EventlogRecordedStore::open(erased, authority, limits)
            .await
            .expect("bound store");
        let seeding = std::time::Instant::now();
        let mut seeded = 0;
        while seeded < records {
            let end = (seeded + chunk).min(records);
            let actions: Vec<_> = (seeded..end)
                .map(|i| create(&format!("seed-{i:06}"), &format!("seed-record-{i:06}")))
                .collect();
            store
                .operation(context(&format!("seed-{seeded}")))
                .execute_batch(
                    &registry,
                    BatchKey::Named(format!("seed-{seeded}")),
                    actions,
                )
                .await
                .expect("seed batch commits");
            seeded = end;
        }
        let record = store
            .lookup_record("seed-record-000000")
            .await
            .expect("seed lookup")
            .expect("seeded record");
        let RecordLookup::Committed(saved) = record else {
            panic!("seeded record is committed")
        };
        println!(
            "seeded {records} records ({fields} fields, record blob {} bytes) in {:?}",
            saved.record_bytes.len(),
            seeding.elapsed()
        );
        let mut elapsed = Vec::with_capacity(decisions);
        for index in 0..decisions {
            let before = store.calls();
            let start = std::time::Instant::now();
            store
                .operation(context(&format!("decision-{index}")))
                .execute_batch(
                    &registry,
                    BatchKey::SingleRecord(format!("decision-{index}")),
                    vec![touch(
                        &format!("seed-{index:06}"),
                        &format!("decision-{index}"),
                    )],
                )
                .await
                .expect("measured decision commits");
            let took = start.elapsed();
            elapsed.push(took);
            println!(
                "decision {index}: {took:?}; calls before {before:?}, after {:?}",
                store.calls()
            );
        }
        elapsed.sort();
        println!(
            "decisions: {decisions}, median {:?}, total {:?}",
            elapsed[elapsed.len() / 2],
            elapsed.iter().sum::<std::time::Duration>()
        );
    }
}

impl<T> EventlogBackend for T where
    T: EventStore + AtomicEventStore + ConsistentTenantCapture + InlineProjectionAdmin
{
}

/// Caller-owned operational facts for one possible Eventlog command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventlogOperationContext {
    /// Opaque principal for whom the operation runs.
    pub subject: String,
    /// Opaque agent or service issuing it.
    pub actor: String,
    /// Caller-stable request identity.
    pub request_id: String,
    /// Caller-stable trace identity.
    pub trace_id: String,
    /// Optional causing event identity.
    pub causation_id: Option<String>,
    /// Bounded automation depth.
    pub causation_depth: u32,
    /// Caller-understood occurrence time.
    pub occurred_at: OffsetDateTime,
}

impl EventlogOperationContext {
    fn meta(
        &self,
        idempotency_key: String,
        request_hash: String,
    ) -> Result<CommandMeta, AsyncStoreError> {
        let meta = CommandMeta {
            idempotency_key,
            request_hash,
            subject: self.subject.clone(),
            actor: self.actor.clone(),
            request_id: self.request_id.clone(),
            trace_id: self.trace_id.clone(),
            causation_id: self.causation_id.clone(),
            causation_depth: self.causation_depth,
            occurred_at: self.occurred_at,
            claim: None,
        };
        meta.validate().map_err(input_eventlog)?;
        Ok(meta)
    }
}

/// What one store handle has actually asked its provider for, and verified from its answers.
///
/// A capture verifies every blob digest of the whole authority, so its cost grows with the store,
/// and so does a whole model build from one. A caller proving that a batch costs a fixed number of
/// them — rather than a number that grows with the batch or the store — reads these counters,
/// because no assertion about wall clock can say the same thing on a machine under load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StoreCalls {
    /// Complete authoritative tenant captures taken through this handle.
    pub captures: usize,
    /// Verified models built from a whole capture: every event, blob, history and row checked.
    pub model_builds: usize,
    /// Verified models advanced by only the events this handle itself committed.
    pub model_advances: usize,
    /// Complete records decoded and re-encoded to their canonical bytes while verifying.
    pub records_decoded: usize,
}

/// Bound, non-initializing Eventlog implementation of complete recorded reads.
pub struct EventlogRecordedStore {
    backend: Arc<dyn EventlogBackend>,
    authority: Authority,
    tenant: TenantId,
    limits: CaptureLimits,
    native_captures: AtomicUsize,
    model_builds: AtomicUsize,
    model_advances: AtomicUsize,
    records_decoded: AtomicUsize,
    /// The last capture this handle verified, and the model it verified it into.
    verified: Mutex<Option<Verified>>,
    /// Events this handle's own committed appends returned, not yet folded into `verified`.
    own_events: Mutex<BTreeSet<String>>,
}

/// One verified capture and the model built from it.
///
/// The capture is kept whole — events, every bound blob's bytes and every projection row — because
/// it is the key the model is reused under: a later capture reuses this model only if it is this
/// capture exactly, and is advanced from it only if it is this capture plus events this handle
/// itself appended. Anything else, including one changed byte, is built again from nothing.
struct Verified {
    capture: TenantCapture,
    model: Arc<CapturedModel>,
}

impl std::fmt::Debug for EventlogRecordedStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EventlogRecordedStore")
            .field("authority", &self.authority)
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

impl EventlogRecordedStore {
    /// Exact immutable logical/physical authority verified by this handle.
    #[must_use]
    pub const fn authority(&self) -> &Authority {
        &self.authority
    }

    /// Provider work this handle has done since it was opened.
    #[must_use]
    pub fn calls(&self) -> StoreCalls {
        StoreCalls {
            captures: self.native_captures.load(Ordering::Relaxed),
            model_builds: self.model_builds.load(Ordering::Relaxed),
            model_advances: self.model_advances.load(Ordering::Relaxed),
            records_decoded: self.records_decoded.load(Ordering::Relaxed),
        }
    }

    /// Opens an already provisioned, already attached store without mutating provider state.
    ///
    /// # Errors
    /// Refuses missing attachment, binding, generation, references, or exact index equality.
    pub async fn open(
        backend: Arc<dyn EventlogBackend>,
        authority: Authority,
        limits: CaptureLimits,
    ) -> Result<Self, AsyncStoreError> {
        authority.validate()?;
        let tenant = TenantId::new(authority.tenant.clone()).map_err(input_eventlog)?;
        if !backend.is_inline(PROJECTOR_NAME).await {
            return Err(integrity("the fixed inline projector is not attached"));
        }
        let store = Self {
            backend,
            authority,
            tenant,
            limits,
            native_captures: AtomicUsize::new(0),
            model_builds: AtomicUsize::new(0),
            model_advances: AtomicUsize::new(0),
            records_decoded: AtomicUsize::new(0),
            verified: Mutex::new(None),
            own_events: Mutex::new(BTreeSet::new()),
        };
        let model = store.capture_model().await?;
        if model.binding.is_none() {
            return Err(integrity("the tenant has no authoritative binding"));
        }
        Ok(store)
    }

    /// Returns a per-operation facade supplying facts only when a new physical write is needed.
    #[must_use]
    pub fn operation(&self, context: EventlogOperationContext) -> EventlogOperationStore<'_> {
        EventlogOperationStore {
            store: self,
            context,
        }
    }

    /// Rebuilds the fixed registered inline projector and verifies a fresh complete capture.
    ///
    /// # Errors
    /// Provider failure or any post-rebuild authority/index mismatch.
    pub async fn rebuild_indexes(
        &self,
    ) -> Result<eventlog_core::InlineRebuildResult, AsyncStoreError> {
        let result = self
            .backend
            .rebuild_inline_projection(PROJECTOR_NAME, &self.tenant)
            .await
            .map_err(map_read_error)?;
        self.capture_model().await?;
        Ok(result)
    }

    async fn capture(&self) -> Result<TenantCapture, AsyncStoreError> {
        self.native_captures.fetch_add(1, Ordering::Relaxed);
        let capture = self
            .backend
            .capture_tenant(&self.tenant, projection_specs(), self.limits)
            .await
            .map_err(map_capture)?;
        if capture.tenant != self.tenant
            || capture.stream_identity != self.authority.stream_identity
        {
            return Err(integrity("native capture substituted tenant generation"));
        }
        Ok(capture)
    }

    /// One fresh verified capture, and the model of exactly what it observed.
    ///
    /// Every call still asks the provider for a complete capture: that is the only evidence of
    /// where the head is, and the observation a tampered, truncated or substituted authority is
    /// refused from. What is no longer repeated is turning an observation already verified back
    /// into the same model. A capture equal to the verified one reuses its model; a capture that
    /// is the verified one plus events this handle's own appends returned is advanced by exactly
    /// those events; every other capture is built and verified whole, as before.
    async fn capture_model(&self) -> Result<Arc<CapturedModel>, AsyncStoreError> {
        let capture = self.capture().await?;
        self.model_of(capture)
    }

    fn model_of(&self, capture: TenantCapture) -> Result<Arc<CapturedModel>, AsyncStoreError> {
        let previous = {
            let mut verified = self
                .verified
                .lock()
                .map_err(|_| integrity("verified model lock poisoned"))?;
            if let Some(held) = verified.as_ref()
                && held.capture == capture
            {
                return Ok(Arc::clone(&held.model));
            }
            verified.take()
        };
        if let Some(previous) = previous
            && let Some(from) = self.appended_by_this_handle(&previous.capture, &capture)
        {
            let mut model = Arc::try_unwrap(previous.model).unwrap_or_else(|shared| {
                // A reader still holds the verified model. Advancing a copy costs a clone of the
                // model, which is still not a decode, hash or replay of a single record.
                (*shared).clone()
            });
            model.decoded = 0;
            if advance_model(&self.authority, &mut model, &capture, from).is_ok() {
                self.model_advances.fetch_add(1, Ordering::Relaxed);
                self.records_decoded
                    .fetch_add(model.decoded, Ordering::Relaxed);
                self.forget_own(&capture.events[from..]);
                return Ok(self.install(capture, model));
            }
            // An advance that refuses is not the answer: the whole build below is, so a refusal
            // reaches the caller exactly as the verifying path words it.
        }
        let model = build_model(&self.authority, &capture)?;
        self.model_builds.fetch_add(1, Ordering::Relaxed);
        self.records_decoded
            .fetch_add(model.decoded, Ordering::Relaxed);
        Ok(self.install(capture, model))
    }

    fn install(&self, capture: TenantCapture, model: CapturedModel) -> Arc<CapturedModel> {
        let model = Arc::new(model);
        if let Ok(mut verified) = self.verified.lock() {
            *verified = Some(Verified {
                capture,
                model: Arc::clone(&model),
            });
        }
        model
    }

    /// Where `current` continues `verified` with events only this handle appended, if it does.
    ///
    /// The verified prefix must be byte-identical — every event and every blob it bound — so the
    /// part of the model built from it is the part a whole build would build from it again.
    /// Projection rows are not compared here: the advance holds every row against the advanced
    /// model, exactly as a whole build does.
    fn appended_by_this_handle(
        &self,
        verified: &TenantCapture,
        current: &TenantCapture,
    ) -> Option<usize> {
        let from = verified.events.len();
        if current.tenant != verified.tenant
            || current.stream_identity != verified.stream_identity
            || current.events.len() < from
            || current.events[..from] != verified.events[..]
        {
            return None;
        }
        let own = self.own_events.lock().ok()?;
        if !current.events[from..]
            .iter()
            .all(|event| own.contains(&event.event_id))
        {
            return None;
        }
        drop(own);
        let bound: BTreeMap<&str, &[u8]> = current
            .blobs
            .iter()
            .map(|blob| (blob.digest.as_str(), blob.bytes.as_slice()))
            .collect();
        verified
            .blobs
            .iter()
            .all(|blob| bound.get(blob.digest.as_str()) == Some(&blob.bytes.as_slice()))
            .then_some(from)
    }

    /// Records the events one of this handle's own committed appends returned.
    fn remember_own(&self, result: &eventlog_core::AppendGroupResult) {
        if let Ok(mut own) = self.own_events.lock() {
            for append in &result.appends {
                for event in &append.events {
                    own.insert(event.event_id.clone());
                }
            }
        }
    }

    fn forget_own(&self, events: &[RecordedEvent]) {
        if let Ok(mut own) = self.own_events.lock() {
            for event in events {
                own.remove(&event.event_id);
            }
        }
    }
}

/// One operation-scoped writer and reader facade.
#[derive(Debug)]
pub struct EventlogOperationStore<'a> {
    store: &'a EventlogRecordedStore,
    context: EventlogOperationContext,
}

/// The executor's reads for one command share a verified capture. The writer clears it before
/// attempting an append, so replay and uncertain-outcome recovery acquire fresh authority.
#[cfg(feature = "sync-bridge")]
pub(crate) struct BatchReadStore<'a> {
    operation: EventlogOperationStore<'a>,
    model: Mutex<Option<Arc<CapturedModel>>>,
    #[cfg(test)]
    return_uncertain_after_commit: bool,
}

#[cfg(feature = "sync-bridge")]
impl<'a> BatchReadStore<'a> {
    pub(crate) fn new(operation: EventlogOperationStore<'a>) -> Self {
        Self {
            operation,
            model: Mutex::new(None),
            #[cfg(test)]
            return_uncertain_after_commit: false,
        }
    }

    async fn model(&self) -> Result<Arc<CapturedModel>, AsyncStoreError> {
        if let Some(model) = self.model.lock().expect("batch read lock").as_ref() {
            return Ok(Arc::clone(model));
        }
        let captured = self.operation.store.capture_model().await?;
        let mut slot = self.model.lock().expect("batch read lock");
        Ok(Arc::clone(slot.get_or_insert(captured)))
    }
}

#[cfg(feature = "sync-bridge")]
impl AsyncStateReader for BatchReadStore<'_> {
    fn load<'a>(
        &'a self,
        subject: &'a Subject,
    ) -> BoxFuture<'a, Result<Option<EntityInstance>, AsyncStoreError>> {
        Box::pin(async move {
            let model = self.model().await?;
            refuse_forked(&model, subject)?;
            Ok(model.terminals.get(subject).cloned())
        })
    }
}

#[cfg(feature = "sync-bridge")]
impl AsyncRecordedReader for BatchReadStore<'_> {
    fn lookup_record<'a>(
        &'a self,
        record_id: &'a str,
    ) -> BoxFuture<'a, Result<Option<RecordLookup>, AsyncStoreError>> {
        Box::pin(async move { Ok(self.model().await?.records.get(record_id).cloned()) })
    }

    fn lookup_batch<'a>(
        &'a self,
        key: &'a BatchKey,
    ) -> BoxFuture<'a, Result<Option<StoredBatch>, AsyncStoreError>> {
        Box::pin(async move { Ok(self.model().await?.batches.get(key).cloned()) })
    }

    fn history<'a>(
        &'a self,
        subject: &'a Subject,
    ) -> BoxFuture<'a, Result<SubjectHistory, AsyncStoreError>> {
        Box::pin(async move {
            Ok(self
                .model()
                .await?
                .histories
                .get(subject)
                .cloned()
                .unwrap_or_else(|| SubjectHistory {
                    subject: subject.clone(),
                    origin: HistoryOrigin::Genesis,
                    records: Vec::new(),
                }))
        })
    }

    fn complete_snapshot<'a>(
        &'a self,
        scope: &'a str,
    ) -> BoxFuture<'a, Result<CompleteStoreSnapshot, AsyncStoreError>> {
        self.operation.complete_snapshot(scope)
    }
}

#[cfg(feature = "sync-bridge")]
impl AsyncRecordedWriter for BatchReadStore<'_> {
    fn append<'a>(
        &'a self,
        request: AppendRequest,
    ) -> BoxFuture<'a, Result<AppendOutcome, WriteFailure>> {
        Box::pin(async move {
            self.model.lock().expect("batch read lock").take();
            #[cfg(test)]
            let key = request.key.clone();
            let outcome = self.operation.append(request).await;
            #[cfg(test)]
            if self.return_uncertain_after_commit
                && matches!(&outcome, Ok(AppendOutcome::Committed { .. }))
            {
                return Err(WriteFailure::Uncertain {
                    key: key.expect("committed request has a key"),
                    cause: "test-only lost append reply".into(),
                });
            }
            outcome
        })
    }
}

impl AsyncStateReader for EventlogRecordedStore {
    fn load<'a>(
        &'a self,
        subject: &'a Subject,
    ) -> BoxFuture<'a, Result<Option<EntityInstance>, AsyncStoreError>> {
        Box::pin(async move {
            let model = self.capture_model().await?;
            refuse_forked(&model, subject)?;
            Ok(model.terminals.get(subject).cloned())
        })
    }
}

impl AsyncRecordedReader for EventlogRecordedStore {
    fn lookup_record<'a>(
        &'a self,
        record_id: &'a str,
    ) -> BoxFuture<'a, Result<Option<RecordLookup>, AsyncStoreError>> {
        Box::pin(async move { Ok(self.capture_model().await?.records.get(record_id).cloned()) })
    }
    fn lookup_batch<'a>(
        &'a self,
        key: &'a BatchKey,
    ) -> BoxFuture<'a, Result<Option<StoredBatch>, AsyncStoreError>> {
        Box::pin(async move { Ok(self.capture_model().await?.batches.get(key).cloned()) })
    }
    fn history<'a>(
        &'a self,
        subject: &'a Subject,
    ) -> BoxFuture<'a, Result<SubjectHistory, AsyncStoreError>> {
        Box::pin(async move {
            Ok(self
                .capture_model()
                .await?
                .histories
                .get(subject)
                .cloned()
                .unwrap_or_else(|| SubjectHistory {
                    subject: subject.clone(),
                    origin: HistoryOrigin::Genesis,
                    records: Vec::new(),
                }))
        })
    }
    fn complete_snapshot<'a>(
        &'a self,
        scope: &'a str,
    ) -> BoxFuture<'a, Result<CompleteStoreSnapshot, AsyncStoreError>> {
        Box::pin(async move {
            if scope != self.authority.logical_scope {
                return Err(AsyncStoreError::InvalidInput(
                    "requested scope differs from bound authority".into(),
                ));
            }
            let model = self.capture_model().await?;
            let histories = model
                .histories
                .iter()
                .map(|(subject, history)| {
                    let terminal = model
                        .terminals
                        .get(subject)
                        .cloned()
                        .ok_or_else(|| integrity("subject history has no terminal state"))?;
                    Ok(SubjectSnapshot {
                        history: history.clone(),
                        terminal,
                    })
                })
                .collect::<Result<Vec<_>, AsyncStoreError>>()?;
            Ok(CompleteStoreSnapshot {
                scope: scope.to_owned(),
                coverage: StoreCoverage::CompleteSnapshot,
                histories,
            })
        })
    }
}

impl AsyncStateReader for EventlogOperationStore<'_> {
    fn load<'a>(
        &'a self,
        subject: &'a Subject,
    ) -> BoxFuture<'a, Result<Option<EntityInstance>, AsyncStoreError>> {
        self.store.load(subject)
    }
}
impl AsyncRecordedReader for EventlogOperationStore<'_> {
    fn lookup_record<'a>(
        &'a self,
        id: &'a str,
    ) -> BoxFuture<'a, Result<Option<RecordLookup>, AsyncStoreError>> {
        self.store.lookup_record(id)
    }
    fn lookup_batch<'a>(
        &'a self,
        key: &'a BatchKey,
    ) -> BoxFuture<'a, Result<Option<StoredBatch>, AsyncStoreError>> {
        self.store.lookup_batch(key)
    }
    fn history<'a>(
        &'a self,
        subject: &'a Subject,
    ) -> BoxFuture<'a, Result<SubjectHistory, AsyncStoreError>> {
        self.store.history(subject)
    }
    fn complete_snapshot<'a>(
        &'a self,
        scope: &'a str,
    ) -> BoxFuture<'a, Result<CompleteStoreSnapshot, AsyncStoreError>> {
        self.store.complete_snapshot(scope)
    }
}

impl AsyncRecordedWriter for EventlogOperationStore<'_> {
    fn append<'a>(
        &'a self,
        request: AppendRequest,
    ) -> BoxFuture<'a, Result<AppendOutcome, WriteFailure>> {
        Box::pin(async move { self.append_inner(request).await })
    }
}

impl EventlogOperationStore<'_> {
    /// Executes one recorded batch with a verified read view scoped to this call.
    ///
    /// # Errors
    /// Invalid input, kernel refusal, conflicting or corrupt evidence, or append failure.
    #[cfg(feature = "sync-bridge")]
    pub(crate) async fn execute_batch(
        self,
        registry: &Registry,
        key: BatchKey,
        actions: Vec<BatchAction>,
    ) -> Result<AppendOutcome, ExecutionError> {
        let reads = BatchReadStore::new(self);
        Executor::new(registry, &reads).batch(key, actions).await
    }

    async fn append_inner(&self, request: AppendRequest) -> Result<AppendOutcome, WriteFailure> {
        request.validate().map_err(WriteFailure::NotCommitted)?;
        let Some(key) = request.key.clone() else {
            return Ok(AppendOutcome::Empty);
        };
        let model = self
            .store
            .capture_model()
            .await
            .map_err(WriteFailure::NotCommitted)?;
        if let Some(outcome) =
            recover_from(&model, &key, &request).map_err(WriteFailure::NotCommitted)?
        {
            return Ok(outcome);
        }
        // The expected heads come from the capture recovery just verified. Uploading this call's
        // blobs moves no head, and a head another writer moves in the meantime is refused by the
        // provider's expected-version check and the guard, exactly as one moved after a second
        // capture here would have been.
        let mut heads: BTreeMap<Subject, Option<u64>> = BTreeMap::new();
        for member in &request.members {
            // A merge decision is the one write a forked subject admits.
            if member.merge.is_none() {
                refuse_forked(&model, &member.entry.subject())
                    .map_err(WriteFailure::NotCommitted)?;
            }
            heads.entry(member.entry.subject()).or_insert_with(|| {
                model
                    .histories
                    .get(&member.entry.subject())
                    .and_then(|h| h.records.last().map(|r| r.position.subject))
                    .or_else(|| {
                        model.histories.get(&member.entry.subject()).and_then(|h| {
                            matches!(h.origin, HistoryOrigin::Imported(_)).then_some(1)
                        })
                    })
            });
        }
        // Released before the commit, so the post-commit verification advances the model in place.
        drop(model);
        let batch_bytes =
            batch_comparison_bytes(&key, &request.members).map_err(WriteFailure::NotCommitted)?;
        let batch_digest =
            framed_key(BATCH_BLOB_DOMAIN, &batch_bytes).map_err(WriteFailure::NotCommitted)?;
        let mut wrappers = Vec::with_capacity(request.members.len());
        for (index, member) in request.members.iter().enumerate() {
            let record_bytes =
                record_comparison_bytes(&member.entry).map_err(WriteFailure::NotCommitted)?;
            let record_digest = framed_key(RECORD_BLOB_DOMAIN, &record_bytes)
                .map_err(WriteFailure::NotCommitted)?;
            let request_digest = framed_key(REQUEST_BLOB_DOMAIN, &member.request_bytes)
                .map_err(WriteFailure::NotCommitted)?;
            let wrapper = RecordedEntryWrapper {
                authority: self.store.authority.clone(),
                batch_blob: batch_digest.clone(),
                batch_key: crate::encoding::BatchKeyWire::from(&key),
                member_index: u64::try_from(index).map_err(|_| {
                    WriteFailure::NotCommitted(AsyncStoreError::PositionExhausted {
                        domain: "batch member".into(),
                    })
                })?,
                record_blob: record_digest.clone(),
                request_blob: request_digest.clone(),
                subject: SubjectWire::from(&member.entry.subject()),
            };
            let wrapper_bytes = encode_entry(&wrapper).map_err(WriteFailure::NotCommitted)?;
            let wrapper_digest = framed_key(ENTRY_BLOB_DOMAIN, &wrapper_bytes)
                .map_err(WriteFailure::NotCommitted)?;
            wrappers.push((
                wrapper,
                wrapper_bytes,
                wrapper_digest,
                record_bytes,
                record_digest,
            ));
        }
        let command_key = key_for_value("er.eventlog.batch-command-key/1", json!({"authority":self.store.authority,"batch_key":crate::encoding::BatchKeyWire::from(&key)})).map_err(WriteFailure::NotCommitted)?;
        let meta = self
            .context
            .meta(command_key, batch_digest.clone())
            .map_err(WriteFailure::NotCommitted)?;
        self.store
            .backend
            .put_blob(&self.store.tenant, &batch_digest, &batch_bytes)
            .await
            .map_err(|e| WriteFailure::NotCommitted(map_put_error(e)))?;
        for (wrapper, wrapper_bytes, wrapper_digest, record_bytes, record_digest) in &wrappers {
            self.store
                .backend
                .put_blob(&self.store.tenant, record_digest, record_bytes)
                .await
                .map_err(|e| WriteFailure::NotCommitted(map_put_error(e)))?;
            let member = &request.members
                [usize::try_from(wrapper.member_index).expect("checked from usize")];
            self.store
                .backend
                .put_blob(
                    &self.store.tenant,
                    &wrapper.request_blob,
                    &member.request_bytes,
                )
                .await
                .map_err(|e| WriteFailure::NotCommitted(map_put_error(e)))?;
            self.store
                .backend
                .put_blob(&self.store.tenant, wrapper_digest, wrapper_bytes)
                .await
                .map_err(|e| WriteFailure::NotCommitted(map_put_error(e)))?;
        }
        let mut appends = Vec::with_capacity(wrappers.len());
        for ((_, _, wrapper_digest, _, _), member) in wrappers.iter().zip(&request.members) {
            let subject = member.entry.subject();
            let head = heads.get_mut(&subject).expect("subject head was collected");
            let expected = match &member.merge {
                Some(merge) => Expected::Merge(eventlog_core::HeadSetDigest::of(&merge.heads)),
                None => head.map_or(Expected::NoStream, Expected::Exact),
            };
            *head = Some(head.unwrap_or(0).checked_add(1).ok_or_else(|| {
                WriteFailure::NotCommitted(AsyncStoreError::PositionExhausted {
                    domain: "subject event stream".into(),
                })
            })?);
            appends.push(StreamAppend {
                stream: StreamId::new(
                    self.store.tenant.clone(),
                    "er.subject",
                    subject_stream_id(&self.store.authority, &subject)
                        .map_err(WriteFailure::NotCommitted)?,
                )
                .map_err(|e| WriteFailure::NotCommitted(input_eventlog(e)))?,
                expected,
                events: vec![
                    NewEvent::new("er.recorded_entry", 1, json!({"blob":wrapper_digest}))
                        .map_err(|e| WriteFailure::NotCommitted(input_eventlog(e)))?,
                ],
            });
        }
        let group = AppendGroup {
            tenant: self.store.tenant.clone(),
            appends,
            meta,
        };
        group
            .fingerprint()
            .map_err(|e| WriteFailure::NotCommitted(input_eventlog(e)))?;
        let slot = Arc::new(Mutex::new(None));
        let guard = Arc::new(AppendGuard {
            authority: self.store.authority.clone(),
            tenant: self.store.tenant.clone(),
            key: key.clone(),
            request: request.clone(),
            slot: slot.clone(),
        });
        match self.store.backend.append_group_guarded(&group, guard).await {
            Ok(result) => {
                if validate_group_result(&result, wrappers.len()).is_err() {
                    return Err(WriteFailure::Uncertain {
                        key,
                        cause: "Eventlog reported a commit with an invalid result shape".into(),
                    });
                }
                self.store.remember_own(&result);
                committed_outcome(self.store, &key, false, &result)
                    .await
                    .map_err(|_| {
                    WriteFailure::Uncertain {
                        key,
                        cause: "Eventlog reported a commit but authoritative verification is unavailable"
                            .into(),
                    }
                })
            }
            Err(error) => {
                self.resolve_append_error(error, &key, &request, &slot)
                    .await
            }
        }
    }

    async fn resolve_append_error(
        &self,
        error: EventLogError,
        key: &BatchKey,
        request: &AppendRequest,
        slot: &Arc<Mutex<Option<GuardRefusal>>>,
    ) -> Result<AppendOutcome, WriteFailure> {
        match error {
            EventLogError::GuardRefused { code } => {
                let refusal = slot
                    .lock()
                    .map_err(|_| {
                        WriteFailure::NotCommitted(integrity("guard refusal slot was poisoned"))
                    })?
                    .take();
                match refusal {
                    Some(refusal)
                        if refusal.code.as_str() == code
                            && refusal.code.matches(&refusal.error) =>
                    {
                        Err(WriteFailure::NotCommitted(refusal.error))
                    }
                    _ => Err(WriteFailure::NotCommitted(integrity(
                        "guard refusal code and typed slot disagree",
                    ))),
                }
            }
            EventLogError::UnknownCommit => match recover_append(self.store, key, request).await {
                Ok(Some(outcome)) => Ok(outcome),
                Ok(None) | Err(_) => Err(WriteFailure::Uncertain {
                    key: key.clone(),
                    cause:
                        "Eventlog commit outcome is unknown and semantic recovery is unavailable"
                            .into(),
                }),
            },
            EventLogError::Conflict { .. } => match recover_append(self.store, key, request).await {
                Ok(Some(outcome)) => Ok(outcome),
                Ok(None) => Box::pin(self.append_inner(request.clone())).await,
                Err(_) => Err(WriteFailure::Uncertain {
                    key: key.clone(),
                    cause:
                        "Eventlog physical conflict cannot be resolved against semantic authority"
                            .into(),
                }),
            },
            EventLogError::IdempotencyMismatch { .. } => {
                match recover_append(self.store, key, request).await {
                    Ok(Some(outcome)) => Ok(outcome),
                    Ok(None) => Err(WriteFailure::NotCommitted(integrity(
                        "Eventlog command identity exists without matching ER authority",
                    ))),
                    Err(_) => Err(WriteFailure::Uncertain {
                        key: key.clone(),
                        cause: "Eventlog command identity cannot be resolved against semantic authority"
                            .into(),
                    }),
                }
            }
            other => Err(WriteFailure::NotCommitted(map_append_error(other))),
        }
    }
}

async fn recover_append(
    store: &EventlogRecordedStore,
    key: &BatchKey,
    request: &AppendRequest,
) -> Result<Option<AppendOutcome>, AsyncStoreError> {
    let model = store.capture_model().await?;
    recover_from(&model, key, request)
}

/// What a verified model already says about this request: committed, conflicting, or absent.
fn recover_from(
    model: &CapturedModel,
    key: &BatchKey,
    request: &AppendRequest,
) -> Result<Option<AppendOutcome>, AsyncStoreError> {
    if let Some(batch) = model.batches.get(key) {
        let expected = batch_comparison_bytes(key, &request.members)?;
        if batch.comparison_bytes == expected {
            return Ok(Some(AppendOutcome::Committed {
                receipt: batch.receipt.clone(),
                replayed: true,
            }));
        }
        return Err(AsyncStoreError::BatchConflict { key: key.clone() });
    }
    let mut occupied = Vec::new();
    for (index, member) in request.members.iter().enumerate() {
        if let Some(found) = model.records.get(member.entry.record_id()) {
            if let (BatchKey::SingleRecord(_), RecordLookup::Committed(record)) = (key, found)
                && record.entry == member.entry
                && record.request_bytes == member.request_bytes
            {
                return Ok(Some(AppendOutcome::Committed {
                    receipt: CommitReceipt::Single(record.receipt.clone()),
                    replayed: true,
                }));
            }
            if let (BatchKey::SingleRecord(_), RecordLookup::Imported(evidence)) = (key, found)
                && evidence.entry == member.entry
            {
                let history = model
                    .histories
                    .get(&member.entry.subject())
                    .ok_or_else(|| {
                        corrupt(
                            &member.entry.subject(),
                            "imported lookup has no subject history",
                        )
                    })?;
                let assurance = verify_subject_history(
                    history,
                    model
                        .terminals
                        .get(&member.entry.subject())
                        .ok_or_else(|| {
                            corrupt(&member.entry.subject(), "imported history has no terminal")
                        })?,
                )?;
                return Ok(Some(AppendOutcome::Historical {
                    evidence: evidence.clone(),
                    assurance,
                }));
            }
            occupied.push(u64::try_from(index).map_err(|_| {
                AsyncStoreError::PositionExhausted {
                    domain: "batch member".into(),
                }
            })?);
        }
    }
    if let BatchKey::SingleRecord(record_id) = key {
        if !occupied.is_empty() {
            return Err(AsyncStoreError::RecordConflict {
                record_id: record_id.clone(),
            });
        }
    } else if !occupied.is_empty() {
        return Err(AsyncStoreError::PreviouslyRecordedBatchEntries { indices: occupied });
    }
    Ok(None)
}

async fn committed_outcome(
    store: &EventlogRecordedStore,
    key: &BatchKey,
    replayed: bool,
    result: &eventlog_core::AppendGroupResult,
) -> Result<AppendOutcome, AsyncStoreError> {
    let model = store.capture_model().await?;
    let batch = model
        .batches
        .get(key)
        .ok_or_else(|| integrity("committed group is absent from authoritative capture"))?;
    for (saved, append) in batch.records.iter().zip(&result.appends) {
        let returned = append
            .events
            .first()
            .ok_or_else(|| integrity("committed group result has no event"))?;
        if model.record_physical.get(saved.entry.record_id()) != Some(&physical(returned)) {
            return Err(integrity(
                "committed group result differs from authoritative physical coordinates",
            ));
        }
    }
    Ok(AppendOutcome::Committed {
        receipt: batch.receipt.clone(),
        replayed,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GuardRefusalCode {
    RevisionConflict,
    RecordConflict,
    BatchConflict,
    PreviouslyRecordedBatchEntries,
    CorruptHistory,
    ProviderIntegrity,
}
impl GuardRefusalCode {
    fn as_str(self) -> &'static str {
        match self {
            Self::RevisionConflict => "er_revision_conflict",
            Self::RecordConflict => "er_record_conflict",
            Self::BatchConflict => "er_batch_conflict",
            Self::PreviouslyRecordedBatchEntries => "er_previously_recorded_batch_entries",
            Self::CorruptHistory => "er_corrupt_history",
            Self::ProviderIntegrity => "er_provider_integrity",
        }
    }
    fn for_error(error: &AsyncStoreError) -> Self {
        match error {
            AsyncStoreError::RevisionConflict { .. } => Self::RevisionConflict,
            AsyncStoreError::RecordConflict { .. } => Self::RecordConflict,
            AsyncStoreError::BatchConflict { .. } => Self::BatchConflict,
            AsyncStoreError::PreviouslyRecordedBatchEntries { .. } => {
                Self::PreviouslyRecordedBatchEntries
            }
            AsyncStoreError::CorruptHistory { .. } => Self::CorruptHistory,
            _ => Self::ProviderIntegrity,
        }
    }
    fn matches(self, error: &AsyncStoreError) -> bool {
        Self::for_error(error) == self
    }
}
struct GuardRefusal {
    code: GuardRefusalCode,
    error: AsyncStoreError,
}

struct AppendGuard {
    authority: Authority,
    tenant: TenantId,
    key: BatchKey,
    request: AppendRequest,
    slot: Arc<Mutex<Option<GuardRefusal>>>,
}

impl Guard for AppendGuard {
    fn check<'a>(
        &'a self,
        store: &'a mut dyn ProjectionStore,
    ) -> eventlog_core::BoxFuture<'a, Result<(), EventLogError>> {
        Box::pin(async move {
            let result = self.check_inner(store).await;
            match result {
                Ok(()) => Ok(()),
                Err(GuardCheckError::Store(error)) => Err(error),
                Err(GuardCheckError::Domain(error)) => {
                    let code = GuardRefusalCode::for_error(&error);
                    let mut slot = self
                        .slot
                        .lock()
                        .map_err(|_| EventLogError::Invalid("guard slot poisoned".into()))?;
                    if slot.is_some() {
                        return Err(EventLogError::Invalid("guard slot written twice".into()));
                    }
                    *slot = Some(GuardRefusal { code, error });
                    Err(EventLogError::GuardRefused {
                        code: code.as_str().into(),
                    })
                }
            }
        })
    }
}

enum GuardCheckError {
    Store(EventLogError),
    Domain(AsyncStoreError),
}
impl From<EventLogError> for GuardCheckError {
    fn from(value: EventLogError) -> Self {
        Self::Store(value)
    }
}
impl AppendGuard {
    async fn check_inner(&self, store: &mut dyn ProjectionStore) -> Result<(), GuardCheckError> {
        let mut locks: Vec<(u8, String, &'static eventlog_core::ProjectionSpec)> =
            vec![(0, "singleton".into(), binding_spec())];
        locks.push((
            1,
            physical_batch_key(&self.authority, &self.key).map_err(GuardCheckError::Domain)?,
            batch_spec(),
        ));
        for member in &self.request.members {
            locks.push((
                2,
                record_key(&self.authority, member.entry.record_id())
                    .map_err(GuardCheckError::Domain)?,
                record_spec(),
            ));
            locks.push((
                3,
                subject_key(&self.authority, &member.entry.subject())
                    .map_err(GuardCheckError::Domain)?,
                subject_spec(),
            ));
        }
        locks.sort_by(|a, b| (a.0, a.1.as_bytes()).cmp(&(b.0, b.1.as_bytes())));
        locks.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
        let mut rows = BTreeMap::new();
        for (rank, key, spec) in locks {
            let row = store.get_for_update(spec, &self.tenant, &key).await?;
            rows.insert((rank, key), row);
        }
        let binding = rows
            .get(&(0, "singleton".into()))
            .and_then(Option::as_ref)
            .ok_or_else(|| GuardCheckError::Domain(integrity("binding row is absent")))?;
        let body =
            tagged_body(binding, "er.eventlog.binding-index/1").map_err(GuardCheckError::Store)?;
        let found: Authority = serde_json::from_value(body["authority"].clone())
            .map_err(|e| GuardCheckError::Domain(integrity(e.to_string())))?;
        if found != self.authority {
            return Err(GuardCheckError::Domain(integrity(
                "binding row authority changed",
            )));
        }
        let batch_key =
            physical_batch_key(&self.authority, &self.key).map_err(GuardCheckError::Domain)?;
        if rows.get(&(1, batch_key)).is_some_and(Option::is_some) {
            return Err(GuardCheckError::Domain(AsyncStoreError::BatchConflict {
                key: self.key.clone(),
            }));
        }
        let mut occupied = Vec::new();
        for (index, member) in self.request.members.iter().enumerate() {
            let key = record_key(&self.authority, member.entry.record_id())
                .map_err(GuardCheckError::Domain)?;
            if rows.get(&(2, key)).is_some_and(Option::is_some) {
                occupied.push(u64::try_from(index).map_err(|_| {
                    GuardCheckError::Domain(AsyncStoreError::PositionExhausted {
                        domain: "batch member".into(),
                    })
                })?);
            }
        }
        if !occupied.is_empty() {
            let error = match &self.key {
                BatchKey::SingleRecord(id) => AsyncStoreError::RecordConflict {
                    record_id: id.clone(),
                },
                BatchKey::Named(_) => {
                    AsyncStoreError::PreviouslyRecordedBatchEntries { indices: occupied }
                }
            };
            return Err(GuardCheckError::Domain(error));
        }
        let mut overlay: BTreeMap<Subject, Option<EntityInstance>> = BTreeMap::new();
        for member in &self.request.members {
            let subject = member.entry.subject();
            if !overlay.contains_key(&subject) {
                let key =
                    subject_key(&self.authority, &subject).map_err(GuardCheckError::Domain)?;
                let state = match rows.get(&(3, key)).and_then(Option::as_ref) {
                    Some(row) => Some(resolve_subject_state(row, store).await?),
                    None => None,
                };
                overlay.insert(subject.clone(), state);
            }
            // A merge decision was decided on the state its first head reached, which is not the
            // subject row: that row follows whichever branch the store replayed last.
            let base = member.merge.as_ref().map(|merge| &merge.base);
            let next = validate_entry_against_state(
                &member.entry,
                member.expect,
                base.or_else(|| overlay.get(&subject).and_then(Option::as_ref)),
            )
            .map_err(GuardCheckError::Domain)?;
            overlay.insert(subject, next);
        }
        Ok(())
    }
}

async fn resolve_subject_state(
    row: &Value,
    store: &mut dyn ProjectionStore,
) -> Result<EntityInstance, GuardCheckError> {
    let body = tagged_body(row, "er.eventlog.subject-index/1").map_err(GuardCheckError::Store)?;
    let source = body
        .get("state_source")
        .and_then(Value::as_object)
        .ok_or_else(|| GuardCheckError::Domain(integrity("subject row lacks state source")))?;
    let digest = source
        .get("record_blob")
        .or_else(|| source.get("anchor_blob"))
        .and_then(Value::as_str)
        .ok_or_else(|| GuardCheckError::Domain(integrity("state source has no blob")))?;
    let bytes = store
        .get_blob(digest)
        .await?
        .ok_or_else(|| GuardCheckError::Domain(integrity("state source blob is missing")))?;
    match source.get("kind").and_then(Value::as_str) {
        Some("decision") => match decode_record(&bytes).map_err(GuardCheckError::Domain)? {
            entity_store::asynchronous::RecordedEntry::Decision(commit) => Ok(commit.instance),
            _ => Err(GuardCheckError::Domain(integrity(
                "state source is not a decision",
            ))),
        },
        Some("anchor") => Ok(decode_anchor(&bytes)
            .map_err(GuardCheckError::Domain)?
            .instance),
        _ => Err(GuardCheckError::Domain(integrity("unknown state source"))),
    }
}

/// The revision a destination can point at for a subject it already answers for.
///
/// `None` means the destination holds nothing for this subject — which is the absence of a
/// conflict, not a conflict whose value is unknown. Callers use that distinction to keep
/// `expected: Absent, found: None` — a report that a subject was expected absent and found
/// absent — from being constructible.
fn occupied_revision(model: &CapturedModel, subject: &Subject) -> Option<u64> {
    model
        .terminals
        .get(subject)
        .map(|instance| instance.revision)
        .or_else(|| match model.histories.get(subject)?.origin {
            HistoryOrigin::Imported(ref anchor) => Some(anchor.instance.revision),
            HistoryOrigin::Genesis => None,
        })
}

fn validate_group_result(
    result: &eventlog_core::AppendGroupResult,
    members: usize,
) -> Result<(), AsyncStoreError> {
    if result.appends.len() != members || result.appends.iter().any(|a| a.events.len() != 1) {
        return Err(integrity(
            "provider group result does not reproduce member events",
        ));
    }
    Ok(())
}

fn map_put_error(error: EventLogError) -> AsyncStoreError {
    match error {
        EventLogError::Invalid(v) => AsyncStoreError::ProviderIntegrity {
            provider: "eventlog".into(),
            detail: v,
        },
        other => map_read_error(other),
    }
}
fn map_append_error(error: EventLogError) -> AsyncStoreError {
    match error {
        EventLogError::Invalid(v) => integrity(v),
        EventLogError::Overloaded => AsyncStoreError::Unreachable {
            provider: "eventlog".into(),
            detail: "resource budget exhausted".into(),
        },
        EventLogError::Closed => AsyncStoreError::Unreachable {
            provider: "eventlog".into(),
            detail: "closed".into(),
        },
        EventLogError::Deadline { operation } => AsyncStoreError::Unreachable {
            provider: "eventlog".into(),
            detail: format!("deadline during {operation}"),
        },
        EventLogError::CausationDepthExceeded { .. } => {
            integrity("provider rejected locally validated causation depth")
        }
        EventLogError::NotFound => integrity("provider lost required append authority"),
        EventLogError::Backend(v) => AsyncStoreError::Backend(v),
        other => integrity(other.to_string()),
    }
}

/// Administrative binding capability, separate from runtime open.
pub trait AsyncBindingProvisioner: Send + Sync {
    /// Establishes one immutable binding or recovers its exact winner.
    fn provision_binding<'a>(
        &'a self,
        authority: Authority,
        context: EventlogOperationContext,
    ) -> BoxFuture<'a, Result<ProvisionBindingOutcome, ProvisionBindingFailure>>;
    /// Reads and verifies one binding without mutation.
    fn recover_binding<'a>(
        &'a self,
        authority: Authority,
    ) -> BoxFuture<'a, Result<Option<ProvisionBindingOutcome>, ProvisionBindingFailure>>;
}

/// Unbound administrative owner used under caller-established maintenance exclusion.
pub struct EventlogBindingProvisioner {
    backend: Arc<dyn EventlogBackend>,
    limits: CaptureLimits,
}

impl EventlogBindingProvisioner {
    /// Constructs a non-initializing provisioner over a prepared native handle.
    #[must_use]
    pub fn new(backend: Arc<dyn EventlogBackend>, limits: CaptureLimits) -> Self {
        Self { backend, limits }
    }
}

/// Settled binding coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvisionBindingOutcome {
    /// Exact immutable authority.
    pub authority: Authority,
    /// Actual provider-minted coordinates.
    pub physical: PhysicalRef,
    /// Whether an already committed binding was recovered.
    pub replayed: bool,
}

/// Binding failure with conflict and uncertainty preserved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvisionBindingFailure {
    /// This invocation definitely did not publish a binding.
    NotCommitted(AsyncStoreError),
    /// Another immutable authority won.
    Conflict {
        /// Authority the caller attempted to bind.
        requested: Authority,
        /// Already committed immutable authority.
        found: Authority,
    },
    /// Publication may have happened and recovery could not settle it.
    Uncertain {
        /// Authority whose publication is unresolved.
        authority: Authority,
        /// Recovery condition that prevented a conclusion.
        cause: ProvisionBindingUncertainty,
    },
}

/// Why a binding result is unresolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvisionBindingUncertainty {
    /// The provider may have committed before losing its reply.
    UnknownCommit,
    /// A complete authoritative recovery observation was unavailable.
    RecoveryUnavailable,
}

impl AsyncBindingProvisioner for EventlogBindingProvisioner {
    fn recover_binding<'a>(
        &'a self,
        authority: Authority,
    ) -> BoxFuture<'a, Result<Option<ProvisionBindingOutcome>, ProvisionBindingFailure>> {
        Box::pin(async move { self.recover_inner(authority).await })
    }
    fn provision_binding<'a>(
        &'a self,
        authority: Authority,
        context: EventlogOperationContext,
    ) -> BoxFuture<'a, Result<ProvisionBindingOutcome, ProvisionBindingFailure>> {
        Box::pin(async move {
            authority
                .validate()
                .map_err(ProvisionBindingFailure::NotCommitted)?;
            if let Some(outcome) = self.recover_inner(authority.clone()).await? {
                return Ok(outcome);
            }
            let tenant = TenantId::new(authority.tenant.clone())
                .map_err(|e| ProvisionBindingFailure::NotCommitted(input_eventlog(e)))?;
            let bytes =
                encode_binding(&authority).map_err(ProvisionBindingFailure::NotCommitted)?;
            let digest = framed_key(BINDING_BLOB_DOMAIN, &bytes)
                .map_err(ProvisionBindingFailure::NotCommitted)?;
            let meta = context
                .meta("er.binding/1".into(), digest.clone())
                .map_err(ProvisionBindingFailure::NotCommitted)?;
            self.backend
                .put_blob(&tenant, &digest, &bytes)
                .await
                .map_err(|e| ProvisionBindingFailure::NotCommitted(map_put_error(e)))?;
            let group = AppendGroup {
                tenant: tenant.clone(),
                appends: vec![StreamAppend {
                    stream: StreamId::new(tenant.clone(), "er.binding", "singleton")
                        .map_err(|e| ProvisionBindingFailure::NotCommitted(input_eventlog(e)))?,
                    expected: Expected::NoStream,
                    events: vec![
                        NewEvent::new("er.binding", 1, json!({"blob":digest})).map_err(|e| {
                            ProvisionBindingFailure::NotCommitted(input_eventlog(e))
                        })?,
                    ],
                }],
                meta,
            };
            let slot = Arc::new(Mutex::new(None));
            let guard = Arc::new(BindingGuard {
                authority: authority.clone(),
                tenant: tenant.clone(),
                slot: slot.clone(),
            });
            match self.backend.append_group_guarded(&group, guard).await {
                Ok(result) => {
                    if validate_group_result(&result, 1).is_err() {
                        return Err(ProvisionBindingFailure::Uncertain {
                            authority,
                            cause: ProvisionBindingUncertainty::RecoveryUnavailable,
                        });
                    }
                    let mut settled = match self.recover_inner(authority.clone()).await {
                        Ok(Some(settled)) => settled,
                        Ok(None) | Err(_) => {
                            return Err(ProvisionBindingFailure::Uncertain {
                                authority,
                                cause: ProvisionBindingUncertainty::RecoveryUnavailable,
                            });
                        }
                    };
                    if settled.physical != physical(&result.appends[0].events[0]) {
                        return Err(ProvisionBindingFailure::Uncertain {
                            authority,
                            cause: ProvisionBindingUncertainty::RecoveryUnavailable,
                        });
                    }
                    settled.replayed = result.deduplicated;
                    Ok(settled)
                }
                Err(EventLogError::UnknownCommit) => {
                    match self.recover_inner(authority.clone()).await {
                        Ok(Some(mut outcome)) => {
                            outcome.replayed = true;
                            Ok(outcome)
                        }
                        Err(conflict @ ProvisionBindingFailure::Conflict { .. }) => Err(conflict),
                        Ok(None) | Err(_) => Err(ProvisionBindingFailure::Uncertain {
                            authority,
                            cause: ProvisionBindingUncertainty::UnknownCommit,
                        }),
                    }
                }
                Err(
                    EventLogError::Conflict { .. }
                    | EventLogError::IdempotencyMismatch { .. }
                    | EventLogError::GuardRefused { .. },
                ) => match self.recover_inner(authority.clone()).await {
                    Ok(Some(outcome)) => Ok(outcome),
                    Ok(None) => Err(ProvisionBindingFailure::NotCommitted(integrity(
                        "binding command conflicted without binding authority",
                    ))),
                    Err(error) => Err(error),
                },
                Err(error) => Err(ProvisionBindingFailure::NotCommitted(map_append_error(
                    error,
                ))),
            }
        })
    }
}

impl EventlogBindingProvisioner {
    #[allow(clippy::result_large_err)]
    async fn recover_inner(
        &self,
        authority: Authority,
    ) -> Result<Option<ProvisionBindingOutcome>, ProvisionBindingFailure> {
        authority
            .validate()
            .map_err(ProvisionBindingFailure::NotCommitted)?;
        let tenant = TenantId::new(authority.tenant.clone())
            .map_err(|e| ProvisionBindingFailure::NotCommitted(input_eventlog(e)))?;
        if !self.backend.is_inline(PROJECTOR_NAME).await {
            return Err(ProvisionBindingFailure::NotCommitted(integrity(
                "the fixed inline projector is not attached",
            )));
        }
        let capture = self
            .backend
            .capture_tenant(&tenant, projection_specs(), self.limits)
            .await
            .map_err(|error| ProvisionBindingFailure::Uncertain {
                authority: authority.clone(),
                cause: match error {
                    CaptureError::TenantIdentityMissing
                    | CaptureError::Store(EventLogError::NotFound) => {
                        ProvisionBindingUncertainty::RecoveryUnavailable
                    }
                    _ => ProvisionBindingUncertainty::RecoveryUnavailable,
                },
            })?;
        if capture.tenant != tenant || capture.stream_identity != authority.stream_identity {
            return Err(ProvisionBindingFailure::NotCommitted(integrity(
                "native capture substituted tenant or generation",
            )));
        }
        let observed =
            captured_binding_authority(&capture).map_err(ProvisionBindingFailure::NotCommitted)?;
        let Some(observed) = observed else {
            let model =
                build_model(&authority, &capture).map_err(ProvisionBindingFailure::NotCommitted)?;
            debug_assert!(model.binding.is_none());
            return Ok(None);
        };
        if observed.tenant != authority.tenant
            || observed.stream_identity != authority.stream_identity
        {
            return Err(ProvisionBindingFailure::NotCommitted(integrity(
                "binding authority differs from the captured tenant or generation",
            )));
        }
        let model =
            build_model(&observed, &capture).map_err(ProvisionBindingFailure::NotCommitted)?;
        let physical = model.binding.ok_or_else(|| {
            ProvisionBindingFailure::NotCommitted(integrity(
                "binding authority has no physical event",
            ))
        })?;
        if observed != authority {
            return Err(ProvisionBindingFailure::Conflict {
                requested: authority,
                found: observed,
            });
        }
        Ok(Some(ProvisionBindingOutcome {
            authority,
            physical,
            replayed: true,
        }))
    }
}

fn captured_binding_authority(
    capture: &TenantCapture,
) -> Result<Option<Authority>, AsyncStoreError> {
    let blobs = capture_blobs(capture);
    let mut found = None;
    for event in &capture.events {
        if event.name != "er.binding" {
            continue;
        }
        if event.is_redacted() || event.schema_version != 1 {
            return Err(integrity(
                "binding recovery found redacted or unknown-version authority",
            ));
        }
        if event.stream_type != "er.binding" || event.stream_id != "singleton" || event.version != 1
        {
            return Err(integrity(
                "binding event has inconsistent physical identity",
            ));
        }
        let digest = reference_digest(event)?;
        let bytes = BoundBlobs::new(&blobs).get(
            digest,
            BINDING_BLOB_DOMAIN,
            "referenced blob is missing",
        )?;
        let authority = decode_binding(bytes)?;
        if found.replace(authority).is_some() {
            return Err(integrity("more than one binding event exists"));
        }
    }
    Ok(found)
}

struct BindingGuard {
    authority: Authority,
    tenant: TenantId,
    slot: Arc<Mutex<Option<GuardRefusal>>>,
}
impl Guard for BindingGuard {
    fn check<'a>(
        &'a self,
        store: &'a mut dyn ProjectionStore,
    ) -> eventlog_core::BoxFuture<'a, Result<(), EventLogError>> {
        Box::pin(async move {
            match store
                .get_for_update(binding_spec(), &self.tenant, "singleton")
                .await?
            {
                None => Ok(()),
                Some(row) => {
                    let body = tagged_body(&row, "er.eventlog.binding-index/1")?;
                    let found: Authority = serde_json::from_value(body["authority"].clone())
                        .map_err(|_| EventLogError::Invalid("binding row is malformed".into()))?;
                    let error = if found == self.authority {
                        integrity("binding retry reached guard instead of Eventlog deduplication")
                    } else {
                        integrity("binding row already names different authority")
                    };
                    let code = GuardRefusalCode::ProviderIntegrity;
                    *self
                        .slot
                        .lock()
                        .map_err(|_| EventLogError::Invalid("guard slot poisoned".into()))? =
                        Some(GuardRefusal { code, error });
                    Err(EventLogError::GuardRefused {
                        code: code.as_str().into(),
                    })
                }
            }
        })
    }
}

/// Separate administrative imported-boundary writer.
pub trait AsyncImportedAnchorWriter: Send + Sync {
    /// Atomically establishes one verified imported boundary.
    fn import_anchor<'a>(
        &'a self,
        history: SubjectHistory,
    ) -> BoxFuture<'a, Result<ImportAnchorOutcome, ImportAnchorFailure>>;

    /// Atomically establishes every verified imported boundary of one batch.
    ///
    /// The batch writes exactly the anchors, blob keys and receipts that the same histories
    /// written one at a time through [`AsyncImportedAnchorWriter::import_anchor`] write, in the
    /// same subject streams, and returns one outcome per input in the input's order. What it does
    /// not do is verify the whole destination once per subject: it takes one capture to verify
    /// every anchor against, publishes one atomic append group, and takes one capture to verify
    /// the result — a fixed cost rather than one that grows with the batch.
    ///
    /// A batch commits completely or not at all: any refusal, from this adapter or from the
    /// destination guard, leaves none of its members committed. An empty batch settles without
    /// reaching the provider.
    fn import_anchors<'a>(
        &'a self,
        histories: Vec<SubjectHistory>,
    ) -> BoxFuture<'a, Result<Vec<ImportAnchorOutcome>, ImportAnchorFailure>>;
}

/// Result of a settled imported boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportAnchorOutcome {
    /// Verification assurance established for the imported boundary.
    pub assurance: SubjectAssurance,
    /// Whether the exact anchor was already committed.
    pub replayed: bool,
}

/// Imported-boundary failure preserving subject-keyed uncertainty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportAnchorFailure {
    /// The invocation definitely did not publish the anchor.
    NotCommitted(AsyncStoreError),
    /// The anchor may have committed and recovery could not settle it.
    Uncertain {
        /// Subject whose import is unresolved.
        subject: Subject,
        /// Recovery condition that prevented a conclusion.
        cause: ImportAnchorUncertainty,
    },
}

/// Why an import result is unresolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportAnchorUncertainty {
    /// The provider may have committed before losing its reply.
    UnknownCommit,
    /// A complete authoritative recovery observation was unavailable.
    RecoveryUnavailable,
}

impl entity_store::asynchronous::AsyncRefusalRecorder for EventlogOperationStore<'_> {
    fn record_refusal<'a>(
        &'a self,
        refusal: &'a entity_store::asynchronous::RecordedRefusal,
    ) -> BoxFuture<'a, Result<bool, AsyncStoreError>> {
        Box::pin(async move {
            let bytes = crate::encoding::encode_refusal(&self.store.authority, refusal)?;
            let digest = framed_key(crate::encoding::REFUSAL_BLOB_DOMAIN, &bytes)?;
            // One stream per distinct refusal, named by its content: a retry refused for the same
            // reason is the same record, and one refused for another reason is another.
            let stream = StreamId::new(self.store.tenant.clone(), "er.refusal", digest.clone())
                .map_err(input_eventlog)?;
            let command_key = key_for_value(
                "er.eventlog.refusal-command-key/1",
                json!({"authority": self.store.authority, "refusal": digest}),
            )?;
            let meta = self.context.meta(command_key, digest.clone())?;
            self.store
                .backend
                .put_blob(&self.store.tenant, &digest, &bytes)
                .await
                .map_err(map_put_error)?;
            let event = NewEvent::new("er.refused_request", 1, json!({ "blob": digest }))
                .map_err(input_eventlog)?;
            match self
                .store
                .backend
                .append(&stream, Expected::NoStream, &[event], &meta)
                .await
            {
                Ok(result) => Ok(result.deduplicated),
                Err(EventLogError::Conflict { .. }) => Ok(true),
                Err(error) => Err(AsyncStoreError::Backend(error.to_string())),
            }
        })
    }

    fn refusals<'a>(
        &'a self,
    ) -> BoxFuture<'a, Result<Vec<entity_store::asynchronous::RecordedRefusal>, AsyncStoreError>>
    {
        Box::pin(async move { Ok(self.store.capture_model().await?.refusals.clone()) })
    }
}

impl AsyncImportedAnchorWriter for EventlogOperationStore<'_> {
    fn import_anchor<'a>(
        &'a self,
        history: SubjectHistory,
    ) -> BoxFuture<'a, Result<ImportAnchorOutcome, ImportAnchorFailure>> {
        Box::pin(async move { self.import_inner(history, None).await })
    }

    fn import_anchors<'a>(
        &'a self,
        histories: Vec<SubjectHistory>,
    ) -> BoxFuture<'a, Result<Vec<ImportAnchorOutcome>, ImportAnchorFailure>> {
        Box::pin(async move { self.import_batch_inner(histories).await })
    }
}

/// Everything one imported boundary contributes to the destination, decided without reading it.
///
/// Both import paths build this with the same code, which is what makes a batch's bytes the
/// bytes the singular path would have written: the anchor, the blob keys it is stored under and
/// the record blobs it binds are all decided here, from the caller's history and this store's
/// authority alone.
struct PreparedImport {
    subject: Subject,
    assurance: SubjectAssurance,
    record_ids: Vec<String>,
    uploads: Vec<(String, Vec<u8>)>,
    anchor_bytes: Vec<u8>,
    anchor_digest: String,
}

impl EventlogOperationStore<'_> {
    /// Imports a legacy boundary with its acquisition source bound into durable replay identity.
    /// An unbound older anchor cannot establish that source and is not an equal retry.
    pub async fn import_source_anchor(
        &self,
        source_id: String,
        history: SubjectHistory,
    ) -> Result<ImportAnchorOutcome, ImportAnchorFailure> {
        self.import_inner(history, Some(source_id)).await
    }

    fn prepare_import(
        &self,
        history: &SubjectHistory,
        source_id: Option<&str>,
    ) -> Result<PreparedImport, ImportAnchorFailure> {
        let HistoryOrigin::Imported(anchor) = &history.origin else {
            return Err(ImportAnchorFailure::NotCommitted(
                AsyncStoreError::InvalidInput("import requires an Imported origin".into()),
            ));
        };
        if !history.records.is_empty() {
            return Err(ImportAnchorFailure::NotCommitted(
                AsyncStoreError::InvalidInput(
                    "import anchor input must have an empty suffix".into(),
                ),
            ));
        }
        let assurance = verify_subject_history(history, &anchor.instance)
            .map_err(ImportAnchorFailure::NotCommitted)?;
        let mut record_blobs = Vec::new();
        let mut record_ids = Vec::new();
        let mut uploads = Vec::new();
        for evidence in &anchor.evidence {
            if let entity_store::asynchronous::LegacyEvidence::Envelope(saved) = evidence {
                let bytes = record_comparison_bytes(&saved.entry)
                    .map_err(ImportAnchorFailure::NotCommitted)?;
                let digest = framed_key(RECORD_BLOB_DOMAIN, &bytes)
                    .map_err(ImportAnchorFailure::NotCommitted)?;
                record_blobs.push(digest.clone());
                record_ids.push(saved.entry.record_id().to_owned());
                uploads.push((digest, bytes));
            }
        }
        let wrapper = anchor_from_history(self.store.authority.clone(), history, &record_blobs)
            .map_err(ImportAnchorFailure::NotCommitted)?;
        let anchor_bytes = match source_id {
            Some(source_id) => encode_source_anchor(source_id, &wrapper),
            None => encode_anchor(&wrapper),
        }
        .map_err(ImportAnchorFailure::NotCommitted)?;
        let anchor_digest = framed_key(ANCHOR_BLOB_DOMAIN, &anchor_bytes)
            .map_err(ImportAnchorFailure::NotCommitted)?;
        Ok(PreparedImport {
            subject: history.subject.clone(),
            assurance,
            record_ids,
            uploads,
            anchor_bytes,
            anchor_digest,
        })
    }

    fn import_append(
        &self,
        prepared: &PreparedImport,
    ) -> Result<StreamAppend, ImportAnchorFailure> {
        Ok(StreamAppend {
            stream: StreamId::new(
                self.store.tenant.clone(),
                "er.subject",
                subject_stream_id(&self.store.authority, &prepared.subject)
                    .map_err(ImportAnchorFailure::NotCommitted)?,
            )
            .map_err(|e| ImportAnchorFailure::NotCommitted(input_eventlog(e)))?,
            expected: Expected::NoStream,
            events: vec![
                NewEvent::new(
                    "er.import_anchor",
                    1,
                    json!({ "blob": prepared.anchor_digest }),
                )
                .map_err(|e| ImportAnchorFailure::NotCommitted(input_eventlog(e)))?,
            ],
        })
    }

    /// Refuses an anchor the destination already answers for, and reports an exact equal retry.
    ///
    /// `Ok(true)` means the destination already holds exactly these bytes.
    fn settled_against(
        model: &CapturedModel,
        prepared: &PreparedImport,
    ) -> Result<bool, ImportAnchorFailure> {
        if let Some(existing) = model.anchors.get(&prepared.subject) {
            if *existing == prepared.anchor_bytes {
                return Ok(true);
            }
            return Err(ImportAnchorFailure::NotCommitted(
                AsyncStoreError::RevisionConflict {
                    subject: prepared.subject.clone(),
                    expected: Expect::Absent,
                    found: model.terminals.get(&prepared.subject).map(|i| i.revision),
                },
            ));
        }
        if let Some(existing) = model.histories.get(&prepared.subject) {
            return Err(ImportAnchorFailure::NotCommitted(
                AsyncStoreError::RevisionConflict {
                    subject: prepared.subject.clone(),
                    expected: Expect::Absent,
                    found: model.terminals.get(&existing.subject).map(|i| i.revision),
                },
            ));
        }
        Ok(false)
    }

    async fn upload(&self, key: &str, value: &[u8]) -> Result<(), ImportAnchorFailure> {
        self.store
            .backend
            .put_blob(&self.store.tenant, key, value)
            .await
            .map_err(|e| ImportAnchorFailure::NotCommitted(map_put_error(e)))
    }

    async fn import_inner(
        &self,
        history: SubjectHistory,
        source_id: Option<String>,
    ) -> Result<ImportAnchorOutcome, ImportAnchorFailure> {
        let subject = history.subject.clone();
        let prepared = self.prepare_import(&history, source_id.as_deref())?;
        let model = self
            .store
            .capture_model()
            .await
            .map_err(ImportAnchorFailure::NotCommitted)?;
        if Self::settled_against(&model, &prepared)? {
            return Ok(ImportAnchorOutcome {
                assurance: prepared.assurance,
                replayed: true,
            });
        }
        let digest = prepared.anchor_digest.clone();
        let bytes = prepared.anchor_bytes.clone();
        let command_key = key_for_value(
            "er.eventlog.import-command-key/1",
            json!({"authority":self.store.authority,"subject":SubjectWire::from(&history.subject)}),
        )
        .map_err(ImportAnchorFailure::NotCommitted)?;
        let meta = self
            .context
            .meta(command_key, digest.clone())
            .map_err(ImportAnchorFailure::NotCommitted)?;
        for (key, value) in &prepared.uploads {
            self.upload(key, value).await?;
        }
        self.upload(&digest, &bytes).await?;
        let group = AppendGroup {
            tenant: self.store.tenant.clone(),
            appends: vec![self.import_append(&prepared)?],
            meta,
        };
        let slot = Arc::new(Mutex::new(None));
        let guard = Arc::new(ImportGuard {
            authority: self.store.authority.clone(),
            tenant: self.store.tenant.clone(),
            members: vec![ImportGuardMember {
                subject: history.subject.clone(),
                record_ids: prepared.record_ids.clone(),
                anchor_digest: prepared.anchor_digest.clone(),
            }],
            slot: slot.clone(),
        });
        match self.store.backend.append_group_guarded(&group, guard).await {
            Ok(result) => {
                if validate_group_result(&result, 1).is_err() {
                    return Err(ImportAnchorFailure::Uncertain {
                        subject,
                        cause: ImportAnchorUncertainty::RecoveryUnavailable,
                    });
                }
                self.store.remember_own(&result);
                let returned = physical(&result.appends[0].events[0]);
                match self.store.capture_model().await {
                    Ok(model)
                        if model.anchors.get(&history.subject) == Some(&bytes)
                            && model.anchor_physical.get(&history.subject) == Some(&returned) =>
                    {
                        Ok(ImportAnchorOutcome {
                            assurance: prepared.assurance,
                            replayed: result.deduplicated,
                        })
                    }
                    _ => Err(ImportAnchorFailure::Uncertain {
                        subject: history.subject,
                        cause: ImportAnchorUncertainty::RecoveryUnavailable,
                    }),
                }
            }
            Err(EventLogError::UnknownCommit) => match self.store.capture_model().await {
                Ok(model) if model.anchors.get(&history.subject) == Some(&bytes) => {
                    Ok(ImportAnchorOutcome {
                        assurance: prepared.assurance,
                        replayed: true,
                    })
                }
                _ => Err(ImportAnchorFailure::Uncertain {
                    subject: history.subject,
                    cause: ImportAnchorUncertainty::UnknownCommit,
                }),
            },
            Err(EventLogError::Conflict { .. } | EventLogError::IdempotencyMismatch { .. }) => {
                match self.store.capture_model().await {
                    Ok(model) if model.anchors.get(&history.subject) == Some(&bytes) => {
                        Ok(ImportAnchorOutcome {
                            assurance: prepared.assurance,
                            replayed: true,
                        })
                    }
                    Ok(model) => Err(ImportAnchorFailure::NotCommitted(
                        AsyncStoreError::RevisionConflict {
                            subject: history.subject.clone(),
                            expected: Expect::Absent,
                            found: model.terminals.get(&history.subject).map(|i| i.revision),
                        },
                    )),
                    Err(_) => Err(ImportAnchorFailure::Uncertain {
                        subject: history.subject,
                        cause: ImportAnchorUncertainty::RecoveryUnavailable,
                    }),
                }
            }
            Err(EventLogError::GuardRefused { code }) => Err(Self::guard_failure(&slot, &code)),
            Err(error) => Err(ImportAnchorFailure::NotCommitted(map_append_error(error))),
        }
    }

    /// Imports a batch of unbound legacy boundaries.
    ///
    /// There is deliberately no source-bound batch. `prepare_import` already takes the source, so
    /// one would be a line of plumbing — but it would be a line no caller reaches and no case
    /// covers, and the encoding it selects (`er.eventlog.import-anchor/2`) would go out under a
    /// batch's name having never been written by one.
    async fn import_batch_inner(
        &self,
        histories: Vec<SubjectHistory>,
    ) -> Result<Vec<ImportAnchorOutcome>, ImportAnchorFailure> {
        if histories.is_empty() {
            return Ok(Vec::new());
        }
        let mut prepared = Vec::with_capacity(histories.len());
        for history in &histories {
            prepared.push(self.prepare_import(history, None)?);
        }

        // A subject named more than once in one batch. One group holds at most one
        // `Expected::NoStream` append per stream, so the second mention cannot be a second
        // append — but it need not be a refusal either. N singular calls settle an exact repeat
        // against the destination the first call just wrote, and report it `replayed: true`; the
        // batch agrees by settling it against the first mention instead. A second mention whose
        // bytes differ is a different boundary claiming one subject, which no destination can
        // hold and which the adapter can refuse without asking one.
        let mut first_mention: BTreeMap<Subject, usize> = BTreeMap::new();
        let mut repeats = vec![false; prepared.len()];
        for index in 0..prepared.len() {
            match first_mention.get(&prepared[index].subject) {
                Some(&first) => {
                    if prepared[first].anchor_bytes != prepared[index].anchor_bytes {
                        return Err(ImportAnchorFailure::NotCommitted(
                            AsyncStoreError::InvalidInput(
                                "one import batch names a subject twice with different anchors"
                                    .into(),
                            ),
                        ));
                    }
                    repeats[index] = true;
                }
                None => {
                    first_mention.insert(prepared[index].subject.clone(), index);
                }
            }
        }

        let model = self
            .store
            .capture_model()
            .await
            .map_err(ImportAnchorFailure::NotCommitted)?;
        let mut already = Vec::with_capacity(prepared.len());
        for (index, item) in prepared.iter().enumerate() {
            already.push(repeats[index] || Self::settled_against(&model, item)?);
        }
        // A record identity the destination already answers for, refused from the capture this
        // batch already holds rather than from the destination guard. The guard remains the
        // authority — it runs in the append's own transaction, against concurrent writers — but
        // reaching it costs every blob of the batch an upload first, and on a provider that does
        // not bind blobs inside the group that upload is what leaves orphans behind a refusal.
        // Refusing here spends nothing and leaves nothing.
        //
        // The same refusal covers a record identity two members of one batch share. The guard
        // cannot see that one: it runs once, before entries, so a member's own record key is not
        // in the store yet when a later member is checked, and the collision would fall through
        // to the inline projector as a `ProviderIntegrity` about an authority that never changed.
        // N singular calls report `RecordConflict`, because each commits before the next is
        // admitted. Holding every member's record identities in one set here reports the same.
        let mut claimed: BTreeSet<&str> = BTreeSet::new();
        for (item, settled) in prepared.iter().zip(&already) {
            if *settled {
                continue;
            }
            for record_id in &item.record_ids {
                if model.records.contains_key(record_id) || !claimed.insert(record_id.as_str()) {
                    return Err(ImportAnchorFailure::NotCommitted(
                        AsyncStoreError::RecordConflict {
                            record_id: record_id.clone(),
                        },
                    ));
                }
            }
        }
        drop(claimed);
        let held = model.held.clone();
        drop(model);
        let pending: Vec<&PreparedImport> = prepared
            .iter()
            .zip(&already)
            .filter_map(|(item, settled)| (!settled).then_some(item))
            .collect();
        if pending.is_empty() {
            return Ok(prepared
                .into_iter()
                .map(|item| ImportAnchorOutcome {
                    assurance: item.assurance,
                    replayed: true,
                })
                .collect());
        }

        // What this batch would add to what the destination already holds, against the bounds
        // this handle reads it back with. A writer that commits past its own reader leaves a
        // destination it cannot read — and because this call reports its outcome by reading the
        // destination back, it would also lose every receipt for what it had just committed.
        // The singular path overshoots its reader by at most the one event it wrote and the next
        // call refuses; a batch would overshoot by the whole batch, in one commit.
        //
        // Counted before a byte is uploaded, so the refusal costs nothing and leaves nothing. A
        // caller that meets it divides the work and calls again; `CaptureLimits` is the caller's
        // own, so the bound it is measured against is the one it chose.
        let limits = self.store.limits;
        let added_rows: u64 = pending
            .iter()
            .map(|item| 1 + item.record_ids.len() as u64)
            .sum();
        let added_blobs = pending
            .iter()
            .flat_map(|item| {
                item.uploads
                    .iter()
                    .map(|(digest, _)| digest)
                    .chain(std::iter::once(&item.anchor_digest))
            })
            .filter(|digest| !held.digests.contains(digest.as_str()))
            .collect::<BTreeSet<_>>()
            .len() as u64;
        for (bound, limit, would_hold) in [
            (
                "max_events",
                limits.max_events,
                held.events.saturating_add(pending.len() as u64),
            ),
            (
                "max_blobs",
                limits.max_blobs,
                held.blobs.saturating_add(added_blobs),
            ),
            (
                "max_projection_rows",
                limits.max_projection_rows,
                held.rows.saturating_add(added_rows),
            ),
        ] {
            if would_hold > limit {
                return Err(ImportAnchorFailure::NotCommitted(
                    AsyncStoreError::BatchExceedsReadBounds {
                        bound: bound.to_owned(),
                        limit,
                        would_hold,
                    },
                ));
            }
        }

        let command_key = key_for_value(
            "er.eventlog.import-batch-command-key/1",
            json!({
                "authority": self.store.authority,
                "subjects": pending
                    .iter()
                    .map(|item| SubjectWire::from(&item.subject))
                    .collect::<Vec<_>>(),
            }),
        )
        .map_err(ImportAnchorFailure::NotCommitted)?;
        let request_hash = key_for_value(
            "er.eventlog.import-batch-request/1",
            json!(
                pending
                    .iter()
                    .map(|item| item.anchor_digest.clone())
                    .collect::<Vec<_>>()
            ),
        )
        .map_err(ImportAnchorFailure::NotCommitted)?;
        let meta = self
            .context
            .meta(command_key, request_hash)
            .map_err(ImportAnchorFailure::NotCommitted)?;

        let mut appends = Vec::with_capacity(pending.len());
        let mut members = Vec::with_capacity(pending.len());
        for item in &pending {
            appends.push(self.import_append(item)?);
            members.push(ImportGuardMember {
                subject: item.subject.clone(),
                record_ids: item.record_ids.clone(),
                anchor_digest: item.anchor_digest.clone(),
            });
        }
        // Every blob the group binds, in the order the singular path uploads them — the record
        // blobs of each member, then that member's anchor. They travel with the group so the
        // whole batch costs the one durability barrier the group already commits, instead of one
        // barrier per blob. Content addressing makes the bytes and the keys the same either way;
        // what changes is only how many times the provider is made to wait for the disk.
        let mut blobs = Vec::new();
        for item in &pending {
            blobs.extend(item.uploads.iter().cloned());
            blobs.push((item.anchor_digest.clone(), item.anchor_bytes.clone()));
        }
        let group = AppendGroup {
            tenant: self.store.tenant.clone(),
            appends,
            meta,
        };
        group
            .fingerprint()
            .map_err(|e| ImportAnchorFailure::NotCommitted(input_eventlog(e)))?;
        let slot = Arc::new(Mutex::new(None));
        let guard = Arc::new(ImportGuard {
            authority: self.store.authority.clone(),
            tenant: self.store.tenant.clone(),
            members,
            slot: slot.clone(),
        });
        // Every member shares one atomic group, so every member shares its outcome: the subject
        // named on an uncertain batch is the first of them, and settling it settles the rest.
        let uncertain = |cause| ImportAnchorFailure::Uncertain {
            subject: pending[0].subject.clone(),
            cause,
        };
        // The provider may not implement guarded blob-bearing groups at all. The port's default
        // says so and fails closed — it writes nothing, commits nothing, and refuses with
        // `UNAVAILABLE` — precisely so that a caller holding a trait object is never silently
        // given the weaker guarantee under the stronger name. Taking the slow path is this
        // caller's decision to make, and it makes it here: every blob on its own path, then the
        // same guarded group. Same bytes, same keys, same guard, same receipts; what is lost is
        // only the single durability barrier, which is what the provider was unable to offer.
        let attempted = self
            .store
            .backend
            .append_group_guarded_with_blobs(&group, guard.clone(), &blobs)
            .await;
        let attempted = match attempted {
            Err(EventLogError::Invalid(ref detail)) if detail == UNAVAILABLE => {
                for (key, value) in &blobs {
                    self.upload(key, value).await?;
                }
                self.store.backend.append_group_guarded(&group, guard).await
            }
            settled => settled,
        };
        let deduplicated = match attempted {
            Ok(result) => {
                if validate_group_result(&result, pending.len()).is_err() {
                    return Err(uncertain(ImportAnchorUncertainty::RecoveryUnavailable));
                }
                self.store.remember_own(&result);
                let returned: Vec<PhysicalRef> = result
                    .appends
                    .iter()
                    .map(|append| physical(&append.events[0]))
                    .collect();
                let Ok(model) = self.store.capture_model().await else {
                    return Err(uncertain(ImportAnchorUncertainty::RecoveryUnavailable));
                };
                if !pending.iter().zip(&returned).all(|(item, physical)| {
                    model.anchors.get(&item.subject) == Some(&item.anchor_bytes)
                        && model.anchor_physical.get(&item.subject) == Some(physical)
                }) {
                    return Err(uncertain(ImportAnchorUncertainty::RecoveryUnavailable));
                }
                result.deduplicated
            }
            Err(EventLogError::UnknownCommit) => {
                match self.store.capture_model().await {
                    Ok(model)
                        if pending.iter().all(|item| {
                            model.anchors.get(&item.subject) == Some(&item.anchor_bytes)
                        }) => {}
                    _ => return Err(uncertain(ImportAnchorUncertainty::UnknownCommit)),
                }
                true
            }
            Err(EventLogError::Conflict { .. } | EventLogError::IdempotencyMismatch { .. }) => {
                match self.store.capture_model().await {
                    Ok(model) => {
                        // Every member present under this batch's own bytes: the group is a
                        // replay of one already committed, which is what idempotency is for.
                        if pending.iter().all(|item| {
                            model.anchors.get(&item.subject) == Some(&item.anchor_bytes)
                        }) {
                            true
                        } else {
                            // Otherwise something holds a subject this group required absent.
                            // Name the member that is actually held — not the first member whose
                            // anchor is missing, which is an innocent member that simply was not
                            // written because the group refused as a whole. Reporting that one
                            // produced `expected: Absent, found: None`: the subject was expected
                            // absent and found absent, which describes no conflict at all.
                            //
                            // A conflict is only claimed when the destination can say what it
                            // holds. Where it cannot, the outcome is unresolved and says so,
                            // which is why `found: None` cannot be produced here.
                            let occupied = pending.iter().find_map(|item| {
                                occupied_revision(&model, &item.subject)
                                    .map(|revision| (item, revision))
                            });
                            match occupied {
                                Some((item, revision)) => {
                                    return Err(ImportAnchorFailure::NotCommitted(
                                        AsyncStoreError::RevisionConflict {
                                            subject: item.subject.clone(),
                                            expected: Expect::Absent,
                                            found: Some(revision),
                                        },
                                    ));
                                }
                                None => {
                                    return Err(uncertain(
                                        ImportAnchorUncertainty::RecoveryUnavailable,
                                    ));
                                }
                            }
                        }
                    }
                    Err(_) => return Err(uncertain(ImportAnchorUncertainty::RecoveryUnavailable)),
                }
            }
            Err(EventLogError::GuardRefused { code }) => {
                return Err(Self::guard_failure(&slot, &code));
            }
            // One call carried this batch's blobs and its group, and its error does not say which
            // half failed, so every one of them is mapped as an append. `EventStore::put_blob`
            // documents exactly one variant — `Invalid` — and `map_put_error` and
            // `map_append_error` produce the same value for it, so no blob fault the port
            // promises reaches the caller as something the blob path would not have said. The
            // variants where the two mappings differ are pinned by
            // `the_blob_and_append_mappings_agree_on_the_only_variant_put_blob_documents`.
            Err(error) => {
                return Err(ImportAnchorFailure::NotCommitted(map_append_error(error)));
            }
        };
        Ok(prepared
            .into_iter()
            .zip(already)
            .map(|(item, settled)| ImportAnchorOutcome {
                assurance: item.assurance,
                replayed: settled || deduplicated,
            })
            .collect())
    }

    fn guard_failure(slot: &Arc<Mutex<Option<GuardRefusal>>>, code: &str) -> ImportAnchorFailure {
        let Ok(mut held) = slot.lock() else {
            return ImportAnchorFailure::NotCommitted(integrity(
                "import guard refusal slot was poisoned",
            ));
        };
        match held.take() {
            Some(refusal)
                if refusal.code.as_str() == code && refusal.code.matches(&refusal.error) =>
            {
                ImportAnchorFailure::NotCommitted(refusal.error)
            }
            _ => ImportAnchorFailure::NotCommitted(integrity(
                "import guard refusal code and typed slot disagree",
            )),
        }
    }
}

/// One imported boundary the guard has to find room for.
struct ImportGuardMember {
    subject: Subject,
    record_ids: Vec<String>,
    /// The digest of the anchor this member is submitting, which is how the guard tells a row
    /// this very group already wrote from a row somebody else owns.
    anchor_digest: String,
}

struct ImportGuard {
    authority: Authority,
    tenant: TenantId,
    members: Vec<ImportGuardMember>,
    slot: Arc<Mutex<Option<GuardRefusal>>>,
}
impl Guard for ImportGuard {
    fn check<'a>(
        &'a self,
        store: &'a mut dyn ProjectionStore,
    ) -> eventlog_core::BoxFuture<'a, Result<(), EventLogError>> {
        Box::pin(async move {
            let binding = store
                .get_for_update(binding_spec(), &self.tenant, "singleton")
                .await?;
            let Some(binding) = binding else {
                return self.refuse(integrity("binding row is absent"));
            };
            let body = tagged_body(&binding, "er.eventlog.binding-index/1")?;
            let found: Authority = serde_json::from_value(body["authority"].clone())
                .map_err(|_| EventLogError::Invalid("binding row is malformed".into()))?;
            if found != self.authority {
                return self.refuse(integrity("binding row authority changed"));
            }

            // Every member's keys in one order derived from the keys themselves, not from the
            // order the caller happened to list its members in. Two batches that overlap take
            // the same locks in the same sequence whoever assembled them, so they cannot hold
            // each other's next lock. A per-member loop in caller order gives that sequence away
            // to the caller.
            //
            // A duplicate key within the group is not refused here, because nothing can present
            // one. Two members sharing a record identity are refused from the batch's own capture
            // in step 1 (`a_batch_refuses_a_record_identity_two_of_its_members_share`), and one
            // anchor naming a record twice is refused further up still, by
            // `verify_subject_history`, as `CorruptHistory { detail: "an imported record identity
            // appears more than once" }` (`one_anchor_naming_a_record_identity_twice_is_refused`).
            // A refusal arm nothing can reach is not a safety net; it is a comment that compiles,
            // and it would report a different error than either reacher already does.
            let mut records = BTreeMap::new();
            for member in &self.members {
                for id in &member.record_ids {
                    let key = record_key(&self.authority, id)
                        .map_err(|error| EventLogError::Invalid(error.to_string()))?;
                    records.insert(key, (id, member.anchor_digest.as_str()));
                }
            }
            // A row this group already wrote is not somebody else's. A batch-bearing retry is
            // admitted again — the port runs admission before it answers from the command it
            // recorded, so that replaying a committed key cannot be used to ask whether a digest
            // is bound — which means this guard is handed a group it has already admitted and
            // committed. Refusing "occupied" without asking *by what* would refuse exactly the
            // retry idempotency exists to serve, and the caller's only way forward would be a new
            // command key: every member appended a second time, into a log that has no delete.
            //
            // The anchor digest is what distinguishes the two. It is a content hash over the
            // anchor bytes, which carry the authority and the subject, so a row naming this
            // member's digest was written by an anchor byte-identical to the one being submitted.
            // Anything else — another anchor, or a subject that is not imported at all — is
            // genuinely another writer's and is still refused.
            for (key, (record_id, anchor_digest)) in &records {
                if let Some(row) = store
                    .get_for_update(record_spec(), &self.tenant, key)
                    .await?
                {
                    let body = tagged_body(&row, "er.eventlog.record-index/1")?;
                    if body
                        .get("entry")
                        .and_then(|entry| entry.get("anchor_blob"))
                        .and_then(Value::as_str)
                        != Some(*anchor_digest)
                    {
                        return self.refuse(AsyncStoreError::RecordConflict {
                            record_id: (*record_id).clone(),
                        });
                    }
                }
            }

            let mut subjects = BTreeMap::new();
            for member in &self.members {
                let key = subject_key(&self.authority, &member.subject)
                    .map_err(|error| EventLogError::Invalid(error.to_string()))?;
                subjects.insert(key, (&member.subject, member.anchor_digest.as_str()));
            }
            for (key, (subject, anchor_digest)) in &subjects {
                if let Some(row) = store
                    .get_for_update(subject_spec(), &self.tenant, key)
                    .await?
                {
                    let body = tagged_body(&row, "er.eventlog.subject-index/1")?;
                    // `origin` alone is not evidence the row is still the row this anchor wrote.
                    // `fold_subject_source` copies `origin` verbatim onto every later entry
                    // (`projection.rs:371-378`), so a subject that was imported and has since
                    // taken a decision or an observation still names this anchor there. What
                    // moves with the subject is `state_source`: the import writes
                    // `{"kind":"anchor","anchor_blob":…}` and the first suffix entry replaces it.
                    // Requiring both is what distinguishes "the row my own commit wrote" from
                    // "a subject my anchor started and somebody has since appended to".
                    let still_the_anchor = body
                        .get("origin")
                        .and_then(|origin| origin.get("anchor_blob"))
                        .and_then(Value::as_str)
                        == Some(*anchor_digest)
                        && body
                            .get("state_source")
                            .and_then(|source| source.get("anchor_blob"))
                            .and_then(Value::as_str)
                            == Some(*anchor_digest);
                    if !still_the_anchor {
                        let found = body.get("revision").and_then(Value::as_u64);
                        return self.refuse(AsyncStoreError::RevisionConflict {
                            subject: (*subject).clone(),
                            expected: Expect::Absent,
                            found,
                        });
                    }
                }
            }
            Ok(())
        })
    }
}

impl ImportGuard {
    fn refuse(&self, error: AsyncStoreError) -> Result<(), EventLogError> {
        let code = GuardRefusalCode::for_error(&error);
        let mut slot = self
            .slot
            .lock()
            .map_err(|_| EventLogError::Invalid("import guard slot poisoned".into()))?;
        if slot.is_some() {
            return Err(EventLogError::Invalid(
                "import guard slot written twice".into(),
            ));
        }
        *slot = Some(GuardRefusal { code, error });
        Err(EventLogError::GuardRefused {
            code: code.as_str().into(),
        })
    }
}

/// What the capture this model was built from actually charged against its limits.
///
/// Counts only. The payload-byte charge is deliberately absent: computing it would re-encode
/// every event, blob and projection row of the whole authority on every capture, which is the
/// per-capture cost over the whole store that batching exists to remove. The three counts are
/// free — they are vector lengths — and are exact.
#[derive(Default, Clone)]
struct CaptureHeld {
    events: u64,
    blobs: u64,
    rows: u64,
    /// Which blobs are already bound, so a batch counts only the ones it would add.
    digests: BTreeSet<String>,
}

/// The blob digests one record's projection row repeats.
///
/// Held because the capture already admitted them. Recomputing one from the bytes it names is a
/// second SHA-256 that can only agree with the first: the digest *is* the bytes' identity, and the
/// bytes reached this model by being checked against it.
#[derive(Clone)]
struct RecordBlobDigests {
    record: String,
    /// Absent for an imported record, which binds no request blob.
    request: Option<String>,
}

#[derive(Default, Clone)]
struct CapturedModel {
    held: CaptureHeld,
    binding: Option<PhysicalRef>,
    histories: BTreeMap<Subject, SubjectHistory>,
    terminals: BTreeMap<Subject, EntityInstance>,
    /// Subjects two merged branches both wrote, with their heads. Their history is held and
    /// verified branch by branch, but they have no one state: reads and ordinary writes refuse
    /// them until a merge decision joins every head.
    forked: BTreeMap<Subject, Vec<String>>,
    /// Commands the store received and refused, in store order.
    refusals: Vec<entity_store::asynchronous::RecordedRefusal>,
    records: BTreeMap<String, RecordLookup>,
    record_physical: BTreeMap<String, PhysicalRef>,
    batches: BTreeMap<BatchKey, StoredBatch>,
    anchors: BTreeMap<Subject, Vec<u8>>,
    anchor_physical: BTreeMap<Subject, PhysicalRef>,
    binding_blob_digest: Option<String>,
    record_blob_digests: BTreeMap<String, RecordBlobDigests>,
    batch_blob_digests: BTreeMap<BatchKey, String>,
    anchor_blob_digests: BTreeMap<Subject, String>,
    /// Complete records this model's construction decoded, each a full canonical re-encoding.
    decoded: usize,
}

struct PendingRecord {
    wrapper: RecordedEntryWrapper,
    entry: entity_store::asynchronous::RecordedEntry,
    /// The record blob exactly as it was bound. `decode_record` has already held it against the
    /// entry it decoded, so re-encoding the entry to obtain these bytes produces the same bytes at
    /// the cost of encoding every record in the store a second time.
    record_bytes: Vec<u8>,
    request_bytes: Vec<u8>,
    event: RecordedEvent,
}

/// One capture's bound content, hashed at most once per blob.
///
/// A committed authority names one digest from several places: the reference event, the wrapper
/// that binds a record's record, request and batch blobs, once more per batch member for the batch
/// blob a group shares, and once per projection row that repeats it. Each of those was a fresh
/// SHA-256 over the same bytes, so a seeded open hashed 3.4x its own captured bytes before a caller
/// had read anything.
struct BoundBlobs<'a> {
    blobs: &'a BTreeMap<&'a str, &'a [u8]>,
    admitted: BTreeMap<&'a str, &'static str>,
}

impl<'a> BoundBlobs<'a> {
    fn new(blobs: &'a BTreeMap<&'a str, &'a [u8]>) -> Self {
        Self {
            blobs,
            admitted: BTreeMap::new(),
        }
    }

    /// The bytes bound under `digest`, verified against `domain` exactly once.
    ///
    /// The second and later reads are not unverified: they are the same bytes, still owned by this
    /// capture, whose digest this function already computed and compared. Only one domain can
    /// match a given digest — `framed_key` frames the domain into the hash — so a later reference
    /// naming a different domain is exactly the mismatch a second hash would have reported.
    fn get(
        &mut self,
        digest: &str,
        domain: &'static str,
        missing: &'static str,
    ) -> Result<&'a [u8], AsyncStoreError> {
        let (key, bytes) = self
            .blobs
            .get_key_value(digest)
            .map(|(key, bytes)| (*key, *bytes))
            .ok_or_else(|| integrity(missing))?;
        match self.admitted.get(key).copied() {
            Some(admitted) if admitted == domain => Ok(bytes),
            Some(_) => Err(integrity("blob digest/domain mismatch")),
            None => {
                verify_digest(domain, digest, bytes)?;
                self.admitted.insert(key, domain);
                Ok(bytes)
            }
        }
    }
}

/// Every blob one capture binds, by digest, borrowed from the capture rather than copied out.
fn capture_blobs(capture: &TenantCapture) -> BTreeMap<&str, &[u8]> {
    capture
        .blobs
        .iter()
        .map(|blob| (blob.digest.as_str(), blob.bytes.as_slice()))
        .collect()
}

fn capture_held(capture: &TenantCapture, blobs: &BTreeMap<&str, &[u8]>, rows: u64) -> CaptureHeld {
    CaptureHeld {
        events: capture.events.len() as u64,
        blobs: blobs.len() as u64,
        rows,
        digests: blobs.keys().map(|digest| (*digest).to_owned()).collect(),
    }
}

fn projection_rows(capture: &TenantCapture) -> u64 {
    capture
        .projections
        .iter()
        .map(|projection| projection.rows.len() as u64)
        .sum()
}

/// A forked subject has no one state to serve or to write after.
fn refuse_forked(model: &CapturedModel, subject: &Subject) -> Result<(), AsyncStoreError> {
    match model.forked.get(subject) {
        Some(heads) => Err(AsyncStoreError::Forked {
            subject: subject.clone(),
            heads: heads.clone(),
        }),
        None => Ok(()),
    }
}

fn build_model(
    authority: &Authority,
    capture: &TenantCapture,
) -> Result<CapturedModel, AsyncStoreError> {
    let model = build_model_events(authority, capture, projection_rows(capture))?;
    validate_projection_sets(authority, &capture.projections, &model)?;
    Ok(model)
}

/// The authoritative model of one capture's events and blobs, before its materialized rows are
/// held against it.
fn build_model_events(
    authority: &Authority,
    capture: &TenantCapture,
    rows: u64,
) -> Result<CapturedModel, AsyncStoreError> {
    let blobs = capture_blobs(capture);
    let mut model = CapturedModel {
        held: capture_held(capture, &blobs, rows),
        ..CapturedModel::default()
    };
    let mut bound = BoundBlobs::new(&blobs);
    let mut pending = Vec::new();
    for event in &capture.events {
        admit_event(authority, event, &mut bound, &mut model, &mut pending)?;
    }
    if model.binding.is_none() && !capture.events.is_empty() {
        return Err(integrity("authoritative events exist without a binding"));
    }
    build_committed(pending, &mut bound, &mut model)?;
    Ok(model)
}

/// Advances a model verified from a capture's first `from` events by the events after them.
///
/// The caller has established that `capture` begins with exactly the events, and binds exactly
/// the blobs, the model was verified from, so the part of a whole build that would read them
/// would build what the model already holds. Every event after them is admitted by the code a
/// whole build admits it with; every subject they touch is verified from the state its verified
/// history reached, or from its origin where it has none; and every projection row of the capture
/// is held against the whole advanced model, as a whole build holds them.
fn advance_model(
    authority: &Authority,
    model: &mut CapturedModel,
    capture: &TenantCapture,
    from: usize,
) -> Result<(), AsyncStoreError> {
    let blobs = capture_blobs(capture);
    model.held = capture_held(capture, &blobs, projection_rows(capture));
    let mut bound = BoundBlobs::new(&blobs);
    let mut pending = Vec::new();
    let mut imported = BTreeSet::new();
    for event in &capture.events[from..] {
        if let Some(subject) = admit_event(authority, event, &mut bound, model, &mut pending)? {
            imported.insert(subject);
        }
    }
    if model.binding.is_none() && !capture.events.is_empty() {
        return Err(integrity("authoritative events exist without a binding"));
    }
    let mut prior: BTreeMap<Subject, Option<(usize, EntityInstance)>> = BTreeMap::new();
    for subject in imported {
        prior.insert(subject, None);
    }
    for record in &pending {
        let subject = record.entry.subject();
        if prior.contains_key(&subject) {
            continue;
        }
        let verified = model
            .histories
            .get(&subject)
            .map(|history| history.records.len())
            .zip(model.terminals.get(&subject).cloned());
        prior.insert(subject, verified);
    }
    insert_committed(pending, &mut bound, model)?;
    for (subject, verified) in prior {
        let history = model
            .histories
            .get_mut(&subject)
            .ok_or_else(|| integrity("an advanced subject has no history"))?;
        let terminal = settle_history(
            history,
            verified.as_ref().map(|(records, state)| (*records, state)),
        );
        let terminal = settled(&mut model.forked, history, terminal)?;
        model.terminals.insert(subject, terminal);
    }
    validate_projection_sets(authority, &capture.projections, model)
}

/// Admits one authority event, deferring a recorded entry to `pending` for its batch.
///
/// Returns the subject an import anchor established, which has no verified history to extend.
fn admit_event(
    authority: &Authority,
    event: &RecordedEvent,
    bound: &mut BoundBlobs<'_>,
    model: &mut CapturedModel,
    pending: &mut Vec<PendingRecord>,
) -> Result<Option<Subject>, AsyncStoreError> {
    const MISSING_REFERENCE: &str = "authority event references a missing blob";
    const MISSING_BOUND: &str = "referenced blob is missing";
    if event.is_redacted() || event.schema_version != 1 {
        return Err(integrity("redacted or unknown-version authority event"));
    }
    let digest = reference_digest(event)?;
    match event.name.as_str() {
        "er.binding" => {
            let bytes = bound.get(digest, BINDING_BLOB_DOMAIN, MISSING_REFERENCE)?;
            let found = decode_binding(bytes)?;
            if found != *authority
                || event.stream_type != "er.binding"
                || event.stream_id != "singleton"
                || event.version != 1
                || model.binding.is_some()
            {
                return Err(integrity("binding authority is duplicated or inconsistent"));
            }
            model.binding = Some(physical(event));
            model.binding_blob_digest = Some(digest.to_owned());
            Ok(None)
        }
        "er.recorded_entry" => {
            let bytes = bound.get(digest, ENTRY_BLOB_DOMAIN, MISSING_REFERENCE)?;
            let wrapper = decode_entry(bytes)?;
            require_authority(authority, &wrapper.authority)?;
            let subject: Subject = wrapper.subject.clone().into();
            if event.stream_type != "er.subject"
                || event.stream_id != subject_stream_id(authority, &subject)?
            {
                return Err(integrity("record reference is in another stream"));
            }
            let record_bytes = bound
                .get(&wrapper.record_blob, RECORD_BLOB_DOMAIN, MISSING_BOUND)?
                .to_vec();
            let entry = decode_record(&record_bytes)?;
            model.decoded += 1;
            if entry.subject() != subject {
                return Err(integrity("record wrapper substitutes its subject"));
            }
            let request_bytes = bound
                .get(&wrapper.request_blob, REQUEST_BLOB_DOMAIN, MISSING_BOUND)?
                .to_vec();
            if original_request_comparison_bytes(&entry)? != request_bytes {
                return Err(integrity("request blob differs from record"));
            }
            // Admitted here so that a batch blob a group shares is hashed once for the group
            // rather than once for each of its members.
            bound.get(&wrapper.batch_blob, BATCH_BLOB_DOMAIN, MISSING_BOUND)?;
            pending.push(PendingRecord {
                wrapper,
                entry,
                record_bytes,
                request_bytes,
                event: event.clone(),
            });
            Ok(None)
        }
        "er.import_anchor" => build_import(authority, event, digest, bound, model).map(Some),
        "er.refused_request" => {
            let bytes = bound.get(
                digest,
                crate::encoding::REFUSAL_BLOB_DOMAIN,
                MISSING_REFERENCE,
            )?;
            let (found, refusal) = crate::encoding::decode_refusal(bytes)?;
            require_authority(authority, &found)?;
            if event.stream_type != "er.refusal" || event.stream_id != digest || event.version != 1
            {
                return Err(integrity("refusal reference is in another stream"));
            }
            model.refusals.push(refusal);
            Ok(None)
        }
        _ => Err(integrity("unknown event exists in the bound ER tenant")),
    }
}

fn build_import(
    authority: &Authority,
    event: &RecordedEvent,
    digest: &str,
    bound: &mut BoundBlobs<'_>,
    model: &mut CapturedModel,
) -> Result<Subject, AsyncStoreError> {
    let bytes = bound.get(
        digest,
        ANCHOR_BLOB_DOMAIN,
        "authority event references a missing blob",
    )?;
    let wrapper = decode_anchor(bytes)?;
    require_authority(authority, &wrapper.authority)?;
    let subject: Subject = wrapper.subject.clone().into();
    if event.stream_type != "er.subject"
        || event.stream_id != subject_stream_id(authority, &subject)?
    {
        return Err(integrity("import anchor is in another subject stream"));
    }
    if model.histories.contains_key(&subject) {
        return Err(corrupt(&subject, "a subject has more than one origin"));
    }
    let mut record_bytes = Vec::new();
    let mut evidence_blobs = Vec::new();
    for item in &wrapper.evidence {
        if let EvidenceWire::Envelope { record_blob, .. } = item {
            record_bytes.push(Vec::from(bound.get(
                record_blob,
                RECORD_BLOB_DOMAIN,
                "referenced blob is missing",
            )?));
            evidence_blobs.push(record_blob.clone());
        }
    }
    let history = history_from_anchor(&wrapper, &record_bytes)?;
    model.decoded += record_bytes.len();
    if let HistoryOrigin::Imported(anchor) = &history.origin {
        let mut envelope = 0usize;
        for evidence in &anchor.evidence {
            if let entity_store::asynchronous::LegacyEvidence::Envelope(saved) = evidence {
                let record_id = saved.entry.record_id().to_owned();
                if model
                    .records
                    .insert(record_id.clone(), RecordLookup::Imported(saved.clone()))
                    .is_some()
                {
                    return Err(corrupt(&subject, "global imported record identity repeats"));
                }
                // `history_from_anchor` has already held each envelope against the record blob at
                // the same position, so the digest this capture admitted is this record's.
                let record = evidence_blobs
                    .get(envelope)
                    .ok_or_else(|| integrity("imported evidence has no admitted record blob"))?
                    .clone();
                model.record_blob_digests.insert(
                    record_id.clone(),
                    RecordBlobDigests {
                        record,
                        request: None,
                    },
                );
                envelope += 1;
                model.record_physical.insert(record_id, physical(event));
            }
        }
        model
            .terminals
            .insert(subject.clone(), anchor.instance.clone());
    }
    model.anchors.insert(subject.clone(), bytes.to_vec());
    model
        .anchor_blob_digests
        .insert(subject.clone(), digest.to_owned());
    model
        .anchor_physical
        .insert(subject.clone(), physical(event));
    model.histories.insert(subject.clone(), history);
    Ok(subject)
}

fn insert_committed(
    pending: Vec<PendingRecord>,
    bound: &mut BoundBlobs<'_>,
    model: &mut CapturedModel,
) -> Result<(), AsyncStoreError> {
    let mut grouped: BTreeMap<BatchKey, Vec<PendingRecord>> = BTreeMap::new();
    for record in pending {
        grouped
            .entry(record.wrapper.batch_key.clone().into())
            .or_default()
            .push(record);
    }
    for (key, mut group) in grouped {
        group.sort_by_key(|record| record.wrapper.member_index);
        let batch_blob = group[0].wrapper.batch_blob.clone();
        // Already admitted in `build_model_events`, so this is the bytes, not another hash of them.
        let batch_bytes = bound
            .get(&batch_blob, BATCH_BLOB_DOMAIN, "referenced blob is missing")?
            .to_vec();
        let (decoded_key, members) = decode_batch(&batch_bytes)?;
        if decoded_key != key || group.len() != members.len() {
            return Err(integrity("batch references are incomplete"));
        }
        let mut stored = Vec::with_capacity(group.len());
        let mut prior_position = 0;
        for (index, record) in group.into_iter().enumerate() {
            let index_u64 =
                u64::try_from(index).map_err(|_| AsyncStoreError::PositionExhausted {
                    domain: "batch member".into(),
                })?;
            if record.wrapper.member_index != index_u64 || record.event.global_seq <= prior_position
            {
                return Err(integrity("batch member order or position is crossed"));
            }
            let member = &members[index];
            if member.entry != record.entry || member.request_bytes != record.request_bytes {
                return Err(integrity("batch member differs from reference"));
            }
            prior_position = record.event.global_seq;
            let subject = record.entry.subject();
            let position = RecordPosition {
                subject: record.event.version,
                store: record.event.global_seq,
            };
            let receipt = RecordReceipt {
                record_id: record.entry.record_id().to_owned(),
                subject: subject.clone(),
                kind: record.entry.kind(),
                revision: record.entry.revision(),
                position,
                batch_key: key.clone(),
                member_index: index_u64,
            };
            model.record_blob_digests.insert(
                receipt.record_id.clone(),
                RecordBlobDigests {
                    record: record.wrapper.record_blob.clone(),
                    request: Some(record.wrapper.request_blob.clone()),
                },
            );
            let saved = StoredRecord {
                entry: record.entry,
                position,
                receipt: receipt.clone(),
                expect: member.expect,
                request_bytes: record.request_bytes,
                lineage: record.event.digest.clone().map(|digest| {
                    Box::new(entity_store::asynchronous::Lineage {
                        digest,
                        parents: record.event.parents.clone(),
                    })
                }),
                // The bound record blob. `decode_record` held it against the entry it produced and
                // that entry was just held against this batch member, so re-encoding the member
                // here would encode every record in the store a second time to obtain these bytes.
                record_bytes: record.record_bytes,
            };
            if model
                .records
                .insert(
                    receipt.record_id.clone(),
                    RecordLookup::Committed(saved.clone()),
                )
                .is_some()
            {
                return Err(AsyncStoreError::RecordConflict {
                    record_id: receipt.record_id,
                });
            }
            model
                .record_physical
                .insert(receipt.record_id.clone(), physical(&record.event));
            let history =
                model
                    .histories
                    .entry(subject.clone())
                    .or_insert_with(|| SubjectHistory {
                        subject: subject.clone(),
                        origin: HistoryOrigin::Genesis,
                        records: Vec::new(),
                    });
            history.records.push(saved.clone());
            stored.push(saved);
        }
        let receipt = match &key {
            BatchKey::SingleRecord(_) => CommitReceipt::Single(stored[0].receipt.clone()),
            BatchKey::Named(_) => CommitReceipt::Batch(BatchReceipt {
                key: key.clone(),
                members: stored.iter().map(|r| r.receipt.clone()).collect(),
            }),
        };
        model.batch_blob_digests.insert(key.clone(), batch_blob);
        if model
            .batches
            .insert(
                key.clone(),
                StoredBatch {
                    key,
                    records: stored,
                    // `decode_batch` refuses bytes that are not `batch_comparison_bytes` of what it
                    // decoded, so the bound blob is the comparison material already.
                    comparison_bytes: batch_bytes,
                    receipt,
                },
            )
            .is_some()
        {
            return Err(AsyncStoreError::BatchConflict { key: decoded_key });
        }
    }
    Ok(())
}

fn build_committed(
    pending: Vec<PendingRecord>,
    bound: &mut BoundBlobs<'_>,
    model: &mut CapturedModel,
) -> Result<(), AsyncStoreError> {
    insert_committed(pending, bound, model)?;
    for history in model.histories.values_mut() {
        let terminal = settle_history(history, None);
        let terminal = settled(&mut model.forked, history, terminal)?;
        model.terminals.insert(history.subject.clone(), terminal);
    }
    Ok(())
}

/// The terminal to hold for a settled history, recording a fork rather than refusing the capture.
///
/// A forked subject keeps the state of its last decision in store order as its terminal: that is
/// the row the inline projection wrote for it, and the capture's projection check holds rows to
/// terminals. Nothing serves that state, because the subject is also listed as forked.
fn settled(
    forked: &mut BTreeMap<Subject, Vec<String>>,
    history: &SubjectHistory,
    terminal: Result<EntityInstance, AsyncStoreError>,
) -> Result<EntityInstance, AsyncStoreError> {
    match terminal {
        Ok(terminal) => {
            forked.remove(&history.subject);
            Ok(terminal)
        }
        Err(AsyncStoreError::Forked { subject, heads }) => {
            forked.insert(subject.clone(), heads);
            history
                .records
                .iter()
                .rev()
                .find_map(|record| match &record.entry {
                    entity_store::asynchronous::RecordedEntry::Decision(commit) => {
                        Some(commit.instance.clone())
                    }
                    entity_store::asynchronous::RecordedEntry::Observation(_) => None,
                })
                .ok_or_else(|| corrupt(&subject, "a forked history has no state-producing record"))
        }
        Err(other) => Err(other),
    }
}

/// Orders one subject's history, derives its terminal state and verifies the history reaches it.
///
/// `verified` names a prefix already verified to reach a state — the records the subject held
/// before an advance appended to it, and the terminal they reached. It is used only when the
/// appended records all follow that prefix in store order, so ordering the history leaves the
/// prefix where it was; otherwise, or without one, the whole history is verified from its origin.
fn settle_history(
    history: &mut SubjectHistory,
    verified: Option<(usize, &EntityInstance)>,
) -> Result<EntityInstance, AsyncStoreError> {
    let extension = verified.filter(|(records, _)| {
        *records > 0
            && *records <= history.records.len()
            && history.records[*records..]
                .iter()
                .all(|record| record.position.store > history.records[*records - 1].position.store)
    });
    history.records.sort_by_key(|record| record.position.store);
    let terminal = history
        .records
        .iter()
        .rev()
        .find_map(|record| match &record.entry {
            entity_store::asynchronous::RecordedEntry::Decision(commit) => {
                Some(commit.instance.clone())
            }
            entity_store::asynchronous::RecordedEntry::Observation(_) => None,
        })
        .or_else(|| match &history.origin {
            HistoryOrigin::Imported(anchor) => Some(anchor.instance.clone()),
            HistoryOrigin::Genesis => None,
        })
        .ok_or_else(|| corrupt(&history.subject, "history has no state-producing record"))?;
    match extension {
        Some((records, state)) => {
            verify_subject_history_extension(history, records, state, &terminal)?;
        }
        None => {
            verify_subject_history(history, &terminal)?;
        }
    }
    Ok(terminal)
}

fn validate_projection_sets(
    authority: &Authority,
    projections: &[eventlog_core::CapturedProjection],
    model: &CapturedModel,
) -> Result<(), AsyncStoreError> {
    if projections.len() != 4
        || projections
            .iter()
            .map(|p| p.specification.name)
            .ne(projection_specs().iter().map(|p| p.name))
    {
        return Err(integrity(
            "native capture did not return the exact projection set",
        ));
    }
    let expected = expected_projection_rows(authority, model)?;
    for (capture, expected_rows) in projections.iter().zip(expected) {
        let actual: BTreeMap<_, _> = capture.rows.iter().cloned().collect();
        if actual.len() != capture.rows.len() || actual != expected_rows {
            return Err(integrity(format!(
                "projection {} differs from authoritative events",
                capture.specification.name
            )));
        }
    }
    Ok(())
}

fn expected_projection_rows(
    authority: &Authority,
    model: &CapturedModel,
) -> Result<[BTreeMap<String, Value>; 4], AsyncStoreError> {
    let mut binding = BTreeMap::new();
    if let Some(physical) = &model.binding {
        let digest = model
            .binding_blob_digest
            .as_ref()
            .ok_or_else(|| integrity("binding has no admitted blob digest"))?;
        binding.insert(
            "singleton".to_owned(),
            json!(["er.eventlog.binding-index/1", {
                "authority":authority,
                "binding_blob":digest,
                "physical":physical,
            }]),
        );
    }

    let mut records = BTreeMap::new();
    for (record_id, lookup) in &model.records {
        let physical = model
            .record_physical
            .get(record_id)
            .ok_or_else(|| integrity("record has no authoritative physical reference"))?;
        // The digests this capture admitted for this record. A row names the same digest the
        // capture verified, so hashing the bytes again here would only re-derive it.
        let digests = model
            .record_blob_digests
            .get(record_id)
            .ok_or_else(|| integrity("record has no admitted blob digests"))?;
        let row = match lookup {
            RecordLookup::Committed(saved) => {
                // The row no longer reads the batch's bytes, but a committed record whose batch
                // this model does not hold is still the refusal it always was.
                if !model.batches.contains_key(&saved.receipt.batch_key) {
                    return Err(integrity("committed record has no authoritative batch"));
                }
                json!(["er.eventlog.record-index/1", {"entry": {
                    "kind":"committed", "authority":authority, "record_id":record_id,
                    "subject":SubjectWire::from(&saved.entry.subject()),
                    "record_kind":record_kind_name(saved.entry.kind()),
                    "revision":saved.entry.revision(),
                    "batch_key":crate::encoding::BatchKeyWire::from(&saved.receipt.batch_key),
                    "member_index":saved.receipt.member_index,
                    "record_blob":digests.record,
                    "request_blob":digests.request.as_ref()
                        .ok_or_else(|| integrity("committed record has no admitted request blob"))?,
                    "batch_blob":model.batch_blob_digests.get(&saved.receipt.batch_key)
                        .ok_or_else(|| integrity("committed record has no admitted batch blob"))?,
                    "physical":physical,
                }}])
            }
            RecordLookup::Imported(saved) => {
                let subject = saved.entry.subject();
                let anchor = model
                    .anchor_blob_digests
                    .get(&subject)
                    .ok_or_else(|| integrity("imported record has no authoritative anchor"))?;
                json!(["er.eventlog.record-index/1", {"entry": {
                    "kind":"imported", "authority":authority, "record_id":record_id,
                    "subject":SubjectWire::from(&subject),
                    "record_kind":record_kind_name(saved.entry.kind()),
                    "revision":saved.entry.revision(),
                    "anchor_blob":anchor,
                    "evidence_index":imported_evidence_index(model, &subject, record_id)?,
                    "record_blob":digests.record,
                    "anchor_physical":physical,
                }}])
            }
        };
        records.insert(record_key(authority, record_id)?, row);
    }

    let mut batches = BTreeMap::new();
    for (key, batch) in &model.batches {
        let mut members = Vec::with_capacity(batch.records.len());
        for saved in &batch.records {
            let physical = model
                .record_physical
                .get(saved.entry.record_id())
                .ok_or_else(|| integrity("batch member has no physical reference"))?;
            let digests = model
                .record_blob_digests
                .get(saved.entry.record_id())
                .ok_or_else(|| integrity("batch member has no admitted blob digests"))?;
            members.push(json!({
                "member_index":saved.receipt.member_index,
                "record_id":saved.entry.record_id(),
                "record_key":record_key(authority, saved.entry.record_id())?,
                "subject":SubjectWire::from(&saved.entry.subject()),
                "record_kind":record_kind_name(saved.entry.kind()),
                "revision":saved.entry.revision(),
                "record_blob":digests.record,
                "request_blob":digests.request.as_ref()
                    .ok_or_else(|| integrity("batch member has no admitted request blob"))?,
                "physical":physical,
            }));
        }
        batches.insert(
            physical_batch_key(authority, key)?,
            json!(["er.eventlog.batch-index/1", {
                "authority":authority,
                "batch_key":crate::encoding::BatchKeyWire::from(key),
                "batch_blob":model.batch_blob_digests.get(key)
                    .ok_or_else(|| integrity("batch has no admitted batch blob"))?,
                "members":members,
            }]),
        );
    }

    let mut subjects = BTreeMap::new();
    for (subject, history) in &model.histories {
        let terminal = model
            .terminals
            .get(subject)
            .ok_or_else(|| integrity("subject has no terminal state"))?;
        let anchor = model.anchor_blob_digests.get(subject);
        let anchor_physical = model.anchor_physical.get(subject);
        let origin = match (&history.origin, anchor, anchor_physical) {
            (HistoryOrigin::Genesis, None, None) => json!({"kind":"genesis"}),
            (HistoryOrigin::Imported(_), Some(digest), Some(physical)) => json!({
                "kind":"imported", "anchor_blob":digest,
                "anchor_physical":physical,
            }),
            _ => {
                return Err(integrity(
                    "subject origin does not match its authoritative anchor",
                ));
            }
        };
        let last_decision = history.records.iter().rev().find(|saved| {
            matches!(
                saved.entry,
                entity_store::asynchronous::RecordedEntry::Decision(_)
            )
        });
        let state_source = if let Some(saved) = last_decision {
            json!({"kind":"decision", "record_blob":model.record_blob_digests
                .get(saved.entry.record_id())
                .ok_or_else(|| integrity("subject head has no admitted record blob"))?
                .record})
        } else if let Some(digest) = anchor {
            json!({"kind":"anchor", "anchor_blob":digest})
        } else {
            return Err(integrity("subject has no authoritative state source"));
        };
        let physical_head = history
            .records
            .last()
            .and_then(|saved| model.record_physical.get(saved.entry.record_id()))
            .or(anchor_physical)
            .ok_or_else(|| integrity("subject has no authoritative physical head"))?;
        subjects.insert(
            subject_key(authority, subject)?,
            json!(["er.eventlog.subject-index/1", {
                "authority":authority, "subject":SubjectWire::from(subject), "origin":origin,
                "revision":terminal.revision, "state_source":state_source,
                "physical_head":physical_head,
            }]),
        );
    }
    Ok([binding, records, batches, subjects])
}

fn imported_evidence_index(
    model: &CapturedModel,
    subject: &Subject,
    record_id: &str,
) -> Result<usize, AsyncStoreError> {
    let history = model
        .histories
        .get(subject)
        .ok_or_else(|| integrity("imported record has no subject history"))?;
    let HistoryOrigin::Imported(anchor) = &history.origin else {
        return Err(integrity("imported record belongs to a genesis history"));
    };
    anchor
        .evidence
        .iter()
        .position(|item| matches!(item, entity_store::asynchronous::LegacyEvidence::Envelope(saved) if saved.entry.record_id() == record_id))
        .ok_or_else(|| integrity("imported record is absent from its anchor evidence"))
}

fn record_kind_name(kind: entity_store::asynchronous::RecordKind) -> &'static str {
    match kind {
        entity_store::asynchronous::RecordKind::Decision => "decision",
        entity_store::asynchronous::RecordKind::Observation => "observation",
    }
}

fn reference_digest(event: &RecordedEvent) -> Result<&str, AsyncStoreError> {
    let object = event
        .data
        .as_object()
        .ok_or_else(|| integrity("reference body is not an object"))?;
    if object.len() != 1 {
        return Err(integrity("reference body has unknown fields"));
    }
    object
        .get("blob")
        .and_then(Value::as_str)
        .ok_or_else(|| integrity("reference body has no blob"))
}
fn verify_digest(domain: &str, digest: &str, bytes: &[u8]) -> Result<(), AsyncStoreError> {
    if framed_key(domain, bytes)? == digest {
        Ok(())
    } else {
        Err(integrity("blob digest/domain mismatch"))
    }
}
fn require_authority(expected: &Authority, found: &Authority) -> Result<(), AsyncStoreError> {
    if expected == found {
        Ok(())
    } else {
        Err(integrity("reference substitutes authority"))
    }
}
fn corrupt(subject: &Subject, detail: impl Into<String>) -> AsyncStoreError {
    AsyncStoreError::CorruptHistory {
        subject: subject.clone(),
        detail: detail.into(),
    }
}
fn integrity(detail: impl Into<String>) -> AsyncStoreError {
    AsyncStoreError::ProviderIntegrity {
        provider: "eventlog".into(),
        detail: detail.into(),
    }
}
fn input_eventlog(error: EventLogError) -> AsyncStoreError {
    AsyncStoreError::InvalidInput(error.to_string())
}
fn map_read_error(error: EventLogError) -> AsyncStoreError {
    match error {
        EventLogError::Backend(v) => AsyncStoreError::Backend(v),
        EventLogError::Closed => AsyncStoreError::Unreachable {
            provider: "eventlog".into(),
            detail: "closed".into(),
        },
        other => integrity(other.to_string()),
    }
}
fn map_capture(error: CaptureError) -> AsyncStoreError {
    match error {
        CaptureError::Store(error) => map_read_error(error),
        other => integrity(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eventlog_core::BlobMigrationReport;

    fn subject() -> Subject {
        Subject::new("ticket", "fault-matrix").expect("subject")
    }

    /// `import_anchors` hands its blobs and its group to one port call, so a failure does not say
    /// which half produced it and every error of that call is mapped as an append. That is only
    /// sound while the blob half cannot produce a variant the two mappings disagree about.
    /// `EventStore::put_blob` documents exactly one — `Invalid`, for a digest that is not the
    /// bytes — and on that one the mappings are the same value.
    ///
    /// They are not the same value on others, which is why this is pinned rather than assumed:
    /// a later `put_blob` that returns `Overloaded`, `Deadline`, `NotFound` or
    /// `CausationDepthExceeded` would be reported as something `map_put_error` would not have
    /// said, and the first sign of it would be a caller matching on the wrong variant.
    #[test]
    fn the_blob_and_append_mappings_agree_on_the_only_variant_put_blob_documents() {
        assert_eq!(
            map_put_error(EventLogError::Invalid("digest is not the bytes".into())),
            map_append_error(EventLogError::Invalid("digest is not the bytes".into())),
            "a blob fault routed as an append must reach the caller unchanged"
        );

        for divergent in [
            EventLogError::Overloaded,
            EventLogError::Deadline { operation: "put" },
            EventLogError::NotFound,
            EventLogError::CausationDepthExceeded { depth: 9, limit: 8 },
        ] {
            assert_ne!(
                map_put_error(divergent.clone()),
                map_append_error(divergent.clone()),
                "{divergent:?} maps differently, so it must stay outside what put_blob promises"
            );
        }
    }

    #[test]
    fn every_guard_refusal_has_a_stable_code_and_typed_match() {
        let key = BatchKey::Named("fault-matrix".into());
        let cases = [
            (
                GuardRefusalCode::RevisionConflict,
                AsyncStoreError::RevisionConflict {
                    subject: subject(),
                    expected: Expect::Absent,
                    found: Some(1),
                },
                "er_revision_conflict",
            ),
            (
                GuardRefusalCode::RecordConflict,
                AsyncStoreError::RecordConflict {
                    record_id: "record".into(),
                },
                "er_record_conflict",
            ),
            (
                GuardRefusalCode::BatchConflict,
                AsyncStoreError::BatchConflict { key: key.clone() },
                "er_batch_conflict",
            ),
            (
                GuardRefusalCode::PreviouslyRecordedBatchEntries,
                AsyncStoreError::PreviouslyRecordedBatchEntries { indices: vec![0] },
                "er_previously_recorded_batch_entries",
            ),
            (
                GuardRefusalCode::CorruptHistory,
                AsyncStoreError::CorruptHistory {
                    subject: subject(),
                    detail: "corrupt".into(),
                },
                "er_corrupt_history",
            ),
            (
                GuardRefusalCode::ProviderIntegrity,
                integrity("provider"),
                "er_provider_integrity",
            ),
        ];
        for (code, error, wire) in cases {
            assert_eq!(code.as_str(), wire);
            assert_eq!(GuardRefusalCode::for_error(&error), code);
            assert!(code.matches(&error));
        }
    }

    #[test]
    fn every_eventlog_append_error_has_an_explicit_er_classification() {
        let cases = [
            (EventLogError::Invalid("invalid".into()), "integrity"),
            (
                EventLogError::GuardRefused {
                    code: "foreign".into(),
                },
                "integrity",
            ),
            (EventLogError::Overloaded, "unreachable"),
            (EventLogError::Closed, "unreachable"),
            (
                EventLogError::Deadline {
                    operation: "append",
                },
                "unreachable",
            ),
            (EventLogError::UnknownCommit, "integrity"),
            (EventLogError::BlobMigrationCommitUnknown, "integrity"),
            (
                EventLogError::BlobMigrationCompleted {
                    report: BlobMigrationReport {
                        upgraded: true,
                        trusted_legacy_rows: 1,
                    },
                    cleanup: Box::new(EventLogError::Backend("cleanup".into())),
                },
                "integrity",
            ),
            (
                EventLogError::Conflict {
                    expected: 0,
                    actual: 1,
                },
                "integrity",
            ),
            (
                EventLogError::IdempotencyMismatch { key: "key".into() },
                "integrity",
            ),
            (
                EventLogError::CausationDepthExceeded { depth: 2, limit: 1 },
                "integrity",
            ),
            (EventLogError::NotFound, "integrity"),
            (EventLogError::Backend("backend".into()), "backend"),
        ];
        for (error, class) in cases {
            let mapped = map_append_error(error);
            assert!(
                matches!(
                    (&mapped, class),
                    (AsyncStoreError::ProviderIntegrity { .. }, "integrity")
                        | (AsyncStoreError::Unreachable { .. }, "unreachable")
                        | (AsyncStoreError::Backend(_), "backend")
                ),
                "unexpected {class} classification: {mapped:?}"
            );
        }
    }
}

/// The cost of one seeded open, and the verifications that cost pays for.
///
/// `build_model` is the whole of a capture that is not the provider read, and it is measured here
/// against a capture this module builds itself rather than against a provider: a hand-built
/// [`TenantCapture`] is deterministic to the byte — a file store mints `stream_identity` and every
/// `event_id` from `new_event_id()` — which is what lets the model be pinned by digest at all.
#[cfg(test)]
mod seeded_open {
    use std::{cell::Cell, time::Instant};

    use entity_core::{Registry, Runtime};
    use entity_store::{
        Recording,
        asynchronous::{AppendMember, RecordedEntry},
    };
    use eventlog_core::{CapturedBlob, CapturedProjection};
    use sha2::{Digest, Sha256};

    use super::*;

    const TENANT: &str = "seeded-open-fixture";

    /// Records a gate run builds when nothing asks for the measured shape.
    ///
    /// The guards below are about *ratios* and *refusals*, both of which hold at any size, so the
    /// gate pays for eight records and the measurement asks for the real one by environment.
    const GATE_RECORDS: usize = 8;

    /// The largest blob a gate run builds. The real store's is 5.9 MB; carrying that into every
    /// gate would buy one more data point and cost every run thirty seconds.
    const GATE_MAX_BODY: usize = 64 * 1024;

    fn authority() -> Authority {
        Authority {
            logical_scope: "seeded-open-fixture-scope".to_owned(),
            tenant: TENANT.to_owned(),
            stream_identity: "seeded-open-fixture-identity".to_owned(),
        }
    }

    fn registry() -> Registry {
        let definition = serde_json::from_value(json!({
            "entity": "ticket",
            "version": 1,
            "schema": { "fields": { "body": { "type": "string", "required": true } } },
            "lifecycle": { "initial": "open", "states": ["open"] },
            "operations": {
                "touch": {
                    "transitions": [{ "from": "open", "to": "open" }],
                    "arguments": { "fields": {} },
                    "emits": []
                }
            }
        }))
        .expect("definition parses");
        let mut registry = Registry::new();
        registry.register(definition).expect("definition validates");
        registry
    }

    /// Deterministic heavy-tailed payload lengths.
    ///
    /// A real planning store's blobs are skewed — median 2,268 B, mean 9,526 B, max 5.9 MB — and a
    /// uniform fixture would hide exactly the per-byte costs this unit is about. One record binds
    /// four blobs whose sizes are a fixed multiple of its payload, so the payloads are lognormal
    /// about the median that *produces* that blob median, with the one extreme member placed
    /// explicitly; at 1,953 records the multiset lands on the store's own three statistics. The
    /// generator is a fixed-seed xorshift, so the fixture and the pinned model digest below are
    /// reproducible.
    fn body_lengths(records: usize, max_body: usize) -> Vec<usize> {
        let mut state: u64 = 0x2545_f491_4f6c_dd1d;
        let mut next = || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            f64::from(u32::try_from(state >> 32).expect("shifted to 32 bits")) / 4_294_967_296.0
        };
        let mut lengths = Vec::with_capacity(records);
        for _ in 0..records {
            let normal: f64 = (0..12).map(|_| next()).sum::<f64>() - 6.0;
            let length = (1127.0 * (1.412 * normal).exp()) as usize;
            lengths.push(length.max(16));
        }
        if let Some(first) = lengths.first_mut() {
            *first = max_body;
        }
        lengths
    }

    fn recording(record_id: &str) -> Recording {
        Recording {
            record_id: record_id.to_owned(),
            recorded_at: "2026-09-22T00:00:00Z".to_owned(),
            correlation: Some("seeded-open-fixture".to_owned()),
            causation: None,
            actor: None,
        }
    }

    fn recorded(index: usize, body_len: usize) -> RecordedEntry {
        let registry = registry();
        let decision = Runtime::new(&registry)
            .create(
                "ticket",
                1,
                format!("subject-{index:06}"),
                json!({ "body": "x".repeat(body_len) }),
            )
            .expect("creation decision");
        RecordedEntry::Decision(
            entity_store::RecordedCommit::new(decision, &recording(&format!("record-{index:06}")))
                .expect("recorded commit"),
        )
    }

    fn event(
        tenant: &TenantId,
        global_seq: u64,
        stream_type: &str,
        stream_id: &str,
        name: &str,
        digest: &str,
    ) -> RecordedEvent {
        RecordedEvent {
            global_seq,
            tenant: tenant.clone(),
            stream_type: stream_type.to_owned(),
            stream_id: stream_id.to_owned(),
            version: 1,
            event_id: format!("event-{global_seq:08}"),
            name: name.to_owned(),
            schema_version: 1,
            occurred_at: OffsetDateTime::UNIX_EPOCH,
            recorded_at: OffsetDateTime::UNIX_EPOCH,
            subject: "seeded-open".to_owned(),
            actor: "seeded-open".to_owned(),
            request_id: format!("request-{global_seq:08}"),
            trace_id: format!("trace-{global_seq:08}"),
            causation_id: None,
            causation_depth: 0,
            redacted_at: None,
            data: json!({ "blob": digest }),
            digest: None,
            parents: Vec::new(),
        }
    }

    /// One binding and `records` committed single-record entries, exactly as `append_inner` binds
    /// them: a batch blob, a record blob, a request blob and an entry wrapper blob per record.
    fn fixture(records: usize, max_body: usize) -> (Authority, TenantCapture) {
        let authority = authority();
        let tenant = TenantId::new(TENANT.to_owned()).expect("fixture tenant");
        let mut blobs: Vec<CapturedBlob> = Vec::new();
        let mut events: Vec<RecordedEvent> = Vec::new();

        let binding_bytes = encode_binding(&authority).expect("binding encodes");
        let binding_digest =
            framed_key(BINDING_BLOB_DOMAIN, &binding_bytes).expect("binding digest");
        events.push(event(
            &tenant,
            1,
            "er.binding",
            "singleton",
            "er.binding",
            &binding_digest,
        ));
        blobs.push(CapturedBlob {
            digest: binding_digest,
            bytes: binding_bytes,
        });

        for (index, body_len) in body_lengths(records, max_body).into_iter().enumerate() {
            let entry = recorded(index, body_len);
            let subject = entry.subject();
            let key = BatchKey::SingleRecord(format!("record-{index:06}"));
            let request_bytes =
                original_request_comparison_bytes(&entry).expect("request comparison bytes");
            let member = AppendMember::new(Expect::Absent, entry.clone(), request_bytes.clone());
            let batch_bytes = batch_comparison_bytes(&key, std::slice::from_ref(&member))
                .expect("batch comparison bytes");
            let record_bytes = record_comparison_bytes(&entry).expect("record comparison bytes");
            let batch_blob = framed_key(BATCH_BLOB_DOMAIN, &batch_bytes).expect("batch digest");
            let record_blob = framed_key(RECORD_BLOB_DOMAIN, &record_bytes).expect("record digest");
            let request_blob =
                framed_key(REQUEST_BLOB_DOMAIN, &request_bytes).expect("request digest");
            let wrapper = RecordedEntryWrapper {
                authority: authority.clone(),
                batch_blob: batch_blob.clone(),
                batch_key: crate::encoding::BatchKeyWire::from(&key),
                member_index: 0,
                record_blob: record_blob.clone(),
                request_blob: request_blob.clone(),
                subject: SubjectWire::from(&subject),
            };
            let wrapper_bytes = encode_entry(&wrapper).expect("wrapper encodes");
            let wrapper_digest =
                framed_key(ENTRY_BLOB_DOMAIN, &wrapper_bytes).expect("wrapper digest");
            let stream_id = subject_stream_id(&authority, &subject).expect("subject stream");
            events.push(event(
                &tenant,
                u64::try_from(index).expect("fixture index") + 2,
                "er.subject",
                &stream_id,
                "er.recorded_entry",
                &wrapper_digest,
            ));
            for (digest, bytes) in [
                (batch_blob, batch_bytes),
                (record_blob, record_bytes),
                (request_blob, request_bytes),
                (wrapper_digest, wrapper_bytes),
            ] {
                blobs.push(CapturedBlob { digest, bytes });
            }
        }
        blobs.sort_by(|left, right| left.digest.as_bytes().cmp(right.digest.as_bytes()));

        let unvalidated = TenantCapture {
            tenant: tenant.clone(),
            stream_identity: authority.stream_identity.clone(),
            events: events.clone(),
            blobs: blobs.clone(),
            projections: Vec::new(),
        };
        // The rows are derived from the model, which makes `validate_projection_sets` cost what it
        // costs on a real capture but proves nothing about the rows: a row naming the wrong digest
        // would agree with itself here. Row correctness is pinned by the provider-backed cases in
        // `tests/providers.rs`, which read what the inline projector actually wrote, and that was
        // measured — swapping `record_blob` for `request_blob` in the record row leaves this module
        // green and turns three of those red.
        let model = build_model_events(&authority, &unvalidated, 0).expect("fixture model");
        let rows = expected_projection_rows(&authority, &model).expect("fixture projection rows");
        let projections: Vec<CapturedProjection> = projection_specs()
            .iter()
            .copied()
            .zip(rows)
            .map(|(specification, rows)| CapturedProjection {
                specification,
                rows: rows.into_iter().collect(),
            })
            .collect();
        let capture = TenantCapture {
            tenant,
            stream_identity: authority.stream_identity.clone(),
            events,
            blobs,
            projections,
        };
        (authority, capture)
    }

    /// One binding and `subjects` import anchors, each carrying **two** evidence envelopes.
    ///
    /// Two, not one, because the digest an imported record's row names is now taken from the
    /// envelope at that record's own position instead of from a fresh hash. Every imported case in
    /// this crate's suite anchors exactly one envelope, so taking the first envelope for every
    /// record passes all of them — measured, not assumed.
    fn imported_fixture(subjects: usize) -> (Authority, TenantCapture) {
        let authority = authority();
        let tenant = TenantId::new(TENANT.to_owned()).expect("fixture tenant");
        let registry = registry();
        let mut blobs: Vec<CapturedBlob> = Vec::new();
        let mut events: Vec<RecordedEvent> = Vec::new();

        let binding_bytes = encode_binding(&authority).expect("binding encodes");
        let binding_digest =
            framed_key(BINDING_BLOB_DOMAIN, &binding_bytes).expect("binding digest");
        events.push(event(
            &tenant,
            1,
            "er.binding",
            "singleton",
            "er.binding",
            &binding_digest,
        ));
        blobs.push(CapturedBlob {
            digest: binding_digest,
            bytes: binding_bytes,
        });

        for index in 0..subjects {
            let runtime = Runtime::new(&registry);
            let created = runtime
                .create(
                    "ticket",
                    1,
                    format!("imported-{index:06}"),
                    json!({"body":"first"}),
                )
                .expect("creation decision");
            let first = entity_store::RecordedCommit::new(
                created,
                &recording(&format!("imported-{index:06}-create")),
            )
            .expect("first record");
            let touched = runtime
                .execute(&first.instance, "touch", json!({}))
                .expect("execution decision");
            let second = entity_store::RecordedCommit::new(
                touched,
                &recording(&format!("imported-{index:06}-touch")),
            )
            .expect("second record");
            let terminal = second.instance.clone();
            let history = SubjectHistory {
                subject: Subject::new("ticket", format!("imported-{index:06}"))
                    .expect("imported subject"),
                origin: HistoryOrigin::Imported(entity_store::asynchronous::LegacyAnchor {
                    instance: terminal,
                    completeness:
                        entity_store::asynchronous::LegacyCompleteness::AvailableEvidenceOnly,
                    order: entity_store::asynchronous::LegacyOrderDeclaration::PerKindOnly,
                    evidence: vec![
                        envelope(RecordedEntry::Decision(first), 0),
                        envelope(RecordedEntry::Decision(second), 1),
                    ],
                }),
                records: Vec::new(),
            };
            let mut record_blobs = Vec::new();
            let HistoryOrigin::Imported(anchor) = &history.origin else {
                unreachable!("the fixture just built an imported origin")
            };
            for evidence in &anchor.evidence {
                if let entity_store::asynchronous::LegacyEvidence::Envelope(saved) = evidence {
                    let bytes = record_comparison_bytes(&saved.entry).expect("record bytes");
                    let digest = framed_key(RECORD_BLOB_DOMAIN, &bytes).expect("record digest");
                    record_blobs.push(digest.clone());
                    blobs.push(CapturedBlob { digest, bytes });
                }
            }
            let wrapper = anchor_from_history(authority.clone(), &history, &record_blobs)
                .expect("anchor wrapper");
            let anchor_bytes = encode_anchor(&wrapper).expect("anchor encodes");
            let anchor_digest =
                framed_key(ANCHOR_BLOB_DOMAIN, &anchor_bytes).expect("anchor digest");
            let stream_id =
                subject_stream_id(&authority, &history.subject).expect("subject stream");
            events.push(event(
                &tenant,
                u64::try_from(index).expect("fixture index") + 2,
                "er.subject",
                &stream_id,
                "er.import_anchor",
                &anchor_digest,
            ));
            blobs.push(CapturedBlob {
                digest: anchor_digest,
                bytes: anchor_bytes,
            });
        }
        blobs.sort_by(|left, right| left.digest.as_bytes().cmp(right.digest.as_bytes()));

        let model = build_model_events(
            &authority,
            &TenantCapture {
                tenant: tenant.clone(),
                stream_identity: authority.stream_identity.clone(),
                events: events.clone(),
                blobs: blobs.clone(),
                projections: Vec::new(),
            },
            0,
        )
        .expect("imported fixture model");
        let rows = expected_projection_rows(&authority, &model).expect("fixture projection rows");
        let projections: Vec<CapturedProjection> = projection_specs()
            .iter()
            .copied()
            .zip(rows)
            .map(|(specification, rows)| CapturedProjection {
                specification,
                rows: rows.into_iter().collect(),
            })
            .collect();
        let capture = TenantCapture {
            tenant,
            stream_identity: authority.stream_identity.clone(),
            events,
            blobs,
            projections,
        };
        (authority, capture)
    }

    fn envelope(entry: RecordedEntry, order: u64) -> entity_store::asynchronous::LegacyEvidence {
        entity_store::asynchronous::LegacyEvidence::Envelope(
            entity_store::asynchronous::ImportedRecordEvidence::new(
                entry,
                "seeded-open-fixture".to_owned(),
                format!("records/{order}"),
                entity_store::asynchronous::KnownLegacyOrder::PerKind(order),
            )
            .expect("imported evidence"),
        )
    }

    /// Every field of the model, rendered and hashed.
    ///
    /// `Debug` rather than `Serialize` because these types do not all serialize, and because a
    /// derived `Debug` prints every field of every member: a model that differs anywhere differs
    /// here.
    fn model_digest(model: &CapturedModel) -> String {
        let mut hasher = Sha256::new();
        // A linear store's records carry no lineage. The field postdates the pin below, so it is
        // left out of the text the pin covers while it is empty; a record that does carry one
        // still changes the digest.
        let pinned = |text: String| text.replace(", lineage: None", "");
        for part in [
            format!("{:?}", model.binding),
            format!(
                "{:?}",
                (
                    model.held.events,
                    model.held.blobs,
                    model.held.rows,
                    &model.held.digests
                )
            ),
            pinned(format!("{:?}", model.histories)),
            format!("{:?}", model.terminals),
            pinned(format!("{:?}", model.records)),
            format!("{:?}", model.record_physical),
            pinned(format!("{:?}", model.batches)),
            format!("{:?}", model.anchors),
            format!("{:?}", model.anchor_physical),
        ] {
            hasher.update(part.as_bytes());
            hasher.update([0u8]);
        }
        format!("{:x}", hasher.finalize())
    }

    fn charged<T>(work: impl FnOnce() -> T) -> (T, u64) {
        HASHED_BYTES.with(|charged| charged.set(0));
        let value = work();
        (value, HASHED_BYTES.with(Cell::get))
    }

    fn captured_bytes(capture: &TenantCapture) -> u64 {
        capture
            .blobs
            .iter()
            .map(|blob| blob.bytes.len() as u64)
            .sum()
    }

    /// The defect this unit exists for, stated as a ratio rather than as a duration.
    ///
    /// A seeded open reads every bound blob once. `build_model` hashed each of them between three
    /// and five times — once where the reference names it, once more for every projection row that
    /// repeats the digest, and once per batch member for the batch blob they share — and a capture
    /// cannot need more SHA-256 than it has bytes.
    #[test]
    fn a_capture_is_hashed_once_over_rather_than_several_times_over() {
        let (authority, capture) = fixture(GATE_RECORDS, GATE_MAX_BODY);
        let bytes = captured_bytes(&capture);
        let (model, hashed) = charged(|| build_model(&authority, &capture).expect("model builds"));
        assert!(model.binding.is_some(), "the fixture binds its authority");
        assert!(
            hashed <= bytes,
            "build_model hashed {hashed} bytes of a {bytes}-byte capture"
        );
    }

    /// The invariant the unit is worth nothing without.
    ///
    /// Pinned from the verifying path at base `b652c6ca`, on a fixture that is deterministic to the
    /// byte. Any change to what the model holds — a field dropped, a digest carried instead of
    /// recomputed but not the same digest, a record decoded differently — moves this.
    #[test]
    fn the_model_is_byte_identical_to_the_verifying_path() {
        let (authority, capture) = fixture(GATE_RECORDS, GATE_MAX_BODY);
        let model = build_model(&authority, &capture).expect("model builds");
        assert_eq!(
            model_digest(&model),
            "ce4e957f73231f0beb65058ae11742893aaa0c3688781b528d87ef93f7d2539e",
            "the model this open produces is not the model the verifying path produced"
        );
    }

    /// A capture of `whole`'s first `events` events, binding every blob `whole` binds — orphans
    /// are part of a real capture too — with the projection rows those events produce.
    fn prefix_of(authority: &Authority, whole: &TenantCapture, events: usize) -> TenantCapture {
        let mut prefix = TenantCapture {
            events: whole.events[..events].to_vec(),
            projections: Vec::new(),
            ..whole.clone()
        };
        let model = build_model_events(authority, &prefix, 0).expect("prefix model");
        let rows = expected_projection_rows(authority, &model).expect("prefix rows");
        prefix.projections = projection_specs()
            .iter()
            .copied()
            .zip(rows)
            .map(|(specification, rows)| CapturedProjection {
                specification,
                rows: rows.into_iter().collect(),
            })
            .collect();
        prefix
    }

    /// The advance is worth nothing unless it lands where a whole build lands.
    ///
    /// Both fixtures, split at every event: the model verified from the prefix and advanced by the
    /// rest must be the whole build's model field for field, and must have decoded only the
    /// records the advanced events carry.
    #[test]
    fn a_model_advanced_by_appended_events_is_the_model_a_whole_build_produces() {
        for (label, (authority, whole)) in [
            ("committed", fixture(GATE_RECORDS, GATE_MAX_BODY)),
            ("imported", imported_fixture(GATE_RECORDS)),
        ] {
            let built = build_model(&authority, &whole).expect("whole build");
            for from in 1..=whole.events.len() {
                let prefix = prefix_of(&authority, &whole, from);
                let mut advanced = build_model(&authority, &prefix).expect("prefix build");
                let decoded_before = advanced.decoded;
                advanced.decoded = 0;
                advance_model(&authority, &mut advanced, &whole, from).expect("advance");
                assert_eq!(
                    model_digest(&advanced),
                    model_digest(&built),
                    "{label}: advancing from event {from} is not the whole build"
                );
                assert_eq!(
                    decoded_before + advanced.decoded,
                    built.decoded,
                    "{label}: advancing from event {from} decoded a record twice or not at all"
                );
            }
        }
    }

    /// Binds one committed single-record entry the way `append_inner` binds it.
    fn push_record(
        authority: &Authority,
        capture: &mut TenantCapture,
        entry: RecordedEntry,
        expect: Expect,
        version: u64,
    ) {
        let subject = entry.subject();
        let key = BatchKey::SingleRecord(entry.record_id().to_owned());
        let request_bytes = original_request_comparison_bytes(&entry).expect("request bytes");
        let member = AppendMember::new(expect, entry.clone(), request_bytes.clone());
        let batch_bytes =
            batch_comparison_bytes(&key, std::slice::from_ref(&member)).expect("batch bytes");
        let record_bytes = record_comparison_bytes(&entry).expect("record bytes");
        let batch_blob = framed_key(BATCH_BLOB_DOMAIN, &batch_bytes).expect("batch digest");
        let record_blob = framed_key(RECORD_BLOB_DOMAIN, &record_bytes).expect("record digest");
        let request_blob = framed_key(REQUEST_BLOB_DOMAIN, &request_bytes).expect("request digest");
        let wrapper = RecordedEntryWrapper {
            authority: authority.clone(),
            batch_blob: batch_blob.clone(),
            batch_key: crate::encoding::BatchKeyWire::from(&key),
            member_index: 0,
            record_blob: record_blob.clone(),
            request_blob: request_blob.clone(),
            subject: SubjectWire::from(&subject),
        };
        let wrapper_bytes = encode_entry(&wrapper).expect("wrapper encodes");
        let wrapper_digest = framed_key(ENTRY_BLOB_DOMAIN, &wrapper_bytes).expect("wrapper digest");
        let stream_id = subject_stream_id(authority, &subject).expect("subject stream");
        let global_seq = capture
            .events
            .last()
            .map_or(1, |event| event.global_seq + 1);
        let mut recorded = event(
            &capture.tenant,
            global_seq,
            "er.subject",
            &stream_id,
            "er.recorded_entry",
            &wrapper_digest,
        );
        recorded.version = version;
        capture.events.push(recorded);
        for (digest, bytes) in [
            (batch_blob, batch_bytes),
            (record_blob, record_bytes),
            (request_blob, request_bytes),
            (wrapper_digest, wrapper_bytes),
        ] {
            capture.blobs.push(CapturedBlob { digest, bytes });
        }
        capture
            .blobs
            .sort_by(|left, right| left.digest.as_bytes().cmp(right.digest.as_bytes()));
        capture
            .blobs
            .dedup_by(|left, right| left.digest == right.digest);
    }

    /// A subject created in the verified prefix and moved on by the advanced suffix, so the
    /// advance verifies that subject from its verified state rather than from its origin — and a
    /// suffix record that does not follow that state is refused by the advance as by a whole build.
    #[test]
    fn an_advanced_subject_is_verified_from_its_verified_state_and_refused_where_it_breaks() {
        let authority = authority();
        let (_, base) = fixture(1, 64);
        let registry = registry();
        let created = match &base.events[..] {
            [_, record] => {
                let digest = reference_digest(record).expect("reference");
                let blob = base
                    .blobs
                    .iter()
                    .find(|blob| blob.digest == digest)
                    .expect("wrapper bound");
                let wrapper = decode_entry(&blob.bytes).expect("wrapper");
                let record = base
                    .blobs
                    .iter()
                    .find(|blob| blob.digest == wrapper.record_blob)
                    .expect("record bound");
                decode_record(&record.bytes).expect("record")
            }
            _ => panic!("one binding and one record"),
        };
        let RecordedEntry::Decision(commit) = &created else {
            panic!("the fixture creates")
        };
        let touched = Runtime::new(&registry)
            .execute(&commit.instance, "touch", json!({}))
            .expect("touch decides");
        let touch = RecordedEntry::Decision(
            entity_store::RecordedCommit::new(touched, &recording("record-touch"))
                .expect("touch commit"),
        );
        let prefix = prefix_of(&authority, &base, base.events.len());
        for (label, expect, sound) in [
            ("sound", Expect::Revision(1), true),
            (
                "an expectation its predecessor does not meet",
                Expect::Absent,
                false,
            ),
        ] {
            let mut whole = TenantCapture {
                projections: Vec::new(),
                ..base.clone()
            };
            push_record(&authority, &mut whole, touch.clone(), expect, 2);
            let whole = if sound {
                prefix_of(&authority, &whole, whole.events.len())
            } else {
                // The rows a sound capture would carry: the refusal must come from the history.
                let mut sound_whole = TenantCapture {
                    projections: Vec::new(),
                    ..base.clone()
                };
                push_record(
                    &authority,
                    &mut sound_whole,
                    touch.clone(),
                    Expect::Revision(1),
                    2,
                );
                whole.projections =
                    prefix_of(&authority, &sound_whole, sound_whole.events.len()).projections;
                whole
            };
            let mut advanced = build_model(&authority, &prefix).expect("prefix build");
            let outcome = advance_model(&authority, &mut advanced, &whole, prefix.events.len());
            let built = build_model(&authority, &whole);
            if sound {
                outcome.expect("sound advance");
                assert_eq!(
                    model_digest(&advanced),
                    model_digest(&built.expect("sound whole build")),
                    "{label}: the advance is not the whole build"
                );
            } else {
                assert!(
                    matches!(built, Err(AsyncStoreError::CorruptHistory { .. })),
                    "{label}: the whole build is the reference and must refuse: {:?}",
                    built.as_ref().err()
                );
                assert!(
                    matches!(outcome, Err(AsyncStoreError::CorruptHistory { .. })),
                    "{label}: the advance admitted it: {outcome:?}"
                );
            }
        }
    }

    /// Where each verification happens now, named blob by blob.
    ///
    /// Acceptance item 3. Nothing was moved out of the open path: every one of the six domains is
    /// still checked inside `build_model`, and the enumeration is the point — a fix that
    /// de-duplicated hashing by dropping one domain's check would leave that domain's blob reaching
    /// a caller unverified, and would pass a test that only corrupted a record blob.
    #[test]
    fn every_bound_domain_is_still_refused_when_its_blob_is_not_its_digest() {
        let (authority, capture) = fixture(GATE_RECORDS, GATE_MAX_BODY);
        let domains: Vec<String> = capture
            .blobs
            .iter()
            .map(|blob| {
                for domain in [
                    BINDING_BLOB_DOMAIN,
                    ENTRY_BLOB_DOMAIN,
                    RECORD_BLOB_DOMAIN,
                    REQUEST_BLOB_DOMAIN,
                    BATCH_BLOB_DOMAIN,
                ] {
                    if crate::encoding::framed_key(domain, &blob.bytes).expect("digest")
                        == blob.digest
                    {
                        return domain.to_owned();
                    }
                }
                panic!("fixture blob {} belongs to no domain", blob.digest);
            })
            .collect();
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for (index, domain) in domains.iter().enumerate() {
            if !seen.insert(domain.as_str()) {
                continue;
            }
            let mut corrupted = capture.clone();
            corrupted.blobs[index].bytes.push(b' ');
            // The *refusal* is asserted, not merely that something refused. Every encoding here is
            // canonical, so a changed byte also fails to decode: a test that accepts any error
            // passes with the digest check deleted, which is how this one was first written and
            // how deleting `verify_digest` survived its own mutation.
            assert!(
                matches!(
                    build_model(&authority, &corrupted),
                    Err(AsyncStoreError::ProviderIntegrity { ref detail, .. })
                        if detail == "blob digest/domain mismatch"
                ),
                "a {domain} blob whose bytes are not its digest was not refused as one"
            );
        }
        assert_eq!(
            seen.len(),
            5,
            "the fixture must exercise every domain a committed authority binds: {seen:?}"
        );
    }

    /// The one arm of the single-hash admission that no other case reaches.
    ///
    /// A digest is now hashed once and remembered under the domain it was admitted for. A later
    /// reference naming a different domain is refused from that memory rather than from a second
    /// hash — the domain is framed into the hash, so only one domain can ever match a digest — and
    /// this pins that the refusal is still made.
    #[test]
    fn a_blob_named_under_two_domains_is_refused_without_being_hashed_twice() {
        let (authority, mut capture) = fixture(GATE_RECORDS, GATE_MAX_BODY);
        let reference = capture
            .events
            .iter()
            .find(|event| event.name == "er.recorded_entry")
            .map(|event| reference_digest(event).expect("entry reference").to_owned())
            .expect("the fixture commits records");
        let index = capture
            .blobs
            .iter()
            .position(|blob| blob.digest == reference)
            .expect("the entry wrapper is bound");
        let mut wrapper = decode_entry(&capture.blobs[index].bytes).expect("wrapper decodes");
        // One wrapper claims its record blob is also its request blob.
        wrapper.request_blob = wrapper.record_blob.clone();
        let bytes = encode_entry(&wrapper).expect("wrapper re-encodes");
        let digest = crate::encoding::framed_key(ENTRY_BLOB_DOMAIN, &bytes).expect("digest");
        capture.blobs[index] = CapturedBlob {
            digest: digest.clone(),
            bytes,
        };
        for event in &mut capture.events {
            if reference_digest(event).is_ok_and(|found| found == reference) {
                event.data = json!({ "blob": digest });
            }
        }
        assert!(
            matches!(
                build_model(&authority, &capture),
                Err(AsyncStoreError::ProviderIntegrity { ref detail, .. })
                    if detail == "blob digest/domain mismatch"
            ),
            "a blob bound under one domain was admitted under another"
        );
    }

    /// Every digest this model carries instead of recomputing is the digest of what it carries.
    ///
    /// The class, checked rather than listed. `build_model` stopped hashing the same bytes three
    /// and four times over by carrying forward the digest the capture admitted, and each place that
    /// stopped hashing is now a place that trusts a carried string. This walks the model and hashes
    /// every one of them exactly as the verifying path did, on both fixtures — committed records
    /// and imported anchors with two envelopes each — so a digest carried from the wrong place
    /// fails here without anyone having to think of the row it would have corrupted.
    #[test]
    fn every_carried_digest_is_the_digest_of_the_bytes_the_model_holds() {
        for (label, (authority, capture)) in [
            ("committed", fixture(GATE_RECORDS, GATE_MAX_BODY)),
            ("imported", imported_fixture(GATE_RECORDS)),
        ] {
            let model = build_model(&authority, &capture).expect("model builds");
            let binding = crate::encoding::framed_key(
                BINDING_BLOB_DOMAIN,
                &encode_binding(&authority).expect("binding encodes"),
            )
            .expect("binding digest");
            assert_eq!(
                model.binding_blob_digest.as_deref(),
                Some(binding.as_str()),
                "{label}: the binding digest is not the binding's"
            );
            let mut committed = 0usize;
            let mut imported = 0usize;
            for (record_id, lookup) in &model.records {
                let carried = model
                    .record_blob_digests
                    .get(record_id)
                    .unwrap_or_else(|| panic!("{label}: {record_id} carries no digests"));
                let (record_bytes, request) = match lookup {
                    RecordLookup::Committed(saved) => {
                        committed += 1;
                        (
                            saved.record_bytes.clone(),
                            Some(saved.request_bytes.clone()),
                        )
                    }
                    RecordLookup::Imported(saved) => {
                        imported += 1;
                        (
                            record_comparison_bytes(&saved.entry).expect("record bytes"),
                            None,
                        )
                    }
                };
                assert_eq!(
                    carried.record,
                    crate::encoding::framed_key(RECORD_BLOB_DOMAIN, &record_bytes)
                        .expect("record digest"),
                    "{label}: {record_id} carries another record's blob digest"
                );
                assert_eq!(
                    carried.request,
                    request.map(
                        |bytes| crate::encoding::framed_key(REQUEST_BLOB_DOMAIN, &bytes)
                            .expect("request digest")
                    ),
                    "{label}: {record_id} carries another request's blob digest"
                );
            }
            for (key, batch) in &model.batches {
                assert_eq!(
                    model.batch_blob_digests.get(key),
                    Some(
                        &crate::encoding::framed_key(BATCH_BLOB_DOMAIN, &batch.comparison_bytes)
                            .expect("batch digest")
                    ),
                    "{label}: a batch carries another batch's blob digest"
                );
            }
            for (subject, bytes) in &model.anchors {
                assert_eq!(
                    model.anchor_blob_digests.get(subject),
                    Some(
                        &crate::encoding::framed_key(ANCHOR_BLOB_DOMAIN, bytes)
                            .expect("anchor digest")
                    ),
                    "{label}: a subject carries another anchor's blob digest"
                );
            }
            match label {
                "committed" => assert!(
                    committed == GATE_RECORDS && imported == 0 && !model.batches.is_empty(),
                    "the committed fixture must reach the committed arms: {committed}/{imported}"
                ),
                _ => assert!(
                    imported == GATE_RECORDS * 2 && committed == 0 && !model.anchors.is_empty(),
                    "the imported fixture must anchor two envelopes each: {committed}/{imported}"
                ),
            }
        }
    }

    /// One committed single-member batch, exactly as `append_inner` writes one.
    /// One named mutation of a parsed batch document.
    type Tamper = (&'static str, Box<dyn Fn(&mut Value)>);

    fn one_member_batch() -> (BatchKey, AppendMember, Vec<u8>) {
        let entry = recorded(0, 512);
        let key = BatchKey::SingleRecord("record-000000".to_owned());
        let request_bytes =
            original_request_comparison_bytes(&entry).expect("request comparison bytes");
        let member = AppendMember::new(Expect::Absent, entry, request_bytes);
        let bytes = batch_comparison_bytes(&key, std::slice::from_ref(&member))
            .expect("batch comparison bytes");
        (key, member, bytes)
    }

    fn parsed<T>(work: impl FnOnce() -> T) -> (T, u64) {
        crate::encoding::PARSED_BYTES.with(|charged| charged.set(0));
        let value = work();
        (value, crate::encoding::PARSED_BYTES.with(Cell::get))
    }

    /// The defect this round exists for, stated as a ratio rather than a duration.
    ///
    /// A batch document repeats each member record verbatim, so parsing the batch has already read
    /// every record in it. `decode_batch` then rendered each member back to bytes and parsed those
    /// bytes again, which is a second full decode of the largest thing in the store.
    #[test]
    fn a_batch_is_parsed_once_rather_than_once_for_every_member_as_well() {
        let (key, _, bytes) = one_member_batch();
        let ((decoded_key, members), read) =
            parsed(|| decode_batch(&bytes).expect("batch decodes"));
        assert_eq!(decoded_key, key);
        assert_eq!(members.len(), 1);
        assert_eq!(
            read,
            bytes.len() as u64,
            "decoding a {}-byte batch read {read} bytes of stored document",
            bytes.len()
        );
    }

    /// What the deleted re-serialisation proved about the bytes, kept and named.
    ///
    /// `batch_comparison_bytes(key, decoded) == bytes` was the only check that the stored batch is
    /// the canonical rendering of the batch it parses to — and canonical is not cosmetic here: the
    /// bytes are the blob's digest preimage and the material a retry is compared against, so two
    /// renderings of one batch are two batches that never deduplicate. These are the inputs it
    /// caught, including the one **inside a member record**, which is the part the second decode
    /// was doing.
    #[test]
    fn a_batch_whose_stored_bytes_are_not_their_own_canonical_rendering_is_refused() {
        let (_, _, bytes) = one_member_batch();
        let member_record = bytes
            .windows(10)
            .position(|window| window == br#""record":["#)
            .expect("the member names its record");
        for (what, at) in [
            ("the document", 1usize),
            ("a member record", member_record + 10),
        ] {
            let mut slack = bytes.clone();
            slack.insert(at, b' ');
            assert!(
                matches!(
                    decode_batch(&slack),
                    Err(AsyncStoreError::ProviderIntegrity { ref detail, .. })
                        if detail == "batch bytes are not canonical"
                ),
                "a space in {what} left the batch admitted: {:?}",
                decode_batch(&slack).err()
            );
        }
    }

    /// What the deleted re-serialisation proved about the *members*, which canonicality cannot see.
    ///
    /// Both of these are perfectly canonical JSON — sorted keys, no slack — and both decode. They
    /// are still not the record they claim to be, because the round trip through the typed record
    /// is lossy in two documented places: `EntityInstance` carries no `deny_unknown_fields`, so a
    /// field added to an instance is read and dropped; and `DecisionRecord::removed` is
    /// `skip_serializing_if = "BTreeSet::is_empty"`, so an explicit empty one is read and not
    /// written back. This is the half of the old comparison that is **not** redundant, and it is
    /// why this round replaced it rather than deleting it.
    #[test]
    fn a_batch_member_that_does_not_reproduce_its_own_record_is_refused() {
        let (_, _, bytes) = one_member_batch();
        for (what, mutate) in [
            (
                "a field added to the recorded instance",
                Box::new(|record: &mut Value| {
                    record["commit"]["instance"]["surplus"] = json!(1);
                }) as Box<dyn Fn(&mut Value)>,
            ),
            (
                "an explicit empty removed set",
                Box::new(|record: &mut Value| {
                    record["commit"]["envelope"]["record"]["removed"] = json!([]);
                }),
            ),
        ] {
            let mut document: Value = serde_json::from_slice(&bytes).expect("batch parses");
            mutate(&mut document[2][0]["record"][1]);
            let restored = serde_json::to_vec(&document).expect("batch re-encodes");
            assert!(
                matches!(
                    decode_batch(&restored),
                    Err(AsyncStoreError::ProviderIntegrity { ref detail, .. })
                        if detail == "record bytes do not reproduce the complete record"
                ),
                "{what} left the batch admitted: {:?}",
                decode_batch(&restored).err()
            );
        }
    }

    /// The member expectation is rebuilt and compared, and this is the input that makes it earn it.
    ///
    /// `ExpectWire` is internally tagged with `deny_unknown_fields`, which looks like it settles
    /// the question and does not: serde does not apply `deny_unknown_fields` to an internally
    /// tagged **unit** variant, so `{"kind":"absent","surplus":1}` decodes to `Expect::Absent` and
    /// writes back without the surplus field. The document is perfectly canonical, so the
    /// document-wide check cannot see it; it is still a batch whose bytes are not the rendering of
    /// the batch they decode to, which is the thing the old rebuild-and-compare refused.
    #[test]
    fn a_member_expectation_carrying_a_field_of_its_own_is_refused() {
        let (_, _, bytes) = one_member_batch();
        let mut document: Value = serde_json::from_slice(&bytes).expect("batch parses");
        document[2][0]["expect"]["surplus"] = json!(1);
        let restored = serde_json::to_vec(&document).expect("batch re-encodes");
        assert_eq!(
            serde_json::to_vec(&serde_json::from_slice::<Value>(&restored).expect("parses"))
                .expect("re-encodes"),
            restored,
            "the input must be canonical, or it proves nothing the document-wide check does not"
        );
        assert!(
            matches!(
                decode_batch(&restored),
                Err(AsyncStoreError::ProviderIntegrity { ref detail, .. })
                    if detail == "batch bytes are not canonical"
            ),
            "a surplus field on an expectation left the batch admitted: {:?}",
            decode_batch(&restored).err()
        );
    }

    /// The two parts of the removed rebuild-and-compare that need no replacement, and why.
    ///
    /// The old check rebuilt the whole batch document — bytes, key, expectation, member envelope
    /// and record — and compared it to the stored bytes. Three of those five have a replacement
    /// above. The batch key and the member envelope need none: each admits exactly one document per
    /// value, and every other shape fails `from_value` before a comparison could run. That is
    /// measured here rather than argued — the expectation was assumed to belong in this list and
    /// did not, which is why it has a case of its own — and it is what a later widening of one of
    /// these types would have to walk past: `#[serde(other)]`, a dropped `deny_unknown_fields` or a
    /// third variant would make the rebuild necessary again, and would fail here first.
    #[test]
    fn the_batch_key_expectation_and_member_wires_admit_exactly_one_shape_each() {
        let (_, _, bytes) = one_member_batch();
        let refused: Vec<Tamper> = vec![
            (
                "a batch key of three elements",
                Box::new(|d: &mut Value| d[1] = json!(["single_record", "record-000000", "extra"])),
            ),
            (
                "a batch key naming an unknown tag",
                Box::new(|d: &mut Value| d[1] = json!(["other", "record-000000"])),
            ),
            (
                "an expectation whose revision is not an integer",
                Box::new(|d: &mut Value| {
                    d[2][0]["expect"] = json!({"kind":"revision","revision":1.0});
                }),
            ),
            (
                "a member envelope carrying a third field",
                Box::new(|d: &mut Value| d[2][0]["surplus"] = json!(1)),
            ),
        ];
        for (what, mutate) in refused {
            let mut document: Value = serde_json::from_slice(&bytes).expect("batch parses");
            mutate(&mut document);
            let restored = serde_json::to_vec(&document).expect("batch re-encodes");
            assert!(
                matches!(decode_batch(&restored), Err(AsyncStoreError::Encoding(_))),
                "{what} was not refused by the wire type itself: {:?}",
                decode_batch(&restored).err()
            );
        }
        // And the one shape each does admit still decodes, so the refusals above are about the
        // shape and not about the fixture being unreadable.
        let mut document: Value = serde_json::from_slice(&bytes).expect("batch parses");
        document[2][0]["expect"] = json!({"kind":"revision","revision":1});
        let restored = serde_json::to_vec(&document).expect("batch re-encodes");
        let (_, members) = decode_batch(&restored).expect("the admitted shape decodes");
        assert_eq!(members[0].expect, Expect::Revision(1));
    }

    /// The measurement. Asks for nothing unless `ENTITY_EVENTLOG_SEEDED_OPEN_RECORDS` is set.
    #[test]
    fn seeded_open_measurement() {
        let Ok(records) = std::env::var("ENTITY_EVENTLOG_SEEDED_OPEN_RECORDS") else {
            return;
        };
        let records: usize = records.parse().expect("record count");
        let built = Instant::now();
        let (authority, capture) = fixture(records, 1_475_000);
        let mut sizes: Vec<usize> = capture.blobs.iter().map(|blob| blob.bytes.len()).collect();
        sizes.sort_unstable();
        let total: usize = sizes.iter().sum();
        println!(
            "fixture: {} events, {} blobs, {total} bytes, median {}, mean {}, max {}, built in {:?}",
            capture.events.len(),
            sizes.len(),
            sizes[sizes.len() / 2],
            total / sizes.len(),
            sizes[sizes.len() - 1],
            built.elapsed()
        );
        let bytes = captured_bytes(&capture);
        let mut digests = Vec::new();
        for run in 1..=3 {
            let capture = capture.clone();
            let start = Instant::now();
            let (model, hashed) =
                charged(|| build_model(&authority, &capture).expect("model builds"));
            let elapsed = start.elapsed();
            // Digested outside the timed region, and only when asked: rendering the whole model
            // through `Debug` costs more than building it, so a profile of a run that digests is a
            // profile of the harness. One run establishes the digest, another the symbols.
            if std::env::var_os("ENTITY_EVENTLOG_SEEDED_OPEN_DIGEST").is_some() {
                digests.push(model_digest(&model));
            } else {
                drop(model);
            }
            println!(
                "run {run}: build_model {elapsed:?}, hashed {hashed} of {bytes} captured bytes \
                 ({:.2}x)",
                hashed as f64 / bytes as f64,
            );
        }
        println!("model digests: {digests:?}");
        provider_capture(&capture);
    }

    /// The other half of a seeded open, measured here rather than divided out of something else.
    ///
    /// The provider reads and SHA-256s every bound blob inside `capture_tenant`
    /// (`eventlog-file/src/capture.rs`, `read_blob`), which is what a seeded open pays before
    /// `build_model` is entered at all. The tenant's identity and its blobs are written; its 1,954
    /// events and four projections are not, so this is the blob half of the capture and is named
    /// as such rather than as the whole of it.
    #[cfg(feature = "file")]
    fn provider_capture(capture: &TenantCapture) {
        if std::env::var_os("ENTITY_EVENTLOG_SEEDED_OPEN_PROVIDER").is_none() {
            return;
        }
        let directory = tempfile::tempdir().expect("capture fixture directory");
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("measurement runtime")
            .block_on(async {
                let store = eventlog_file::FileEventStore::open(directory.path())
                    .await
                    .expect("file store");
                let tenant = capture.tenant.clone();
                store
                    .stream_identity(&tenant)
                    .await
                    .expect("tenant identity");
                let written = Instant::now();
                for blob in &capture.blobs {
                    store
                        .put_blob(&tenant, &blob.digest, &blob.bytes)
                        .await
                        .expect("blob bound");
                }
                println!(
                    "provider: {} blobs bound in {:?}",
                    capture.blobs.len(),
                    written.elapsed()
                );
                let limits = CaptureLimits {
                    max_events: 1_000_000,
                    max_blobs: 1_000_000,
                    max_projection_rows: 1_000_000,
                    max_payload_bytes: 1 << 34,
                };
                for run in 1..=3 {
                    let start = Instant::now();
                    let observed = store
                        .capture_tenant(&tenant, &[], limits)
                        .await
                        .expect("tenant captured");
                    println!(
                        "provider run {run}: capture_tenant {:?} for {} blobs",
                        start.elapsed(),
                        observed.blobs.len()
                    );
                }
            });
    }

    #[cfg(not(feature = "file"))]
    fn provider_capture(_: &TenantCapture) {}
}
