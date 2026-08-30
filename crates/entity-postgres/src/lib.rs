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

use entity_core::{Decision, DomainEvent, EntityInstance};
use entity_store::{
    check, AtomicBatchStore, AtomicCommit, EventProvider, Expect, StateProvider, Store, StoreError,
};
use postgres::{Client, NoTls};

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

/// A revision as PostgreSQL holds it (`BIGINT`), or the refusal a negative one earns.
fn revision_of(value: i64) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| StoreError::Backend(format!("revision {value} is negative")))
}

impl PostgresStore {
    /// Connects to `url` without TLS and prepares the schema.
    ///
    /// `url` is a libpq-style connection string or URL, such as
    /// `postgres://user:password@host:5432/database`.
    ///
    /// # Errors
    ///
    /// [`StoreError::Unreachable`] when the server does not answer — nothing was learned, and a
    /// caller must not read that as an empty store — and [`StoreError::Backend`] when the schema
    /// cannot be established.
    pub fn connect(url: &str) -> Result<Self, StoreError> {
        let client = Client::connect(url, NoTls).map_err(|error| StoreError::Unreachable {
            provider: "postgres".to_owned(),
            detail: error.to_string(),
        })?;
        let mut store = Self {
            client: Mutex::new(client),
        };
        store.migrate()?;
        Ok(store)
    }

    /// Connects as [`Self::connect`] and keeps everything under `schema`, creating it if needed.
    ///
    /// For a test that needs a store of its own on a shared server, and for a deployment that
    /// keeps several plans in one database. The schema name is quoted as an identifier.
    ///
    /// # Errors
    ///
    /// As [`Self::connect`].
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
            .map_err(|error| backend("selecting the schema", &error))?;
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
                 );",
            )
            .map_err(|error| backend("establishing the schema", &error))
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
            .map_err(|error| backend("dropping the schema", &error))
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
            .map_err(|error| backend("reading an instance", &error))?;
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
            .map_err(|error| backend("reading a revision", &error))?
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
            .map_err(|error| backend("listing instances", &error))?;
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
            .map_err(|error| backend("reading events", &error))?;
        rows.iter()
            .map(|row| {
                let text: String = row.get(0);
                serde_json::from_str(&text).map_err(|error| backend("parsing an event", &error))
            })
            .collect()
    }
}

fn write_decision(
    transaction: &mut postgres::Transaction<'_>,
    decision: &Decision,
    expect: Expect,
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
        .map_err(|error| backend("reading the current revision", &error))?
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
    .map_err(|error| backend("writing the instance", &error))?;

    // Two concurrent creations have no row to lock. `ON CONFLICT DO NOTHING` waits for the other
    // transaction without poisoning this one, so the whole batch can still roll back and report
    // the ordinary optimistic conflict with the revision that won.
    if found.is_none() && written == 0 {
        let landed = transaction
            .query_opt(
                "SELECT revision FROM instances WHERE entity = $1 AND id = $2",
                &[&entity, &id],
            )
            .map_err(|error| backend("reading the revision that landed", &error))?
            .map(|row| revision_of(row.get::<_, i64>(0)))
            .transpose()?;
        return Err(StoreError::RevisionConflict {
            entity: entity.to_owned(),
            id: id.to_owned(),
            expected: expect,
            found: landed,
        });
    }

    for event in &decision.events {
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
            .map_err(|error| backend("reading the event position", &error))?
            .get(0);
        transaction
            .execute(
                "INSERT INTO events (entity, id, revision, position, document) \
                 VALUES ($1, $2, $3, $4, $5)",
                &[&entity, &id, &at_revision, &position, &document],
            )
            .map_err(|error| backend("appending an event", &error))?;
    }
    Ok(())
}

impl Store for PostgresStore {
    fn commit(&mut self, decision: &Decision, expect: Expect) -> Result<(), StoreError> {
        self.commit_batch(&[AtomicCommit::new(decision.clone(), expect)])
    }
}

impl AtomicBatchStore for PostgresStore {
    fn commit_batch(&mut self, commits: &[AtomicCommit]) -> Result<(), StoreError> {
        let mut client = self.client()?;
        let mut transaction = client
            .transaction()
            .map_err(|error| backend("beginning the transaction", &error))?;
        for commit in commits {
            write_decision(&mut transaction, &commit.decision, commit.expect)?;
        }

        // Every instance and event lands here or none does.
        transaction
            .commit()
            .map_err(|error| backend("committing", &error))
    }
}

impl PostgresStore {
    /// The connection, for one statement or one transaction.
    fn client(&self) -> Result<std::sync::MutexGuard<'_, Client>, StoreError> {
        self.client.lock().map_err(|_| {
            StoreError::Backend(
                "the connection is poisoned: a previous call panicked while holding it".to_owned(),
            )
        })
    }
}
