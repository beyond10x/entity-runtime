//! A store that writes state and events in one transaction.
//!
//! `FileStore` is readable and diffable and has a crash window it states in its own first screen:
//! it appends events, then writes state, and a crash in between leaves a recorded fact whose state
//! did not land. That is recoverable but it is a window, and R-83 asks for the two to arrive
//! *together*.
//!
//! This is the provider that can actually give that. One `BEGIN`, both writes, one `COMMIT`: there
//! is no moment at which a reader or a crash can see one without the other.
//!
//! # Why the third provider exists at all
//!
//! Not to be a second implementation for its own sake — `MemoryStore` and `FileStore` were already
//! two, and `tests/both_providers.rs` in `entity-store` already held them to one answer. This one
//! exists because it can keep a promise neither of the others can, and because a trait with one
//! *transactional* implementor is a trait nobody has tested the transactional case against.
//!
//! # Schema
//!
//! ```sql
//! CREATE TABLE instances (entity, id, revision, document, PRIMARY KEY (entity, id));
//! CREATE TABLE events (entity, id, revision, position, document, PRIMARY KEY (entity, id, revision, position));
//! ```
//!
//! The instance is one JSON document rather than a column per field, because the fields are the
//! definition's and change when it does — a column per field would make every definition edit a
//! migration. `revision` is lifted out as its own column because it is the one thing a write
//! compares before deciding, and comparing it should not mean parsing a document.
//!
//! # SQLite is bundled
//!
//! Compiled from vendored C rather than linked against whatever the machine has. A store whose
//! behaviour depends on the host's library version is a store two machines can disagree about,
//! which is the one thing a provider must not be.

use std::path::Path;

use entity_core::{Decision, DomainEvent, EntityInstance};
use entity_store::{check, EventProvider, Expect, StateProvider, Store, StoreError};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

/// A [`Store`] over one SQLite database.
pub struct SqliteStore {
    connection: Connection,
}

/// Turns a driver or serialisation failure into a backend error saying what it was doing.
fn backend(operation: &str, error: &impl std::fmt::Display) -> StoreError {
    StoreError::Backend(format!("{operation}: {error}"))
}

impl SqliteStore {
    /// Opens (and creates, if needed) the database at `path`.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the database cannot be opened or its schema not established.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let connection =
            Connection::open(path).map_err(|error| backend("opening the database", &error))?;
        Self::prepare(connection)
    }

    /// Opens a database that exists only for as long as this value does.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the database cannot be opened.
    pub fn in_memory() -> Result<Self, StoreError> {
        let connection =
            Connection::open_in_memory().map_err(|error| backend("opening in memory", &error))?;
        Self::prepare(connection)
    }

    fn prepare(connection: Connection) -> Result<Self, StoreError> {
        // A second writer waits its turn rather than failing. Without this the default is zero:
        // the loser of a race gets `SQLITE_BUSY` immediately, which arrives at the caller as
        // `Backend` — "the system is broken, stop retrying" — for what is only contention. Five
        // seconds is long enough that a writer holding the lock for one small transaction is never
        // the cause, and short enough that a genuinely stuck writer still surfaces.
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|error| backend("setting the busy timeout", &error))?;

        // Foreign keys off, journal default: nothing here needs either, and a pragma nobody needs
        // is a difference between two deployments waiting to be discovered.
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS instances (
                     entity   TEXT NOT NULL,
                     id       TEXT NOT NULL,
                     revision INTEGER NOT NULL,
                     document TEXT NOT NULL,
                     PRIMARY KEY (entity, id)
                 );
                 CREATE TABLE IF NOT EXISTS events (
                     entity   TEXT NOT NULL,
                     id       TEXT NOT NULL,
                     revision INTEGER NOT NULL,
                     position INTEGER NOT NULL,
                     document TEXT NOT NULL,
                     PRIMARY KEY (entity, id, revision, position)
                 );",
            )
            .map_err(|error| backend("establishing the schema", &error))?;
        Ok(Self { connection })
    }
}

impl StateProvider for SqliteStore {
    fn load(&self, entity: &str, id: &str) -> Result<Option<EntityInstance>, StoreError> {
        let document: Option<String> = self
            .connection
            .query_row(
                "SELECT document FROM instances WHERE entity = ?1 AND id = ?2",
                params![entity, id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| backend("reading an instance", &error))?;

        document
            .map(|text| {
                serde_json::from_str(&text).map_err(|error| backend("parsing an instance", &error))
            })
            .transpose()
    }

    fn revision_of(&self, entity: &str, id: &str) -> Result<Option<u64>, StoreError> {
        // The one thing a write compares, answered without parsing a document.
        self.connection
            .query_row(
                "SELECT revision FROM instances WHERE entity = ?1 AND id = ?2",
                params![entity, id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| backend("reading a revision", &error))
            .map(|found| found.map(|revision| revision as u64))
    }
}

impl EventProvider for SqliteStore {
    fn events(&self, entity: &str, id: &str) -> Result<Vec<DomainEvent>, StoreError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT document FROM events WHERE entity = ?1 AND id = ?2 \
                 ORDER BY revision, position",
            )
            .map_err(|error| backend("preparing the event read", &error))?;

        let rows = statement
            .query_map(params![entity, id], |row| row.get::<_, String>(0))
            .map_err(|error| backend("reading events", &error))?;

        rows.map(|row| {
            let text = row.map_err(|error| backend("reading an event", &error))?;
            serde_json::from_str(&text).map_err(|error| backend("parsing an event", &error))
        })
        .collect()
    }
}

impl Store for SqliteStore {
    fn commit(&mut self, decision: &Decision, expect: Expect) -> Result<(), StoreError> {
        let instance = &decision.instance;
        let (entity, id) = (instance.entity.as_str(), instance.id.as_str());

        // `Immediate`, not the default `Deferred`. A deferred transaction takes a *shared* lock on
        // the first read and tries to upgrade it at the first write, and two writers that both got
        // that far cannot both upgrade — SQLite refuses one with `SQLITE_BUSY` and no amount of
        // waiting helps, because neither will let go. It refuses writers to *unrelated* rows the
        // same way, since the lock is over the database. Taking the write lock up front makes the
        // second writer wait (see `busy_timeout` in `prepare`) rather than fail, and a genuine
        // clash then arrives as `RevisionConflict` — something the caller is told to retry —
        // instead of `Backend`, which this crate documents as "stop retrying".
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| backend("beginning the transaction", &error))?;

        // Read inside the transaction, so what is checked is what is written against — a check
        // outside it would be a check against a state another writer could have replaced.
        let found: Option<i64> = transaction
            .query_row(
                "SELECT revision FROM instances WHERE entity = ?1 AND id = ?2",
                params![entity, id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| backend("reading the current revision", &error))?;
        check(entity, id, expect, found.map(|revision| revision as u64))?;

        let document = serde_json::to_string(instance)
            .map_err(|error| backend("serialising the instance", &error))?;
        transaction
            .execute(
                "INSERT INTO instances (entity, id, revision, document) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (entity, id) DO UPDATE SET revision = ?3, document = ?4",
                params![entity, id, instance.revision as i64, document],
            )
            .map_err(|error| backend("writing the instance", &error))?;

        for (position, event) in decision.events.iter().enumerate() {
            let document = serde_json::to_string(event)
                .map_err(|error| backend("serialising an event", &error))?;
            transaction
                .execute(
                    "INSERT INTO events (entity, id, revision, position, document) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![entity, id, event.revision as i64, position as i64, document],
                )
                .map_err(|error| backend("appending an event", &error))?;
        }

        // Both writes land here or neither does. This is the promise `FileStore` cannot make.
        transaction
            .commit()
            .map_err(|error| backend("committing", &error))
    }
}
