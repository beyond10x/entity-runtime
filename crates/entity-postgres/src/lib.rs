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

use std::sync::Mutex;

use entity_core::{Decision, DecisionRecord, DomainEvent, EntityInstance};
use entity_query::{DocumentPage, DocumentQuery, DocumentQueryProvider, QueryError};
use entity_store::{
    check, AtomicBatchStore, AtomicCommit, Envelope, EventProvider, Expect, HistoryProvider,
    RecordedCommit, RecordedObservation, StateProvider, Store, StoreError,
};
use postgres::{Client, GenericClient, NoTls};

/// A [`Store`] over one PostgreSQL connection.
///
/// The synchronous client needs `&mut` for every query and the read half of the SPI is `&self`, so
/// the client sits behind a mutex: one connection, one statement at a time, which is what a
/// connection is anyway. A caller that wants parallel readers opens more stores.
pub struct PostgresStore {
    client: Mutex<Client>,
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
    /// # Errors
    ///
    /// As [`Self::connect_no_tls`].
    pub fn connect_in_schema(url: &str, schema: &str) -> Result<Self, StoreError> {
        let mut client = Client::connect(url, NoTls).map_err(|error| StoreError::Unreachable {
            provider: "postgres".to_owned(),
            detail: error.to_string(),
        })?;
        let quoted = quote_identifier(schema);
        client
            .batch_execute(&format!(
                "CREATE SCHEMA IF NOT EXISTS {quoted}; SET search_path TO {quoted};"
            ))
            .map_err(|error| database("selecting the schema", &error))?;
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
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the server refuses the statements.
    pub fn migrate(&mut self) -> Result<(), StoreError> {
        self.client()?
            .batch_execute(
                "CREATE TABLE IF NOT EXISTS instances (
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
                 CREATE INDEX IF NOT EXISTS instances_document_query
                     ON instances USING GIN ((document::jsonb) jsonb_path_ops);",
            )
            .map_err(|error| database("establishing the schema", &error))
    }

    /// Drops everything under `schema`. For a test cleaning up after itself; a deployment does not
    /// call this.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the server refuses.
    pub fn drop_schema(&mut self, schema: &str) -> Result<(), StoreError> {
        self.client()?
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
        .query(
            "SELECT document FROM instances
             WHERE entity = $1 AND id COLLATE \"C\" > $2
               AND document::jsonb @> $3::jsonb
             ORDER BY id COLLATE \"C\" LIMIT $4",
            &[&query.entity, &after, &matching, &limit],
        )
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

    /// Serializes contenders for an identity that may not have a row yet.
    pub fn lock_identity(&mut self, namespace: &str, identity: &str) -> Result<(), StoreError> {
        self.transaction
            .query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
                &[&format!("{namespace}\0{identity}")],
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
        for commit in commits {
            write_decision(&mut self.transaction, &commit.decision, commit.expect, true)?;
        }
        Ok(())
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
        let mut client = self.client()?;
        let rows = client
            .query(
                "SELECT document FROM events WHERE entity = $1 AND id = $2 \
                 ORDER BY revision, position",
                &[&entity, &id],
            )
            .map_err(|error| database("reading events", &error))?;
        let mut events: Vec<DomainEvent> = rows
            .iter()
            .map(|row| {
                let text: String = row.get(0);
                serde_json::from_str(&text).map_err(|error| backend("parsing an event", &error))
            })
            .collect::<Result<_, _>>()?;
        drop(client);
        for record in self.records(entity, id)? {
            events.extend(record.record.events);
        }
        Ok(events)
    }
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
    for event in &decision.record.events {
        let document = serde_json::to_string(event)
            .map_err(|error| backend("serialising an event", &error))?;
        let at_revision = i64::try_from(event.revision)
            .map_err(|_| StoreError::Backend("the revision does not fit a BIGINT".to_owned()))?;
        let position: i64 = transaction
            .query_one(
                "SELECT COALESCE(MAX(position) + 1, 0) FROM events \
                 WHERE entity = $1 AND id = $2 AND revision = $3",
                &[&entity, &id, &at_revision],
            )
            .map_err(|error| database("reading the event position", &error))?
            .get(0);
        transaction
            .execute(
                "INSERT INTO events (entity, id, revision, position, document) \
                 VALUES ($1, $2, $3, $4, $5)",
                &[&entity, &id, &at_revision, &position, &document],
            )
            .map_err(|error| database("appending an event", &error))?;
    }
    Ok(())
}

impl Store for PostgresStore {
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
