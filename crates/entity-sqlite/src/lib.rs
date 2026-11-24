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

use std::{
    collections::{btree_map::Entry, BTreeMap},
    path::Path,
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
#[cfg(feature = "eventlog-facade")]
use entity_query::{DocumentPage, DocumentQuery, DocumentQueryProvider, QueryError};
use entity_store::{
    asynchronous::{
        HistoryOrigin, ImportedRecordEvidence, KnownLegacyOrder, LegacyAnchor, LegacyCompleteness,
        LegacyEvidence, LegacyOrderDeclaration, RecordedEntry, Subject, SubjectHistory,
    },
    check,
    stored_record::{read_decision, same_record, store_decision, DefinitionCache},
    AtomicBatchStore, AtomicCommit, Envelope, EventProvider, Expect, HistoryProvider,
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
        connection
            .execute_batch(DEFINITIONS_BY_DIGEST)
            .map_err(|error| backend("establishing the definition table", &error))?;
        Ok(Self { connection })
    }
}

/// Since 0.34.0 a decision record names its definition by digest and the definition is kept once
/// in `definitions` ([`entity_store::stored_record`]). Both statements are additive and run on
/// every open, so a database written by an older release gains them the first time this release
/// opens it, and its records keep their bytes.
///
/// The trigger is the fence. An older release reads a by-digest record as a parse error rather
/// than misreading it, but it would still write decision records that embed their definition, and
/// a retry of one of its records would compare against the other form. Once this release has
/// opened a database, a decision record in the embedded form is refused at insert, so an older
/// writer fails with this message instead of mixing forms.
const DEFINITIONS_BY_DIGEST: &str = "CREATE TABLE IF NOT EXISTS definitions (
         digest   TEXT NOT NULL PRIMARY KEY,
         document TEXT NOT NULL
     );
     CREATE TRIGGER IF NOT EXISTS history_names_definitions_by_digest
     BEFORE INSERT ON history
     WHEN NEW.kind = 'decision'
          AND substr(NEW.document, 1, 22) <> '[\"er.stored-record/1\",'
     BEGIN
         SELECT RAISE(ABORT, 'this store names definitions by digest (entity-sqlite 0.34.0 or \
             later); a decision record that embeds its definition comes from an older writer: \
             upgrade every writer of this store');
     END;";

/// The definition document stored under `digest`, read in `connection`'s current view.
fn fetch_definition(connection: &Connection, digest: &str) -> Result<Option<String>, StoreError> {
    connection
        .query_row(
            "SELECT document FROM definitions WHERE digest = ?1",
            params![digest],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| backend("reading a definition", &error))
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
        // One read transaction, so the definitions are read in the same view as the records that
        // name them.
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|error| backend("beginning the history read", &error))?;
        let mut definitions = DefinitionCache::new();
        let records = history_documents(&transaction, entity, id, "decision")?
            .iter()
            .map(|document| {
                read_decision(document, &mut definitions, |digest| {
                    fetch_definition(&transaction, digest)
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        transaction
            .commit()
            .map_err(|error| backend("ending the history read", &error))?;
        Ok(records)
    }

    fn observations(&self, entity: &str, id: &str) -> Result<Vec<RecordedObservation>, StoreError> {
        history_documents(&self.connection, entity, id, "observation")?
            .iter()
            .map(|text| {
                serde_json::from_str(text)
                    .map_err(|error| backend("parsing a history record", &error))
            })
            .collect()
    }
}

fn history_documents(
    connection: &Connection,
    entity: &str,
    id: &str,
    kind: &str,
) -> Result<Vec<String>, StoreError> {
    let mut statement = connection
        .prepare(
            "SELECT document FROM history WHERE entity = ?1 AND id = ?2 AND kind = ?3 \
             ORDER BY position",
        )
        .map_err(|error| backend("preparing the history read", &error))?;
    let rows = statement
        .query_map(params![entity, id, kind], |row| row.get::<_, String>(0))
        .map_err(|error| backend("reading history", &error))?;
    rows.map(|row| row.map_err(|error| backend("reading a history record", &error)))
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
    // One position read per revision inside this transaction, then one prepared insert reused for
    // every event: positions continue from what each revision already holds, consecutively and in
    // the order the decision emitted them.
    let mut next_positions: BTreeMap<i64, i64> = BTreeMap::new();
    let mut append = transaction
        .prepare_cached(
            "INSERT INTO events (entity, id, revision, position, document) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .map_err(|error| backend("preparing the event append", &error))?;
    for event in &decision.record.events {
        let event_revision = revision_to_sql(event.revision)?;
        let document = serde_json::to_string(event)
            .map_err(|error| backend("serialising an event", &error))?;
        let next = match next_positions.entry(event_revision) {
            Entry::Occupied(next) => next.into_mut(),
            Entry::Vacant(vacant) => vacant.insert(
                transaction
                    .query_row(
                        "SELECT COALESCE(MAX(position) + 1, 0) FROM events \
                         WHERE entity = ?1 AND id = ?2 AND revision = ?3",
                        params![entity, id, event_revision],
                        |row| row.get(0),
                    )
                    .map_err(|error| backend("reading the event position", &error))?,
            ),
        };
        let position = *next;
        *next = position.checked_add(1).ok_or_else(|| {
            StoreError::Backend("the event position exceeds what SQLite can store".to_owned())
        })?;
        append
            .execute(params![entity, id, event_revision, position, document])
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
        let stored = store_decision(&commit.envelope)?;
        if let Some(existing) = existing_record(&transaction, &commit.envelope.record_id)? {
            if same_record(&existing, &commit.envelope, &stored)? {
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
        if let Some(definition) = &stored.definition {
            transaction
                .execute(
                    "INSERT INTO definitions (digest, document) VALUES (?1, ?2) \
                     ON CONFLICT (digest) DO NOTHING",
                    params![definition.digest, definition.document],
                )
                .map_err(|error| backend("keeping the definition", &error))?;
        }
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
                    stored.document
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

        // Every subject's origin, bare events and history, read as three ordered statements in this
        // transaction rather than three per subject: acquisition costs four statements whatever the
        // store holds. Every grouped row belongs to a coordinate above, because the coordinates are
        // the union of these tables.
        let mut origins = grouped(
            &transaction,
            "SELECT entity, id, revision FROM legacy_origins",
            "acquiring legacy origins",
            |row| row.get::<_, i64>(2),
        )?;
        let mut events = grouped(
            &transaction,
            "SELECT entity, id, document FROM events ORDER BY entity, id, revision, position",
            "acquiring legacy events",
            |row| row.get::<_, String>(2),
        )?;
        let mut histories_by_subject = grouped(
            &transaction,
            "SELECT entity, id, position, kind, record_id, document FROM history \
             ORDER BY entity, id, position",
            "acquiring legacy history",
            |row| {
                Ok((
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )?;

        let mut definitions = DefinitionCache::new();
        let mut histories = Vec::with_capacity(coordinates.len());
        for (entity, id, stored_revision, document) in coordinates {
            let (Some(stored_revision), Some(document)) = (stored_revision, document) else {
                return Err(StoreError::Backend(format!(
                    "legacy SQLite evidence at {entity:?}/{id:?} has no terminal instance"
                )));
            };
            let subject = (entity, id);
            let legacy_origin = origins.remove(&subject).unwrap_or_default();
            let event_documents = events.remove(&subject).unwrap_or_default();
            let history_rows = histories_by_subject.remove(&subject).unwrap_or_default();
            let (entity, id) = subject;
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
            let legacy_origin = legacy_origin
                .into_iter()
                .next()
                .map(revision_from_sql)
                .transpose()?;
            if legacy_origin.is_some_and(|revision| revision > instance.revision) {
                return Err(StoreError::Backend(format!(
                    "legacy SQLite origin for {entity:?}/{id:?} is later than terminal revision {}",
                    instance.revision
                )));
            }
            let has_bare_events = !event_documents.is_empty();
            let mut evidence = Vec::new();
            let mut decision_position = 0_u64;
            let mut observation_position = 0_u64;
            for (position, kind, record_id, document) in history_rows {
                let position = u64::try_from(position).map_err(|_| {
                    StoreError::Backend("legacy SQLite history has a negative position".to_owned())
                })?;
                let (entry, per_kind_position) = match kind.as_str() {
                    "decision" => {
                        let envelope = read_decision(&document, &mut definitions, |digest| {
                            fetch_definition(&transaction, digest)
                        })?;
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
            for document in event_documents {
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
            .map_err(|error| backend("ending legacy acquisition", &error))?;
        LegacyStoreSnapshot::new(source_id, histories)
    }
}

/// The rows of one statement whose first two columns are `entity` and `id`, grouped by subject in
/// the order the statement returned them; `value` reads the remaining columns of a row.
fn grouped<T>(
    transaction: &Transaction<'_>,
    sql: &str,
    operation: &str,
    value: impl Fn(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
) -> Result<BTreeMap<(String, String), Vec<T>>, StoreError> {
    let mut statement = transaction
        .prepare(sql)
        .map_err(|error| backend(operation, &error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                (row.get::<_, String>(0)?, row.get::<_, String>(1)?),
                value(row)?,
            ))
        })
        .map_err(|error| backend(operation, &error))?;
    let mut grouped: BTreeMap<(String, String), Vec<T>> = BTreeMap::new();
    for row in rows {
        let (subject, value) = row.map_err(|error| backend(operation, &error))?;
        grouped.entry(subject).or_default().push(value);
    }
    Ok(grouped)
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use entity_core::{Registry, Runtime};
    use entity_store::{Expect, LegacyStoreSource, Store};
    use rusqlite::trace::{TraceEvent, TraceEventCodes};

    use super::SqliteStore;

    #[test]
    fn legacy_acquisition_reads_a_fixed_number_of_statements_whatever_the_subject_count() {
        let mut registry = Registry::new();
        registry
            .register(
                serde_json::from_value(serde_json::json!({
                    "entity": "ticket",
                    "version": 1,
                    "schema": { "fields": { "title": { "type": "string", "required": true } } },
                    "lifecycle": { "initial": "open", "states": ["open"] }
                }))
                .expect("definition parses"),
            )
            .expect("definition validates");
        let mut store = SqliteStore::in_memory().expect("store");
        for n in 0..64 {
            let created = Runtime::new(&registry)
                .create(
                    "ticket",
                    1,
                    format!("t{n:03}"),
                    serde_json::json!({"title": "T"}),
                )
                .expect("creation");
            store.commit(&created, Expect::Absent).expect("commit");
        }

        let selects = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&selects);
        store
            .connection
            .trace_v2(
                TraceEventCodes::SQLITE_TRACE_STMT,
                Some(move |event: TraceEvent<'_>| {
                    if let TraceEvent::Stmt(_, sql) = event {
                        if sql.trim_start().starts_with("SELECT") {
                            counter.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }),
            )
            .expect("trace installed");
        let snapshot = store.acquire_legacy("sqlite/count").expect("acquired");

        assert_eq!(snapshot.histories.len(), 64, "every subject acquired");
        assert_eq!(
            selects.load(Ordering::Relaxed),
            4,
            "coordinates, origins, events and history: one statement each, not one per subject"
        );
    }

    #[test]
    fn a_commit_reads_one_base_position_per_revision_however_many_events_it_appends() {
        let mut registry = Registry::new();
        registry
            .register(
                serde_json::from_value(serde_json::json!({
                    "entity": "ticket",
                    "version": 1,
                    "schema": { "fields": { "title": { "type": "string", "required": true } } },
                    "lifecycle": { "initial": "open", "states": ["open"] },
                    "create": { "emit": { "type": "TicketOpened", "payload": { "ticket": "$id" } } }
                }))
                .expect("definition parses"),
            )
            .expect("definition validates");
        let mut decision = Runtime::new(&registry)
            .create("ticket", 1, "many", serde_json::json!({"title": "T"}))
            .expect("creation");
        let first = decision.record.events[0].clone();
        decision.record.events = (0..128_u64)
            .map(|n| {
                let mut event = first.clone();
                event.changed.insert("n".to_owned(), serde_json::json!(n));
                event
            })
            .collect();
        let mut store = SqliteStore::in_memory().expect("store");

        let position_reads = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&position_reads);
        store
            .connection
            .trace_v2(
                TraceEventCodes::SQLITE_TRACE_STMT,
                Some(move |event: TraceEvent<'_>| {
                    if let TraceEvent::Stmt(_, sql) = event {
                        if sql.trim_start().starts_with("SELECT") && sql.contains("FROM events") {
                            counter.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }),
            )
            .expect("trace installed");
        store.commit(&decision, Expect::Absent).expect("commit");
        let reads = position_reads.load(Ordering::Relaxed);

        let mut statement = store
            .connection
            .prepare(
                "SELECT position, document FROM events \
                 WHERE entity = 'ticket' AND id = 'many' AND revision = 1 ORDER BY position",
            )
            .expect("prepared");
        let numbered: Vec<(i64, u64)> = statement
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .expect("events answer")
            .map(|row| {
                let (position, document) = row.expect("a row");
                let event: entity_core::DomainEvent =
                    serde_json::from_str(&document).expect("an event");
                (position, event.changed["n"].as_u64().expect("numbered"))
            })
            .collect();
        let expected: Vec<(i64, u64)> = (0..128_i64).map(|n| (n, n.unsigned_abs())).collect();
        assert_eq!(
            numbered, expected,
            "positions are consecutive from 0 in the order the decision emitted the events"
        );
        assert_eq!(
            reads, 1,
            "the base position is read once for the revision, not once per event"
        );
    }
}
