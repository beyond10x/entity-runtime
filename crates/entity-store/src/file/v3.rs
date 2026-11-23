//! File Store v3: a subject's current state apart from its append-only history, and each entity
//! definition kept once per store under its digest.
//!
//! ```text
//! .entity-store-format                 entity.file-store/3
//! definitions/<64 hex>.json            ["er.definition/1", definition], canonical; the name is its digest
//! subjects/<hex entity>/<hex id>.log   history: one JSON line per entry, appended, never rewritten
//! subjects/<hex entity>/<hex id>.json  current state, replaced whole; a cache of the history
//! ```
//!
//! **The history is the authority.** One write appends its entry, syncs the history file (the
//! commit point), and then replaces the state document. The state document names the history
//! length it reflects; a reader trusts it only when the history still has exactly that length,
//! and otherwise folds the history. So the state document needs no sync of its own: one lost to a
//! crash, torn, or left behind is rebuilt from the history, which is why a v2 write that rewrote
//! the whole subject (and synced it) becomes one appended record and a small state document.
//!
//! A history line that ends without a newline is a write that never completed: readers ignore it
//! and the next writer cuts it before it appends. Every complete line must parse.
//!
//! A record names its definition by the digest `definition_digest` gives the definition document,
//! as `er.record/5` does on Eventlog; the document is written and synced before the first line
//! naming it, and checked against its name whenever it is read.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use entity_core::{DecisionRecord, DomainEvent, EntityDefinition, EntityInstance};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    backend, create_directory_durably, create_private, hex, keep_permissions, lock,
    refuse_overlong_names, reject_symlink, reject_symlink_components, sync_directory,
    temporary_extension, FileStore, StoredSubject, SubjectOrigin, WRITES,
};
use crate::asynchronous::{definition_comparison_bytes, definition_digest, DEFINITION_FRAMING};
use crate::{Envelope, RecordedObservation, StoreError};

/// The format of a v3 state document.
pub(super) const STATE_FORMAT: &str = "entity.file-subject/3";
/// The format named by the first line of a v3 history.
const HISTORY_FORMAT: &str = "entity.file-history/3";
/// The file name extension of a history.
pub(super) const HISTORY_EXTENSION: &str = "log";

/// Where a history read under the write guard ended: what the next append continues from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct HistoryEnd {
    /// Bytes up to and including the last complete line.
    committed: u64,
    /// The file's length; more than `committed` when a write was interrupted mid-line.
    length: u64,
}

/// One line of a history.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Line {
    /// The first line: which subject this history is about, and how it began.
    Subject(Header),
    /// A decision record whose `record.definition` is a definition digest or `null`.
    Record(Value),
    /// A recorded observation.
    Observation(RecordedObservation),
    /// A state written without a decision record: a plain commit, or a migrated subject's state.
    State(StateEntry),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    format: String,
    entity: String,
    id: String,
    origin: SubjectOrigin,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StateEntry {
    instance: EntityInstance,
    #[serde(default)]
    events: Vec<DomainEvent>,
}

/// The state document: the instance and the history length it reflects.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StateDocument {
    format: String,
    instance: EntityInstance,
    history_bytes: u64,
}

/// What one write adds to a subject's history.
pub(super) enum Entry<'a> {
    /// A decision record.
    Record(&'a Envelope<DecisionRecord>),
    /// A recorded observation.
    Observation(&'a RecordedObservation),
    /// A state and its events without a decision record.
    State {
        instance: &'a EntityInstance,
        events: &'a [DomainEvent],
    },
}

/// The definition documents one encoding produced, by digest, to be written before the lines.
pub(super) type Definitions = BTreeMap<String, (Vec<u8>, EntityDefinition)>;

impl FileStore {
    pub(super) fn history_path(&self, entity: &str, id: &str) -> PathBuf {
        self.entity_directory(entity)
            .join(format!("{}.{HISTORY_EXTENSION}", hex(id.as_bytes())))
    }

    fn definition_path(&self, digest: &str) -> Result<PathBuf, StoreError> {
        let name = digest
            .strip_prefix("sha256:")
            .filter(|hex| {
                hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
            })
            .ok_or_else(|| {
                StoreError::Backend(format!(
                    "File Store record names definition {digest:?}, which is not a definition digest"
                ))
            })?;
        Ok(self.root.join("definitions").join(format!("{name}.json")))
    }

    /// The definition stored under `digest`, held to its digest. Cached per handle: a document
    /// that hashed to its name once is the same document every later time.
    fn definition(&self, digest: &str) -> Result<EntityDefinition, StoreError> {
        if let Some(definition) = lock(&self.definitions).get(digest) {
            return Ok(definition.clone());
        }
        let path = self.definition_path(digest)?;
        reject_symlink_components(&path)?;
        let bytes = fs::read(&path).map_err(|error| backend("reading", &path, &error))?;
        let actual = definition_digest(&bytes);
        if actual != digest {
            return Err(StoreError::Backend(format!(
                "File Store definition document {} does not hash to {digest} but to {actual}",
                path.display()
            )));
        }
        let (framing, definition): (String, EntityDefinition) =
            serde_json::from_slice(&bytes).map_err(|error| backend("parsing", &path, &error))?;
        if framing != DEFINITION_FRAMING {
            return Err(StoreError::Backend(format!(
                "File Store definition document {} is framed {framing:?}, not {DEFINITION_FRAMING:?}",
                path.display()
            )));
        }
        lock(&self.definitions).insert(digest.to_owned(), definition.clone());
        Ok(definition)
    }

    /// Writes and syncs each definition document the store does not hold yet, before any line
    /// that names it. A document already present was synced by the write that created it, and is
    /// held to its name when read.
    fn store_definitions(&self, definitions: Definitions) -> Result<(), StoreError> {
        for (digest, (bytes, definition)) in definitions {
            if lock(&self.definitions).contains_key(&digest) {
                continue;
            }
            let path = self.definition_path(&digest)?;
            reject_symlink_components(&path)?;
            if fs::symlink_metadata(&path).is_err() {
                let directory = self.root.join("definitions");
                create_directory_durably(&directory)?;
                reject_symlink_components(&directory)?;
                let temporary = path
                    .with_extension(temporary_extension(WRITES.fetch_add(1, Ordering::Relaxed)));
                let result = (|| {
                    let mut file = create_private(&temporary)?;
                    file.write_all(&bytes)
                        .map_err(|error| backend("writing", &temporary, &error))?;
                    #[cfg(feature = "kpi-counters")]
                    crate::kpi_counters::add(&crate::kpi_counters::FILE_SYNCS, 1);
                    file.sync_all()
                        .map_err(|error| backend("syncing", &temporary, &error))?;
                    drop(file);
                    fs::rename(&temporary, &path)
                        .map_err(|error| backend("installing", &path, &error))?;
                    sync_directory(&directory)
                })();
                if result.is_err() {
                    let _ = fs::remove_file(&temporary);
                }
                result?;
            }
            lock(&self.definitions).insert(digest, definition);
        }
        Ok(())
    }

    /// The current state of a v3 subject: the state document when it reflects the whole
    /// history, the history folded otherwise.
    pub(super) fn load_v3(
        &self,
        entity: &str,
        id: &str,
    ) -> Result<Option<EntityInstance>, StoreError> {
        let state = self.subject_path(entity, id);
        let history = self.history_path(entity, id);
        reject_symlink_components(&state)?;
        reject_symlink_components(&history)?;
        if let Ok(bytes) = fs::read(&state) {
            if let Ok(document) = serde_json::from_slice::<StateDocument>(&bytes) {
                let current = document.format == STATE_FORMAT
                    && document.instance.entity == entity
                    && document.instance.id == id
                    && fs::symlink_metadata(&history)
                        .is_ok_and(|metadata| metadata.len() == document.history_bytes);
                if current {
                    return Ok(Some(document.instance));
                }
            }
        }
        Ok(self.read_v3(entity, id)?.map(|subject| subject.instance))
    }

    /// Reads and folds one v3 history: `None` when the store holds no committed entry for it.
    pub(super) fn read_v3(
        &self,
        entity: &str,
        id: &str,
    ) -> Result<Option<StoredSubject>, StoreError> {
        let path = self.history_path(entity, id);
        reject_symlink_components(&path)?;
        #[cfg(test)]
        super::tests::count_subject_read(&path);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(backend("reading", &path, &error)),
        };
        fold(&path, &bytes, entity, id, &mut |digest: &str| {
            self.definition(digest)
        })
    }

    /// Appends `entry` to the subject `stored` describes after the change, syncs the history, and
    /// replaces the state document. Called under the write guard after the epoch advanced.
    pub(super) fn append_v3(
        &self,
        stored: &StoredSubject,
        entry: &Entry<'_>,
    ) -> Result<(), StoreError> {
        let (line, definitions) = encode(entry)?;
        let mut bytes = Vec::new();
        if stored.history.is_none() {
            bytes.extend(header(stored)?);
        }
        bytes.extend(line);
        self.write_history(stored, &bytes, definitions)
    }

    /// Writes a subject's complete history at once: what the v2 migration does.
    pub(super) fn install_v3(&self, stored: &StoredSubject) -> Result<(), StoreError> {
        let (bytes, definitions) = migrated_history(stored)?;
        self.write_history(stored, &bytes, definitions)
    }

    fn write_history(
        &self,
        stored: &StoredSubject,
        bytes: &[u8],
        definitions: Definitions,
    ) -> Result<(), StoreError> {
        let entity = &stored.instance.entity;
        let id = &stored.instance.id;
        let counter = WRITES.fetch_add(1, Ordering::Relaxed);
        refuse_overlong_names(entity, id, counter)?;
        self.prepare()?;
        self.store_definitions(definitions)?;
        let directory = self.entity_directory(entity);
        reject_symlink_components(&directory)?;
        create_directory_durably(&directory)?;
        reject_symlink_components(&directory)?;
        let path = self.history_path(entity, id);
        reject_symlink(&path)?;

        // The first committed entry: its directory entry is synced below, whoever created the file.
        let created = stored.history.is_none();
        let mut file = match stored.history {
            Some(end) => {
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .map_err(|error| backend("opening", &path, &error))?;
                if end.length != end.committed {
                    // An interrupted append: never committed, cut before this one.
                    file.set_len(end.committed)
                        .map_err(|error| backend("cutting a torn line from", &path, &error))?;
                }
                file.seek(SeekFrom::Start(end.committed))
                    .map_err(|error| backend("seeking", &path, &error))?;
                file
            }
            // No committed entry: a new subject, or the remains of a first write that never
            // completed a line, which are replaced.
            None => {
                let mut options = fs::OpenOptions::new();
                options.write(true).create(true).truncate(true);
                #[cfg(unix)]
                std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
                options
                    .open(&path)
                    .map_err(|error| backend("creating", &path, &error))?
            }
        };
        let start = stored.history.map_or(0, |end| end.committed);
        file.write_all(bytes)
            .map_err(|error| backend("appending to", &path, &error))?;
        #[cfg(feature = "kpi-counters")]
        crate::kpi_counters::add(&crate::kpi_counters::FILE_SYNCS, 1);
        file.sync_all()
            .map_err(|error| backend("syncing", &path, &error))?;
        drop(file);

        let history_bytes = start + bytes.len() as u64;
        self.write_state(stored, history_bytes, counter)?;
        if created {
            // The history's directory entry, and the state document's with it.
            sync_directory(&directory)?;
        }
        Ok(())
    }

    /// Replaces the state document. Not synced: the history is synced and decides, and a state
    /// document that did not reach the disk is rebuilt from it.
    fn write_state(
        &self,
        stored: &StoredSubject,
        history_bytes: u64,
        counter: u64,
    ) -> Result<(), StoreError> {
        let path = self.subject_path(&stored.instance.entity, &stored.instance.id);
        reject_symlink(&path)?;
        let temporary = path.with_extension(temporary_extension(counter));
        let mut text = serde_json::to_vec(&StateDocument {
            format: STATE_FORMAT.to_owned(),
            instance: stored.instance.clone(),
            history_bytes,
        })
        .map_err(|error| backend("serialising", &path, &error))?;
        text.push(b'\n');
        let result = (|| {
            let mut file = create_private(&temporary)?;
            keep_permissions(&file, &path, &temporary)?;
            file.write_all(&text)
                .map_err(|error| backend("writing", &temporary, &error))?;
            drop(file);
            fs::rename(&temporary, &path).map_err(|error| backend("installing", &path, &error))
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

/// Whether a v3 history read under the shared acquisition lock ends in an interrupted write.
pub(super) fn torn(stored: &StoredSubject) -> bool {
    stored
        .history
        .is_some_and(|end| end.length != end.committed)
}

/// Folds history bytes into the subject they describe: `None` when no entry is committed.
pub(super) fn fold(
    path: &Path,
    bytes: &[u8],
    entity: &str,
    id: &str,
    definition: &mut dyn FnMut(&str) -> Result<EntityDefinition, StoreError>,
) -> Result<Option<StoredSubject>, StoreError> {
    let committed = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |last| last + 1);
    if committed == 0 {
        return Ok(None);
    }
    let parse = |number: usize, line: &[u8]| -> Result<Line, StoreError> {
        serde_json::from_slice(line).map_err(|error| {
            StoreError::Backend(format!("parsing {} line {number}: {error}", path.display()))
        })
    };
    // Every complete line, without the newline that ends the last one.
    let mut lines = bytes[..committed - 1].split(|byte| *byte == b'\n');
    let first = lines.next().unwrap_or_default();
    let Line::Subject(header) = parse(1, first)? else {
        return Err(StoreError::Backend(format!(
            "{} line 1 is not a subject header",
            path.display()
        )));
    };
    if header.format != HISTORY_FORMAT || header.entity != entity || header.id != id {
        return Err(StoreError::Backend(format!(
            "{} is stored under {entity}/{id} but its header names {}/{} in format {:?}",
            path.display(),
            header.entity,
            header.id,
            header.format
        )));
    }
    let mut instance = None;
    let mut records = Vec::new();
    let mut observations = Vec::new();
    let mut events = Vec::new();
    for (index, line) in lines.enumerate() {
        let number = index + 2;
        match parse(number, line)? {
            Line::Subject(_) => {
                return Err(StoreError::Backend(format!(
                    "{} line {number} is a second subject header",
                    path.display()
                )))
            }
            Line::Record(value) => {
                let envelope = decode_record(path, number, value, definition)?;
                instance = Some(envelope.record.result.clone());
                records.push(envelope);
            }
            Line::Observation(observation) => observations.push(observation),
            Line::State(state) => {
                instance = Some(state.instance);
                events.extend(state.events);
            }
        }
    }
    // A header whose first entry never completed: the subject was never created.
    let Some(instance) = instance else {
        return Ok(None);
    };
    if instance.entity != entity || instance.id != id {
        return Err(StoreError::Backend(format!(
            "{} is stored under {entity}/{id} but holds {}/{}",
            path.display(),
            instance.entity,
            instance.id
        )));
    }
    Ok(Some(StoredSubject {
        format: STATE_FORMAT.to_owned(),
        origin: header.origin,
        instance,
        records,
        observations,
        events,
        history: Some(HistoryEnd {
            committed: committed as u64,
            length: bytes.len() as u64,
        }),
    }))
}

fn decode_record(
    path: &Path,
    number: usize,
    mut value: Value,
    definition: &mut dyn FnMut(&str) -> Result<EntityDefinition, StoreError>,
) -> Result<Envelope<DecisionRecord>, StoreError> {
    let slot = value
        .pointer_mut("/record/definition")
        .map(std::mem::take)
        .unwrap_or(Value::Null);
    let resolved = match slot {
        Value::Null => None,
        Value::String(digest) => Some(definition(&digest)?),
        other => {
            return Err(StoreError::Backend(format!(
                "{} line {number} names its definition as {other}, not a digest",
                path.display()
            )))
        }
    };
    let mut envelope: Envelope<DecisionRecord> =
        serde_json::from_value(value).map_err(|error| {
            StoreError::Backend(format!("parsing {} line {number}: {error}", path.display()))
        })?;
    envelope.record.definition = resolved;
    Ok(envelope)
}

/// One history line for `entry`, and the definition document it names.
fn encode(entry: &Entry<'_>) -> Result<(Vec<u8>, Definitions), StoreError> {
    let encoding = |error: serde_json::Error| StoreError::Backend(error.to_string());
    let mut definitions = Definitions::new();
    let line = match entry {
        Entry::Record(envelope) => {
            let mut value = serde_json::to_value(envelope).map_err(encoding)?;
            if let Some(definition) = &envelope.record.definition {
                let document = definition_comparison_bytes(definition)
                    .map_err(|error| StoreError::Backend(error.to_string()))?;
                let digest = definition_digest(&document);
                if let Some(slot) = value.pointer_mut("/record/definition") {
                    *slot = Value::String(digest.clone());
                }
                definitions.insert(digest, (document, definition.clone()));
            }
            Line::Record(value)
        }
        Entry::Observation(observation) => Line::Observation((*observation).clone()),
        Entry::State { instance, events } => Line::State(StateEntry {
            instance: (*instance).clone(),
            events: events.to_vec(),
        }),
    };
    let mut bytes = serde_json::to_vec(&line).map_err(encoding)?;
    bytes.push(b'\n');
    Ok((bytes, definitions))
}

fn header(stored: &StoredSubject) -> Result<Vec<u8>, StoreError> {
    let mut bytes = serde_json::to_vec(&Line::Subject(Header {
        format: HISTORY_FORMAT.to_owned(),
        entity: stored.instance.entity.clone(),
        id: stored.instance.id.clone(),
        origin: stored.origin,
    }))
    .map_err(|error| StoreError::Backend(error.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// The complete v3 history of a subject read from v2: its records and observations in their
/// order, then one state entry holding the v2 state and its events, so the fold ends exactly where
/// the v2 document did.
pub(super) fn migrated_history(
    stored: &StoredSubject,
) -> Result<(Vec<u8>, Definitions), StoreError> {
    let mut bytes = header(stored)?;
    let mut definitions = Definitions::new();
    let mut add = |entry: Entry<'_>| -> Result<(), StoreError> {
        let (line, named) = encode(&entry)?;
        bytes.extend(line);
        definitions.extend(named);
        Ok(())
    };
    for envelope in &stored.records {
        add(Entry::Record(envelope))?;
    }
    for observation in &stored.observations {
        add(Entry::Observation(observation))?;
    }
    add(Entry::State {
        instance: &stored.instance,
        events: &stored.events,
    })?;
    Ok((bytes, definitions))
}
