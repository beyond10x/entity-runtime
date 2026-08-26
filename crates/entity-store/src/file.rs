//! A store that is a directory.
//!
//! One JSON file per instance and one JSONL log of its events, under a root the caller names:
//!
//! ```text
//! <root>/<entity>/<id>.json          the instance, as the kernel last left it
//! <root>/<entity>/<id>.events.jsonl  every event, oldest first, one per line
//! ```
//!
//! Readable, diffable and greppable, which is the whole reason to have it: a store you can open in
//! an editor is a store you can debug without another tool.
//!
//! # The crash window, stated rather than implied
//!
//! [`Store::commit`] appends the events **before** writing the state, and the instance is written
//! through a temporary file and a rename so it is never half-written. A crash in between therefore
//! leaves an event whose state did not land — recoverable, because replaying the log reaches the
//! state the event describes. The other order would lose the event instead, and nothing could
//! recover a fact nobody recorded.
//!
//! It is still a window. **This provider is not transactional**, and a deployment that needs it to
//! be wants one over a database with a real transaction — which is a different provider behind the
//! same traits, and is exactly why these are traits.
//!
//! # No locking
//!
//! Two processes writing one root can both pass the revision check before either writes. The
//! revision check makes concurrent writers *visible* within a process; it does not make this
//! provider safe across them, and saying so here is cheaper than somebody discovering it.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use entity_core::{Decision, DomainEvent, EntityInstance};

use crate::{check, EventProvider, Expect, StateProvider, Store, StoreError};

/// A [`Store`] backed by a directory.
#[derive(Debug, Clone)]
pub struct FileStore {
    root: PathBuf,
}

impl FileStore {
    /// Opens the store rooted at `root`. The directory need not exist yet.
    pub fn open(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The directory it writes into.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where an instance's state file is.
    fn state_path(&self, entity: &str, id: &str) -> PathBuf {
        self.root.join(entity).join(format!("{id}.json"))
    }

    /// Where an instance's event log is.
    fn events_path(&self, entity: &str, id: &str) -> PathBuf {
        self.root.join(entity).join(format!("{id}.events.jsonl"))
    }
}

/// Turns any IO or serialisation failure into a backend error carrying what it was doing.
fn backend(operation: &str, path: &Path, error: &impl std::fmt::Display) -> StoreError {
    StoreError::Backend(format!("{operation} {}: {error}", path.display()))
}

impl StateProvider for FileStore {
    fn load(&self, entity: &str, id: &str) -> Result<Option<EntityInstance>, StoreError> {
        let path = self.state_path(entity, id);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(backend("reading", &path, &error)),
        };
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|error| backend("parsing", &path, &error))
    }
}

impl EventProvider for FileStore {
    fn events(&self, entity: &str, id: &str) -> Result<Vec<DomainEvent>, StoreError> {
        let path = self.events_path(entity, id);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(backend("reading", &path, &error)),
        };
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                serde_json::from_str(line).map_err(|error| backend("parsing", &path, &error))
            })
            .collect()
    }
}

impl Store for FileStore {
    fn commit(&mut self, decision: &Decision, expect: Expect) -> Result<(), StoreError> {
        let instance = &decision.instance;
        let (entity, id) = (instance.entity.as_str(), instance.id.as_str());

        // Before anything is written, so a refusal leaves the directory exactly as it was.
        let found = self.load(entity, id)?.map(|held| held.revision);
        check(entity, id, expect, found)?;

        let directory = self.root.join(entity);
        fs::create_dir_all(&directory).map_err(|error| backend("creating", &directory, &error))?;

        // Events first: see the module's note on the crash window.
        if !decision.events.is_empty() {
            let path = self.events_path(entity, id);
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|error| backend("opening", &path, &error))?;
            for event in &decision.events {
                let line = serde_json::to_string(event)
                    .map_err(|error| backend("serialising an event for", &path, &error))?;
                writeln!(file, "{line}").map_err(|error| backend("appending to", &path, &error))?;
            }
            file.flush()
                .map_err(|error| backend("flushing", &path, &error))?;
        }

        // Then the state, through a rename so no reader ever sees half of it.
        let path = self.state_path(entity, id);
        let temporary = path.with_extension("json.writing");
        let text = serde_json::to_string_pretty(instance)
            .map_err(|error| backend("serialising", &path, &error))?;
        fs::write(&temporary, format!("{text}\n"))
            .map_err(|error| backend("writing", &temporary, &error))?;
        fs::rename(&temporary, &path).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            backend("installing", &path, &error)
        })
    }
}
