//! Async suspension, recorded integrity and atomicity at the application boundary.

use std::{
    future::{poll_fn, Future},
    pin::pin,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
};

use entity_core::{
    DecisionRecord, DomainEvent, EntityDefinition, EntityInstance, Registry, Runtime,
};
use entity_shell::{asynchronous::AsyncStoredRuntime, ShellError};
use entity_store::{
    asynchronous::{
        AsyncAtomicRecordedStore, AsyncRecordedStore, AtomicRecordedCommit, BlockingRecordedStore,
        StoreFuture,
    },
    Envelope, EventProvider, Expect, HistoryProvider, MemoryStore, RecordedCommit,
    RecordedObservation, Recording, StateProvider, Store, StoreError,
};
use serde_json::json;

struct Unpark(std::thread::Thread);
impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

// Only the test host drives futures; no library starts or nests an executor.
fn run<T>(future: impl Future<Output = T>) -> T {
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::park(),
        }
    }
}

async fn suspend() {
    let mut yielded = false;
    poll_fn(|cx| {
        if yielded {
            Poll::Ready(())
        } else {
            yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    })
    .await;
}

#[derive(Default)]
struct SuspendingStore {
    inner: MemoryStore,
    tamper: bool,
    fail_commit: bool,
    concurrent_commit: Option<RecordedCommit>,
    waits: usize,
}

macro_rules! read {
    ($method:ident, $output:ty, $($arg:ident),+) => {
        fn $method<'a>(&'a mut self, $($arg: &'a str),+) -> StoreFuture<'a, $output> {
            Box::pin(async move {
                suspend().await;
                self.waits += 1;
                self.inner.$method($($arg),+)
            })
        }
    };
}

impl AsyncRecordedStore for SuspendingStore {
    read!(load, Option<EntityInstance>, entity, id);
    read!(ids, Vec<String>, entity);
    read!(events, Vec<DomainEvent>, entity, id);
    read!(observations, Vec<RecordedObservation>, entity, id);

    fn records<'a>(
        &'a mut self,
        entity: &'a str,
        id: &'a str,
    ) -> StoreFuture<'a, Vec<Envelope<DecisionRecord>>> {
        Box::pin(async move {
            suspend().await;
            self.waits += 1;
            let mut records = self.inner.records(entity, id)?;
            if self.tamper {
                if let Some(record) = records.last_mut() {
                    record.record.result.lifecycle_state = "forged".into();
                }
            }
            Ok(records)
        })
    }
    fn commit_recorded<'a>(
        &'a mut self,
        commit: &'a RecordedCommit,
        expect: Expect,
    ) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            suspend().await;
            self.waits += 1;
            if let Some(concurrent) = self.concurrent_commit.take() {
                self.inner
                    .commit_recorded(&concurrent, Expect::Revision(1))?;
            }
            if self.fail_commit {
                return Err(StoreError::Unreachable {
                    provider: "test".into(),
                    detail: "not submitted".into(),
                });
            }
            self.inner.commit_recorded(commit, expect)
        })
    }
    fn observe<'a>(&'a mut self, observation: &'a RecordedObservation) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            suspend().await;
            self.waits += 1;
            self.inner.observe(observation)
        })
    }
}

#[test]
fn a_writer_advancing_during_async_commit_is_fenced_at_publication() {
    let registry = registry();
    let mut store = SuspendingStore::default();
    let created = run(AsyncStoredRuntime::new(&registry, &mut store).create(
        "thing",
        1,
        "one",
        json!({}),
        &recording("create"),
    ))
    .unwrap();
    let concurrent = RecordedCommit::new(
        Runtime::new(&registry)
            .execute(&created.instance, "touch", json!({}))
            .unwrap(),
        &recording("concurrent"),
    )
    .unwrap();
    store.concurrent_commit = Some(concurrent.clone());
    let error = run(AsyncStoredRuntime::new(&registry, &mut store).execute(
        "thing",
        "one",
        1,
        "finish",
        json!({}),
        &recording("loser"),
    ))
    .unwrap_err();
    assert!(matches!(
        error,
        ShellError::Store(StoreError::RevisionConflict {
            expected: Expect::Revision(1),
            found: Some(2),
            ..
        })
    ));
    assert_eq!(
        store.inner.load("thing", "one").unwrap(),
        Some(concurrent.instance)
    );
    let records = store.inner.records("thing", "one").unwrap();
    assert_eq!(
        records
            .iter()
            .map(|r| r.record_id.as_str())
            .collect::<Vec<_>>(),
        vec!["create", "concurrent"]
    );
}

fn registry() -> Registry {
    let definition: EntityDefinition = serde_json::from_value(json!({
        "entity": "thing", "schema": {},
        "lifecycle": { "initial": "new", "states": ["new", "done"] },
        "operations": {
            "touch": { "transitions": [{ "from": "new", "to": "new" }] },
            "finish": { "transitions": [{ "from": "new", "to": "done" }] }
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
        recorded_at: "2026-09-10T12:00:00Z".into(),
        correlation: None,
        causation: None,
        actor: Some("test".into()),
    }
}

#[test]
fn asynchronous_decisions_suspend_and_replay_zero_event_history() {
    let registry = registry();
    let mut store = SuspendingStore::default();
    run(async {
        let mut runtime = AsyncStoredRuntime::new(&registry, &mut store);
        let created = runtime
            .create("thing", 1, "one", json!({}), &recording("create"))
            .await
            .unwrap();
        assert!(created.envelope.record.events.is_empty());
        let accepted = runtime
            .execute("thing", "one", 1, "finish", json!({}), &recording("finish"))
            .await
            .unwrap();
        assert!(accepted.envelope.record.events.is_empty());
        assert_eq!(
            runtime.get("thing", "one").await.unwrap(),
            accepted.instance
        );
        assert_eq!(runtime.list("thing").await.unwrap(), vec!["one"]);
    });
    assert_eq!(store.inner.records("thing", "one").unwrap().len(), 2);
    assert_eq!(
        store.inner.load("thing", "one").unwrap().unwrap().revision,
        2
    );
    assert!(
        store.waits >= 6,
        "every provider boundary must actually suspend"
    );
}

#[test]
fn asynchronous_retries_use_original_definitions_after_state_and_registry_advance() {
    let registry = registry();
    let mut store = SuspendingStore::default();
    let (created, touched) = run(async {
        let mut runtime = AsyncStoredRuntime::new(&registry, &mut store);
        let created = runtime
            .create("thing", 1, "one", json!({}), &recording("create"))
            .await
            .unwrap();
        let touched = runtime
            .execute("thing", "one", 1, "touch", json!({}), &recording("touch"))
            .await
            .unwrap();
        runtime
            .execute("thing", "one", 2, "finish", json!({}), &recording("finish"))
            .await
            .unwrap();
        (created, touched)
    });
    // No current registry entry can answer either retry. The pinned historical definition must.
    let changed_registry = Registry::new();
    run(async {
        let mut runtime = AsyncStoredRuntime::new(&changed_registry, &mut store);
        assert_eq!(
            runtime
                .create("thing", 1, "one", json!({}), &recording("create"))
                .await
                .unwrap(),
            created
        );
        assert_eq!(
            runtime
                .execute("thing", "one", 1, "touch", json!({}), &recording("touch"))
                .await
                .unwrap(),
            touched
        );
        let error = runtime
            .execute("thing", "one", 1, "finish", json!({}), &recording("touch"))
            .await
            .unwrap_err();
        assert!(
            matches!(error, ShellError::Store(StoreError::RecordConflict { record_id }) if record_id == "touch")
        );
        let error = runtime
            .create("thing", 2, "one", json!({}), &recording("create"))
            .await
            .unwrap_err();
        assert!(
            matches!(error, ShellError::Store(StoreError::RecordConflict { record_id }) if record_id == "create")
        );
    });
    assert_eq!(store.inner.records("thing", "one").unwrap().len(), 3);
}

#[test]
fn asynchronous_stale_intent_and_failed_commits_leave_history_unchanged() {
    let registry = registry();
    let mut store = SuspendingStore::default();
    run(AsyncStoredRuntime::new(&registry, &mut store).create(
        "thing",
        1,
        "one",
        json!({}),
        &recording("create"),
    ))
    .unwrap();
    let error = run(AsyncStoredRuntime::new(&registry, &mut store).execute(
        "thing",
        "one",
        9,
        "unknown",
        json!({}),
        &recording("stale"),
    ))
    .unwrap_err();
    assert!(matches!(
        error,
        ShellError::StaleRevision {
            expected: 9,
            found: 1,
            ..
        }
    ));
    store.fail_commit = true;
    let error = run(AsyncStoredRuntime::new(&registry, &mut store).execute(
        "thing",
        "one",
        1,
        "finish",
        json!({}),
        &recording("finish"),
    ))
    .unwrap_err();
    assert!(matches!(
        error,
        ShellError::Store(StoreError::Unreachable { .. })
    ));
    assert_eq!(store.inner.records("thing", "one").unwrap().len(), 1);
    assert_eq!(
        store.inner.load("thing", "one").unwrap().unwrap().revision,
        1
    );
}

#[test]
fn asynchronous_tampered_history_is_refused_even_for_an_exact_retry() {
    let registry = registry();
    let mut store = SuspendingStore::default();
    run(AsyncStoredRuntime::new(&registry, &mut store).create(
        "thing",
        1,
        "one",
        json!({}),
        &recording("create"),
    ))
    .unwrap();
    run(AsyncStoredRuntime::new(&registry, &mut store).execute(
        "thing",
        "one",
        1,
        "touch",
        json!({}),
        &recording("touch"),
    ))
    .unwrap();
    store.tamper = true;
    for retry in [false, true] {
        let error = if retry {
            run(AsyncStoredRuntime::new(&registry, &mut store).execute(
                "thing",
                "one",
                1,
                "touch",
                json!({}),
                &recording("touch"),
            ))
            .unwrap_err()
        } else {
            run(AsyncStoredRuntime::new(&registry, &mut store).get("thing", "one")).unwrap_err()
        };
        assert!(
            matches!(
                error,
                ShellError::Core(entity_core::CoreError::Validation(_))
            ),
            "{error:?}"
        );
    }
    assert_eq!(
        store
            .inner
            .load("thing", "one")
            .unwrap()
            .unwrap()
            .lifecycle_state,
        "new"
    );
}

#[test]
fn asynchronous_legacy_state_is_refused_without_inventing_recorded_history() {
    let registry = registry();
    let mut store = SuspendingStore::default();
    store
        .inner
        .commit(
            &Runtime::new(&registry)
                .create("thing", 1, "legacy", json!({}))
                .unwrap(),
            Expect::Absent,
        )
        .unwrap();
    let error =
        run(AsyncStoredRuntime::new(&registry, &mut store).get("thing", "legacy")).unwrap_err();
    assert!(
        matches!(error, ShellError::Store(StoreError::Backend(ref detail)) if detail.contains("no complete recorded history"))
    );
    let error =
        run(AsyncStoredRuntime::new(&registry, &mut store).get("thing", "absent")).unwrap_err();
    assert!(matches!(error, ShellError::NotFound { .. }));
}

#[test]
fn asynchronous_observations_preserve_revision_order_and_global_record_identity() {
    let registry = registry();
    let mut store = SuspendingStore::default();
    run(async {
        let mut runtime = AsyncStoredRuntime::new(&registry, &mut store);
        runtime
            .create("thing", 1, "one", json!({}), &recording("create"))
            .await
            .unwrap();
        let first = RecordedObservation {
            entity: "thing".into(),
            id: "one".into(),
            revision: 1,
            envelope: recording("observation")
                .seal(json!({"seen": true}))
                .unwrap(),
        };
        runtime.observe(&first).await.unwrap();
        assert_eq!(runtime.get("thing", "one").await.unwrap().revision, 1);
        runtime
            .execute("thing", "one", 1, "finish", json!({}), &recording("finish"))
            .await
            .unwrap();
        runtime.observe(&first).await.unwrap();
        let mut changed = first.clone();
        changed.envelope.record = json!({"seen": false});
        assert!(matches!(
            runtime.observe(&changed).await.unwrap_err(),
            ShellError::Store(StoreError::RecordConflict { .. })
        ));
        let second = RecordedObservation {
            revision: 2,
            envelope: recording("observation-2").seal(json!({})).unwrap(),
            ..first
        };
        runtime.observe(&second).await.unwrap();
        let error = runtime
            .create("thing", 1, "other", json!({}), &recording("observation"))
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            ShellError::Store(StoreError::RecordConflict { .. })
        ));
    });
    let observations = store.inner.observations("thing", "one").unwrap();
    assert_eq!(
        observations.iter().map(|o| o.revision).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert!(store.inner.events("thing", "one").unwrap().is_empty());
    assert_eq!(store.inner.ids("thing").unwrap(), vec!["one"]);
}

#[test]
fn recorded_batches_roll_back_state_history_and_retry_bookkeeping() {
    let registry = registry();
    let create = |id: &str, record_id: &str| AtomicRecordedCommit {
        commit: RecordedCommit::new(
            Runtime::new(&registry)
                .create("thing", 1, id, json!({}))
                .unwrap(),
            &recording(record_id),
        )
        .unwrap(),
        expect: Expect::Absent,
    };
    let first = create("one", "one-create");
    let second = AtomicRecordedCommit {
        commit: RecordedCommit::new(
            Runtime::new(&registry)
                .execute(&first.commit.instance, "touch", json!({}))
                .unwrap(),
            &recording("one-touch"),
        )
        .unwrap(),
        expect: Expect::Revision(1),
    };
    let mut store = BlockingRecordedStore(MemoryStore::new());
    let mut wrong = create("two", "two-create");
    wrong.expect = Expect::Revision(9);
    let error =
        run(store.commit_recorded_batch(&[first.clone(), second.clone(), wrong])).unwrap_err();
    assert!(
        matches!(error, StoreError::RevisionConflict { entity, id, found: None, .. } if entity == "thing" && id == "two")
    );
    assert!(store.0.is_empty());
    assert!(store.0.records("thing", "one").unwrap().is_empty());
    // A rolled-back record id can be used for different content. The index must roll back too.
    let reused = create("different", "one-create");
    run(store.commit_recorded_batch(&[reused])).unwrap();
    let error = run(store.commit_recorded_batch(std::slice::from_ref(&first))).unwrap_err();
    assert!(matches!(error, StoreError::RecordConflict { record_id } if record_id == "one-create"));
    let mut ordered = BlockingRecordedStore(MemoryStore::new());
    let batch = [first, second, create("two", "two-create")];
    run(ordered.commit_recorded_batch(&batch)).unwrap();
    run(ordered.commit_recorded_batch(&batch)).unwrap();
    run(ordered.commit_recorded_batch(&[])).unwrap();
    assert_eq!(ordered.0.load("thing", "one").unwrap().unwrap().revision, 2);
    assert_eq!(ordered.0.records("thing", "one").unwrap().len(), 2);
    assert_eq!(ordered.0.ids("thing").unwrap(), vec!["one", "two"]);
}

#[test]
fn asynchronous_execution_can_be_cancelled_while_awaiting_storage() {
    let registry = registry();
    let mut store = SuspendingStore::default();
    let metadata = recording("create");
    {
        let mut runtime = AsyncStoredRuntime::new(&registry, &mut store);
        let mut future = pin!(runtime.create("thing", 1, "one", json!({}), &metadata));
        fn require_send<T: Send>(_: &T) {}
        require_send(&future);
        let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
        assert!(matches!(
            future.as_mut().poll(&mut Context::from_waker(&waker)),
            Poll::Pending
        ));
    }
    assert!(store.inner.is_empty(), "cancelled before submission");
    // The same call remains usable after cancellation; no hidden executor kept it running.
    let port: &mut dyn AsyncRecordedStore = &mut store;
    run(AsyncStoredRuntime::new(&registry, port).create("thing", 1, "one", json!({}), &metadata))
        .unwrap();
}
