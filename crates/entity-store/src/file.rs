//! A confined, crash-safe directory store.
//!
//! File Store v2 writes one document per subject. Entity names and identities are hexadecimal
//! UTF-8 path components, so data can never become `..`, an absolute path or a separator. State
//! and history are replaced together through a synced temporary file and rename.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use entity_core::{Decision, DecisionRecord, DomainEvent, EntityInstance};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    check, Envelope, EventProvider, Expect, HistoryProvider, RecordedCommit, RecordedObservation,
    StateProvider, Store, StoreError,
};

const FORMAT: &str = "entity.file-store/2";
const FORMAT_FILE: &str = ".entity-store-format";
const SUBJECT_FORMAT: &str = "entity.file-subject/2";

/// A [`Store`] backed by one versioned directory.
#[derive(Debug, Clone)]
pub struct FileStore {
    root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredSubject {
    format: String,
    origin: SubjectOrigin,
    instance: EntityInstance,
    #[serde(default)]
    records: Vec<Envelope<DecisionRecord>>,
    #[serde(default)]
    observations: Vec<RecordedObservation>,
    #[serde(default)]
    events: Vec<DomainEvent>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SubjectOrigin {
    Current,
    LegacySnapshot,
}

/// What an out-of-place legacy migration found or wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FileMigrationReport {
    /// Number of subject documents validated.
    pub subjects: usize,
    /// Number of legacy events preserved.
    pub events: usize,
    /// Whether no destination bytes were written.
    pub dry_run: bool,
}

impl FileStore {
    /// Opens the store rooted at `root`. Validation happens on the first operation.
    pub fn open(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The directory it writes into.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn entity_directory(&self, entity: &str) -> PathBuf {
        self.root.join("subjects").join(hex(entity.as_bytes()))
    }

    fn subject_path(&self, entity: &str, id: &str) -> PathBuf {
        self.entity_directory(entity)
            .join(format!("{}.json", hex(id.as_bytes())))
    }

    fn format_path(&self) -> PathBuf {
        self.root.join(FORMAT_FILE)
    }

    fn root_state(&self) -> Result<RootState, StoreError> {
        reject_symlink_components(&self.root)?;
        let metadata = match fs::symlink_metadata(&self.root) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(RootState::Missing)
            }
            Err(error) => return Err(backend("inspecting", &self.root, &error)),
        };
        if !metadata.is_dir() {
            return Err(StoreError::Backend(format!(
                "File Store root {} is not a directory",
                self.root.display()
            )));
        }
        let marker = self.format_path();
        match fs::read_to_string(&marker) {
            Ok(value) if value.trim() == FORMAT => Ok(RootState::V2),
            Ok(value) => Err(StoreError::Backend(format!(
                "File Store root {} declares unsupported format {:?}",
                self.root.display(),
                value.trim()
            ))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let mut entries = fs::read_dir(&self.root)
                    .map_err(|error| backend("listing", &self.root, &error))?;
                if entries.next().is_none() {
                    Ok(RootState::Empty)
                } else {
                    Err(StoreError::Backend(format!(
                        "File Store root {} has no v2 marker; migrate the legacy store with `entity store migrate-file`",
                        self.root.display()
                    )))
                }
            }
            Err(error) => Err(backend("reading", &marker, &error)),
        }
    }

    fn prepare(&self) -> Result<(), StoreError> {
        match self.root_state()? {
            RootState::V2 => return Ok(()),
            RootState::Missing | RootState::Empty => {}
        }
        fs::create_dir_all(&self.root).map_err(|error| backend("creating", &self.root, &error))?;
        reject_symlink_components(&self.root)?;
        let marker = self.format_path();
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker)
            .map_err(|error| backend("creating", &marker, &error))?;
        file.write_all(format!("{FORMAT}\n").as_bytes())
            .map_err(|error| backend("writing", &marker, &error))?;
        file.sync_all()
            .map_err(|error| backend("syncing", &marker, &error))?;
        sync_directory(&self.root)?;
        Ok(())
    }

    fn read_subject(&self, entity: &str, id: &str) -> Result<Option<StoredSubject>, StoreError> {
        match self.root_state()? {
            RootState::Missing | RootState::Empty => return Ok(None),
            RootState::V2 => {}
        }
        let path = self.subject_path(entity, id);
        reject_symlink(&path)?;
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(backend("reading", &path, &error)),
        };
        let subject: StoredSubject =
            serde_json::from_str(&text).map_err(|error| backend("parsing", &path, &error))?;
        if subject.format != SUBJECT_FORMAT {
            return Err(StoreError::Backend(format!(
                "subject {} declares unsupported format {:?}",
                path.display(),
                subject.format
            )));
        }
        if subject.instance.entity != entity || subject.instance.id != id {
            return Err(StoreError::Backend(format!(
                "subject {} is stored under {entity}/{id} but contains {}/{}",
                path.display(),
                subject.instance.entity,
                subject.instance.id
            )));
        }
        Ok(Some(subject))
    }

    fn write_subject(&self, subject: &StoredSubject) -> Result<(), StoreError> {
        self.prepare()?;
        let entity = &subject.instance.entity;
        let id = &subject.instance.id;
        let directory = self.entity_directory(entity);
        fs::create_dir_all(&directory).map_err(|error| backend("creating", &directory, &error))?;
        reject_symlink_components(&directory)?;
        let path = self.subject_path(entity, id);
        reject_symlink(&path)?;
        let temporary = path.with_extension(format!(
            "json.writing.{}.{}",
            std::process::id(),
            WRITES.fetch_add(1, Ordering::Relaxed)
        ));
        let text = serde_json::to_string_pretty(subject)
            .map_err(|error| backend("serialising", &path, &error))?;
        let result = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|error| backend("creating", &temporary, &error))?;
            file.write_all(format!("{text}\n").as_bytes())
                .map_err(|error| backend("writing", &temporary, &error))?;
            file.sync_all()
                .map_err(|error| backend("syncing", &temporary, &error))?;
            drop(file);
            fs::rename(&temporary, &path).map_err(|error| backend("installing", &path, &error))?;
            sync_directory(&directory)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn record_document(&self, record_id: &str) -> Result<Option<Value>, StoreError> {
        match self.root_state()? {
            RootState::Missing | RootState::Empty => return Ok(None),
            RootState::V2 => {}
        }
        let subjects = self.root.join("subjects");
        let entities = match fs::read_dir(&subjects) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(backend("listing", &subjects, &error)),
        };
        for entity_entry in entities {
            let entity_entry =
                entity_entry.map_err(|error| backend("listing", &subjects, &error))?;
            let entity_path = entity_entry.path();
            reject_symlink(&entity_path)?;
            if !entity_entry
                .file_type()
                .map_err(|error| backend("inspecting", &entity_path, &error))?
                .is_dir()
            {
                return Err(StoreError::Backend(format!(
                    "unexpected non-directory in File Store subjects {}",
                    entity_path.display()
                )));
            }
            let encoded_entity = entity_entry.file_name();
            let entity = unhex(encoded_entity.to_str().ok_or_else(|| {
                StoreError::Backend(format!(
                    "non-UTF-8 File Store entity directory {}",
                    entity_path.display()
                ))
            })?)
            .map_err(|detail| {
                StoreError::Backend(format!(
                    "invalid File Store entity directory {}: {detail}",
                    entity_path.display()
                ))
            })?;
            for id in self.ids(&entity)? {
                let subject = self
                    .read_subject(&entity, &id)?
                    .expect("ids validates every returned subject");
                for envelope in subject.records {
                    if envelope.record_id == record_id {
                        let commit = RecordedCommit {
                            instance: envelope.record.result.clone(),
                            envelope,
                        };
                        return serde_json::to_value(commit).map(Some).map_err(|error| {
                            backend("serialising a stored record", &entity_path, &error)
                        });
                    }
                }
                for observation in subject.observations {
                    if observation.envelope.record_id == record_id {
                        return serde_json::to_value(observation)
                            .map(Some)
                            .map_err(|error| {
                                backend("serialising a stored observation", &entity_path, &error)
                            });
                    }
                }
            }
        }
        Ok(None)
    }
}

#[derive(Debug, Clone, Copy)]
enum RootState {
    Missing,
    Empty,
    V2,
}

fn backend(operation: &str, path: &Path, error: &impl std::fmt::Display) -> StoreError {
    StoreError::Backend(format!("{operation} {}: {error}", path.display()))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn unhex(text: &str) -> Result<String, String> {
    if text.len() % 2 != 0 {
        return Err("hex identity has odd length".to_owned());
    }
    let mut bytes = Vec::with_capacity(text.len() / 2);
    for pair in text.as_bytes().chunks_exact(2) {
        let digit = |byte: u8| match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            _ => None,
        };
        let high = digit(pair[0]).ok_or_else(|| "identity is not lowercase hex".to_owned())?;
        let low = digit(pair[1]).ok_or_else(|| "identity is not lowercase hex".to_owned())?;
        bytes.push((high << 4) | low);
    }
    String::from_utf8(bytes).map_err(|error| format!("identity is not UTF-8: {error}"))
}

fn reject_symlink(path: &Path) -> Result<(), StoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(StoreError::Backend(format!(
            "refusing symlink in File Store path {}",
            path.display()
        ))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(backend("inspecting", path, &error)),
    }
}

fn reject_symlink_components(path: &Path) -> Result<(), StoreError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_)
            | Component::RootDir
            | Component::CurDir
            | Component::ParentDir => {
                current.push(component.as_os_str());
            }
            Component::Normal(part) => current.push(part),
        }
        reject_symlink(&current)?;
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), StoreError> {
    fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| backend("syncing directory", path, &error))
}

impl StateProvider for FileStore {
    fn load(&self, entity: &str, id: &str) -> Result<Option<EntityInstance>, StoreError> {
        Ok(self.read_subject(entity, id)?.map(|stored| stored.instance))
    }

    fn ids(&self, entity: &str) -> Result<Vec<String>, StoreError> {
        match self.root_state()? {
            RootState::Missing | RootState::Empty => return Ok(Vec::new()),
            RootState::V2 => {}
        }
        let directory = self.entity_directory(entity);
        reject_symlink(&directory)?;
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(backend("listing", &directory, &error)),
        };
        let mut ids = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| backend("listing", &directory, &error))?;
            let path = entry.path();
            reject_symlink(&path)?;
            if !entry
                .file_type()
                .map_err(|error| backend("inspecting", &path, &error))?
                .is_file()
            {
                return Err(StoreError::Backend(format!(
                    "unexpected non-file in File Store entity directory {}",
                    path.display()
                )));
            }
            let name = entry.file_name();
            let name = name.to_str().ok_or_else(|| {
                StoreError::Backend(format!("non-UTF-8 File Store filename {}", path.display()))
            })?;
            let encoded = name.strip_suffix(".json").ok_or_else(|| {
                StoreError::Backend(format!("unexpected File Store file {}", path.display()))
            })?;
            let id = unhex(encoded).map_err(|detail| {
                StoreError::Backend(format!(
                    "invalid File Store file {}: {detail}",
                    path.display()
                ))
            })?;
            self.read_subject(entity, &id)?;
            ids.push(id);
        }
        ids.sort();
        Ok(ids)
    }
}

impl EventProvider for FileStore {
    fn events(&self, entity: &str, id: &str) -> Result<Vec<DomainEvent>, StoreError> {
        Ok(self
            .read_subject(entity, id)?
            .map_or_else(Vec::new, |stored| {
                let mut events = stored.events;
                for record in stored.records {
                    events.extend(record.record.events);
                }
                events
            }))
    }
}

impl HistoryProvider for FileStore {
    fn records(&self, entity: &str, id: &str) -> Result<Vec<Envelope<DecisionRecord>>, StoreError> {
        Ok(self
            .read_subject(entity, id)?
            .map_or_else(Vec::new, |stored| stored.records))
    }

    fn observations(&self, entity: &str, id: &str) -> Result<Vec<RecordedObservation>, StoreError> {
        Ok(self
            .read_subject(entity, id)?
            .map_or_else(Vec::new, |stored| stored.observations))
    }
}

static WRITES: AtomicU64 = AtomicU64::new(0);

impl Store for FileStore {
    fn commit(&mut self, decision: &Decision, expect: Expect) -> Result<(), StoreError> {
        let instance = &decision.instance;
        let held = self.read_subject(&instance.entity, &instance.id)?;
        check(
            &instance.entity,
            &instance.id,
            expect,
            held.as_ref().map(|stored| stored.instance.revision),
        )?;
        let mut stored = held.unwrap_or_else(|| StoredSubject {
            format: SUBJECT_FORMAT.to_owned(),
            origin: SubjectOrigin::Current,
            instance: instance.clone(),
            records: Vec::new(),
            observations: Vec::new(),
            events: Vec::new(),
        });
        for event in &decision.record.events {
            if !stored.events.contains(event) {
                stored.events.push(event.clone());
            }
        }
        stored.instance = instance.clone();
        self.write_subject(&stored)
    }

    fn commit_recorded(
        &mut self,
        commit: &RecordedCommit,
        expect: Expect,
    ) -> Result<(), StoreError> {
        commit.validate()?;
        let document =
            serde_json::to_value(commit).map_err(|error| StoreError::Backend(error.to_string()))?;
        if let Some(existing) = self.record_document(&commit.envelope.record_id)? {
            return if existing == document {
                Ok(())
            } else {
                Err(StoreError::RecordConflict {
                    record_id: commit.envelope.record_id.clone(),
                })
            };
        }
        let entity = &commit.instance.entity;
        let id = &commit.instance.id;
        let mut held = self.read_subject(entity, id)?;
        if let Some(stored) = &held {
            if let Some(existing) = stored
                .records
                .iter()
                .find(|record| record.record_id == commit.envelope.record_id)
            {
                return if existing == &commit.envelope && stored.instance == commit.instance {
                    Ok(())
                } else {
                    Err(StoreError::RecordConflict {
                        record_id: commit.envelope.record_id.clone(),
                    })
                };
            }
            if stored
                .observations
                .iter()
                .any(|observation| observation.envelope.record_id == commit.envelope.record_id)
            {
                return Err(StoreError::RecordConflict {
                    record_id: commit.envelope.record_id.clone(),
                });
            }
        }
        check(
            entity,
            id,
            expect,
            held.as_ref().map(|stored| stored.instance.revision),
        )?;
        let mut stored = held.take().unwrap_or_else(|| StoredSubject {
            format: SUBJECT_FORMAT.to_owned(),
            origin: SubjectOrigin::Current,
            instance: commit.instance.clone(),
            records: Vec::new(),
            observations: Vec::new(),
            events: Vec::new(),
        });
        stored.instance = commit.instance.clone();
        stored.records.push(commit.envelope.clone());
        self.write_subject(&stored)
    }

    fn observe(&mut self, observation: &RecordedObservation) -> Result<(), StoreError> {
        observation.validate()?;
        let document = serde_json::to_value(observation)
            .map_err(|error| StoreError::Backend(error.to_string()))?;
        if let Some(existing) = self.record_document(&observation.envelope.record_id)? {
            return if existing == document {
                Ok(())
            } else {
                Err(StoreError::RecordConflict {
                    record_id: observation.envelope.record_id.clone(),
                })
            };
        }
        let mut stored = self
            .read_subject(&observation.entity, &observation.id)?
            .ok_or_else(|| StoreError::RevisionConflict {
                entity: observation.entity.clone(),
                id: observation.id.clone(),
                expected: Expect::Revision(observation.revision),
                found: None,
            })?;
        if let Some(existing) = stored
            .observations
            .iter()
            .find(|existing| existing.envelope.record_id == observation.envelope.record_id)
        {
            return if existing == observation {
                Ok(())
            } else {
                Err(StoreError::RecordConflict {
                    record_id: observation.envelope.record_id.clone(),
                })
            };
        }
        if stored
            .records
            .iter()
            .any(|record| record.record_id == observation.envelope.record_id)
        {
            return Err(StoreError::RecordConflict {
                record_id: observation.envelope.record_id.clone(),
            });
        }
        check(
            &observation.entity,
            &observation.id,
            Expect::Revision(observation.revision),
            Some(stored.instance.revision),
        )?;
        stored.observations.push(observation.clone());
        self.write_subject(&stored)
    }
}

/// Validates or migrates a legacy split-file store into File Store v2.
///
/// # Errors
///
/// [`StoreError::Backend`] for malformed, partial, symlinked or ambiguous source data, an existing
/// destination, or an IO failure. The source is never modified.
pub fn migrate_file_store_v1(
    source: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    dry_run: bool,
) -> Result<FileMigrationReport, StoreError> {
    let source = source.as_ref();
    let destination = destination.as_ref();
    reject_symlink_components(source)?;
    reject_symlink_components(destination)?;
    if !source.is_dir() {
        return Err(StoreError::Backend(format!(
            "legacy File Store source {} is not a directory",
            source.display()
        )));
    }
    if fs::symlink_metadata(destination).is_ok() {
        return Err(StoreError::Backend(format!(
            "migration destination {} already exists",
            destination.display()
        )));
    }

    let subjects = read_legacy(source)?;
    let report = FileMigrationReport {
        subjects: subjects.len(),
        events: subjects.values().map(|subject| subject.events.len()).sum(),
        dry_run,
    };
    if dry_run {
        return Ok(report);
    }

    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| backend("creating", parent, &error))?;
    reject_symlink_components(parent)?;
    let file_name = destination.file_name().ok_or_else(|| {
        StoreError::Backend("migration destination must name one directory".to_owned())
    })?;
    let staging = parent.join(format!(
        ".{}.migrating.{}",
        file_name.to_string_lossy(),
        std::process::id()
    ));
    if fs::symlink_metadata(&staging).is_ok() {
        return Err(StoreError::Backend(format!(
            "migration staging path {} already exists",
            staging.display()
        )));
    }

    let result = (|| {
        let store = FileStore::open(&staging);
        store.prepare()?;
        for subject in subjects.values() {
            store.write_subject(subject)?;
        }
        sync_directory(&staging)?;
        fs::rename(&staging, destination)
            .map_err(|error| backend("publishing", destination, &error))?;
        sync_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result.map(|()| report)
}

fn read_legacy(root: &Path) -> Result<BTreeMap<(String, String), StoredSubject>, StoreError> {
    let mut subjects = BTreeMap::new();
    for entity_entry in fs::read_dir(root).map_err(|error| backend("listing", root, &error))? {
        let entity_entry = entity_entry.map_err(|error| backend("listing", root, &error))?;
        let entity_path = entity_entry.path();
        reject_symlink(&entity_path)?;
        if !entity_entry
            .file_type()
            .map_err(|error| backend("inspecting", &entity_path, &error))?
            .is_dir()
        {
            return Err(StoreError::Backend(format!(
                "unexpected file in legacy File Store root {}",
                entity_path.display()
            )));
        }
        let entity = entity_entry.file_name().into_string().map_err(|_| {
            StoreError::Backend(format!(
                "non-UTF-8 legacy entity path {}",
                entity_path.display()
            ))
        })?;
        for state_entry in
            fs::read_dir(&entity_path).map_err(|error| backend("listing", &entity_path, &error))?
        {
            let state_entry =
                state_entry.map_err(|error| backend("listing", &entity_path, &error))?;
            let state_path = state_entry.path();
            reject_symlink(&state_path)?;
            if !state_entry
                .file_type()
                .map_err(|error| backend("inspecting", &state_path, &error))?
                .is_file()
            {
                return Err(StoreError::Backend(format!(
                    "nested directory in legacy File Store {}",
                    state_path.display()
                )));
            }
            let name = state_entry.file_name();
            let name = name.to_str().ok_or_else(|| {
                StoreError::Backend(format!(
                    "non-UTF-8 legacy filename {}",
                    state_path.display()
                ))
            })?;
            if name.ends_with(".events.jsonl") {
                continue;
            }
            let id = name.strip_suffix(".json").ok_or_else(|| {
                StoreError::Backend(format!("unexpected legacy file {}", state_path.display()))
            })?;
            let text = fs::read_to_string(&state_path)
                .map_err(|error| backend("reading", &state_path, &error))?;
            let instance: EntityInstance = serde_json::from_str(&text)
                .map_err(|error| backend("parsing", &state_path, &error))?;
            if instance.entity != entity || instance.id != id {
                return Err(StoreError::Backend(format!(
                    "legacy path {entity}/{id} contains {}/{}",
                    instance.entity, instance.id
                )));
            }
            let events_path = entity_path.join(format!("{id}.events.jsonl"));
            reject_symlink(&events_path)?;
            let events = match fs::read_to_string(&events_path) {
                Ok(events) => parse_legacy_events(&events_path, &events)?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
                Err(error) => return Err(backend("reading", &events_path, &error)),
            };
            for event in &events {
                if event.entity != entity || event.id != id {
                    return Err(StoreError::Backend(format!(
                        "legacy event in {} is about {}/{}",
                        events_path.display(),
                        event.entity,
                        event.id
                    )));
                }
            }
            let key = (entity.clone(), id.to_owned());
            if subjects
                .insert(
                    key.clone(),
                    StoredSubject {
                        format: SUBJECT_FORMAT.to_owned(),
                        origin: SubjectOrigin::LegacySnapshot,
                        instance,
                        records: Vec::new(),
                        observations: Vec::new(),
                        events,
                    },
                )
                .is_some()
            {
                return Err(StoreError::Backend(format!(
                    "duplicate legacy subject {}/{}",
                    key.0, key.1
                )));
            }
        }
        for event_entry in
            fs::read_dir(&entity_path).map_err(|error| backend("listing", &entity_path, &error))?
        {
            let event_entry =
                event_entry.map_err(|error| backend("listing", &entity_path, &error))?;
            let event_path = event_entry.path();
            let name = event_entry.file_name();
            let name = name.to_str().ok_or_else(|| {
                StoreError::Backend(format!(
                    "non-UTF-8 legacy filename {}",
                    event_path.display()
                ))
            })?;
            if let Some(id) = name.strip_suffix(".events.jsonl") {
                let state_path = entity_path.join(format!("{id}.json"));
                if !state_path.is_file() {
                    return Err(StoreError::Backend(format!(
                        "legacy event log {} has no matching state document",
                        event_path.display()
                    )));
                }
            }
        }
    }
    Ok(subjects)
}

fn parse_legacy_events(path: &Path, text: &str) -> Result<Vec<DomainEvent>, StoreError> {
    if !text.is_empty() && !text.ends_with('\n') {
        return Err(StoreError::Backend(format!(
            "legacy event log {} ends in a partial record",
            path.display()
        )));
    }
    text.lines()
        .enumerate()
        .map(|(index, line)| {
            if line.trim().is_empty() {
                return Err(StoreError::Backend(format!(
                    "legacy event log {} contains an empty record at line {}",
                    path.display(),
                    index + 1
                )));
            }
            serde_json::from_str(line).map_err(|error| backend("parsing", path, &error))
        })
        .collect()
}
