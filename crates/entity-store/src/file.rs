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
use std::sync::atomic::{AtomicU64, Ordering};

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

    fn ids(&self, entity: &str) -> Result<Vec<String>, StoreError> {
        self.ids_under(entity)
    }
}

impl FileStore {
    /// The identities under `<root>/<entity>/`: one `<id>.json` per instance.
    ///
    /// `<id>.events.jsonl` sits beside each and is not an instance; an event log whose state file
    /// never landed (the crash window in the module doc) is therefore **not** listed — what is
    /// listed is what `load` would answer, and nothing else.
    fn ids_under(&self, entity: &str) -> Result<Vec<String>, StoreError> {
        let directory = self.root.join(entity);
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(backend("listing", &directory, &error)),
        };
        let mut ids = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| backend("listing", &directory, &error))?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if name.ends_with(".events.jsonl") {
                continue;
            }
            if let Some(id) = name.strip_suffix(".json") {
                ids.push(id.to_owned());
            }
        }
        ids.sort();
        Ok(ids)
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

/// Distinguishes one write's temporary file from another's in the same process.
static WRITES: AtomicU64 = AtomicU64::new(0);

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
        //
        // **Appended once, however many times this is called.** Events land before the state, so a
        // state write that fails leaves the expectation unchanged — and the retry any caller is
        // entitled to make would append the same events a second time, producing a log that no
        // longer folds. Only what the log does not already hold is written — judged by the event
        // itself and not by its revision, because an *observation* is a new event at a revision the
        // log has already reached (something was seen about the instance; the instance did not
        // change), and a guard on the revision alone would drop it silently.
        let already = self.events(entity, id)?;
        let fresh: Vec<_> = decision
            .events
            .iter()
            .filter(|event| !already.contains(event))
            .collect();
        if !fresh.is_empty() {
            let path = self.events_path(entity, id);
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|error| backend("opening", &path, &error))?;
            for event in fresh {
                let line = serde_json::to_string(event)
                    .map_err(|error| backend("serialising an event for", &path, &error))?;
                writeln!(file, "{line}").map_err(|error| backend("appending to", &path, &error))?;
            }
            // `flush` only empties this process's buffer. The module's recovery story is that a
            // crash leaves an event whose state did not land, and replay reaches the state the
            // event describes — which requires the event to actually be on the disk. Without this
            // the ordering can invert: the state is installed by a rename the filesystem journals,
            // and the event it explains is lost.
            file.sync_all()
                .map_err(|error| backend("syncing", &path, &error))?;
        }

        // Then the state, through a rename so no reader ever sees half of it.
        //
        // The temporary name carries this process's id and a counter, so two writers of the same
        // instance never share one. A shared temporary is worse than no temporary: writer A's
        // rename installs an inode writer B is still filling, and a reader sees B's bytes arrive
        // in a file that was supposed to appear whole.
        let path = self.state_path(entity, id);
        let temporary = path.with_extension(format!(
            "json.writing.{}.{}",
            std::process::id(),
            WRITES.fetch_add(1, Ordering::Relaxed)
        ));
        let text = serde_json::to_string_pretty(instance)
            .map_err(|error| backend("serialising", &path, &error))?;
        let mut file = fs::File::create(&temporary)
            .map_err(|error| backend("creating", &temporary, &error))?;
        file.write_all(format!("{text}\n").as_bytes())
            .map_err(|error| backend("writing", &temporary, &error))?;
        file.sync_all()
            .map_err(|error| backend("syncing", &temporary, &error))?;
        drop(file);
        fs::rename(&temporary, &path).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            backend("installing", &path, &error)
        })
    }
}
