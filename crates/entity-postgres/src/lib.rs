//! A store over PostgreSQL: the provider an organisation actually runs.
//!
//! `entity-sqlite` proved the SPI with one file and no server (R-103: one transaction, both
//! halves). This is the same promise where two writers to one instance is the normal case rather
//! than the exception — and the first provider in this workspace that needs a server to be tested,
//! which shapes everything about how it is tested (see § *A gate that says when it did not run*).
//!
//! # Schema
//!
//! ```sql
//! CREATE TABLE instances (entity, id, revision, document, PRIMARY KEY (entity, id));
//! CREATE TABLE events (entity, id, revision, position, document, PRIMARY KEY (entity, id, revision, position));
//! CREATE TABLE provider_sequences (namespace PRIMARY KEY, next_value);
//! ```
//!
//! The same two tables as `entity-sqlite`, for the same reasons: the instance is one JSON document
//! rather than a column per field, because the fields are the definition's and change when it does;
//! `revision` is lifted out because it is the one thing a write compares before deciding.
//! [`PostgresStore::migrate`] creates both, idempotently, and is the only DDL this crate holds.
//!
//! # One transaction, and how two writers resolve
//!
//! [`Store::commit`] runs in one transaction: it reads the held revision **with `FOR UPDATE`**, so
//! a second writer of the same instance waits on the first's lock and then sees the revision the
//! first wrote — its stale `Expect` is refused as [`StoreError::RevisionConflict`] naming the
//! revision it lost to, never silently overwriting (R-84 under real concurrency). Two writers
//! *creating* one identity have no row to lock; both read *absent*, both insert, and the second
//! insert fails the primary key — that failure is turned into the same `RevisionConflict`, with the
//! revision the first writer landed, by re-reading after the refused insert. `READ COMMITTED`, the
//! default, is enough for this: the row lock is what serialises writers of one instance, and
//! writers of different instances do not wait on each other. `SERIALIZABLE` was considered and not
//! taken: it would refuse with a serialization failure the caller has to retry, for a case a row
//! lock resolves with an answer.
//!
//! # A gate that says when it did not run
//!
//! This repository's gate reaches no network. A provider that cannot be tested without a server
//! cannot be in `task check` unconditionally, and a provider whose tests silently skip reads
//! exactly like a tested one. So the tests run when `ENTITY_POSTGRES_URL` names a server, and the
//! gate's `postgres-check` step prints `postgres-check: skipped, ENTITY_POSTGRES_URL unset` when it
//! does not — a green gate cannot read as a tested provider. When the variable *is* set and the
//! server does not answer, the tests **fail**: a variable somebody set is a claim that the server is
//! there. CI sets it, against a service container.
//!
//! # What the caller owns
//!
//! The connection. [`PostgresStore::connect`] takes a URL and speaks without TLS; pooling, TLS and
//! authentication beyond the URL are the deployment's, not this crate's — choosing a TLS backend
//! here would choose it for every adopter.

use std::{
    collections::{btree_map::Entry, BTreeMap},
    sync::Mutex,
};

use entity_core::{Decision, DecisionRecord, DomainEvent, EntityInstance};
#[cfg(feature = "eventlog-facade")]
use entity_eventlog::{
    sync::{
        BridgeConfig, BridgeStartError, CallWait, EventlogRecordedStoreOwner,
        EventlogRecordedStoreProvisioner, ProvisionAuthority, ShutdownMode, ShutdownOutcome,
    },
    Authority, EventlogOperationContext, RecordedProviderFacade,
};
use entity_query::{DocumentPage, DocumentQuery, DocumentQueryProvider, QueryError};
use entity_store::{
    asynchronous::{
        HistoryOrigin, ImportedRecordEvidence, KnownLegacyOrder, LegacyAnchor, LegacyCompleteness,
        LegacyEvidence, LegacyOrderDeclaration, RecordedEntry, Subject, SubjectHistory,
    },
    check, AtomicBatchStore, AtomicCommit, Envelope, EventProvider, Expect, HistoryProvider,
    LegacyStoreSnapshot, LegacyStoreSource, RecordedCommit, RecordedObservation, StateProvider,
    Store, StoreError,
};
use postgres::{Client, GenericClient, IsolationLevel, NoTls};

/// A [`Store`] over one PostgreSQL connection.
///
/// The synchronous client needs `&mut` for every query and the read half of the SPI is `&self`, so
/// the client sits behind a mutex: one connection, one statement at a time, which is what a
/// connection is anyway. A caller that wants parallel readers opens more stores.
pub struct PostgresStore {
    client: Mutex<Client>,
}

/// Eventlog-backed PostgreSQL facade with complete recorded receipts and atomic groups.
///
/// The caller supplies Eventlog's typed connection authority and finite pool/admission facts;
/// this type never selects credentials, TLS roots, a schema, or a connection string.
#[cfg(feature = "eventlog-facade")]
#[derive(Debug)]
pub struct EventlogPostgresStore {
    facade: RecordedProviderFacade,
}

/// Caller-supplied pool, database-budget and capture bounds for one PostgreSQL facade.
#[cfg(feature = "eventlog-facade")]
#[derive(Debug, Clone)]
pub struct PostgresFacadeAdmission {
    /// Finite process-local pool configuration.
    pub options: eventlog_postgres::PoolOptions,
    /// Observed database-wide connection capacity.
    pub database_connections: usize,
    /// Admitted number of application replicas sharing that capacity.
    pub replicas: usize,
    /// Connections retained outside ordinary application work.
    pub reserved_connections: usize,
    /// Bounds for every authoritative provider capture.
    pub capture_limits: eventlog_core::CaptureLimits,
}

#[cfg(feature = "eventlog-facade")]
impl EventlogPostgresStore {
    /// Opens an already provisioned PostgreSQL Eventlog authority without DDL.
    ///
    /// # Errors
    ///
    /// Worker/runtime construction, provider admission, or exact binding verification failure.
    pub fn open(
        registry: entity_core::Registry,
        config: eventlog_postgres::PostgresConfig,
        admission: PostgresFacadeAdmission,
        authority: Authority,
        bridge: BridgeConfig,
    ) -> Result<Self, BridgeStartError> {
        let facade = RecordedProviderFacade::start(
            registry,
            EventlogRecordedStoreOwner::Postgres {
                config,
                options: admission.options,
                database_connections: admission.database_connections,
                replicas: admission.replicas,
                reserved_connections: admission.reserved_connections,
                authority,
                limits: admission.capture_limits,
            },
            bridge,
        )?;
        Ok(Self { facade })
    }

    /// Creates projections with one caller authority, then opens and binds with another.
    ///
    /// # Errors
    ///
    /// Migration, application admission, binding conflict/uncertainty, or open failure.
    pub fn provision(
        registry: entity_core::Registry,
        migration: eventlog_postgres::PostgresConfig,
        application: eventlog_postgres::PostgresConfig,
        admission: PostgresFacadeAdmission,
        authority: ProvisionAuthority,
        context: EventlogOperationContext,
        bridge: BridgeConfig,
    ) -> Result<Self, BridgeStartError> {
        let facade = RecordedProviderFacade::provision(
            registry,
            EventlogRecordedStoreProvisioner::Postgres {
                migration,
                application,
                options: Box::new(admission.options),
                database_connections: admission.database_connections,
                replicas: admission.replicas,
                reserved_connections: admission.reserved_connections,
                authority,
                limits: admission.capture_limits,
            },
            context,
            bridge,
        )?;
        Ok(Self { facade })
    }

    /// The complete receipt-preserving facade.
    #[must_use]
    pub const fn recorded(&self) -> &RecordedProviderFacade {
        &self.facade
    }

    /// Closes admission and reports actual pool/driver retirement.
    pub fn shutdown(&mut self, mode: ShutdownMode, wait: CallWait) -> ShutdownOutcome {
        self.facade.shutdown(mode, wait)
    }
}

#[cfg(feature = "eventlog-facade")]
impl StateProvider for EventlogPostgresStore {
    fn load(&self, entity: &str, id: &str) -> Result<Option<EntityInstance>, StoreError> {
        self.facade.load(entity, id)
    }

    fn ids(&self, entity: &str) -> Result<Vec<String>, StoreError> {
        self.facade.ids(entity)
    }
}

#[cfg(feature = "eventlog-facade")]
impl EventProvider for EventlogPostgresStore {
    fn events(&self, entity: &str, id: &str) -> Result<Vec<DomainEvent>, StoreError> {
        self.facade.events(entity, id)
    }
}

#[cfg(feature = "eventlog-facade")]
impl HistoryProvider for EventlogPostgresStore {
    fn records(&self, entity: &str, id: &str) -> Result<Vec<Envelope<DecisionRecord>>, StoreError> {
        self.facade.records(entity, id)
    }

    fn observations(&self, entity: &str, id: &str) -> Result<Vec<RecordedObservation>, StoreError> {
        self.facade.observations(entity, id)
    }
}

#[cfg(feature = "eventlog-facade")]
impl DocumentQueryProvider for EventlogPostgresStore {
    fn query_documents(&self, query: &DocumentQuery) -> Result<DocumentPage, QueryError> {
        self.facade.query_documents(query)
    }
}

impl std::fmt::Debug for PostgresStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresStore").finish_non_exhaustive()
    }
}

/// Turns a driver or serialisation failure into a backend error saying what it was doing.
fn backend(operation: &str, error: &impl std::fmt::Display) -> StoreError {
    StoreError::Backend(format!("{operation}: {error}"))
}

/// Classifies failures after connection establishment. A PostgreSQL error response proves the
/// server answered; a driver/transport error proves no such thing and remains `Unreachable`.
fn database(operation: &str, error: &postgres::Error) -> StoreError {
    if error.as_db_error().is_some() {
        backend(operation, error)
    } else {
        StoreError::Unreachable {
            provider: "postgres".to_owned(),
            detail: format!("{operation}: {error}"),
        }
    }
}

/// A revision as PostgreSQL holds it (`BIGINT`), or the refusal a negative one earns.
fn revision_of(value: i64) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| StoreError::Backend(format!("revision {value} is negative")))
}

impl PostgresStore {
    /// Uses a caller-configured client and prepares the schema.
    ///
    /// TLS policy, certificates, connection parameters and authentication belong to the caller;
    /// this constructor accepts the resulting connection without narrowing those choices.
    ///
    /// # Errors
    ///
    /// [`StoreError::Unreachable`] when the connection is lost, and [`StoreError::Backend`] when
    /// the server refuses the schema.
    pub fn from_client(client: Client) -> Result<Self, StoreError> {
        let mut store = Self {
            client: Mutex::new(client),
        };
        store.migrate()?;
        Ok(store)
    }

    /// Connects to `url` explicitly without TLS and prepares the schema.
    ///
    /// `url` is a libpq-style connection string or URL, such as
    /// `postgres://user:password@host:5432/database`.
    ///
    /// # Errors
    ///
    /// [`StoreError::Unreachable`] when the server does not answer — nothing was learned, and a
    /// caller must not read that as an empty store — and [`StoreError::Backend`] when the schema
    /// cannot be established.
    pub fn connect_no_tls(url: &str) -> Result<Self, StoreError> {
        let client = Client::connect(url, NoTls).map_err(|error| StoreError::Unreachable {
            provider: "postgres".to_owned(),
            detail: error.to_string(),
        })?;
        Self::from_client(client)
    }

    /// Compatibility spelling for [`Self::connect_no_tls`].
    ///
    /// # Errors
    ///
    /// As [`Self::connect_no_tls`].
    #[deprecated(note = "use connect_no_tls to make transport policy explicit")]
    pub fn connect(url: &str) -> Result<Self, StoreError> {
        Self::connect_no_tls(url)
    }

    /// Connects as [`Self::connect_no_tls`] and keeps everything under `schema`, creating it if needed.
    ///
    /// For a test that needs a store of its own on a shared server, and for a deployment that
    /// keeps several plans in one database. The schema name is quoted as an identifier.
    ///
    /// PostgreSQL silently truncates an identifier longer than its `max_identifier_length` (63
    /// bytes by default), so two long names with the same first 63 bytes would name one schema.
    /// A name longer than the server's limit is therefore refused before anything is created, and
    /// the schema the session then resolves must be exactly `schema`; if it is not, the creation
    /// is rolled back and the name refused.
    ///
    /// # Errors
    ///
    /// As [`Self::connect_no_tls`], and [`StoreError::Backend`] when `schema` is longer than the
    /// server's identifier limit or does not resolve to itself.
    pub fn connect_in_schema(url: &str, schema: &str) -> Result<Self, StoreError> {
        let mut client = Client::connect(url, NoTls).map_err(|error| StoreError::Unreachable {
            provider: "postgres".to_owned(),
            detail: error.to_string(),
        })?;
        check_identifier_length(&mut client, schema)?;
        let quoted = quote_identifier(schema);
        let mut selecting = client
            .transaction()
            .map_err(|error| database("beginning schema selection", &error))?;
        selecting
            .batch_execute(&format!(
                "CREATE SCHEMA IF NOT EXISTS {quoted}; SET search_path TO {quoted};"
            ))
            .map_err(|error| database("selecting the schema", &error))?;
        let resolved: Option<String> = selecting
            .query_one("SELECT current_schema()", &[])
            .map_err(|error| database("resolving the schema", &error))?
            .get(0);
        if resolved.as_deref() != Some(schema) {
            // Dropping the transaction rolls back the creation and the `SET` with it.
            return Err(StoreError::Backend(format!(
                "schema {schema:?} resolves to {resolved:?} on this server, not to itself"
            )));
        }
        selecting
            .commit()
            .map_err(|error| database("committing schema selection", &error))?;
        let mut store = Self {
            client: Mutex::new(client),
        };
        store.migrate()?;
        Ok(store)
    }

    /// Creates the two tables if they do not exist. Idempotent, and the only DDL in this crate.
    ///
    /// Called by [`Self::connect`]; public so an operator can run it as a step of its own — schema
    /// creation is a command, not a README instruction.
    ///
    /// Every statement is additive (`IF NOT EXISTS`), so a database created by an older release
    /// gains a later index the next time a store opens it. `instances_id_byte_order` exists because
    /// listings and document pages order and seek by `id COLLATE "C"` (byte order, so every
    /// provider agrees), and the primary key uses the column's default collation, which that
    /// expression cannot use: without it a page is a full scan and a sort. Building it takes a
    /// lock that blocks writes to `instances` for the build; an operator with a large table can run
    /// `CREATE INDEX CONCURRENTLY instances_id_byte_order ON instances (entity, id COLLATE "C")`
    /// first, and this statement then finds it and does nothing.
    ///
    /// One statement is not additive: `DROP INDEX IF EXISTS instances_document_query`. That index
    /// cast `document` to `jsonb`, and `jsonb` refuses the escaped U+0000 that a string field may
    /// hold, so an insert of such a document failed (SQLSTATE 22P05). Its replacement,
    /// `instances_document_index`, indexes `entity_index_document(document)`: the same `jsonb`
    /// with every escaped U+0000 and U+0001 in the text rewritten to a pair starting with U+0001,
    /// a one-to-one mapping of strings, so containment over it matches containment over the
    /// document. Building it blocks writes to `instances` like the index above.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the server refuses the statements.
    pub fn migrate(&mut self) -> Result<(), StoreError> {
        self.client()?
            .batch_execute(
                "CREATE OR REPLACE FUNCTION entity_index_document(document TEXT) RETURNS jsonb
                     LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE AS $f$
                     SELECT CASE WHEN strpos(document, chr(92) || 'u') = 0 THEN document::jsonb
                     ELSE replace(replace(replace(replace(document,
                         chr(92) || chr(92), chr(1)),
                         chr(92) || 'u0001', chr(92) || 'u0001' || chr(92) || 'u0001'),
                         chr(92) || 'u0000', chr(92) || 'u0001' || chr(92) || 'u0002'),
                         chr(1), chr(92) || chr(92))::jsonb END
                 $f$;
                 CREATE TABLE IF NOT EXISTS instances (
                     entity   TEXT   NOT NULL,
                     id       TEXT   NOT NULL,
                     revision BIGINT NOT NULL,
                     document TEXT   NOT NULL,
                     PRIMARY KEY (entity, id)
                 );
                 CREATE TABLE IF NOT EXISTS events (
                     entity   TEXT   NOT NULL,
                     id       TEXT   NOT NULL,
                     revision BIGINT NOT NULL,
                     position BIGINT NOT NULL,
                     document TEXT   NOT NULL,
                     PRIMARY KEY (entity, id, revision, position)
                 );
                 CREATE TABLE IF NOT EXISTS history (
                     entity    TEXT   NOT NULL,
                     id        TEXT   NOT NULL,
                     position  BIGINT NOT NULL,
                     kind      TEXT   NOT NULL CHECK (kind IN ('decision', 'observation')),
                     record_id TEXT   NOT NULL UNIQUE,
                     document  TEXT   NOT NULL,
                     PRIMARY KEY (entity, id, position)
                 );
                 CREATE TABLE IF NOT EXISTS legacy_origins (
                     entity   TEXT   NOT NULL,
                     id       TEXT   NOT NULL,
                     revision BIGINT NOT NULL,
                     PRIMARY KEY (entity, id)
                 );
                 CREATE TABLE IF NOT EXISTS provider_sequences (
                     namespace  TEXT   PRIMARY KEY,
                     next_value BIGINT NOT NULL CHECK (next_value >= 0)
                 );
                 DROP INDEX IF EXISTS instances_document_query;
                 CREATE INDEX IF NOT EXISTS instances_document_index
                     ON instances USING GIN (entity_index_document(document) jsonb_path_ops);
                 CREATE INDEX IF NOT EXISTS instances_id_byte_order
                     ON instances (entity, id COLLATE \"C\");",
            )
            .map_err(|error| database("establishing the schema", &error))
    }

    /// Drops everything under `schema`. For a test cleaning up after itself; a deployment does not
    /// call this.
    ///
    /// A name longer than the server's identifier limit is refused rather than truncated, so it
    /// can never drop the schema that a different, shorter name owns.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the server refuses, or when `schema` is longer than the
    /// server's identifier limit.
    pub fn drop_schema(&mut self, schema: &str) -> Result<(), StoreError> {
        let mut client = self.client()?;
        check_identifier_length(&mut client, schema)?;
        client
            .batch_execute(&format!(
                "DROP SCHEMA IF EXISTS {} CASCADE;",
                quote_identifier(schema)
            ))
            .map_err(|error| database("dropping the schema", &error))
    }
}

impl DocumentQueryProvider for PostgresStore {
    fn query_documents(&self, query: &DocumentQuery) -> Result<DocumentPage, QueryError> {
        query_documents(&mut *self.client().map_err(QueryError::from)?, query)
    }
}

/// One page of matching documents after a cursor, in byte order of `id`; served by the
/// `instances_id_byte_order` index rather than a scan and a sort.
const DOCUMENT_PAGE: &str = "SELECT document FROM instances
     WHERE entity = $1 AND id COLLATE \"C\" > $2
       AND entity_index_document(document) @> entity_index_document($3::text)
     ORDER BY id COLLATE \"C\" LIMIT $4";

fn query_documents(
    client: &mut impl GenericClient,
    query: &DocumentQuery,
) -> Result<DocumentPage, QueryError> {
    let after = query.after_id()?;
    let limit = i64::try_from(query.effective_limit()? + 1)
        .map_err(|_| QueryError::Invalid("query limit does not fit PostgreSQL".to_owned()))?;
    let matching = serde_json::to_string(&serde_json::json!({ "fields": query.matching }))
        .map_err(|error| QueryError::Invalid(error.to_string()))?;
    let rows = client
        .query(DOCUMENT_PAGE, &[&query.entity, &after, &matching, &limit])
        .map_err(|error| QueryError::from(database("querying instances", &error)))?;
    let items = rows
        .into_iter()
        .map(|row| {
            let document: String = row.get(0);
            serde_json::from_str(&document)
                .map_err(|error| QueryError::from(backend("parsing a queried instance", &error)))
        })
        .collect::<Result<Vec<_>, _>>()?;
    DocumentPage::from_matches(query, items)
}

/// One caller-owned PostgreSQL transaction with point locks and uncommitted batches.
pub struct PostgresSession<'a> {
    transaction: postgres::Transaction<'a>,
}

impl PostgresSession<'_> {
    /// Reads and locks one existing instance until the outer session completes.
    pub fn load_for_update(
        &mut self,
        entity: &str,
        id: &str,
    ) -> Result<Option<EntityInstance>, StoreError> {
        let row = self
            .transaction
            .query_opt(
                "SELECT document FROM instances WHERE entity = $1 AND id = $2 FOR UPDATE",
                &[&entity, &id],
            )
            .map_err(|error| database("locking an instance", &error))?;
        row.map(|row| {
            let document: String = row.get(0);
            serde_json::from_str(&document)
                .map_err(|error| backend("parsing a locked instance", &error))
        })
        .transpose()
    }

    /// Reads one instance's events from this transaction's current view, oldest first.
    pub fn events(&mut self, entity: &str, id: &str) -> Result<Vec<DomainEvent>, StoreError> {
        read_events(&mut self.transaction, entity, id)
    }

    /// Atomically reserves `count` monotonically increasing values in `namespace`.
    ///
    /// Returns the value immediately before the reserved range, so callers that increment before
    /// minting receive consecutive values beginning at one for a new namespace.
    pub fn reserve_sequence(&mut self, namespace: &str, count: u64) -> Result<u64, StoreError> {
        if count == 0 {
            return Err(StoreError::Backend(
                "a sequence reservation must contain at least one value".to_owned(),
            ));
        }
        let count = i64::try_from(count)
            .map_err(|_| StoreError::Backend("the sequence reservation is too large".to_owned()))?;
        let row = self
            .transaction
            .query_one(
                "INSERT INTO provider_sequences (namespace, next_value) VALUES ($1, $2)
                 ON CONFLICT (namespace) DO UPDATE
                 SET next_value = provider_sequences.next_value + EXCLUDED.next_value
                 RETURNING next_value - $2",
                &[&namespace, &count],
            )
            .map_err(|error| database("reserving provider sequence values", &error))?;
        revision_of(row.get::<_, i64>(0))
    }

    /// Serializes contenders for an identity that may not have a row yet.
    ///
    /// The lock is a transaction-scoped advisory lock. Advisory locks are database-wide, so its key
    /// is seeded with the identity of the `instances` table this transaction resolves: contenders
    /// in one schema serialize, and contenders in two schemas of one database do not wait on each
    /// other.
    pub fn lock_identity(&mut self, namespace: &str, identity: &str) -> Result<(), StoreError> {
        self.transaction
            .query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended($2, hashtextextended($1, \
                 hashtextextended('instances'::regclass::oid::text, 0))))",
                &[&namespace, &identity],
            )
            .map_err(|error| database("locking an absent identity", &error))?;
        Ok(())
    }

    /// Runs an indexed query against this transaction's own current view.
    pub fn query_documents(&mut self, query: &DocumentQuery) -> Result<DocumentPage, QueryError> {
        query_documents(&mut self.transaction, query)
    }

    /// Applies an ordered atomic batch without committing the outer transaction.
    pub fn commit_batch(&mut self, commits: &[AtomicCommit]) -> Result<(), StoreError> {
        let mut batch = self
            .transaction
            .transaction()
            .map_err(|e| database("starting batch savepoint", &e))?;
        for commit in commits {
            write_decision(&mut batch, &commit.decision, commit.expect, true)?;
        }
        batch
            .commit()
            .map_err(|e| database("releasing batch savepoint", &e))
    }
}

impl PostgresStore {
    /// Executes one caller operation in a fresh transaction and commits only its successful result.
    pub fn with_transaction<T>(
        &mut self,
        operation: impl FnOnce(&mut PostgresSession<'_>) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let mut client = self.client()?;
        let transaction = client
            .transaction()
            .map_err(|error| database("beginning the caller transaction", &error))?;
        let mut session = PostgresSession { transaction };
        let result = operation(&mut session)?;
        session
            .transaction
            .commit()
            .map_err(|error| database("committing the caller transaction", &error))?;
        Ok(result)
    }
}

/// An identifier, double-quoted with embedded quotes doubled — the one escaping SQL identifiers
/// have.
fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Refuses an identifier the server would truncate: PostgreSQL keeps only the first
/// `max_identifier_length` bytes of a name, so a longer one would silently name another object.
fn check_identifier_length(client: &mut Client, name: &str) -> Result<(), StoreError> {
    let limit: String = client
        .query_one("SELECT current_setting('max_identifier_length')", &[])
        .map_err(|error| database("reading the identifier limit", &error))?
        .get(0);
    let limit: usize = limit.parse().map_err(|_| {
        StoreError::Backend(format!(
            "the server reports identifier limit {limit:?}, which is not a byte count"
        ))
    })?;
    if name.len() > limit {
        return Err(StoreError::Backend(format!(
            "schema name {name:?} is {} bytes; this server truncates identifiers to {limit} bytes, \
             so it would name a different schema",
            name.len()
        )));
    }
    Ok(())
}

impl StateProvider for PostgresStore {
    fn load(&self, entity: &str, id: &str) -> Result<Option<EntityInstance>, StoreError> {
        let mut client = self.client()?;
        let row = client
            .query_opt(
                "SELECT document FROM instances WHERE entity = $1 AND id = $2",
                &[&entity, &id],
            )
            .map_err(|error| database("reading an instance", &error))?;
        row.map(|row| {
            let text: String = row.get(0);
            serde_json::from_str(&text).map_err(|error| backend("parsing an instance", &error))
        })
        .transpose()
    }

    fn revision_of(&self, entity: &str, id: &str) -> Result<Option<u64>, StoreError> {
        let mut client = self.client()?;
        client
            .query_opt(
                "SELECT revision FROM instances WHERE entity = $1 AND id = $2",
                &[&entity, &id],
            )
            .map_err(|error| database("reading a revision", &error))?
            .map(|row| revision_of(row.get::<_, i64>(0)))
            .transpose()
    }

    fn ids(&self, entity: &str) -> Result<Vec<String>, StoreError> {
        let mut client = self.client()?;
        // `COLLATE "C"`: byte order, which is what `Vec<String>::sort` produces, so every provider
        // agrees — a database's default collation is locale-dependent and would not.
        let rows = client
            .query(
                "SELECT id FROM instances WHERE entity = $1 ORDER BY id COLLATE \"C\"",
                &[&entity],
            )
            .map_err(|error| database("listing instances", &error))?;
        Ok(rows.iter().map(|row| row.get::<_, String>(0)).collect())
    }
}

impl EventProvider for PostgresStore {
    fn events(&self, entity: &str, id: &str) -> Result<Vec<DomainEvent>, StoreError> {
        read_events(&mut *self.client()?, entity, id)
    }
}

/// One subject's events from both places they are kept: the bare-event table and the events inside
/// recorded decisions, oldest first.
///
/// One statement, so one snapshot. Under `READ COMMITTED` every statement takes a snapshot of its
/// own, so two reads would let a bare revision and then a recorded one commit between them: the
/// second read would show the later recorded event and the first would already have missed the
/// bare one before it — a history with a missing middle that sorting cannot repair. Bare rows come
/// first in revision and position order, then decisions in history order; the stable sort by
/// revision afterwards interleaves them as two separate reads used to.
fn read_events(
    client: &mut impl GenericClient,
    entity: &str,
    id: &str,
) -> Result<Vec<DomainEvent>, StoreError> {
    let rows = client
        .query(
            "SELECT 0 AS source, revision, position, document FROM events \
             WHERE entity = $1 AND id = $2 \
             UNION ALL \
             SELECT 1, 0, position, document FROM history \
             WHERE entity = $1 AND id = $2 AND kind = 'decision' \
             ORDER BY source, revision, position",
            &[&entity, &id],
        )
        .map_err(|error| database("reading events", &error))?;
    let mut events: Vec<DomainEvent> = Vec::with_capacity(rows.len());
    for row in rows {
        let source: i32 = row.get(0);
        let text: String = row.get(3);
        if source == 0 {
            events.push(
                serde_json::from_str(&text).map_err(|error| backend("parsing an event", &error))?,
            );
        } else {
            let envelope: Envelope<DecisionRecord> = serde_json::from_str(&text)
                .map_err(|error| backend("parsing a history record", &error))?;
            events.extend(envelope.record.events);
        }
    }
    events.sort_by_key(|event| event.revision);
    Ok(events)
}

impl HistoryProvider for PostgresStore {
    fn records(&self, entity: &str, id: &str) -> Result<Vec<Envelope<DecisionRecord>>, StoreError> {
        self.read_history(entity, id, "decision")
    }

    fn observations(&self, entity: &str, id: &str) -> Result<Vec<RecordedObservation>, StoreError> {
        self.read_history(entity, id, "observation")
    }
}

fn write_decision(
    transaction: &mut postgres::Transaction<'_>,
    decision: &Decision,
    expect: Expect,
    include_legacy_events: bool,
) -> Result<(), StoreError> {
    let instance = &decision.instance;
    let (entity, id) = (instance.entity.as_str(), instance.id.as_str());

    // Locked, so another writer waits and then sees the revision that landed. A later entry in
    // this batch sees the revision an earlier entry wrote in the same transaction.
    let found = transaction
        .query_opt(
            "SELECT revision FROM instances WHERE entity = $1 AND id = $2 FOR UPDATE",
            &[&entity, &id],
        )
        .map_err(|error| database("reading the current revision", &error))?
        .map(|row| revision_of(row.get::<_, i64>(0)))
        .transpose()?;
    check(entity, id, expect, found)?;

    let document = serde_json::to_string(instance)
        .map_err(|error| backend("serialising the instance", &error))?;
    let revision = i64::try_from(instance.revision)
        .map_err(|_| StoreError::Backend("the revision does not fit a BIGINT".to_owned()))?;
    let written = match found {
        Some(_) => transaction.execute(
            "UPDATE instances SET revision = $3, document = $4 WHERE entity = $1 AND id = $2",
            &[&entity, &id, &revision, &document],
        ),
        None => transaction.execute(
            "INSERT INTO instances (entity, id, revision, document) VALUES ($1, $2, $3, $4) \
             ON CONFLICT (entity, id) DO NOTHING",
            &[&entity, &id, &revision, &document],
        ),
    }
    .map_err(|error| database("writing the instance", &error))?;

    // Two concurrent creations have no row to lock. `ON CONFLICT DO NOTHING` waits for the other
    // transaction without poisoning this one, so the whole batch can still roll back and report
    // the ordinary optimistic conflict with the revision that won.
    if found.is_none() && written == 0 {
        let landed = transaction
            .query_opt(
                "SELECT revision FROM instances WHERE entity = $1 AND id = $2",
                &[&entity, &id],
            )
            .map_err(|error| database("reading the revision that landed", &error))?
            .map(|row| revision_of(row.get::<_, i64>(0)))
            .transpose()?;
        return Err(StoreError::RevisionConflict {
            entity: entity.to_owned(),
            id: id.to_owned(),
            expected: expect,
            found: landed,
        });
    }

    if !include_legacy_events {
        return Ok(());
    }
    transaction
        .execute(
            "INSERT INTO legacy_origins (entity, id, revision) VALUES ($1, $2, $3) \
             ON CONFLICT (entity, id) DO NOTHING",
            &[&entity, &id, &revision],
        )
        .map_err(|error| database("marking the legacy snapshot boundary", &error))?;
    // One position read per revision under the instance lock taken above, then one insert for
    // every event: positions continue from what each revision already holds, consecutively and
    // in the order the decision emitted them.
    let events = &decision.record.events;
    let mut next_positions: BTreeMap<i64, i64> = BTreeMap::new();
    let (mut revisions, mut positions, mut documents) = (
        Vec::with_capacity(events.len()),
        Vec::with_capacity(events.len()),
        Vec::with_capacity(events.len()),
    );
    for event in events {
        let document = serde_json::to_string(event)
            .map_err(|error| backend("serialising an event", &error))?;
        let at_revision = i64::try_from(event.revision)
            .map_err(|_| StoreError::Backend("the revision does not fit a BIGINT".to_owned()))?;
        let next = match next_positions.entry(at_revision) {
            Entry::Occupied(next) => next.into_mut(),
            Entry::Vacant(vacant) => vacant.insert(
                transaction
                    .query_one(
                        "SELECT COALESCE(MAX(position) + 1, 0) FROM events \
                         WHERE entity = $1 AND id = $2 AND revision = $3",
                        &[&entity, &id, &at_revision],
                    )
                    .map_err(|error| database("reading the event position", &error))?
                    .get(0),
            ),
        };
        positions.push(*next);
        *next = next.checked_add(1).ok_or_else(|| {
            StoreError::Backend("the event position does not fit a BIGINT".to_owned())
        })?;
        revisions.push(at_revision);
        documents.push(document);
    }
    if documents.is_empty() {
        return Ok(());
    }
    transaction
        .execute(
            "INSERT INTO events (entity, id, revision, position, document) \
             SELECT $1, $2, appended.revision, appended.position, appended.document \
             FROM UNNEST($3::BIGINT[], $4::BIGINT[], $5::TEXT[]) \
                 AS appended (revision, position, document)",
            &[&entity, &id, &revisions, &positions, &documents],
        )
        .map_err(|error| database("appending the events", &error))?;
    Ok(())
}

fn lock_record(
    transaction: &mut postgres::Transaction<'_>,
    record_id: &str,
) -> Result<(), StoreError> {
    // Advisory locks are database-wide, and record ids are unique per `history` table. The key
    // is seeded with the identity of the `history` table this transaction resolves, so writers of
    // one record id in different schemas do not wait on each other, and writers in one schema
    // still do.
    transaction
        .query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1, \
             hashtextextended('history'::regclass::oid::text, 412731)))",
            &[&record_id],
        )
        .map_err(|e| database("locking record identity", &e))?;
    Ok(())
}

impl Store for PostgresStore {
    fn history(&self) -> Option<&dyn HistoryProvider> {
        Some(self)
    }
    fn commit(&mut self, decision: &Decision, expect: Expect) -> Result<(), StoreError> {
        self.commit_batch(&[AtomicCommit::new(decision.clone(), expect)])
    }

    fn commit_recorded(
        &mut self,
        commit: &RecordedCommit,
        expect: Expect,
    ) -> Result<(), StoreError> {
        commit.validate()?;
        let mut client = self.client()?;
        let mut transaction = client
            .transaction()
            .map_err(|error| database("beginning the recorded transaction", &error))?;
        let document = serde_json::to_string(&commit.envelope)
            .map_err(|error| backend("serialising the decision envelope", &error))?;
        lock_record(&mut transaction, &commit.envelope.record_id)?;
        if let Some(row) = transaction
            .query_opt(
                "SELECT document FROM history WHERE record_id = $1",
                &[&commit.envelope.record_id],
            )
            .map_err(|error| database("checking the record id", &error))?
        {
            let existing: String = row.get(0);
            if existing == document {
                transaction
                    .commit()
                    .map_err(|error| database("committing the retry", &error))?;
                return Ok(());
            }
            return Err(StoreError::RecordConflict {
                record_id: commit.envelope.record_id.clone(),
            });
        }
        write_decision(&mut transaction, &commit.decision(), expect, false)?;
        let position: i64 = transaction
            .query_one(
                "SELECT COALESCE(MAX(position) + 1, 0) FROM history WHERE entity = $1 AND id = $2",
                &[&commit.instance.entity, &commit.instance.id],
            )
            .map_err(|error| database("reading the history position", &error))?
            .get(0);
        transaction
            .execute(
                "INSERT INTO history (entity, id, position, kind, record_id, document) \
                 VALUES ($1, $2, $3, 'decision', $4, $5)",
                &[
                    &commit.instance.entity,
                    &commit.instance.id,
                    &position,
                    &commit.envelope.record_id,
                    &document,
                ],
            )
            .map_err(|error| database("appending the decision record", &error))?;
        transaction
            .commit()
            .map_err(|error| database("committing the recorded decision", &error))
    }

    fn observe(&mut self, observation: &RecordedObservation) -> Result<(), StoreError> {
        observation.validate()?;
        let mut client = self.client()?;
        let mut transaction = client
            .transaction()
            .map_err(|error| database("beginning the observation transaction", &error))?;
        let document = serde_json::to_string(observation)
            .map_err(|error| backend("serialising the observation", &error))?;
        lock_record(&mut transaction, &observation.envelope.record_id)?;
        if let Some(row) = transaction
            .query_opt(
                "SELECT document FROM history WHERE record_id = $1",
                &[&observation.envelope.record_id],
            )
            .map_err(|error| database("checking the observation id", &error))?
        {
            let existing: String = row.get(0);
            if existing == document {
                transaction
                    .commit()
                    .map_err(|error| database("committing the observation retry", &error))?;
                return Ok(());
            }
            return Err(StoreError::RecordConflict {
                record_id: observation.envelope.record_id.clone(),
            });
        }
        let found = transaction
            .query_opt(
                "SELECT revision FROM instances WHERE entity = $1 AND id = $2 FOR UPDATE",
                &[&observation.entity, &observation.id],
            )
            .map_err(|error| database("reading the observed revision", &error))?
            .map(|row| revision_of(row.get::<_, i64>(0)))
            .transpose()?;
        check(
            &observation.entity,
            &observation.id,
            Expect::Revision(observation.revision),
            found,
        )?;
        let position: i64 = transaction
            .query_one(
                "SELECT COALESCE(MAX(position) + 1, 0) FROM history WHERE entity = $1 AND id = $2",
                &[&observation.entity, &observation.id],
            )
            .map_err(|error| database("reading the history position", &error))?
            .get(0);
        transaction
            .execute(
                "INSERT INTO history (entity, id, position, kind, record_id, document) \
                 VALUES ($1, $2, $3, 'observation', $4, $5)",
                &[
                    &observation.entity,
                    &observation.id,
                    &position,
                    &observation.envelope.record_id,
                    &document,
                ],
            )
            .map_err(|error| database("appending the observation", &error))?;
        transaction
            .commit()
            .map_err(|error| database("committing the observation", &error))
    }
}

impl AtomicBatchStore for PostgresStore {
    fn commit_batch(&mut self, commits: &[AtomicCommit]) -> Result<(), StoreError> {
        let mut client = self.client()?;
        let mut transaction = client
            .transaction()
            .map_err(|error| database("beginning the transaction", &error))?;
        for commit in commits {
            write_decision(&mut transaction, &commit.decision, commit.expect, true)?;
        }

        // Every instance and event lands here or none does.
        transaction
            .commit()
            .map_err(|error| database("committing", &error))
    }
}

impl LegacyStoreSource for PostgresStore {
    fn acquire_legacy(&mut self, source_id: &str) -> Result<LegacyStoreSnapshot, StoreError> {
        let mut client = self.client()?;
        let mut transaction = client
            .build_transaction()
            .isolation_level(IsolationLevel::RepeatableRead)
            .read_only(true)
            .start()
            .map_err(|error| database("beginning legacy acquisition", &error))?;
        let coordinates = transaction
            .query(
                "SELECT coordinates.entity, coordinates.id, instances.revision, instances.document
                 FROM (
                     SELECT entity, id FROM instances
                     UNION SELECT entity, id FROM events
                     UNION SELECT entity, id FROM history
                     UNION SELECT entity, id FROM legacy_origins
                 ) AS coordinates
                 LEFT JOIN instances USING (entity, id)
                 ORDER BY coordinates.entity COLLATE \"C\", coordinates.id COLLATE \"C\"",
                &[],
            )
            .map_err(|error| database("acquiring legacy instances", &error))?;
        // Every subject's origin, bare events and history, read as three ordered statements in this
        // snapshot rather than three per subject: acquisition costs four statements whatever the
        // store holds. Every grouped row belongs to a coordinate above, because the coordinates are
        // the union of these tables.
        let mut origins = grouped(
            &mut transaction,
            "SELECT entity, id, revision FROM legacy_origins",
            "acquiring legacy origins",
        )?;
        let mut events = grouped(
            &mut transaction,
            "SELECT entity, id, document FROM events ORDER BY entity, id, revision, position",
            "acquiring legacy events",
        )?;
        let mut histories_by_subject = grouped(
            &mut transaction,
            "SELECT entity, id, position, kind, record_id, document FROM history \
             ORDER BY entity, id, position",
            "acquiring legacy history",
        )?;
        let mut histories = Vec::with_capacity(coordinates.len());
        for row in coordinates {
            let entity: String = row.get(0);
            let id: String = row.get(1);
            let stored_revision: Option<i64> = row.get(2);
            let document: Option<String> = row.get(3);
            let (Some(stored_revision), Some(document)) = (stored_revision, document) else {
                return Err(StoreError::Backend(format!(
                    "legacy PostgreSQL evidence at {entity:?}/{id:?} has no terminal instance"
                )));
            };
            let instance: EntityInstance = serde_json::from_str(&document)
                .map_err(|error| backend("parsing a legacy instance", &error))?;
            if instance.entity != entity
                || instance.id != id
                || revision_of(stored_revision)? != instance.revision
            {
                return Err(StoreError::Backend(format!(
                    "legacy PostgreSQL instance row {entity:?}/{id:?} substitutes its identity or revision"
                )));
            }
            let subject = (entity, id);
            let origin_rows = origins.remove(&subject).unwrap_or_default();
            let event_rows = events.remove(&subject).unwrap_or_default();
            let history_rows = histories_by_subject.remove(&subject).unwrap_or_default();
            let (entity, id) = subject;
            let legacy_origin = origin_rows
                .first()
                .map(|row| revision_of(row.get::<_, i64>(2)))
                .transpose()?;
            if legacy_origin.is_some_and(|revision| revision > instance.revision) {
                return Err(StoreError::Backend(format!(
                    "legacy PostgreSQL origin for {entity:?}/{id:?} is later than terminal revision {}",
                    instance.revision
                )));
            }
            let has_bare_events = !event_rows.is_empty();
            let mut evidence = Vec::new();
            let mut decision_position = 0_u64;
            let mut observation_position = 0_u64;
            for row in history_rows {
                let position = revision_of(row.get::<_, i64>(2))?;
                let kind: String = row.get(3);
                let record_id: String = row.get(4);
                let document: String = row.get(5);
                let (entry, per_kind_position) = match kind.as_str() {
                    "decision" => {
                        let envelope: Envelope<DecisionRecord> = serde_json::from_str(&document)
                            .map_err(|error| backend("parsing a legacy decision", &error))?;
                        if envelope.record_id != record_id {
                            return Err(StoreError::Backend(format!(
                                "legacy PostgreSQL decision at {entity:?}/{id:?}/{position} substitutes record identity"
                            )));
                        }
                        let position = decision_position;
                        decision_position = decision_position.checked_add(1).ok_or_else(|| {
                            StoreError::Backend(
                                "legacy PostgreSQL decision position exhausted".to_owned(),
                            )
                        })?;
                        (
                            RecordedEntry::Decision(RecordedCommit {
                                instance: envelope.record.result.clone(),
                                envelope,
                            }),
                            position,
                        )
                    }
                    "observation" => {
                        let observation: RecordedObservation = serde_json::from_str(&document)
                            .map_err(|error| backend("parsing a legacy observation", &error))?;
                        if observation.envelope.record_id != record_id {
                            return Err(StoreError::Backend(format!(
                                "legacy PostgreSQL observation at {entity:?}/{id:?}/{position} substitutes record identity"
                            )));
                        }
                        let position = observation_position;
                        observation_position =
                            observation_position.checked_add(1).ok_or_else(|| {
                                StoreError::Backend(
                                    "legacy PostgreSQL observation position exhausted".to_owned(),
                                )
                            })?;
                        (RecordedEntry::Observation(observation), position)
                    }
                    other => {
                        return Err(StoreError::Backend(format!(
                            "legacy PostgreSQL history has unknown kind {other:?}"
                        )));
                    }
                };
                evidence.push(LegacyEvidence::Envelope(
                    ImportedRecordEvidence::new(
                        entry,
                        source_id,
                        format!("history/{entity}/{id}/{position}"),
                        if has_bare_events {
                            KnownLegacyOrder::PerKind(per_kind_position)
                        } else {
                            KnownLegacyOrder::Subject(position)
                        },
                    )
                    .map_err(|error| StoreError::Backend(error.to_string()))?,
                ));
            }
            for row in event_rows {
                let document: String = row.get(2);
                evidence.push(LegacyEvidence::Event(
                    serde_json::from_str(&document)
                        .map_err(|error| backend("parsing a legacy event", &error))?,
                ));
            }
            histories.push(SubjectHistory {
                subject: Subject::new(entity, id)
                    .map_err(|error| StoreError::Backend(error.to_string()))?,
                origin: HistoryOrigin::Imported(LegacyAnchor {
                    instance,
                    completeness: LegacyCompleteness::CompleteSubject,
                    order: if has_bare_events {
                        LegacyOrderDeclaration::PerKindOnly
                    } else {
                        LegacyOrderDeclaration::Subject
                    },
                    evidence,
                }),
                records: Vec::new(),
            });
        }
        transaction
            .commit()
            .map_err(|error| database("ending legacy acquisition", &error))?;
        drop(client);
        LegacyStoreSnapshot::new(source_id, histories)
    }
}

/// The rows of one statement whose first two columns are `entity` and `id`, grouped by subject in
/// the order the statement returned them.
fn grouped(
    transaction: &mut postgres::Transaction<'_>,
    sql: &str,
    operation: &str,
) -> Result<BTreeMap<(String, String), Vec<postgres::Row>>, StoreError> {
    let mut grouped: BTreeMap<(String, String), Vec<postgres::Row>> = BTreeMap::new();
    for row in transaction
        .query(sql, &[])
        .map_err(|error| database(operation, &error))?
    {
        grouped
            .entry((row.get(0), row.get(1)))
            .or_default()
            .push(row);
    }
    Ok(grouped)
}

impl PostgresStore {
    fn read_history<T: serde::de::DeserializeOwned>(
        &self,
        entity: &str,
        id: &str,
        kind: &str,
    ) -> Result<Vec<T>, StoreError> {
        let mut client = self.client()?;
        let rows = client
            .query(
                "SELECT document FROM history WHERE entity = $1 AND id = $2 AND kind = $3 \
                 ORDER BY position",
                &[&entity, &id, &kind],
            )
            .map_err(|error| database("reading history", &error))?;
        rows.into_iter()
            .map(|row| {
                let text: String = row.get(0);
                serde_json::from_str(&text)
                    .map_err(|error| backend("parsing a history record", &error))
            })
            .collect()
    }

    /// The connection, for one statement or one transaction.
    fn client(&self) -> Result<std::sync::MutexGuard<'_, Client>, StoreError> {
        self.client.lock().map_err(|_| {
            StoreError::Backend(
                "the connection is poisoned: a previous call panicked while holding it".to_owned(),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{PostgresStore, DOCUMENT_PAGE};

    /// Every `Node Type` in an `EXPLAIN (FORMAT JSON)` plan, outermost first.
    fn node_types(plan: &serde_json::Value, into: &mut Vec<String>) {
        if let Some(kind) = plan.get("Node Type").and_then(serde_json::Value::as_str) {
            let index = plan
                .get("Index Name")
                .and_then(serde_json::Value::as_str)
                .map_or(String::new(), |name| format!(" using {name}"));
            into.push(format!("{kind}{index}"));
        }
        for child in plan
            .get("Plans")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            node_types(child, into);
        }
    }

    #[test]
    fn a_document_page_after_a_cursor_reads_an_index_in_byte_order_without_sorting() {
        let Some(url) = std::env::var("ENTITY_POSTGRES_URL")
            .ok()
            .filter(|url| !url.trim().is_empty())
        else {
            eprintln!("skipped: ENTITY_POSTGRES_URL unset, so no server to hold this provider to");
            return;
        };
        let schema = format!("entity_test_{}_document_page_plan", std::process::id());
        let mut store = PostgresStore::connect_in_schema(&url, &schema).unwrap_or_else(|error| {
            panic!("ENTITY_POSTGRES_URL is set and the server refused: {error}")
        });
        let plan: String = {
            let mut client = store.client().expect("connection");
            // 16,384 subjects of one entity, as the review measured; a page of two after the
            // midpoint. With the planner's statistics current, only an index whose order is the
            // query's byte order avoids reading half the table and sorting it.
            client
                .batch_execute(
                    "INSERT INTO instances (entity, id, revision, document)
                     SELECT 'ticket', lpad(n::text, 6, '0'), 1,
                            json_build_object('fields', json_build_object('title', 'T'))::text
                     FROM generate_series(1, 16384) AS n;
                     ANALYZE instances;",
                )
                .expect("seeded");
            // The exact shipped statement, prepared and explained over the simple protocol, which
            // returns the JSON plan as text.
            client
                .batch_execute(&format!(
                    "PREPARE document_page (text, text, text, bigint) AS {DOCUMENT_PAGE}"
                ))
                .expect("prepared");
            client
                .simple_query(
                    "EXPLAIN (FORMAT JSON) \
                     EXECUTE document_page ('ticket', '008192', '{\"fields\":{}}', 3)",
                )
                .expect("explains")
                .into_iter()
                .find_map(|message| match message {
                    postgres::SimpleQueryMessage::Row(row) => row.get(0).map(str::to_owned),
                    _ => None,
                })
                .expect("a plan row")
        };
        let plan: serde_json::Value = serde_json::from_str(&plan).expect("JSON plan");
        let mut nodes = Vec::new();
        node_types(&plan[0]["Plan"], &mut nodes);
        assert!(
            nodes
                .iter()
                .any(|node| node == "Index Scan using instances_id_byte_order"),
            "the page must be read from the byte-order index: {nodes:?}"
        );
        assert!(
            !nodes.iter().any(|node| node == "Sort"),
            "the page must come in index order, not from a sort: {nodes:?}"
        );
        store.drop_schema(&schema).expect("dropped");
    }

    /// Review E23: a string field holding U+0000 (serialized as the escape `\u0000`) is admitted
    /// by the kernel, and the old `document::jsonb` index refused to insert it (SQLSTATE 22P05).
    #[test]
    fn a_document_holding_u0000_saves_and_a_page_finds_exactly_the_documents_it_contains() {
        let Some(url) = std::env::var("ENTITY_POSTGRES_URL")
            .ok()
            .filter(|url| !url.trim().is_empty())
        else {
            eprintln!("skipped: ENTITY_POSTGRES_URL unset, so no server to hold this provider to");
            return;
        };
        let schema = format!("entity_test_{}_document_u0000", std::process::id());
        let mut store = PostgresStore::connect_in_schema(&url, &schema).unwrap_or_else(|error| {
            panic!("ENTITY_POSTGRES_URL is set and the server refused: {error}")
        });
        {
            // A database from an older release still has the old index; migrate drops it.
            let mut client = store.client().expect("connection");
            client
                .batch_execute(
                    "CREATE INDEX instances_document_query
                         ON instances USING GIN ((document::jsonb) jsonb_path_ops);",
                )
                .expect("old index");
        }
        store.migrate().expect("migrated");
        let mut client = store.client().expect("connection");
        let indexes: Vec<String> = client
            .query(
                "SELECT indexname::text FROM pg_indexes
                 WHERE schemaname = current_schema() AND tablename = 'instances'",
                &[],
            )
            .expect("indexes")
            .iter()
            .map(|row| row.get(0))
            .collect();
        assert!(
            !indexes
                .iter()
                .any(|name| name == "instances_document_query"),
            "{indexes:?}"
        );
        assert!(
            indexes
                .iter()
                .any(|name| name == "instances_document_index"),
            "{indexes:?}"
        );
        // Labels whose JSON text differs only around the escapes the index rewrites.
        let labels = [
            ("nul", r"a\u0000b"),
            ("one-two", r"a\u0001\u0002b"),
            ("one", r"a\u0001b"),
            ("escaped-backslash", r"a\\u0000b"),
            ("backslash-nul", r"a\\\u0000b"),
            ("plain", "ab"),
        ];
        for (id, label) in labels {
            let document = format!(r#"{{"fields":{{"label":"{label}"}}}}"#);
            client
                .execute(
                    "INSERT INTO instances (entity, id, revision, document) VALUES ('c', $1, 1, $2)",
                    &[&id, &document],
                )
                .unwrap_or_else(|error| panic!("{id} is saved: {error}"));
        }
        let page = client.prepare(DOCUMENT_PAGE).expect("prepared");
        for (id, label) in labels {
            let filter = format!(r#"{{"fields":{{"label":"{label}"}}}}"#);
            let matched: Vec<String> = client
                .query(&page, &[&"c", &"", &filter, &100_i64])
                .expect("page")
                .iter()
                .map(|row| row.get::<_, String>(0))
                .collect();
            assert_eq!(matched.len(), 1, "{id}: {matched:?}");
            assert!(matched[0].contains(label), "{id}: {matched:?}");
        }
        let everything = client
            .query(&page, &[&"c", &"", &r#"{"fields":{}}"#, &100_i64])
            .expect("page");
        assert_eq!(everything.len(), labels.len());
        drop(client);
        store.drop_schema(&schema).expect("dropped");
    }
}
