//! A recorded batch costs work linear in its members and independent of the store's size.
//!
//! The work is counted, not timed: a clock on a shared machine measures the machine. Two
//! counters see it. The projector the provider runs inside its append transaction is wrapped in
//! [`Counting`], which tallies every event it folds, every blob it reads (calls and bytes) and every
//! projection row it reads or writes; and the store handle's own [`StoreCalls`] count its
//! captures, model builds, per-entity reads and the records it decodes. A batch's blob holds every
//! member, so a projector that reads it once per member reads the square of the members in bytes:
//! that is how 0.30.1 cost 3.7x per doubling, and how a re-read per member shows here. A batch
//! that took a complete capture, or decoded the store's records, would grow with the store.
//!
//! Each provider measures on one store of a few tens of subjects, so the whole file takes seconds.
#![cfg(feature = "sync-bridge")]

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use entity_core::Registry;
use entity_eventlog::{
    AsyncBindingProvisioner, Authority, CapturePolicy, ErRecordedProjector, EventlogBackend,
    EventlogBindingProvisioner, EventlogOperationContext, EventlogRecordedStore, StoreCalls,
};
use entity_executor::{BatchAction, CreateRequest, ExecuteRequest, Executor};
use entity_store::{
    Recording,
    asynchronous::{AppendOutcome, BatchKey, Subject},
};
use eventlog_core::{
    AdmissionPermit, BoxFuture, CaptureLimits, EventLogError, ProjectionSpec, ProjectionStore,
    Projector, RecordedEvent, Reservation, TenantId,
};
use serde_json::{Value, json};
use time::OffsetDateTime;

const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 4_096,
    max_blobs: 16_384,
    max_projection_rows: 16_384,
    max_payload_bytes: 64 * 1024 * 1024,
};

fn registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "ticket",
        "version": 1,
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "open", "states": ["open"] },
        "operations": {
            "touch": { "transitions": [{ "from": "open", "to": "open" }], "emits": [] }
        }
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    registry
}

fn context(label: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "batch-cost".to_owned(),
        actor: "entity-eventlog-test".to_owned(),
        request_id: format!("request-{label}"),
        trace_id: format!("trace-{label}"),
        causation_id: None,
        causation_depth: 0,
        occurred_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn recording(id: &str) -> Recording {
    Recording {
        record_id: id.to_owned(),
        recorded_at: "2026-10-08T00:00:00Z".to_owned(),
        correlation: None,
        causation: None,
        actor: None,
    }
}

fn ticket(id: &str) -> Subject {
    Subject::new("ticket", id).expect("subject")
}

fn create(id: &str) -> BatchAction {
    BatchAction::Create(CreateRequest {
        subject: ticket(id),
        definition_version: 1,
        fields: json!({ "title": id }),
        recording: recording(&format!("create-{id}")),
    })
}

fn touch(id: &str) -> BatchAction {
    BatchAction::Execute(ExecuteRequest {
        subject: ticket(id),
        expected_revision: 1,
        operation: "touch".to_owned(),
        arguments: json!({}),
        fulfillments: Default::default(),
        recording: recording(&format!("touch-{id}")),
    })
}

/// Members alternate between creating a new subject and executing on a distinct seeded one the
/// store already holds, from `first_old` on, as a consumer's mixed batch does. Ids are
/// zero-padded and `label` is one character, so a member encodes to the same number of bytes
/// whichever batch and index it has.
fn members(label: char, batch: usize, first_old: usize) -> Vec<BatchAction> {
    (0..batch)
        .map(|index| {
            if index % 2 == 0 {
                create(&format!("new-{label}-{index:04}"))
            } else {
                touch(&format!("old-{:04}", first_old + index / 2))
            }
        })
        .collect()
}

/// What the wrapped projector has done, summed over every event it folded.
#[derive(Default)]
struct Tally {
    events: AtomicU64,
    blob_reads: AtomicU64,
    blob_bytes: AtomicU64,
    row_reads: AtomicU64,
    row_writes: AtomicU64,
}

fn add(counter: &AtomicU64, amount: u64) {
    counter.fetch_add(amount, Ordering::SeqCst);
}

/// The Entity Runtime projector, counted. It keeps the inner projector's name and tables, so the
/// provider admits and drives it exactly as it drives the projector itself.
struct Counting {
    inner: ErRecordedProjector,
    tally: Arc<Tally>,
}

impl Projector for Counting {
    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn projections(&self) -> &'static [ProjectionSpec] {
        self.inner.projections()
    }
    fn apply<'a>(
        &'a self,
        event: &'a RecordedEvent,
        store: &'a mut dyn ProjectionStore,
    ) -> BoxFuture<'a, Result<(), EventLogError>> {
        Box::pin(async move {
            add(&self.tally.events, 1);
            let mut view = CountingView {
                inner: store,
                tally: &self.tally,
            };
            self.inner.apply(event, &mut view).await
        })
    }
}

/// The provider's transaction view, forwarding every call and counting what the projector asks.
struct CountingView<'v> {
    inner: &'v mut dyn ProjectionStore,
    tally: &'v Tally,
}

impl ProjectionStore for CountingView<'_> {
    fn get_blob<'a>(
        &'a mut self,
        digest: &'a str,
    ) -> BoxFuture<'a, Result<Option<Vec<u8>>, EventLogError>> {
        Box::pin(async move {
            let bytes = self.inner.get_blob(digest).await?;
            add(&self.tally.blob_reads, 1);
            add(
                &self.tally.blob_bytes,
                bytes.as_ref().map_or(0, |bytes| bytes.len() as u64),
            );
            Ok(bytes)
        })
    }
    fn reserve<'a>(
        &'a mut self,
        permit: &'a AdmissionPermit,
        reservations: &'a [Reservation],
    ) -> BoxFuture<'a, Result<Vec<i64>, EventLogError>> {
        self.inner.reserve(permit, reservations)
    }
    fn upsert<'a>(
        &'a mut self,
        projection: &'a ProjectionSpec,
        tenant: &'a TenantId,
        key: &'a str,
        body: &'a Value,
    ) -> BoxFuture<'a, Result<(), EventLogError>> {
        add(&self.tally.row_writes, 1);
        self.inner.upsert(projection, tenant, key, body)
    }
    fn delete<'a>(
        &'a mut self,
        projection: &'a ProjectionSpec,
        tenant: &'a TenantId,
        key: &'a str,
    ) -> BoxFuture<'a, Result<(), EventLogError>> {
        add(&self.tally.row_writes, 1);
        self.inner.delete(projection, tenant, key)
    }
    fn get<'a>(
        &'a mut self,
        projection: &'a ProjectionSpec,
        tenant: &'a TenantId,
        key: &'a str,
    ) -> BoxFuture<'a, Result<Option<Value>, EventLogError>> {
        add(&self.tally.row_reads, 1);
        self.inner.get(projection, tenant, key)
    }
    fn get_for_update<'a>(
        &'a mut self,
        projection: &'a ProjectionSpec,
        tenant: &'a TenantId,
        key: &'a str,
    ) -> BoxFuture<'a, Result<Option<Value>, EventLogError>> {
        add(&self.tally.row_reads, 1);
        self.inner.get_for_update(projection, tenant, key)
    }
    fn find<'a>(
        &'a mut self,
        projection: &'a ProjectionSpec,
        tenant: &'a TenantId,
        field: &'a str,
        value: &'a str,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<Value>, EventLogError>> {
        add(&self.tally.row_reads, 1);
        self.inner.find(projection, tenant, field, value, limit)
    }
}

/// The work one batch did: the projector's tally and the handle's calls, as differences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Work {
    events: u64,
    blob_reads: u64,
    blob_bytes: u64,
    row_reads: u64,
    row_writes: u64,
    captures: u64,
    model_builds: u64,
    records_decoded: u64,
    scoped_reads: u64,
}

impl Work {
    fn of(tally: &Tally, calls: StoreCalls) -> Self {
        Self {
            events: tally.events.load(Ordering::SeqCst),
            blob_reads: tally.blob_reads.load(Ordering::SeqCst),
            blob_bytes: tally.blob_bytes.load(Ordering::SeqCst),
            row_reads: tally.row_reads.load(Ordering::SeqCst),
            row_writes: tally.row_writes.load(Ordering::SeqCst),
            captures: calls.captures as u64,
            model_builds: calls.model_builds as u64,
            records_decoded: calls.records_decoded as u64,
            scoped_reads: calls.scoped_reads as u64,
        }
    }

    fn since(self, before: Self) -> Self {
        Self {
            events: self.events - before.events,
            blob_reads: self.blob_reads - before.blob_reads,
            blob_bytes: self.blob_bytes - before.blob_bytes,
            row_reads: self.row_reads - before.row_reads,
            row_writes: self.row_writes - before.row_writes,
            captures: self.captures - before.captures,
            model_builds: self.model_builds - before.model_builds,
            records_decoded: self.records_decoded - before.records_decoded,
            scoped_reads: self.scoped_reads - before.scoped_reads,
        }
    }

    fn named(self) -> [(&'static str, u64); 9] {
        [
            ("events", self.events),
            ("blob_reads", self.blob_reads),
            ("blob_bytes", self.blob_bytes),
            ("row_reads", self.row_reads),
            ("row_writes", self.row_writes),
            ("captures", self.captures),
            ("model_builds", self.model_builds),
            ("records_decoded", self.records_decoded),
            ("scoped_reads", self.scoped_reads),
        ]
    }
}

#[derive(Clone, Copy, Debug)]
enum Provider {
    #[cfg(feature = "sqlite")]
    Sqlite,
    #[cfg(feature = "file")]
    File,
}

/// One store, its counted projector and a warm handle over it.
struct Bench {
    provider: Provider,
    handle: EventlogRecordedStore,
    tally: Arc<Tally>,
    registry: Registry,
    /// Subjects seeded so far: `old-0000` up to this.
    seeded: usize,
    /// The next seeded subject no measured batch has touched yet.
    untouched: usize,
    _directory: tempfile::TempDir,
}

impl Bench {
    /// A bound store holding `subjects` seeded subjects. SQLite is opened `ProviderTracked`, the
    /// shape the story measured; the File provider with the facade's default policy.
    async fn open(provider: Provider, subjects: usize) -> Self {
        let directory = tempfile::tempdir().expect("store directory");
        let tenant = TenantId::new("batch-cost").expect("tenant");
        let (backend, policy): (Arc<dyn EventlogBackend>, CapturePolicy) = match provider {
            #[cfg(feature = "sqlite")]
            Provider::Sqlite => {
                let path = directory.path().join("store.sqlite3");
                let concrete = eventlog_sqlite::SqliteEventStore::open(
                    path.to_str().expect("UTF-8 path"),
                    "batch_cost",
                )
                .await
                .expect("SQLite provider");
                (Arc::new(concrete), CapturePolicy::ProviderTracked)
            }
            #[cfg(feature = "file")]
            Provider::File => {
                let concrete = eventlog_file::FileEventStore::open(directory.path().join("file"))
                    .await
                    .expect("file provider");
                (Arc::new(concrete), CapturePolicy::FullVerification)
            }
        };
        let stream_identity = backend.stream_identity(&tenant).await.expect("generation");
        let tally = Arc::new(Tally::default());
        let projector = Arc::new(Counting {
            inner: ErRecordedProjector::new(),
            tally: tally.clone(),
        });
        backend
            .create_projections(projector.clone())
            .await
            .expect("projection admission");
        backend
            .attach_inline_existing(projector)
            .await
            .expect("projection attachment");
        let authority = Authority {
            logical_scope: "batch-cost-scope".to_owned(),
            tenant: tenant.as_str().to_owned(),
            stream_identity,
        };
        EventlogBindingProvisioner::new(backend.clone(), LIMITS)
            .provision_binding(authority.clone(), context("binding"))
            .await
            .expect("binding");
        let handle = EventlogRecordedStore::open_with_policy(backend, authority, LIMITS, policy)
            .await
            .expect("bound store");
        let mut bench = Self {
            provider,
            handle,
            tally,
            registry: registry(),
            seeded: 0,
            untouched: 0,
            _directory: directory,
        };
        bench.seed(subjects).await;
        bench
    }

    /// Creates `subjects` more seeded subjects, in one batch.
    async fn seed(&mut self, subjects: usize) {
        let actions = (self.seeded..self.seeded + subjects)
            .map(|index| create(&format!("old-{index:04}")))
            .collect();
        let label = format!("seed-{:04}", self.seeded);
        let operation = self.handle.operation(context(&label));
        Executor::new(&self.registry, &operation)
            .batch(BatchKey::Named(label), actions)
            .await
            .expect("seed batch");
        self.seeded += subjects;
    }

    /// The work of one fresh batch of `batch` members, half of them on seeded subjects no earlier
    /// batch touched.
    async fn measure(&mut self, label: char, batch: usize) -> Work {
        let first_old = self.untouched;
        self.untouched += batch / 2;
        assert!(
            self.untouched <= self.seeded,
            "every touched subject is a distinct untouched seeded one"
        );
        let subjects = self.seeded;
        let before = Work::of(&self.tally, self.handle.calls());
        let operation = self.handle.operation(context(&format!("measured-{label}")));
        let outcome = Executor::new(&self.registry, &operation)
            .batch(
                BatchKey::Named(format!("measured-{label}")),
                members(label, batch, first_old),
            )
            .await;
        let work = Work::of(&self.tally, self.handle.calls()).since(before);
        let Ok(AppendOutcome::Committed {
            receipt,
            replayed: false,
        }) = outcome
        else {
            panic!("the measured batch must commit fresh: {outcome:?}")
        };
        assert_eq!(receipt.members().len(), batch);
        eprintln!(
            "{:?}: {batch:>3} members on {subjects:>3} seeded subjects: {work:?}",
            self.provider
        );
        work
    }
}

/// Twice the members cost at most twice every count, and a store more than twice as large the
/// same counts.
///
/// A count is a sum over members plus what a batch costs once, so linear growth is at most 2x;
/// re-reading the batch blob, which holds every member, once per member is about 3.3x in blob
/// bytes at these sizes. A batch that took a complete capture, or decoded the store's records,
/// would differ between the small and the large store.
fn assert_linear_in_members_and_flat_in_store(provider: Provider) {
    const SMALL: usize = 4;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let mut bench = Bench::open(provider, 2 * SMALL).await;
        let smaller = bench.measure('a', SMALL).await;
        let larger = bench.measure('b', 2 * SMALL).await;
        assert_eq!(
            larger.captures, smaller.captures,
            "{smaller:?} -> {larger:?}"
        );
        assert_eq!(
            larger.model_builds, smaller.model_builds,
            "{smaller:?} -> {larger:?}"
        );
        for ((name, small), (_, large)) in smaller.named().into_iter().zip(larger.named()) {
            assert!(
                large <= 2 * small,
                "{provider:?}: {} members cost {large} {name}, {:.2}x the {small} of {SMALL} \
                 members; linear is at most 2x: {smaller:?} -> {larger:?}",
                2 * SMALL,
                large as f64 / small.max(1) as f64,
            );
        }
        // From 8 seeded subjects (plus the 2 the first batch created) to more than twice that.
        bench.seed(4 * SMALL).await;
        let later = bench.measure('c', SMALL).await;
        assert_eq!(
            smaller, later,
            "{provider:?}: {SMALL} members cost different work on a store more than twice as \
             large"
        );
    });
}

/// A SQLite store opened `ProviderTracked`, the shape the story measured. A batch's blob holds
/// every member, and the projector hashed and decoded it once per member, so 0.30.1 cost the
/// square of the members.
#[cfg(feature = "sqlite")]
#[test]
fn a_recorded_batch_costs_work_linear_in_its_members_and_flat_in_the_store() {
    assert_linear_in_members_and_flat_in_store(Provider::Sqlite);
}

/// The same bounds on the File provider, the one `entity-cli` builds with (`eventlog-providers`
/// enables `file`). Its projector reads the batch blob with `ProjectionStore::get_blob` once per
/// projection pass of a group.
#[cfg(feature = "file")]
#[test]
fn a_file_provider_batch_costs_work_linear_in_its_members_and_flat_in_the_store() {
    assert_linear_in_members_and_flat_in_store(Provider::File);
}
