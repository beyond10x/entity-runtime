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

use entity_core::{Decision, DecisionRecord, DomainEvent, EntityInstance};
#[cfg(feature = "eventlog-facade")]
use entity_eventlog::{
    sync::{
        BridgeConfig, BridgeStartError, CallWait, EventlogRecordedStoreOwner,
        EventlogRecordedStoreProvisioner, ProvisionAuthority, ShutdownMode, ShutdownOutcome,
    },
    Authority, EventlogOperationContext, RecordedProviderFacade,
};
#[cfg(feature = "eventlog-facade")]
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
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

/// A [`Store`] over one SQLite database.
pub struct SqliteStore {
    connection: Connection,
}

/// Eventlog-backed SQLite facade with complete recorded receipts and atomic groups.
#[cfg(feature = "eventlog-facade")]
#[derive(Debug)]
pub struct EventlogSqliteStore {
    facade: RecordedProviderFacade,
}

#[cfg(feature = "eventlog-facade")]
impl EventlogSqliteStore {
    /// Opens an already provisioned SQLite Eventlog authority.
    ///
    /// # Errors
    ///
    /// Worker/runtime construction or exact authority verification failure.
    pub fn open(
        path: impl Into<String>,
        prefix: impl Into<String>,
        registry: entity_core::Registry,
        authority: Authority,
        limits: eventlog_core::CaptureLimits,
        config: BridgeConfig,
    ) -> Result<Self, BridgeStartError> {
        let facade = RecordedProviderFacade::start(
            registry,
            EventlogRecordedStoreOwner::Sqlite {
                path: path.into(),
                prefix: prefix.into(),
                authority,
                limits,
            },
            config,
        )?;
        Ok(Self { facade })
    }

    /// Explicitly creates projections and binds a SQLite Eventlog authority before opening it.
    ///
    /// # Errors
    ///
    /// Native preparation, binding conflict/uncertainty, or open verification failure.
    pub fn provision(
        path: impl Into<String>,
        prefix: impl Into<String>,
        registry: entity_core::Registry,
        authority: ProvisionAuthority,
        context: EventlogOperationContext,
        limits: eventlog_core::CaptureLimits,
        config: BridgeConfig,
    ) -> Result<Self, BridgeStartError> {
        let facade = RecordedProviderFacade::provision(
            registry,
            EventlogRecordedStoreProvisioner::Sqlite {
                path: path.into(),
                prefix: prefix.into(),
                authority,
                limits,
            },
            context,
            config,
        )?;
        Ok(Self { facade })
    }

    /// Explicitly provisions an in-memory SQLite Eventlog authority.
    ///
    /// # Errors
    ///
    /// Native preparation, binding conflict/uncertainty, or open verification failure.
    pub fn provision_memory(
        prefix: impl Into<String>,
        registry: entity_core::Registry,
        authority: ProvisionAuthority,
        context: EventlogOperationContext,
        limits: eventlog_core::CaptureLimits,
        config: BridgeConfig,
    ) -> Result<Self, BridgeStartError> {
        let facade = RecordedProviderFacade::provision(
            registry,
            EventlogRecordedStoreProvisioner::SqliteMemory {
                prefix: prefix.into(),
                authority,
                limits,
            },
            context,
            config,
        )?;
        Ok(Self { facade })
    }

    /// The complete receipt-preserving facade.
    #[must_use]
    pub const fn recorded(&self) -> &RecordedProviderFacade {
        &self.facade
    }

    /// Closes admission and reports actual native retirement.
    pub fn shutdown(&mut self, mode: ShutdownMode, wait: CallWait) -> ShutdownOutcome {
        self.facade.shutdown(mode, wait)
    }
}

#[cfg(feature = "eventlog-facade")]
impl StateProvider for EventlogSqliteStore {
    fn load(&self, entity: &str, id: &str) -> Result<Option<EntityInstance>, StoreError> {
        self.facade.load(entity, id)
    }

    fn ids(&self, entity: &str) -> Result<Vec<String>, StoreError> {
        self.facade.ids(entity)
    }
}

#[cfg(feature = "eventlog-facade")]
impl EventProvider for EventlogSqliteStore {
    fn events(&self, entity: &str, id: &str) -> Result<Vec<DomainEvent>, StoreError> {
        self.facade.events(entity, id)
    }
}

#[cfg(feature = "eventlog-facade")]
impl HistoryProvider for EventlogSqliteStore {
    fn records(&self, entity: &str, id: &str) -> Result<Vec<Envelope<DecisionRecord>>, StoreError> {
        self.facade.records(entity, id)
    }

    fn observations(&self, entity: &str, id: &str) -> Result<Vec<RecordedObservation>, StoreError> {
        self.facade.observations(entity, id)
    }
}

#[cfg(feature = "eventlog-facade")]
impl DocumentQueryProvider for EventlogSqliteStore {
    fn query_documents(&self, query: &DocumentQuery) -> Result<DocumentPage, QueryError> {
        self.facade.query_documents(query)
    }
}

/// Turns a driver or serialisation failure into a backend error saying what it was doing.
fn backend(operation: &str, error: &impl std::fmt::Display) -> StoreError {
    StoreError::Backend(format!("{operation}: {error}"))
}

fn revision_from_sql(revision: i64) -> Result<u64, StoreError> {
    u64::try_from(revision).map_err(|_| {
        StoreError::Backend(format!(
            "the database contains negative revision {revision}, which no entity can have"
        ))
    })
}

fn revision_to_sql(revision: u64) -> Result<i64, StoreError> {
    i64::try_from(revision).map_err(|_| {
        StoreError::Backend(format!(
            "revision {revision} exceeds the largest revision SQLite can store"
        ))
    })
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
                 );
                 CREATE TABLE IF NOT EXISTS history (
                     entity    TEXT NOT NULL,
                     id        TEXT NOT NULL,
                     position  INTEGER NOT NULL,
                     kind      TEXT NOT NULL CHECK (kind IN ('decision', 'observation')),
                     record_id TEXT NOT NULL UNIQUE,
                     document  TEXT NOT NULL,
                     PRIMARY KEY (entity, id, position)
                 );
                 CREATE TABLE IF NOT EXISTS legacy_origins (
                     entity TEXT NOT NULL,
                     id     TEXT NOT NULL,
                     revision INTEGER NOT NULL,
                     PRIMARY KEY (entity, id)
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
            .and_then(|found| found.map(revision_from_sql).transpose())
    }

    fn ids(&self, entity: &str) -> Result<Vec<String>, StoreError> {
        // `ORDER BY id` is the sort the trait promises; SQLite's default text collation is byte
        // order, which is what `Vec<String>::sort` produces, so every provider agrees.
        let mut statement = self
            .connection
            .prepare("SELECT id FROM instances WHERE entity = ?1 ORDER BY id")
            .map_err(|error| backend("preparing the listing", &error))?;
        let rows = statement
            .query_map(params![entity], |row| row.get::<_, String>(0))
            .map_err(|error| backend("listing instances", &error))?;
        rows.map(|row| row.map_err(|error| backend("reading an id", &error)))
            .collect()
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

        let mut events: Vec<DomainEvent> = rows
            .map(|row| {
                let text = row.map_err(|error| backend("reading an event", &error))?;
                serde_json::from_str(&text).map_err(|error| backend("parsing an event", &error))
            })
            .collect::<Result<_, _>>()?;
        for record in self.records(entity, id)? {
            events.extend(record.record.events);
        }
        events.sort_by_key(|event| event.revision);
        Ok(events)
    }
}

impl HistoryProvider for SqliteStore {
    fn records(&self, entity: &str, id: &str) -> Result<Vec<Envelope<DecisionRecord>>, StoreError> {
        read_history(&self.connection, entity, id, "decision")
    }

    fn observations(&self, entity: &str, id: &str) -> Result<Vec<RecordedObservation>, StoreError> {
        read_history(&self.connection, entity, id, "observation")
    }
}

fn read_history<T: serde::de::DeserializeOwned>(
    connection: &Connection,
    entity: &str,
    id: &str,
    kind: &str,
) -> Result<Vec<T>, StoreError> {
    let mut statement = connection
        .prepare(
            "SELECT document FROM history WHERE entity = ?1 AND id = ?2 AND kind = ?3 \
             ORDER BY position",
        )
        .map_err(|error| backend("preparing the history read", &error))?;
    let rows = statement
        .query_map(params![entity, id, kind], |row| row.get::<_, String>(0))
        .map_err(|error| backend("reading history", &error))?;
    rows.map(|row| {
        let text = row.map_err(|error| backend("reading a history record", &error))?;
        serde_json::from_str(&text).map_err(|error| backend("parsing a history record", &error))
    })
    .collect()
}

fn next_history_position(
    transaction: &Transaction<'_>,
    entity: &str,
    id: &str,
) -> Result<i64, StoreError> {
    transaction
        .query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM history WHERE entity = ?1 AND id = ?2",
            params![entity, id],
            |row| row.get(0),
        )
        .map_err(|error| backend("reading the history position", &error))
}

fn existing_record(
    transaction: &Transaction<'_>,
    record_id: &str,
) -> Result<Option<String>, StoreError> {
    transaction
        .query_row(
            "SELECT document FROM history WHERE record_id = ?1",
            params![record_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| backend("checking the record id", &error))
}

fn write_decision(
    transaction: &Transaction<'_>,
    decision: &Decision,
    expect: Expect,
    include_legacy_events: bool,
) -> Result<(), StoreError> {
    let instance = &decision.instance;
    let (entity, id) = (instance.entity.as_str(), instance.id.as_str());

    // Read inside the transaction, so later batch entries see earlier ones while another
    // connection sees neither until commit.
    let found: Option<i64> = transaction
        .query_row(
            "SELECT revision FROM instances WHERE entity = ?1 AND id = ?2",
            params![entity, id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| backend("reading the current revision", &error))?;
    check(
        entity,
        id,
        expect,
        found.map(revision_from_sql).transpose()?,
    )?;

    let revision = revision_to_sql(instance.revision)?;

    let document = serde_json::to_string(instance)
        .map_err(|error| backend("serialising the instance", &error))?;
    transaction
        .execute(
            "INSERT INTO instances (entity, id, revision, document) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (entity, id) DO UPDATE SET revision = ?3, document = ?4",
            params![entity, id, revision, document],
        )
        .map_err(|error| backend("writing the instance", &error))?;

    if !include_legacy_events {
        return Ok(());
    }
    transaction
        .execute(
            "INSERT OR IGNORE INTO legacy_origins (entity, id, revision) VALUES (?1, ?2, ?3)",
            params![entity, id, revision],
        )
        .map_err(|error| backend("marking the legacy snapshot boundary", &error))?;
    for event in &decision.record.events {
        let event_revision = revision_to_sql(event.revision)?;
        let document = serde_json::to_string(event)
            .map_err(|error| backend("serialising an event", &error))?;
        let position: i64 = transaction
            .query_row(
                "SELECT COALESCE(MAX(position) + 1, 0) FROM events \
                 WHERE entity = ?1 AND id = ?2 AND revision = ?3",
                params![entity, id, event_revision],
                |row| row.get(0),
            )
            .map_err(|error| backend("reading the event position", &error))?;
        transaction
            .execute(
                "INSERT INTO events (entity, id, revision, position, document) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![entity, id, event_revision, position, document],
            )
            .map_err(|error| backend("appending an event", &error))?;
    }
    Ok(())
}

impl Store for SqliteStore {
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
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| backend("beginning the recorded transaction", &error))?;
        let document = serde_json::to_string(&commit.envelope)
            .map_err(|error| backend("serialising the decision envelope", &error))?;
        if let Some(existing) = existing_record(&transaction, &commit.envelope.record_id)? {
            if existing == document {
                transaction
                    .commit()
                    .map_err(|error| backend("committing the retry", &error))?;
                return Ok(());
            }
            return Err(StoreError::RecordConflict {
                record_id: commit.envelope.record_id.clone(),
            });
        }
        write_decision(&transaction, &commit.decision(), expect, false)?;
        let position =
            next_history_position(&transaction, &commit.instance.entity, &commit.instance.id)?;
        transaction
            .execute(
                "INSERT INTO history (entity, id, position, kind, record_id, document) \
                 VALUES (?1, ?2, ?3, 'decision', ?4, ?5)",
                params![
                    commit.instance.entity,
                    commit.instance.id,
                    position,
                    commit.envelope.record_id,
                    document
                ],
            )
            .map_err(|error| backend("appending the decision record", &error))?;
        transaction
            .commit()
            .map_err(|error| backend("committing the recorded decision", &error))
    }

    fn observe(&mut self, observation: &RecordedObservation) -> Result<(), StoreError> {
        observation.validate()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| backend("beginning the observation transaction", &error))?;
        let document = serde_json::to_string(observation)
            .map_err(|error| backend("serialising the observation", &error))?;
        if let Some(existing) = existing_record(&transaction, &observation.envelope.record_id)? {
            if existing == document {
                transaction
                    .commit()
                    .map_err(|error| backend("committing the observation retry", &error))?;
                return Ok(());
            }
            return Err(StoreError::RecordConflict {
                record_id: observation.envelope.record_id.clone(),
            });
        }
        let found: Option<i64> = transaction
            .query_row(
                "SELECT revision FROM instances WHERE entity = ?1 AND id = ?2",
                params![observation.entity, observation.id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| backend("reading the observed revision", &error))?;
        check(
            &observation.entity,
            &observation.id,
            Expect::Revision(observation.revision),
            found.map(revision_from_sql).transpose()?,
        )?;
        let position = next_history_position(&transaction, &observation.entity, &observation.id)?;
        transaction
            .execute(
                "INSERT INTO history (entity, id, position, kind, record_id, document) \
                 VALUES (?1, ?2, ?3, 'observation', ?4, ?5)",
                params![
                    observation.entity,
                    observation.id,
                    position,
                    observation.envelope.record_id,
                    document
                ],
            )
            .map_err(|error| backend("appending the observation", &error))?;
        transaction
            .commit()
            .map_err(|error| backend("committing the observation", &error))
    }
}

impl AtomicBatchStore for SqliteStore {
    fn commit_batch(&mut self, commits: &[AtomicCommit]) -> Result<(), StoreError> {
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
        for commit in commits {
            write_decision(&transaction, &commit.decision, commit.expect, true)?;
        }

        // Every instance and event in the slice lands here or none does. This is the promise
        // `FileStore` cannot make.
        transaction
            .commit()
            .map_err(|error| backend("committing", &error))
    }
}

impl LegacyStoreSource for SqliteStore {
    fn acquire_legacy(&mut self, source_id: &str) -> Result<LegacyStoreSnapshot, StoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| backend("beginning legacy acquisition", &error))?;
        let mut instances = transaction
            .prepare(
                "SELECT coordinates.entity, coordinates.id, instances.revision, instances.document
                 FROM (
                     SELECT entity, id FROM instances
                     UNION SELECT entity, id FROM events
                     UNION SELECT entity, id FROM history
                     UNION SELECT entity, id FROM legacy_origins
                 ) AS coordinates
                 LEFT JOIN instances USING (entity, id)
                 ORDER BY coordinates.entity, coordinates.id",
            )
            .map_err(|error| backend("preparing legacy instance acquisition", &error))?;
        let rows = instances
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(|error| backend("acquiring legacy instances", &error))?;
        let coordinates = rows
            .map(|row| row.map_err(|error| backend("reading a legacy instance", &error)))
            .collect::<Result<Vec<_>, _>>()?;
        drop(instances);

        let mut histories = Vec::with_capacity(coordinates.len());
        for (entity, id, stored_revision, document) in coordinates {
            let (Some(stored_revision), Some(document)) = (stored_revision, document) else {
                return Err(StoreError::Backend(format!(
                    "legacy SQLite evidence at {entity:?}/{id:?} has no terminal instance"
                )));
            };
            let instance: EntityInstance = serde_json::from_str(&document)
                .map_err(|error| backend("parsing a legacy instance", &error))?;
            if instance.entity != entity
                || instance.id != id
                || revision_from_sql(stored_revision)? != instance.revision
            {
                return Err(StoreError::Backend(format!(
                    "legacy SQLite instance row {entity:?}/{id:?} substitutes its identity or revision"
                )));
            }
            let legacy_origin = transaction
                .query_row(
                    "SELECT revision FROM legacy_origins WHERE entity = ?1 AND id = ?2",
                    params![entity, id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .map_err(|error| backend("acquiring a legacy origin", &error))?
                .map(revision_from_sql)
                .transpose()?;
            if legacy_origin.is_some_and(|revision| revision > instance.revision) {
                return Err(StoreError::Backend(format!(
                    "legacy SQLite origin for {entity:?}/{id:?} is later than terminal revision {}",
                    instance.revision
                )));
            }
            let has_bare_events = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM events WHERE entity = ?1 AND id = ?2)",
                    params![entity, id],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(|error| backend("checking legacy event evidence", &error))?;
            let mut history_statement = transaction
                .prepare(
                    "SELECT position, kind, record_id, document FROM history \
                     WHERE entity = ?1 AND id = ?2 ORDER BY position",
                )
                .map_err(|error| backend("preparing legacy history acquisition", &error))?;
            let history_rows = history_statement
                .query_map(params![entity, id], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })
                .map_err(|error| backend("acquiring legacy history", &error))?;
            let history_rows = history_rows
                .map(|row| row.map_err(|error| backend("reading legacy history", &error)))
                .collect::<Result<Vec<_>, _>>()?;
            drop(history_statement);
            let mut evidence = Vec::new();
            let mut decision_position = 0_u64;
            let mut observation_position = 0_u64;
            for (position, kind, record_id, document) in history_rows {
                let position = u64::try_from(position).map_err(|_| {
                    StoreError::Backend("legacy SQLite history has a negative position".to_owned())
                })?;
                let (entry, per_kind_position) = match kind.as_str() {
                    "decision" => {
                        let envelope: Envelope<DecisionRecord> = serde_json::from_str(&document)
                            .map_err(|error| backend("parsing a legacy decision", &error))?;
                        if envelope.record_id != record_id {
                            return Err(StoreError::Backend(format!(
                                "legacy SQLite decision at {entity:?}/{id:?}/{position} substitutes record identity"
                            )));
                        }
                        let position = decision_position;
                        decision_position = decision_position.checked_add(1).ok_or_else(|| {
                            StoreError::Backend(
                                "legacy SQLite decision position exhausted".to_owned(),
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
                                "legacy SQLite observation at {entity:?}/{id:?}/{position} substitutes record identity"
                            )));
                        }
                        let position = observation_position;
                        observation_position =
                            observation_position.checked_add(1).ok_or_else(|| {
                                StoreError::Backend(
                                    "legacy SQLite observation position exhausted".to_owned(),
                                )
                            })?;
                        (RecordedEntry::Observation(observation), position)
                    }
                    other => {
                        return Err(StoreError::Backend(format!(
                            "legacy SQLite history has unknown kind {other:?}"
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
            let mut event_statement = transaction
                .prepare(
                    "SELECT document FROM events WHERE entity = ?1 AND id = ?2 \
                     ORDER BY revision, position",
                )
                .map_err(|error| backend("preparing legacy event acquisition", &error))?;
            let event_rows = event_statement
                .query_map(params![entity, id], |row| row.get::<_, String>(0))
                .map_err(|error| backend("acquiring legacy events", &error))?;
            for row in event_rows {
                let document = row.map_err(|error| backend("reading a legacy event", &error))?;
                evidence.push(LegacyEvidence::Event(
                    serde_json::from_str(&document)
                        .map_err(|error| backend("parsing a legacy event", &error))?,
                ));
            }
            drop(event_statement);
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
            .map_err(|error| backend("ending legacy acquisition", &error))?;
        LegacyStoreSnapshot::new(source_id, histories)
    }
}
