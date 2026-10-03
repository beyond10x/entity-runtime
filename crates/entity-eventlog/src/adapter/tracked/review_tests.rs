//! Independent full-versus-incremental integrity probes.
use super::*;
use entity_executor::{CreateRequest, ExecuteRequest};
use entity_store::asynchronous::RecordedEntry;
use entity_store::{RecordedObservation, Recording};
use eventlog_core::{CaptureUsage, CapturedBlob, CapturedProjectionDelta, CapturedRowChange};

const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 128,
    max_blobs: 1024,
    max_projection_rows: 1024,
    max_payload_bytes: 16 * 1024 * 1024,
};
fn context(id: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "review".into(),
        actor: "entity-eventlog-test".into(),
        request_id: id.into(),
        trace_id: id.into(),
        causation_id: None,
        causation_depth: 0,
        occurred_at: OffsetDateTime::UNIX_EPOCH,
    }
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
fn touch(id: &str, revision: u64) -> BatchAction {
    BatchAction::Execute(ExecuteRequest {
        subject: Subject::new("ticket", "clock").unwrap(),
        expected_revision: revision,
        operation: "Touch".into(),
        arguments: json!({}),
        fulfillments: BTreeMap::new(),
        recording: recording(id),
    })
}
fn observe(id: &str, revision: u64) -> BatchAction {
    BatchAction::Observe(RecordedObservation {
        entity: "ticket".into(),
        id: "clock".into(),
        revision,
        envelope: recording(id).seal(json!({"observed":true})).unwrap(),
    })
}
async fn fixture() -> (
    Arc<eventlog_sqlite::SqliteEventStore>,
    EventlogRecordedStore,
    Registry,
) {
    let backend = Arc::new(
        eventlog_sqlite::SqliteEventStore::in_memory("runtime_review")
            .await
            .unwrap(),
    );
    let tenant = TenantId::new("runtime-review").unwrap();
    let authority = Authority {
        logical_scope: "review-scope".into(),
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
    let store = EventlogRecordedStore::open_with_policy(
        backend.clone(),
        authority,
        LIMITS,
        CapturePolicy::ProviderTracked,
    )
    .await
    .unwrap();
    let mut registry = Registry::new();
    registry
        .register(
            serde_json::from_value(json!({"entity":"ticket","version":1,
                "schema":{"fields":{}},"lifecycle":{"initial":"Open","states":["Open"]},
                "operations":{"Touch":{"transitions":[{"from":"Open","to":"Open"}],"emits":[]}}
            }))
            .unwrap(),
        )
        .unwrap();
    store
        .operation(context("create"))
        .execute_batch(
            &registry,
            BatchKey::Named("create".into()),
            vec![BatchAction::Create(CreateRequest {
                subject: Subject::new("ticket", "clock").unwrap(),
                definition_version: 1,
                fields: json!({}),
                recording: recording("created"),
            })],
        )
        .await
        .unwrap();
    (backend, store, registry)
}
async fn capture(
    backend: &eventlog_sqlite::SqliteEventStore,
    store: &EventlogRecordedStore,
) -> TenantCapture {
    backend
        .capture_tenant(&store.tenant, projection_specs(), LIMITS)
        .await
        .unwrap()
}
fn retained(authority: &Authority, capture: TenantCapture) -> Held {
    Held {
        checkpoint: Some(CaptureCheckpoint::new(())),
        model: Arc::new(build_model(authority, &capture).unwrap()),
        last_position: capture.events.last().unwrap().global_seq,
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
fn delta(before: &TenantCapture, after: &TenantCapture) -> TenantCaptureDelta {
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

#[tokio::test]
async fn incremental_receipts_refuse_a_single_record_key_that_names_another_record() {
    let (backend, store, registry) = fixture().await;
    let before = capture(&backend, &store).await;
    store
        .operation(context("touch"))
        .execute_batch(
            &registry,
            BatchKey::Named("touch".into()),
            vec![touch("touched", 1)],
        )
        .await
        .unwrap();
    let mut after = capture(&backend, &store).await;
    let original = build_model(&store.authority, &after).unwrap();
    let RecordLookup::Committed(record) = original.records["touched"].clone() else {
        panic!("commit")
    };
    let key = BatchKey::SingleRecord("another-record".into());
    let batch_bytes = batch_comparison_bytes(
        &key,
        &[AppendMember::new(
            record.expect,
            record.entry.clone(),
            record.request_bytes.clone(),
        )],
    )
    .unwrap();
    let batch_digest = framed_key(BATCH_BLOB_DOMAIN, &batch_bytes).unwrap();
    let event = after.events.last_mut().unwrap();
    let old_digest = reference_digest(event).unwrap();
    let mut wrapper = decode_entry(
        &after
            .blobs
            .iter()
            .find(|b| b.digest == old_digest)
            .unwrap()
            .bytes,
    )
    .unwrap();
    wrapper.batch_key = crate::encoding::BatchKeyWire::from(&key);
    wrapper.batch_blob = batch_digest.clone();
    let wrapper_bytes = encode_entry(&wrapper).unwrap();
    let wrapper_digest = framed_key(ENTRY_BLOB_DOMAIN, &wrapper_bytes).unwrap();
    event.data = json!({"blob":wrapper_digest});
    after.blobs.extend([
        CapturedBlob {
            digest: batch_digest,
            bytes: batch_bytes,
        },
        CapturedBlob {
            digest: wrapper_digest,
            bytes: wrapper_bytes,
        },
    ]);
    after.blobs.sort_by(|a, b| a.digest.cmp(&b.digest));
    // Materialize the malformed storage facts without running the full history verifier.
    // This is a deliberately constructed provider observation, not an honest ER writer output.
    let mut unverified = build_model(&store.authority, &before).unwrap();
    let borrowed: BTreeMap<_, _> = after
        .blobs
        .iter()
        .map(|b| (b.digest.as_str(), b.bytes.as_slice()))
        .collect();
    let mut bound = BoundBlobs::new(&borrowed);
    let mut pending = Vec::new();
    admit_event(
        &store.authority,
        after.events.last().unwrap(),
        &mut bound,
        &mut unverified,
        &mut pending,
    )
    .unwrap();
    insert_committed(pending, &mut bound, &mut unverified).unwrap();
    unverified.terminals = original.terminals.clone();
    let rows = expected_projection_rows(&store.authority, &unverified).unwrap();
    for (projection, rows) in after.projections.iter_mut().zip(rows) {
        projection.rows = rows.into_iter().collect();
    }
    let full = build_model(&store.authority, &after);
    assert!(
        matches!(full,Err(AsyncStoreError::CorruptHistory {ref detail,..})
        if detail.contains("single-record receipt")),
        "full verifier must reject receipt identity"
    );
    let result = advance(
        &store.authority,
        retained(&store.authority, before.clone()),
        delta(&before, &after),
        CaptureCheckpoint::new(()),
    );
    assert!(
        matches!(
            result,
            Err(AsyncStoreError::CorruptHistory { .. })
                | Err(AsyncStoreError::ProviderIntegrity { .. })
        ),
        "incremental verifier accepted a single-record receipt that the full verifier refuses"
    );
}

#[tokio::test]
async fn coalesced_reverse_named_batches_keep_observation_heads_and_latest_state_source() {
    let (backend, store, registry) = fixture().await;
    let before = capture(&backend, &store).await;
    store
        .operation(context("z"))
        .execute_batch(
            &registry,
            BatchKey::Named("z-last-lexically".into()),
            vec![touch("touch-z", 1), observe("observe-z", 2)],
        )
        .await
        .unwrap();
    store
        .operation(context("a"))
        .execute_batch(
            &registry,
            BatchKey::Named("a-first-lexically".into()),
            vec![
                observe("observe-a-before", 2),
                touch("touch-a", 2),
                observe("observe-a-after", 3),
            ],
        )
        .await
        .unwrap();
    let after = capture(&backend, &store).await;
    let full = build_model(&store.authority, &after).unwrap();
    let advanced = advance(
        &store.authority,
        retained(&store.authority, before.clone()),
        delta(&before, &after),
        CaptureCheckpoint::new(()),
    )
    .unwrap();
    let subject = Subject::new("ticket", "clock").unwrap();
    assert_eq!(advanced.model.terminals[&subject].revision, 3);
    assert_eq!(advanced.model.state_records[&subject], "touch-a");
    assert_eq!(advanced.model.histories, full.histories);
    assert_eq!(advanced.model.records, full.records);
    assert_eq!(advanced.model.batches, full.batches);
    assert_eq!(
        advanced.rows,
        after
            .projections
            .iter()
            .map(|p| p.rows.iter().cloned().collect())
            .collect::<Vec<BTreeMap<_, _>>>()
    );
    let mut missing = delta(&before, &after);
    missing.events.remove(1);
    assert!(
        matches!(
            advance(
                &store.authority,
                retained(&store.authority, before),
                missing,
                CaptureCheckpoint::new(())
            ),
            Err(AsyncStoreError::ProviderIntegrity { .. })
                | Err(AsyncStoreError::CorruptHistory { .. })
        ),
        "incomplete newly acknowledged batch must not advance verified state"
    );
}

#[tokio::test]
async fn acknowledged_native_append_with_blank_named_key_is_refused_before_tracked_reuse() {
    let (backend, store, registry) = fixture().await;
    let subject = Subject::new("ticket", "clock").unwrap();
    let predecessor = store.load(&subject).await.unwrap().unwrap();
    let decision = entity_core::Runtime::new(&registry)
        .execute(&predecessor, "Touch", json!({}))
        .unwrap();
    let entry = RecordedEntry::Decision(
        entity_store::RecordedCommit::new(decision, &recording("bad-key-record")).unwrap(),
    );
    let request_bytes = original_request_comparison_bytes(&entry).unwrap();
    let member = AppendMember::new(Expect::Revision(1), entry.clone(), request_bytes.clone());
    let mut batch: Value = serde_json::from_slice(
        &batch_comparison_bytes(&BatchKey::Named("placeholder".into()), &[member]).unwrap(),
    )
    .unwrap();
    batch[1] = json!(["named", ""]);
    let batch_bytes = serde_json::to_vec(&batch).unwrap();
    let batch_digest = framed_key(BATCH_BLOB_DOMAIN, &batch_bytes).unwrap();
    let record_bytes = record_comparison_bytes(&entry).unwrap();
    let record_digest = framed_key(RECORD_BLOB_DOMAIN, &record_bytes).unwrap();
    let request_digest = framed_key(REQUEST_BLOB_DOMAIN, &request_bytes).unwrap();
    let wrapper = RecordedEntryWrapper {
        authority: store.authority.clone(),
        batch_blob: batch_digest.clone(),
        batch_key: crate::encoding::BatchKeyWire::from(&BatchKey::Named(String::new())),
        member_index: 0,
        record_blob: record_digest.clone(),
        request_blob: request_digest.clone(),
        subject: SubjectWire::from(&subject),
    };
    let wrapper_bytes = encode_entry(&wrapper).unwrap();
    let wrapper_digest = framed_key(ENTRY_BLOB_DOMAIN, &wrapper_bytes).unwrap();
    let group = AppendGroup {
        tenant: store.tenant.clone(),
        meta: context("bad-key-provider")
            .meta("bad-key-provider-command".into(), batch_digest.clone())
            .unwrap(),
        appends: vec![StreamAppend {
            stream: StreamId::new(
                store.tenant.clone(),
                "er.subject",
                subject_stream_id(&store.authority, &subject).unwrap(),
            )
            .unwrap(),
            expected: Expected::Exact(1),
            events: vec![
                NewEvent::new("er.recorded_entry", 1, json!({"blob":wrapper_digest})).unwrap(),
            ],
        }],
    };
    let blobs = vec![
        (batch_digest, batch_bytes),
        (record_digest, record_bytes),
        (request_digest, request_bytes),
        (wrapper_digest, wrapper_bytes),
    ];
    let committed = backend
        .append_group_guarded_with_blobs(&group, Arc::new(eventlog_core::NoGuard), &blobs)
        .await
        .expect("real SQLite and default ER projector admit this native group");
    assert!(!committed.deduplicated);
    let full = build_model(&store.authority, &capture(&backend, &store).await);
    assert!(
        matches!(full,Err(AsyncStoreError::CorruptHistory {ref detail,..}) if detail.contains("invalid key")),
        "full verifier must reject invalid receipt batch key"
    );
    let result = store.lookup_record("bad-key-record").await;
    assert!(
        matches!(result,Err(AsyncStoreError::CorruptHistory {ref detail,..}) if detail.contains("invalid key")),
        "tracked read accepted malformed committed receipt: {result:?}"
    );
}
