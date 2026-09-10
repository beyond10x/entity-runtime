//! Real-provider acceptance of the recorded Eventlog mapping.

use std::{future::Future, sync::Arc};

use entity_core::{EntityDefinition, Registry, Runtime};
use entity_eventlog::EventlogStore;
use entity_shell::asynchronous::AsyncStoredRuntime;
use entity_store::{
    Expect, RecordedCommit, RecordedObservation, Recording, StoreError,
    asynchronous::{AsyncAtomicRecordedStore, AsyncRecordedStore, AtomicRecordedCommit},
};
use eventlog_core::{
    AtomicEventStore, BoxFuture, CommandMeta, EventLogError, EventStore, Expected, NewEvent,
    ProjectionSpec, ProjectionStore, Projector, RecordedEvent, TenantId,
};
use eventlog_file::FileEventStore;
use eventlog_sqlite::SqliteEventStore;
use serde_json::json;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

fn run<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

fn registry() -> Registry {
    let definition: EntityDefinition = serde_json::from_value(json!({
        "entity": "thing", "schema": {},
        "lifecycle": { "initial": "new", "states": ["new", "done"] },
        "operations": {
            "touch": { "transitions": [{"from": "new", "to": "new"}] },
            "finish": { "transitions": [{"from": "new", "to": "done"}] }
        }
    }))
    .unwrap();
    let mut registry = Registry::new();
    registry.register(definition).unwrap();
    registry
}

fn recording(id: &str) -> Recording {
    Recording {
        record_id: id.into(),
        recorded_at: "2026-09-10T13:00:00Z".into(),
        correlation: Some("flow".into()),
        causation: None,
        actor: Some("original-actor".into()),
    }
}

fn adapter<S: AtomicEventStore + ?Sized>(store: Arc<S>) -> EventlogStore<S> {
    EventlogStore::new(
        store,
        TenantId::new("tenant-a").unwrap(),
        "application",
        "host-subject",
        "host-actor",
    )
    .unwrap()
}

fn creation(registry: &Registry, id: &str, record_id: &str) -> AtomicRecordedCommit {
    AtomicRecordedCommit {
        commit: RecordedCommit::new(
            Runtime::new(registry)
                .create("thing", 1, id, json!({}))
                .unwrap(),
            &recording(record_id),
        )
        .unwrap(),
        expect: Expect::Absent,
    }
}

async fn recorded_contract<S: AtomicEventStore + ?Sized>(store: Arc<S>) {
    let registry = registry();
    let mut provider = adapter(store.clone());
    let created = creation(&registry, "one", "create");
    provider
        .commit_recorded(&created.commit, created.expect)
        .await
        .unwrap();
    assert!(created.commit.envelope.record.events.is_empty());
    let observation = RecordedObservation {
        entity: "thing".into(),
        id: "one".into(),
        revision: 1,
        envelope: recording("observation")
            .seal(json!({"reachable": true}))
            .unwrap(),
    };
    provider.observe(&observation).await.unwrap();
    provider.observe(&observation).await.unwrap();
    let accepted = AsyncStoredRuntime::new(&registry, &mut provider)
        .execute("thing", "one", 1, "finish", json!({}), &recording("finish"))
        .await
        .unwrap();
    assert_eq!(accepted.instance.revision, 2);
    assert_eq!(
        accepted.envelope.record.definition.as_ref(),
        created.commit.envelope.record.definition.as_ref()
    );
    assert_eq!(
        provider.records("thing", "one").await.unwrap(),
        vec![created.commit.envelope.clone(), accepted.envelope.clone()]
    );
    assert_eq!(
        provider.observations("thing", "one").await.unwrap(),
        vec![observation.clone()]
    );
    assert!(provider.events("thing", "one").await.unwrap().is_empty());
    provider
        .commit_recorded(&created.commit, Expect::Absent)
        .await
        .unwrap();
    provider.observe(&observation).await.unwrap();
    assert_eq!(
        provider.load("thing", "one").await.unwrap(),
        Some(accepted.instance)
    );
    let mut changed = observation.clone();
    changed.envelope.record = json!({"reachable": false});
    assert!(
        matches!(provider.observe(&changed).await.unwrap_err(), StoreError::RecordConflict { record_id } if record_id == "observation")
    );
    let collision = creation(&registry, "two", "observation");
    assert!(
        matches!(provider.commit_recorded(&collision.commit, Expect::Absent).await.unwrap_err(), StoreError::RecordConflict { record_id } if record_id == "observation")
    );
    assert_eq!(provider.ids("thing").await.unwrap(), vec!["one"]);
    assert_eq!(provider.load("thing", "missing").await.unwrap(), None);

    let first = creation(&registry, "batch-one", "batch-create");
    let second = AtomicRecordedCommit {
        commit: RecordedCommit::new(
            Runtime::new(&registry)
                .execute(&first.commit.instance, "touch", json!({}))
                .unwrap(),
            &recording("batch-touch"),
        )
        .unwrap(),
        expect: Expect::Revision(1),
    };
    let third = creation(&registry, "batch-two", "batch-other");
    let mut wrong = third.clone();
    wrong.expect = Expect::Revision(9);
    let error = provider
        .commit_recorded_batch(&[first.clone(), second.clone(), wrong])
        .await
        .unwrap_err();
    assert!(
        matches!(error, StoreError::RevisionConflict { id, expected: Expect::Revision(9), found: None, .. } if id == "batch-two")
    );
    assert_eq!(provider.load("thing", "batch-one").await.unwrap(), None);
    let batch = [first, second, third];
    provider.commit_recorded_batch(&batch).await.unwrap();
    provider.commit_recorded_batch(&batch).await.unwrap();
    provider.commit_recorded_batch(&[]).await.unwrap();
    assert_eq!(
        provider
            .load("thing", "batch-one")
            .await
            .unwrap()
            .unwrap()
            .revision,
        2
    );
    assert_eq!(
        provider.records("thing", "batch-one").await.unwrap().len(),
        2
    );
    let history = store
        .read_feed(&TenantId::new("tenant-a").unwrap(), 0, 100)
        .await
        .unwrap();
    let one: Vec<_> = history
        .events
        .iter()
        .filter(|e| {
            e.name == "entity.record" && e.data["payload"]["record"]["instance"]["id"] == "one"
        })
        .collect();
    assert_eq!(
        one.iter().map(|e| e.version).collect::<Vec<_>>(),
        vec![1, 3],
        "the observation occupies physical position 2, not an entity revision"
    );
}

#[test]
fn file_provider_preserves_the_recorded_entity_contract() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        recorded_contract(Arc::new(
            FileEventStore::open(directory.path()).await.unwrap(),
        ))
        .await;
    });
}

#[test]
fn sqlite_provider_preserves_the_same_recorded_entity_contract() {
    run(async {
        recorded_contract(Arc::new(
            SqliteEventStore::in_memory("entity_test").await.unwrap(),
        ))
        .await;
    });
}

#[test]
fn reopened_file_store_replays_complete_history_and_preserves_retry_claims() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let registry = registry();
        let created = creation(&registry, "one", "create");
        let accepted;
        {
            let store = Arc::new(FileEventStore::open(directory.path()).await.unwrap());
            let mut provider = adapter(store);
            provider
                .commit_recorded(&created.commit, Expect::Absent)
                .await
                .unwrap();
            accepted = AsyncStoredRuntime::new(&registry, &mut provider)
                .execute("thing", "one", 1, "finish", json!({}), &recording("finish"))
                .await
                .unwrap();
        }
        let store = Arc::new(FileEventStore::open(directory.path()).await.unwrap());
        let mut provider = adapter(store);
        let empty_registry = Registry::new();
        let mut executor = AsyncStoredRuntime::new(&empty_registry, &mut provider);
        assert_eq!(
            executor.get("thing", "one").await.unwrap(),
            accepted.instance
        );
        assert_eq!(
            executor
                .execute("thing", "one", 1, "finish", json!({}), &recording("finish"))
                .await
                .unwrap(),
            accepted
        );
        assert_eq!(
            executor
                .create("thing", 1, "one", json!({}), &recording("create"))
                .await
                .unwrap(),
            created.commit
        );
    });
}

struct FailSecondSubject;
impl Projector for FailSecondSubject {
    fn name(&self) -> &'static str {
        "fail_second"
    }
    fn projections(&self) -> &'static [ProjectionSpec] {
        &[]
    }
    fn apply<'a>(
        &'a self,
        event: &'a RecordedEvent,
        _: &'a mut dyn ProjectionStore,
    ) -> BoxFuture<'a, Result<(), EventLogError>> {
        Box::pin(async move {
            if event.name == "entity.record"
                && event.data["payload"]["record"]["instance"]["id"] == "two"
            {
                return Err(EventLogError::GuardRefused {
                    code: "second_subject_refused".into(),
                });
            }
            Ok(())
        })
    }
}

#[test]
fn a_late_transaction_failure_rolls_back_subjects_and_global_identity_claims() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(FileEventStore::open(directory.path()).await.unwrap());
        store
            .register_inline(Arc::new(FailSecondSubject))
            .await
            .unwrap();
        let registry = registry();
        let mut provider = adapter(store.clone());
        let error = provider
            .commit_recorded_batch(&[
                creation(&registry, "one", "record-one"),
                creation(&registry, "two", "record-two"),
            ])
            .await
            .unwrap_err();
        assert!(
            matches!(error, StoreError::Backend(ref message) if message.contains("second_subject_refused")),
            "{error:?}"
        );
        assert!(provider.ids("thing").await.unwrap().is_empty());
        assert!(
            store
                .read_feed(&TenantId::new("tenant-a").unwrap(), 0, 100)
                .await
                .unwrap()
                .events
                .is_empty()
        );
        let reused = creation(&registry, "different", "record-one");
        provider
            .commit_recorded(&reused.commit, Expect::Absent)
            .await
            .unwrap();
        assert_eq!(provider.ids("thing").await.unwrap(), vec!["different"]);
    });
}

#[test]
fn concurrent_file_handles_publish_one_revision_and_resolve_identical_retries() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let registry = registry();
        let left = Arc::new(FileEventStore::open(directory.path()).await.unwrap());
        let right = Arc::new(FileEventStore::open(directory.path()).await.unwrap());
        let mut a = adapter(left);
        let mut b = adapter(right);
        let created = creation(&registry, "one", "create");
        a.commit_recorded(&created.commit, Expect::Absent)
            .await
            .unwrap();
        let decision = Runtime::new(&registry)
            .execute(&created.commit.instance, "touch", json!({}))
            .unwrap();
        let first = RecordedCommit::new(decision.clone(), &recording("first")).unwrap();
        let other = RecordedCommit::new(decision, &recording("other")).unwrap();
        let (one, two) = tokio::join!(
            a.commit_recorded(&first, Expect::Revision(1)),
            b.commit_recorded(&other, Expect::Revision(1))
        );
        assert_ne!(one.is_ok(), two.is_ok());
        let error = one.err().or(two.err()).unwrap();
        assert!(
            matches!(
                error,
                StoreError::RevisionConflict {
                    expected: Expect::Revision(1),
                    found: Some(2),
                    ..
                }
            ),
            "{error:?}"
        );
        let current = a.load("thing", "one").await.unwrap().unwrap();
        let next = RecordedCommit::new(
            Runtime::new(&registry)
                .execute(&current, "finish", json!({}))
                .unwrap(),
            &recording("identical"),
        )
        .unwrap();
        let (one, two) = tokio::join!(
            a.commit_recorded(&next, Expect::Revision(2)),
            b.commit_recorded(&next, Expect::Revision(2))
        );
        one.unwrap();
        two.unwrap();
        assert_eq!(a.records("thing", "one").await.unwrap().len(), 3);
        assert_eq!(b.load("thing", "one").await.unwrap().unwrap().revision, 3);
    });
}

#[test]
fn tenant_and_namespace_selectors_keep_record_id_authorities_separate() {
    run(async {
        let store = Arc::new(SqliteEventStore::in_memory("scope_test").await.unwrap());
        let mut a = adapter(store.clone());
        let mut namespace = EventlogStore::new(
            store.clone(),
            TenantId::new("tenant-a").unwrap(),
            "other",
            "host",
            "host",
        )
        .unwrap();
        let mut tenant = EventlogStore::new(
            store,
            TenantId::new("tenant-b").unwrap(),
            "application",
            "host",
            "host",
        )
        .unwrap();
        let registry = registry();
        for (provider, name) in [
            (&mut a, "one"),
            (&mut namespace, "two"),
            (&mut tenant, "three"),
        ] {
            provider
                .commit_recorded(
                    &creation(&registry, name, "shared-id").commit,
                    Expect::Absent,
                )
                .await
                .unwrap();
            assert_eq!(provider.ids("thing").await.unwrap(), vec![name]);
        }
        assert_eq!(a.ids("thing").await.unwrap(), vec!["one"]);
    });
}

fn forged_meta() -> CommandMeta {
    CommandMeta {
        idempotency_key: "forged".into(),
        request_hash: "forged".into(),
        subject: "test".into(),
        actor: "test".into(),
        request_id: "test".into(),
        trace_id: "test".into(),
        causation_id: None,
        causation_depth: 0,
        occurred_at: OffsetDateTime::parse("2026-09-10T13:00:00Z", &Rfc3339).unwrap(),
        claim: None,
    }
}

#[test]
fn syntactically_valid_but_forged_decision_history_is_refused() {
    run(async {
        let store = Arc::new(SqliteEventStore::in_memory("tamper_test").await.unwrap());
        let mut provider = adapter(store.clone());
        let registry = registry();
        let created = creation(&registry, "one", "create");
        provider
            .commit_recorded(&created.commit, Expect::Absent)
            .await
            .unwrap();
        let mut forged = RecordedCommit::new(
            Runtime::new(&registry)
                .execute(&created.commit.instance, "touch", json!({}))
                .unwrap(),
            &recording("forged"),
        )
        .unwrap();
        forged.instance.lifecycle_state = "done".into();
        forged.envelope.record.result = forged.instance.clone();
        forged.envelope.record.to_state = "done".into();
        let feed = store
            .read_feed(&TenantId::new("tenant-a").unwrap(), 0, 100)
            .await
            .unwrap();
        let stream = feed
            .events
            .iter()
            .find(|e| e.name == "entity.record")
            .unwrap()
            .stream()
            .unwrap();
        let event = NewEvent::new(
            "entity.record",
            1,
            json!({"format":"entity-eventlog/1", "payload":{"kind":"decision", "record":forged}}),
        )
        .unwrap();
        let identity_type = &feed
            .events
            .iter()
            .find(|e| e.name == "entity.record-identity")
            .unwrap()
            .stream_type;
        let identity = eventlog_core::StreamId::new(
            TenantId::new("tenant-a").unwrap(),
            identity_type,
            eventlog_core::request_hash(&"forged").unwrap(),
        )
        .unwrap();
        let claim = NewEvent::new("entity.record-identity", 1, json!({"digest":eventlog_core::request_hash(&event.data["payload"]).unwrap(), "stream":stream, "version":2})).unwrap();
        store
            .append_group(&eventlog_core::AppendGroup {
                tenant: TenantId::new("tenant-a").unwrap(),
                meta: forged_meta(),
                appends: vec![
                    eventlog_core::StreamAppend {
                        stream: identity,
                        expected: Expected::NoStream,
                        events: vec![claim],
                    },
                    eventlog_core::StreamAppend {
                        stream,
                        expected: Expected::Exact(1),
                        events: vec![event],
                    },
                ],
            })
            .await
            .unwrap();
        let error = provider.load("thing", "one").await.unwrap_err();
        assert!(
            matches!(error, StoreError::Backend(ref message) if message.contains("recomputed")),
            "{error:?}"
        );
    });
}

#[test]
fn redacted_history_refuses_state_and_exact_retries() {
    run(async {
        let store = Arc::new(SqliteEventStore::in_memory("redact_test").await.unwrap());
        let mut provider = adapter(store.clone());
        let registry = registry();
        let created = creation(&registry, "one", "create");
        provider
            .commit_recorded(&created.commit, Expect::Absent)
            .await
            .unwrap();
        let feed = store
            .read_feed(&TenantId::new("tenant-a").unwrap(), 0, 100)
            .await
            .unwrap();
        let stream = feed
            .events
            .iter()
            .find(|e| e.name == "entity.record")
            .unwrap()
            .stream()
            .unwrap();
        store.redact(&stream, 1, "erasure").await.unwrap();
        for error in [
            provider.load("thing", "one").await.unwrap_err(),
            provider
                .commit_recorded(&created.commit, Expect::Absent)
                .await
                .unwrap_err(),
        ] {
            assert!(
                matches!(error, StoreError::Backend(ref message) if message.contains("redacted")),
                "{error:?}"
            );
        }
    });
}

#[test]
fn verified_prefix_is_shared_until_new_decisions_and_invalidated_by_redaction() {
    run(async {
        let store = Arc::new(SqliteEventStore::in_memory("cache_test").await.unwrap());
        let registry = registry();
        let created = creation(&registry, "one", "create");
        let mut provider = adapter(store.clone());
        provider
            .commit_recorded(&created.commit, Expect::Absent)
            .await
            .unwrap();
        let first = provider.verified_history("thing", "one").await.unwrap();
        let same = provider.verified_history("thing", "one").await.unwrap();
        assert!(
            Arc::ptr_eq(&first, &same),
            "unchanged history must reuse its proof"
        );
        let observation = RecordedObservation {
            entity: "thing".into(),
            id: "one".into(),
            revision: 1,
            envelope: recording("observation").seal(json!({})).unwrap(),
        };
        provider.observe(&observation).await.unwrap();
        let observed = provider.verified_history("thing", "one").await.unwrap();
        assert!(
            Arc::ptr_eq(&first, &observed),
            "an observation must not reverify decision history"
        );
        let accepted = AsyncStoredRuntime::new(&registry, &mut provider)
            .execute("thing", "one", 1, "finish", json!({}), &recording("finish"))
            .await
            .unwrap();
        let advanced = provider.verified_history("thing", "one").await.unwrap();
        assert!(!Arc::ptr_eq(&first, &advanced));
        assert_eq!(
            first.instance().unwrap().revision,
            1,
            "retained proofs are immutable"
        );
        assert_eq!(advanced.instance(), Some(&accepted.instance));
        let feed = store
            .read_feed(&TenantId::new("tenant-a").unwrap(), 0, 100)
            .await
            .unwrap();
        let stream = feed
            .events
            .iter()
            .find(|e| e.name == "entity.record")
            .unwrap()
            .stream()
            .unwrap();
        store.redact(&stream, 1, "erasure").await.unwrap();
        let error = provider.verified_history("thing", "one").await.unwrap_err();
        assert!(
            matches!(error, StoreError::Backend(ref message) if message.contains("redacted")),
            "{error:?}"
        );
    });
}

#[test]
fn cache_eviction_and_disabled_retention_preserve_durable_state() {
    run(async {
        let store = Arc::new(SqliteEventStore::in_memory("budget_test").await.unwrap());
        let registry = registry();
        let mut provider = adapter(store.clone()).with_cache_budget(1, 1024 * 1024);
        for id in ["one", "two"] {
            provider
                .commit_recorded(&creation(&registry, id, id).commit, Expect::Absent)
                .await
                .unwrap();
        }
        let before = provider.verified_history("thing", "one").await.unwrap();
        provider.verified_history("thing", "two").await.unwrap();
        let after = provider.verified_history("thing", "one").await.unwrap();
        assert!(
            !Arc::ptr_eq(&before, &after),
            "one-subject budget must evict the older subject"
        );
        assert_eq!(before.records(), after.records());
        for (subjects, bytes) in [(0, 1024), (10, 0), (10, 1)] {
            let mut uncached = adapter(store.clone()).with_cache_budget(subjects, bytes);
            let first = uncached.verified_history("thing", "one").await.unwrap();
            let second = uncached.verified_history("thing", "one").await.unwrap();
            assert!(!Arc::ptr_eq(&first, &second));
            assert_eq!(first.instance(), second.instance());
            assert_eq!(uncached.ids("thing").await.unwrap(), vec!["one", "two"]);
        }
    });
}
