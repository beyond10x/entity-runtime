//! Deterministic counters: the same tree measured twice gives the same numbers, on any machine.
//! Each scenario reproduces the fixture of the review finding it is named for.

use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use entity_core::{Decision, EntityInstance, Runtime};
use entity_eventlog::{
    sync::{CallWait, ShutdownMode, ShutdownOutcome},
    EventlogFileStore,
};
use entity_executor::{BatchAction, Executor};
use entity_query::{DocumentQuery, DocumentQueryProvider};
use entity_remote::hybrid::{
    Authority as Side, Divergence, Hybrid, OnDivergence, Policy, ReadPath, StoreSide,
    WhenUnreachable,
};
use entity_shell::StoredRuntime;
use entity_store::{
    asynchronous::{
        AppendOutcome, AppendRequest, AsyncRecordedReader, AsyncRecordedWriter, AsyncStateReader,
        AsyncStoreError, BatchKey, BoxFuture, CompleteStoreSnapshot, MemoryRecordedStore,
        RecordLookup, StoredBatch, Subject, SubjectHistory, WriteFailure,
    },
    AtomicBatchStore, AtomicCommit, Envelope, EventProvider, Expect, FileStore, HistoryProvider,
    MemoryStore, RecordedCommit, RecordedObservation, StateProvider, Store, StoreError,
};
use serde_json::json;

use crate::{
    fixture::{block_on, bridge, context, recording, registry, subject, COUNTER, TICKET},
    probe::{allocated, Io, Opens},
    stores::{create_request, execute_request, fresh},
};

/// One measured counter value.
pub type Value = (String, u64);

/// Runs one scenario and prints how long it took: each must stay under 60 s.
fn step(name: &str, scenario: impl FnOnce() -> Vec<Value>) -> Vec<Value> {
    let start = std::time::Instant::now();
    let values = scenario();
    let seconds = start.elapsed().as_secs_f64();
    eprintln!("er-kpi: {name:<48} {seconds:>6.2} s");
    assert!(
        seconds < 60.0,
        "scenario {name} took {seconds:.1} s; the budget is 60 s"
    );
    values
}

/// Every counter that needs no server, then the PostgreSQL ones when a URL is set.
pub fn run(work: &Path) -> Vec<Value> {
    let mut values = Vec::new();
    values.extend(step("file_execute_io(work)", || file_execute_io(work)));
    values.extend(step("file_create_opens(work)", || file_create_opens(work)));
    values.extend(step("eventlog_file_empty_list(work)", || {
        eventlog_file_empty_list(work)
    }));
    values.extend(step("eventlog_file_record_bytes(work)", || {
        eventlog_file_record_bytes(work)
    }));
    values.extend(step("memory_allocations()", memory_allocations));
    values.extend(step("executor_fresh_create()", executor_fresh_create));
    values.extend(step("executor_batch_retry()", executor_batch_retry));
    values.extend(step("hybrid_catch_up()", hybrid_catch_up));
    values.extend(step(
        "kernel_create_allocations()",
        kernel_create_allocations,
    ));
    values.extend(step("mcp_tools_list()", mcp_tools_list));
    values.extend(step("surface_asyncapi()", surface_asyncapi));
    values.extend(step("eventlog_append_conflict(work)", || {
        eventlog_append_conflict(work)
    }));
    #[cfg(feature = "counters")]
    {
        values.extend(step("hooked::eventlog_batch_projection(work)", || {
            hooked::eventlog_batch_projection(work)
        }));
        values.extend(step("hooked::graph_mermaid()", hooked::graph_mermaid));
        values.extend(step(
            "hooked::executor_kernel_creates()",
            hooked::executor_kernel_creates,
        ));
        values.extend(step("hooked::mcp_descriptors()", hooked::mcp_descriptors));
        values.extend(step("hooked::file_first_write_syncs(work)", || {
            hooked::file_first_write_syncs(work)
        }));
    }
    values.extend(step("crate::pg::counters()", crate::pg::counters));
    values
}

fn id(name: &str) -> String {
    name.to_owned()
}

/// P14: what one CLI-shaped execute reads and writes on a native FileStore subject with 8 and
/// 32 prior records. A fresh handle per command, as each `entity execute` process opens one.
fn file_execute_io(work: &Path) -> Vec<Value> {
    let registry = registry(&[COUNTER]);
    let root = fresh(work, "p14").join("store");
    StoredRuntime::new(&registry, &mut FileStore::open(&root))
        .create(
            "counter",
            1,
            "c000",
            json!({"name": "counter"}),
            &recording("init"),
        )
        .expect("create");
    let mut values = Vec::new();
    for prior in 1..=32u64 {
        let measured = prior == 8 || prior == 32;
        let before = Io::now();
        let mut store = FileStore::open(&root);
        let mut runtime = StoredRuntime::new(&registry, &mut store);
        let revision = runtime.get("counter", "c000").expect("get").revision;
        runtime
            .execute(
                "counter",
                "c000",
                revision,
                "view",
                json!({}),
                &recording(&format!("r{prior:03}")),
            )
            .expect("execute");
        let io = before.since();
        if measured {
            values.push((
                id(&format!("file.execute.read_bytes.prior{prior}")),
                io.rchar,
            ));
            values.push((
                id(&format!("file.execute.write_bytes.prior{prior}")),
                io.wchar,
            ));
        }
    }
    values
}

/// P6: subject files a fresh FileStore handle opens read-only to create subject 9 and 33.
fn file_create_opens(work: &Path) -> Vec<Value> {
    let registry = registry(&[COUNTER]);
    let root = fresh(work, "p6").join("store");
    let mut values = Vec::new();
    for at in 0..33usize {
        let existing = at;
        let watch = (existing == 8 || existing == 32).then(|| Opens::watch(&root.join("subjects")));
        StoredRuntime::new(&registry, &mut FileStore::open(&root))
            .create(
                "counter",
                1,
                format!("c{at:03}"),
                json!({"name": "counter"}),
                &recording(&format!("r{at:03}")),
            )
            .expect("create");
        if let Some(watch) = watch {
            let counts = watch.finish();
            values.push((
                id(&format!("file.create.subject_read_opens.n{}", existing + 1)),
                counts.read_only_total(),
            ));
        }
    }
    values
}

/// P2 (and P8, which the read bytes show): an Eventlog File open, an `ids` for a type nobody
/// stored and the shutdown, over 8 and 64 unrelated tickets. The CLI's `list` does exactly this.
/// The tickets are written in named batches of 8: one create per command cost about 0.6 s each
/// on a loaded workstation, which put the scenario over its budget. A batch keeps one batch blob
/// per 8 tickets, so the counts are lower than the review's one-create-per-process store; they
/// grow with the tickets in the same way, which is what P2 is about.
fn eventlog_file_empty_list(work: &Path) -> Vec<Value> {
    let registry = registry(&[TICKET]);
    let root = fresh(work, "p2").join("eventlog");
    let mut store = EventlogFileStore::provision(
        &root,
        registry.clone(),
        crate::stores::authority("p2"),
        context("provision"),
        crate::fixture::LIMITS,
        bridge(),
    )
    .expect("provisioned");
    let authority = store.recorded().authority().clone();
    let mut values = Vec::new();
    for chunk in 0..8usize {
        let actions = (chunk * 8 + 1..=chunk * 8 + 8)
            .map(|ticket| {
                BatchAction::Create(entity_executor::CreateRequest {
                    subject: subject("ticket", &format!("ticket-{ticket}")),
                    definition_version: 1,
                    fields: json!({ "title": format!("ticket-{ticket}") }),
                    recording: recording(&format!("create-{ticket}")),
                })
            })
            .collect();
        let outcome = store.recorded().execute_batch(
            context(&format!("seed-{chunk}")),
            BatchKey::Named(format!("seed-{chunk}")),
            actions,
            CallWait::Forever,
        );
        assert!(matches!(outcome, Ok(AppendOutcome::Committed { .. })));
        let ticket = chunk * 8 + 8;
        if ticket == 8 || ticket == 64 {
            settle(store);
            let watch = Opens::watch(&root);
            let before = Io::now();
            let mut reader = EventlogFileStore::open(
                &root,
                entity_core::Registry::new(),
                authority.clone(),
                crate::fixture::LIMITS,
                bridge(),
            )
            .expect("opened");
            assert!(reader.ids("absent").expect("ids").is_empty());
            assert_eq!(
                reader.shutdown(ShutdownMode::Drain, CallWait::Forever),
                ShutdownOutcome::Joined { provider: Ok(()) }
            );
            let io = before.since();
            let counts = watch.finish();
            values.push((
                id(&format!(
                    "eventlog_file.list_empty.blob_read_opens.t{ticket}"
                )),
                counts.read_only_under("blobs"),
            ));
            values.push((
                id(&format!("eventlog_file.list_empty.read_bytes.t{ticket}")),
                io.rchar,
            ));
            store = EventlogFileStore::open(
                &root,
                registry.clone(),
                authority.clone(),
                crate::fixture::LIMITS,
                bridge(),
            )
            .expect("reopened");
        }
    }
    settle(store);
    values
}

/// Issue 94: blob bytes one create appends to an Eventlog File store. The first create of a type
/// also stores its definition once; the second appends only its own record, request, batch and
/// entry blobs, which since 0.33.0 name the definition by digest instead of repeating it.
fn eventlog_file_record_bytes(work: &Path) -> Vec<Value> {
    fn blob_bytes(directory: &Path) -> u64 {
        let mut total = 0;
        for entry in std::fs::read_dir(directory).expect("readable store directory") {
            let entry = entry.expect("directory entry");
            let path = entry.path();
            let kind = entry.file_type().expect("file type");
            if kind.is_dir() {
                total += blob_bytes(&path);
            } else if kind.is_file() && path.components().any(|part| part.as_os_str() == "blobs") {
                total += entry.metadata().expect("metadata").len();
            }
        }
        total
    }
    let registry = registry(&[TICKET]);
    let root = fresh(work, "i94").join("eventlog");
    let store = EventlogFileStore::provision(
        &root,
        registry,
        crate::stores::authority("i94"),
        context("provision"),
        crate::fixture::LIMITS,
        bridge(),
    )
    .expect("provisioned");
    let mut values = Vec::new();
    let mut held = blob_bytes(&root);
    for (at, name) in ["first", "second"].into_iter().enumerate() {
        let outcome = store.recorded().execute_batch(
            context(&format!("create-{at}")),
            BatchKey::Named(format!("create-{at}")),
            vec![BatchAction::Create(entity_executor::CreateRequest {
                subject: subject("ticket", &format!("ticket-{at}")),
                definition_version: 1,
                fields: json!({ "title": format!("ticket-{at}") }),
                recording: recording(&format!("create-{at}")),
            })],
            CallWait::Forever,
        );
        assert!(matches!(outcome, Ok(AppendOutcome::Committed { .. })));
        let now = blob_bytes(&root);
        values.push((
            id(&format!("eventlog_file.create.blob_bytes.{name}")),
            now - held,
        ));
        held = now;
    }
    settle(store);
    values
}

fn settle(mut store: EventlogFileStore) {
    assert_eq!(
        store.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
}

fn ticket_import(id: String) -> Decision {
    Decision::legacy_import(
        EntityInstance {
            id,
            entity: "ticket".into(),
            version: 1,
            revision: 1,
            lifecycle_state: "open".into(),
            fields: [("title".into(), json!("t".repeat(1024)))]
                .into_iter()
                .collect(),
        },
        Vec::new(),
    )
}

/// E07 and E08: a one-row MemoryStore page and a one-member `commit_batch` beside 16 and 1,024
/// subjects of 1 KiB each.
fn memory_allocations() -> Vec<Value> {
    let mut values = Vec::new();
    for subjects in [16usize, 1024] {
        let mut store = MemoryStore::new();
        for at in 0..subjects {
            store
                .commit(&ticket_import(format!("{at:04}")), Expect::Absent)
                .expect("seed");
        }
        let query = DocumentQuery::for_entity("ticket").with_limit(1);
        let (page, query_bytes) = allocated(|| store.query_documents(&query).expect("page"));
        assert_eq!(page.items.len(), 1);
        assert!(page.next.is_some());
        drop(page);
        let batch = [AtomicCommit::new(
            ticket_import("extra".to_owned()),
            Expect::Absent,
        )];
        let ((), batch_bytes) = allocated(|| store.commit_batch(&batch).expect("batch"));
        assert_eq!(store.len(), subjects + 1);
        values.push((
            id(&format!("memory.query_one_row.alloc_bytes.n{subjects}")),
            query_bytes,
        ));
        values.push((
            id(&format!("memory.commit_batch_one.alloc_bytes.n{subjects}")),
            batch_bytes,
        ));
    }
    values
}

/// P16: what one kernel create allocates (the definition snapshot is most of it).
fn kernel_create_allocations() -> Vec<Value> {
    let registry = registry(&[COUNTER]);
    let runtime = Runtime::new(&registry);
    let _ = runtime
        .create("counter", 1, "warm", json!({"name": "counter"}))
        .expect("warm");
    let (_, bytes) = allocated(|| {
        runtime
            .create("counter", 1, "c000", json!({"name": "counter"}))
            .expect("create")
    });
    vec![(id("kernel.create.alloc_bytes"), bytes)]
}

/// E06's cost as allocation: one fresh recorded create through the executor.
fn executor_fresh_create() -> Vec<Value> {
    let registry = registry(&[COUNTER]);
    let store = MemoryRecordedStore::default();
    let executor = Executor::new(&registry, &store);
    let _ = block_on(executor.create(create_request("warm", "warm-create"))).expect("warm");
    let (outcome, bytes) =
        allocated(|| block_on(executor.create(create_request("c000", "c000-create"))));
    assert!(matches!(
        outcome,
        Ok(AppendOutcome::Committed {
            replayed: false,
            ..
        })
    ));
    vec![(id("executor.fresh_create.alloc_bytes"), bytes)]
}

/// Counts `history` calls on an async recorded store.
struct CountingRecorded {
    inner: MemoryRecordedStore,
    histories: AtomicU64,
}

impl AsyncStateReader for CountingRecorded {
    fn load<'a>(
        &'a self,
        subject: &'a Subject,
    ) -> BoxFuture<'a, Result<Option<EntityInstance>, AsyncStoreError>> {
        self.inner.load(subject)
    }
}

impl AsyncRecordedReader for CountingRecorded {
    fn lookup_record<'a>(
        &'a self,
        record_id: &'a str,
    ) -> BoxFuture<'a, Result<Option<RecordLookup>, AsyncStoreError>> {
        self.inner.lookup_record(record_id)
    }
    fn lookup_batch<'a>(
        &'a self,
        key: &'a BatchKey,
    ) -> BoxFuture<'a, Result<Option<StoredBatch>, AsyncStoreError>> {
        self.inner.lookup_batch(key)
    }
    fn history<'a>(
        &'a self,
        subject: &'a Subject,
    ) -> BoxFuture<'a, Result<SubjectHistory, AsyncStoreError>> {
        self.histories.fetch_add(1, Ordering::Relaxed);
        self.inner.history(subject)
    }
    fn complete_snapshot<'a>(
        &'a self,
        scope: &'a str,
    ) -> BoxFuture<'a, Result<CompleteStoreSnapshot, AsyncStoreError>> {
        self.inner.complete_snapshot(scope)
    }
}

impl AsyncRecordedWriter for CountingRecorded {
    fn append(&self, request: AppendRequest) -> BoxFuture<'_, Result<AppendOutcome, WriteFailure>> {
        self.inner.append(request)
    }
}

/// E04: a retried named batch of K consecutive decisions on one subject.
fn executor_batch_retry() -> Vec<Value> {
    let registry = registry(&[COUNTER]);
    let mut values = Vec::new();
    for members in [8u64, 32] {
        let store = CountingRecorded {
            inner: MemoryRecordedStore::default(),
            histories: AtomicU64::new(0),
        };
        let executor = Executor::new(&registry, &store);
        let actions = || {
            let mut actions = vec![BatchAction::Create(create_request("c000", "b-create"))];
            for revision in 1..members {
                actions.push(BatchAction::Execute(execute_request(
                    "c000",
                    revision,
                    &format!("b-view-{revision}"),
                )));
            }
            actions
        };
        let key = BatchKey::Named(format!("batch-{members}"));
        let first = block_on(executor.batch(key.clone(), actions()));
        assert!(
            matches!(
                first,
                Ok(AppendOutcome::Committed {
                    replayed: false,
                    ..
                })
            ),
            "{first:?}"
        );
        store.histories.store(0, Ordering::Relaxed);
        let retry = block_on(executor.batch(key, actions()));
        assert!(
            matches!(retry, Ok(AppendOutcome::Committed { replayed: true, .. })),
            "{retry:?}"
        );
        values.push((
            id(&format!("executor.batch_retry.history_loads.k{members}")),
            store.histories.load(Ordering::Relaxed),
        ));
    }
    values
}

/// A native store that counts `observations` reads.
struct CountingStore {
    inner: MemoryStore,
    observation_reads: AtomicU64,
}

impl StateProvider for CountingStore {
    fn load(&self, entity: &str, id: &str) -> Result<Option<EntityInstance>, StoreError> {
        self.inner.load(entity, id)
    }
    fn ids(&self, entity: &str) -> Result<Vec<String>, StoreError> {
        self.inner.ids(entity)
    }
}
impl EventProvider for CountingStore {
    fn events(&self, entity: &str, id: &str) -> Result<Vec<entity_core::DomainEvent>, StoreError> {
        self.inner.events(entity, id)
    }
}
impl HistoryProvider for CountingStore {
    fn records(
        &self,
        entity: &str,
        id: &str,
    ) -> Result<Vec<Envelope<entity_core::DecisionRecord>>, StoreError> {
        self.inner.records(entity, id)
    }
    fn observations(&self, entity: &str, id: &str) -> Result<Vec<RecordedObservation>, StoreError> {
        self.observation_reads.fetch_add(1, Ordering::Relaxed);
        self.inner.observations(entity, id)
    }
}
impl Store for CountingStore {
    fn history(&self) -> Option<&dyn HistoryProvider> {
        Some(self)
    }
    fn commit(&mut self, decision: &Decision, expect: Expect) -> Result<(), StoreError> {
        self.inner.commit(decision, expect)
    }
    fn commit_recorded(
        &mut self,
        commit: &RecordedCommit,
        expect: Expect,
    ) -> Result<(), StoreError> {
        self.inner.commit_recorded(commit, expect)
    }
    fn observe(&mut self, observation: &RecordedObservation) -> Result<(), StoreError> {
        self.inner.observe(observation)
    }
}

/// E09: restoring N contiguous revisions, one observation each, to an empty replica.
fn hybrid_catch_up() -> Vec<Value> {
    let registry = registry(&[COUNTER]);
    let runtime = Runtime::new(&registry);
    let mut values = Vec::new();
    for revisions in [8u64, 32] {
        let mut local = MemoryStore::new();
        let mut instance = None::<EntityInstance>;
        let mut last = String::new();
        for revision in 1..=revisions {
            let decision = match &instance {
                None => runtime
                    .create("counter", 1, "c000", json!({"name": "counter"}))
                    .expect("create"),
                Some(held) => runtime.execute(held, "view", json!({})).expect("view"),
            };
            last = format!("r{revision:03}");
            let commit = RecordedCommit::new(decision, &recording(&last)).expect("record");
            let expect = instance
                .as_ref()
                .map_or(Expect::Absent, |held| Expect::Revision(held.revision));
            local.commit_recorded(&commit, expect).expect("commit");
            local
                .observe(&RecordedObservation {
                    entity: "counter".into(),
                    id: "c000".into(),
                    revision,
                    envelope: recording(&format!("o{revision:03}"))
                        .seal(json!({ "seen": revision }))
                        .expect("seal"),
                })
                .expect("observe");
            instance = Some(commit.instance);
        }
        let remote = CountingStore {
            inner: MemoryStore::new(),
            observation_reads: AtomicU64::new(0),
        };
        let mut hybrid = Hybrid::new(
            local,
            remote,
            Policy::new(
                Side::Local,
                ReadPath::LocalFirst,
                WhenUnreachable::ServeStale,
                OnDivergence::RecordDivergence,
            ),
        );
        hybrid.remember(Divergence {
            entity: "counter".into(),
            id: "c000".into(),
            local_revision: revisions,
            source: StoreSide::Local,
            destination: StoreSide::Remote,
            record_id: Some(last),
            detail: "replica was offline".into(),
        });
        assert_eq!(hybrid.catch_up(), 0, "{:?}", hybrid.divergences());
        assert_eq!(
            hybrid
                .remote()
                .inner
                .observations("counter", "c000")
                .expect("observations")
                .len() as u64,
            revisions
        );
        values.push((
            id(&format!(
                "hybrid.catch_up.destination_observation_reads.n{revisions}"
            )),
            hybrid.remote().observation_reads.load(Ordering::Relaxed),
        ));
    }
    values
}

/// E20's cost as allocation: four `tools/list` requests on one server.
fn mcp_tools_list() -> Vec<Value> {
    let registry = registry(&[COUNTER]);
    let mut server = entity_mcp::Server::new(&registry, MemoryStore::new()).expect("server");
    let _ = server.handle(json!({"jsonrpc": "2.0", "id": 0, "method": "tools/list"}));
    let (_, bytes) = allocated(|| {
        for request in 1..=4 {
            let reply = server
                .handle(json!({"jsonrpc": "2.0", "id": request, "method": "tools/list"}))
                .expect("reply");
            assert_eq!(reply["result"]["tools"].as_array().map(Vec::len), Some(8));
        }
    });
    vec![(id("mcp.tools_list_x4.alloc_bytes"), bytes)]
}

/// E03: messages the AsyncAPI document declares for the service/1 outcome fixture. A correctness
/// counter: the outcome emits `ParcelOpened`, so one message is right and zero is the defect.
fn surface_asyncapi() -> Vec<Value> {
    let document = entity_surface::asyncapi(&[crate::fixture::definition(crate::fixture::OUTCOME)]);
    let messages = document["components"]["messages"]
        .as_object()
        .map_or(0, |messages| messages.len() as u64);
    vec![(id("surface.asyncapi.outcome_messages"), messages)]
}

/// E19: append attempts while every physical append answers `Conflict`. The injecting backend
/// gives up after [`crate::conflict::CAP`] attempts, so an unbounded retry reads as the cap.
fn eventlog_append_conflict(work: &Path) -> Vec<Value> {
    let attempts = crate::conflict::attempts_under_conflict(&fresh(work, "e19"));
    vec![(id("eventlog.append_conflict.attempts"), attempts)]
}

#[cfg(feature = "counters")]
mod hooked {
    use super::*;

    fn read(counter: &AtomicU64) -> u64 {
        counter.load(Ordering::Relaxed)
    }

    /// E05: batch blob reads in one K-member inline projection pass.
    pub fn eventlog_batch_projection(work: &Path) -> Vec<Value> {
        use entity_eventlog::kpi_counters::{BATCH_BLOB_BYTES, BATCH_BLOB_FETCHES};
        let registry = registry(&[TICKET]);
        let mut values = Vec::new();
        for members in [8usize, 32] {
            let directory = fresh(work, &format!("e05-{members}"));
            let mut store = entity_sqlite::EventlogSqliteStore::provision(
                crate::stores::path_string(&directory.join("eventlog.sqlite")),
                "er_kpi",
                registry.clone(),
                crate::stores::authority("e05"),
                context("provision"),
                crate::fixture::LIMITS,
                bridge(),
            )
            .expect("provisioned");
            let actions = (0..members)
                .map(|at| {
                    BatchAction::Create(entity_executor::CreateRequest {
                        subject: subject("ticket", &format!("t{at:03}")),
                        definition_version: 1,
                        fields: json!({ "title": format!("t{at:03}") }),
                        recording: recording(&format!("t{at:03}-create")),
                    })
                })
                .collect();
            let (fetches, bytes) = (read(&BATCH_BLOB_FETCHES), read(&BATCH_BLOB_BYTES));
            let outcome = store.recorded().execute_batch(
                context("batch"),
                BatchKey::Named("batch".into()),
                actions,
                CallWait::Forever,
            );
            assert!(matches!(outcome, Ok(AppendOutcome::Committed { .. })));
            values.push((
                id(&format!(
                    "eventlog.projection.batch_blob_fetches.k{members}"
                )),
                read(&BATCH_BLOB_FETCHES) - fetches,
            ));
            values.push((
                id(&format!("eventlog.projection.batch_blob_bytes.k{members}")),
                read(&BATCH_BLOB_BYTES) - bytes,
            ));
            assert_eq!(
                store.shutdown(ShutdownMode::Drain, CallWait::Forever),
                ShutdownOutcome::Joined { provider: Ok(()) }
            );
        }
        values
    }

    /// E10: endpoint comparisons drawing the 300-state chain as Mermaid.
    pub fn graph_mermaid() -> Vec<Value> {
        use entity_graph::kpi_counters::ENDPOINT_COMPARISONS;
        let definition = crate::fixture::definition(crate::fixture::GRAPH_300);
        let graph = entity_graph::Graph::lifecycle(&definition);
        let before = read(&ENDPOINT_COMPARISONS);
        let drawing = entity_graph::render::mermaid(&graph);
        assert!(drawing.matches(" --> ").count() >= 299);
        vec![(
            id("graph.mermaid.endpoint_comparisons.chain300"),
            read(&ENDPOINT_COMPARISONS) - before,
        )]
    }

    /// E06: kernel creation decisions for one fresh recorded create.
    pub fn executor_kernel_creates() -> Vec<Value> {
        use entity_executor::kpi_counters::KERNEL_CREATES;
        let registry = registry(&[COUNTER]);
        let store = MemoryRecordedStore::default();
        let executor = Executor::new(&registry, &store);
        let before = read(&KERNEL_CREATES);
        let outcome = block_on(executor.create(create_request("c000", "c000-create")));
        assert!(matches!(
            outcome,
            Ok(AppendOutcome::Committed {
                replayed: false,
                ..
            })
        ));
        vec![(
            id("executor.fresh_create.kernel_creates"),
            read(&KERNEL_CREATES) - before,
        )]
    }

    /// E20: descriptors built for four `tools/list` requests on one server.
    pub fn mcp_descriptors() -> Vec<Value> {
        use entity_mcp::kpi_counters::DESCRIPTORS_BUILT;
        let registry = registry(&[COUNTER]);
        let mut server = entity_mcp::Server::new(&registry, MemoryStore::new()).expect("server");
        let before = read(&DESCRIPTORS_BUILT);
        for request in 1..=4 {
            let _ = server.handle(json!({"jsonrpc": "2.0", "id": request, "method": "tools/list"}));
        }
        vec![(
            id("mcp.tools_list_x4.descriptors_built"),
            read(&DESCRIPTORS_BUILT) - before,
        )]
    }

    /// E13: fsyncs (file and directory) of the first write into an empty FileStore, and of the
    /// first write of a second type.
    pub fn file_first_write_syncs(work: &Path) -> Vec<Value> {
        use entity_store::kpi_counters::{DIRECTORY_SYNCS, FILE_SYNCS};
        let registry = registry(&[COUNTER, TICKET]);
        let root = fresh(work, "e13").join("store");
        let syncs = || read(&FILE_SYNCS) + read(&DIRECTORY_SYNCS);
        let before = syncs();
        StoredRuntime::new(&registry, &mut FileStore::open(&root))
            .create(
                "counter",
                1,
                "c000",
                json!({"name": "counter"}),
                &recording("c000"),
            )
            .expect("first write");
        let first = syncs() - before;
        let before = syncs();
        StoredRuntime::new(&registry, &mut FileStore::open(&root))
            .create(
                "ticket",
                1,
                "t000",
                json!({"title": "ticket"}),
                &recording("t000"),
            )
            .expect("first write of a second type");
        let second_type = syncs() - before;
        let before = syncs();
        StoredRuntime::new(&registry, &mut FileStore::open(&root))
            .create(
                "ticket",
                1,
                "t001",
                json!({"title": "ticket"}),
                &recording("t001"),
            )
            .expect("second write of a type");
        let later = syncs() - before;
        vec![
            (id("file.first_write.fsyncs"), first),
            (id("file.first_write_of_type.fsyncs"), second_type),
            (id("file.later_write_of_type.fsyncs"), later),
        ]
    }
}
