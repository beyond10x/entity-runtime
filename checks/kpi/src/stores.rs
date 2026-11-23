//! One shape over every provider a timing scenario measures: create, execute, list and page.

use std::path::{Path, PathBuf};

use entity_core::Registry;
use entity_eventlog::{
    sync::{CallWait, ProvisionAuthority, ShutdownMode, ShutdownOutcome},
    EventlogFileStore,
};
use entity_executor::{CreateRequest, ExecuteRequest};
use entity_query::{DocumentQuery, DocumentQueryProvider};
use entity_shell::StoredRuntime;
use entity_sqlite::{EventlogSqliteStore, SqliteStore};
use entity_store::{asynchronous::AppendOutcome, FileStore, MemoryStore, StateProvider, Store};
use serde_json::json;

use crate::fixture::{bridge, context, recording, subject, LIMITS};

/// The providers the KPI gate measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `MemoryStore`.
    Memory,
    /// Native `FileStore` (the CLI's default).
    File,
    /// Native `SqliteStore` on a file.
    Sqlite,
    /// Eventlog File through `EventlogFileStore`.
    EventlogFile,
    /// Eventlog SQLite through `EventlogSqliteStore`, on a file.
    EventlogSqlite,
    /// Native `PostgresStore`, only when `ENTITY_POSTGRES_URL` is set.
    Postgres,
}

impl Kind {
    /// The id prefix in `kpis.yaml`.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::File => "file",
            Self::Sqlite => "sqlite",
            Self::EventlogFile => "eventlog_file",
            Self::EventlogSqlite => "eventlog_sqlite",
            Self::Postgres => "postgres",
        }
    }

    /// Every provider, PostgreSQL last.
    pub const ALL: [Self; 6] = [
        Self::Memory,
        Self::File,
        Self::Sqlite,
        Self::EventlogFile,
        Self::EventlogSqlite,
        Self::Postgres,
    ];

    /// Whether the provider pages documents (`DocumentQueryProvider`).
    pub const fn pages(self) -> bool {
        matches!(
            self,
            Self::Memory | Self::EventlogFile | Self::EventlogSqlite | Self::Postgres
        )
    }
}

/// The PostgreSQL server the gate may use, from the variable the repository's gate already reads.
pub fn postgres_url() -> Option<String> {
    std::env::var("ENTITY_POSTGRES_URL")
        .ok()
        .filter(|url| !url.trim().is_empty())
}

/// An open provider holding counter subjects.
pub enum Open {
    /// Native stores go through the shared shell, the way the CLI and MCP write.
    Native(Box<dyn NativeStore>),
    /// Eventlog stores go through their complete recorded facade.
    EventlogFile(EventlogFileStore),
    /// Eventlog SQLite.
    EventlogSqlite(EventlogSqliteStore),
}

/// A native store driven through the shared shell.
pub trait NativeStore {
    /// Creates one counter subject.
    fn create(&mut self, registry: &Registry, id: &str, record: &str);
    /// Runs `view` once.
    fn execute(&mut self, registry: &Registry, id: &str, revision: u64, record: &str);
    /// Pages documents when the provider can.
    fn query(&self, query: &DocumentQuery) -> Option<usize>;
    /// Lists identities.
    fn ids(&self, entity: &str) -> Vec<String>;
}

/// Whether a native provider pages documents.
pub trait MaybeQuery {
    /// One page's length, or `None` when the provider has no document query.
    fn page(&self, query: &DocumentQuery) -> Option<usize>;
}

impl MaybeQuery for MemoryStore {
    fn page(&self, query: &DocumentQuery) -> Option<usize> {
        Some(self.query_documents(query).expect("page").items.len())
    }
}
impl MaybeQuery for entity_postgres::PostgresStore {
    fn page(&self, query: &DocumentQuery) -> Option<usize> {
        Some(self.query_documents(query).expect("page").items.len())
    }
}
impl MaybeQuery for FileStore {
    fn page(&self, _: &DocumentQuery) -> Option<usize> {
        None
    }
}
impl MaybeQuery for SqliteStore {
    fn page(&self, _: &DocumentQuery) -> Option<usize> {
        None
    }
}

struct Native<S>(S);

impl<S: Store + MaybeQuery> NativeStore for Native<S> {
    fn create(&mut self, registry: &Registry, id: &str, record: &str) {
        StoredRuntime::new(registry, &mut self.0)
            .create("counter", 1, id, json!({ "name": id }), &recording(record))
            .expect("native create");
    }
    fn execute(&mut self, registry: &Registry, id: &str, revision: u64, record: &str) {
        StoredRuntime::new(registry, &mut self.0)
            .execute(
                "counter",
                id,
                revision,
                "view",
                json!({}),
                &recording(record),
            )
            .expect("native execute");
    }
    fn query(&self, query: &DocumentQuery) -> Option<usize> {
        self.0.page(query)
    }
    fn ids(&self, entity: &str) -> Vec<String> {
        self.0.ids(entity).expect("ids")
    }
}

impl Open {
    /// Provisions an empty provider of `kind` under `directory` (or a fresh PostgreSQL schema).
    pub fn provision(kind: Kind, directory: &Path, registry: &Registry, label: &str) -> Self {
        std::fs::create_dir_all(directory).expect("work directory");
        match kind {
            Kind::Memory => Self::Native(Box::new(Native(MemoryStore::new()))),
            Kind::File => Self::Native(Box::new(Native(FileStore::open(directory.join("store"))))),
            Kind::Sqlite => Self::Native(Box::new(Native(
                SqliteStore::open(directory.join("store.sqlite")).expect("sqlite store"),
            ))),
            Kind::Postgres => {
                let url = postgres_url().expect("PostgreSQL scenarios run only with a URL");
                let schema = format!("er_kpi_{label}_{}", std::process::id());
                let mut store = entity_postgres::PostgresStore::connect_in_schema(&url, &schema)
                    .expect("PostgreSQL store");
                store.drop_schema(&schema).expect("fresh schema");
                let store = entity_postgres::PostgresStore::connect_in_schema(&url, &schema)
                    .expect("PostgreSQL store");
                Self::Native(Box::new(Native(store)))
            }
            Kind::EventlogFile => Self::EventlogFile(
                EventlogFileStore::provision(
                    directory.join("eventlog"),
                    registry.clone(),
                    authority(label),
                    context("provision"),
                    LIMITS,
                    bridge(),
                )
                .expect("Eventlog File provisioned"),
            ),
            Kind::EventlogSqlite => Self::EventlogSqlite(
                EventlogSqliteStore::provision(
                    path_string(&directory.join("eventlog.sqlite")),
                    "er_kpi",
                    registry.clone(),
                    authority(label),
                    context("provision"),
                    LIMITS,
                    bridge(),
                )
                .expect("Eventlog SQLite provisioned"),
            ),
        }
    }

    /// Creates one counter subject.
    pub fn create(&mut self, registry: &Registry, id: &str) {
        let record = format!("{id}-create");
        match self {
            Self::Native(store) => store.create(registry, id, &record),
            Self::EventlogFile(store) => committed(store.recorded().create(
                context(&record),
                create_request(id, &record),
                CallWait::Forever,
            )),
            Self::EventlogSqlite(store) => committed(store.recorded().create(
                context(&record),
                create_request(id, &record),
                CallWait::Forever,
            )),
        }
    }

    /// Runs `view` once on a subject at `revision`.
    pub fn execute(&mut self, registry: &Registry, id: &str, revision: u64) {
        let record = format!("{id}-view-{revision}");
        match self {
            Self::Native(store) => store.execute(registry, id, revision, &record),
            Self::EventlogFile(store) => committed(store.recorded().execute(
                context(&record),
                execute_request(id, revision, &record),
                CallWait::Forever,
            )),
            Self::EventlogSqlite(store) => committed(store.recorded().execute(
                context(&record),
                execute_request(id, revision, &record),
                CallWait::Forever,
            )),
        }
    }

    /// Lists identities of `entity`.
    pub fn ids(&self, entity: &str) -> Vec<String> {
        match self {
            Self::Native(store) => store.ids(entity),
            Self::EventlogFile(store) => store.ids(entity).expect("ids"),
            Self::EventlogSqlite(store) => store.ids(entity).expect("ids"),
        }
    }

    /// Pages one document, when the provider pages.
    pub fn query_one(&self) -> Option<usize> {
        let query = DocumentQuery::for_entity("counter").with_limit(1);
        match self {
            Self::Native(store) => store.query(&query),
            Self::EventlogFile(store) => {
                Some(store.query_documents(&query).expect("page").items.len())
            }
            Self::EventlogSqlite(store) => {
                Some(store.query_documents(&query).expect("page").items.len())
            }
        }
    }

    /// Closes an Eventlog worker and checks it settled.
    pub fn close(self) {
        let outcome = match self {
            Self::Native(_) => return,
            Self::EventlogFile(mut store) => store.shutdown(ShutdownMode::Drain, CallWait::Forever),
            Self::EventlogSqlite(mut store) => {
                store.shutdown(ShutdownMode::Drain, CallWait::Forever)
            }
        };
        assert_eq!(outcome, ShutdownOutcome::Joined { provider: Ok(()) });
    }
}

impl Open {
    /// A native FileStore at `root`.
    pub fn native_file(root: &Path) -> Self {
        Self::Native(Box::new(Native(FileStore::open(root))))
    }
}

/// The provisioning authority for a label.
pub fn authority(label: &str) -> ProvisionAuthority {
    ProvisionAuthority {
        logical_scope: format!("er-kpi/{label}"),
        tenant: format!("er-kpi-{label}"),
        expected_stream_identity: None,
    }
}

/// A counter create request.
pub fn create_request(id: &str, record: &str) -> CreateRequest {
    CreateRequest {
        subject: subject("counter", id),
        definition_version: 1,
        fields: json!({ "name": id }),
        recording: recording(record),
    }
}

/// A counter `view` request.
pub fn execute_request(id: &str, revision: u64, record: &str) -> ExecuteRequest {
    ExecuteRequest {
        subject: subject("counter", id),
        expected_revision: revision,
        operation: "view".to_owned(),
        arguments: json!({}),
        fulfillments: Default::default(),
        recording: recording(record),
    }
}

fn committed<E: std::fmt::Debug>(outcome: Result<AppendOutcome, E>) {
    match outcome {
        Ok(AppendOutcome::Committed {
            replayed: false, ..
        }) => {}
        other => panic!("the command must commit fresh: {other:?}"),
    }
}

/// A UTF-8 path for the providers that take a string.
pub fn path_string(path: &Path) -> String {
    path.to_str().expect("UTF-8 work directory").to_owned()
}

/// A fresh, empty directory under the work root.
pub fn fresh(root: &Path, name: &str) -> PathBuf {
    let path = root.join(name);
    if path.exists() {
        std::fs::remove_dir_all(&path).expect("old work directory removed");
    }
    std::fs::create_dir_all(&path).expect("work directory");
    path
}
