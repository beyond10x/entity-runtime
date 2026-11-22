//! Where generated output lands: the documentation directory `entity generate docs` publishes and
//! the retained crate `entity generate rust-cli` builds in.
//!
//! Both replace a directory only when the caller passed `--force` and the directory carries the
//! generator's marker. A check made before the output is staged says nothing about the directory
//! at the moment it is published: another writer may have created one in between. So publication
//! decides again, under a lock on the destination, and moves into place with a rename that refuses
//! to replace anything (`renameat2(RENAME_NOREPLACE)` on Linux; elsewhere, a check under the same
//! lock followed by a rename, which POSIX refuses over a non-empty directory).

use crate::Failure;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Write `bundle` as a new documentation directory at `destination`.
///
/// Without `force` nothing that exists at `destination` is replaced, whenever it appeared. With
/// `force` only a directory carrying [`entity_surface::DOCS_MARKER`] is, and the marker is checked
/// under the destination lock on the very directory that is moved aside.
pub(crate) fn install_documentation(
    destination: &Path,
    bundle: &entity_surface::DocumentationBundle,
    force: bool,
) -> Result<(), Failure> {
    install_documentation_with(destination, bundle, force, || {})
}

/// [`install_documentation`] with `before_publish` run after staging and before the destination
/// lock is taken — the window in which another writer can create the destination. Tests put that
/// writer there.
pub(crate) fn install_documentation_with(
    destination: &Path,
    bundle: &entity_surface::DocumentationBundle,
    force: bool,
    before_publish: impl FnOnce(),
) -> Result<(), Failure> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let name = destination
        .file_name()
        .ok_or_else(|| Failure::Usage("--out must name a directory, not a filesystem root".into()))?
        .to_string_lossy()
        .into_owned();
    fs::create_dir_all(parent)
        .map_err(|error| Failure::Usage(format!("cannot create {}: {error}", parent.display())))?;
    // Early answer for the common case; the decision that counts is made again under the lock.
    if fs::symlink_metadata(destination).is_ok() {
        judge_existing_documentation(destination, force)?;
    }
    for relative in bundle.keys() {
        let relative = Path::new(relative);
        if relative.is_absolute()
            || relative
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            return Err(Failure::Usage(format!(
                "generator produced unsafe relative path {}",
                relative.display()
            )));
        }
    }
    let stage = parent.join(format!(".{name}.generating.{}", std::process::id()));
    fs::create_dir(&stage).map_err(|error| {
        Failure::Usage(format!(
            "cannot create staging directory {}: {error}",
            stage.display()
        ))
    })?;
    let result = write_bundle(&stage, bundle).and_then(|()| {
        before_publish();
        publish_documentation(&stage, destination, parent, &name, force)
    });
    if result.is_err() {
        // The stage was created above by this process, so removing it removes nobody else's work.
        let _ = fs::remove_dir_all(&stage);
    }
    result
}

fn write_bundle(stage: &Path, bundle: &entity_surface::DocumentationBundle) -> Result<(), Failure> {
    for (relative, contents) in bundle {
        let path = stage.join(relative);
        if let Some(directory) = path.parent() {
            fs::create_dir_all(directory).map_err(|error| {
                Failure::Usage(format!("cannot create {}: {error}", directory.display()))
            })?;
        }
        fs::write(&path, contents)
            .map_err(|error| Failure::Usage(format!("cannot write {}: {error}", path.display())))?;
    }
    Ok(())
}

/// Whether an existing `destination` may be replaced.
fn judge_existing_documentation(destination: &Path, force: bool) -> Result<(), Failure> {
    if !force {
        return Err(Failure::Usage(format!(
            "{} already exists; pass --force to replace a generated directory",
            destination.display()
        )));
    }
    if !destination.join(entity_surface::DOCS_MARKER).is_file() {
        return Err(Failure::Usage(format!(
            "{} is not marked as entity-runtime generated documentation and will not be replaced",
            destination.display()
        )));
    }
    Ok(())
}

fn publish_documentation(
    stage: &Path,
    destination: &Path,
    parent: &Path,
    name: &str,
    force: bool,
) -> Result<(), Failure> {
    let _lock = DestinationLock::acquire(parent, name)?;
    match fs::symlink_metadata(destination) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            publish_new(stage, destination, force)
        }
        Err(error) => Err(Failure::Usage(format!(
            "cannot inspect {}: {error}",
            destination.display()
        ))),
        Ok(_) => {
            judge_existing_documentation(destination, force)?;
            let backup = parent.join(format!(".{name}.replaced.{}", std::process::id()));
            rename_noreplace(destination, &backup).map_err(|error| {
                Failure::Usage(format!(
                    "cannot stage replacement of {}: {error}",
                    destination.display()
                ))
            })?;
            // Judge the directory now held aside, not the path: a writer outside the lock may have
            // swapped it after the check above.
            if !backup.join(entity_surface::DOCS_MARKER).is_file() {
                return Err(restore(
                    &backup,
                    destination,
                    format!(
                        "{} is not marked as entity-runtime generated documentation and will not be replaced",
                        destination.display()
                    ),
                ));
            }
            if let Err(error) = rename_noreplace(stage, destination) {
                return Err(restore(
                    &backup,
                    destination,
                    format!("cannot publish {}: {error}", destination.display()),
                ));
            }
            fs::remove_dir_all(&backup).map_err(|error| {
                Failure::Usage(format!(
                    "published {}, but cannot remove generated backup {}: {error}",
                    destination.display(),
                    backup.display()
                ))
            })
        }
    }
}

fn publish_new(stage: &Path, destination: &Path, force: bool) -> Result<(), Failure> {
    rename_noreplace(stage, destination).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            // Created by a writer that does not take the lock, between the check and the rename.
            judge_existing_documentation(destination, force)
                .err()
                .unwrap_or_else(|| {
                    Failure::Usage(format!(
                        "{} appeared while generating; nothing was replaced",
                        destination.display()
                    ))
                })
        } else {
            Failure::Usage(format!("cannot publish {}: {error}", destination.display()))
        }
    })
}

/// Put the directory held aside at `backup` back at `destination`, and say where it is when that
/// is impossible — nothing is removed on this path.
fn restore(backup: &Path, destination: &Path, reason: String) -> Failure {
    match rename_noreplace(backup, destination) {
        Ok(()) => Failure::Usage(reason),
        Err(error) => Failure::Usage(format!(
            "{reason}; the previous {} is kept at {} because it cannot be moved back: {error}",
            destination.display(),
            backup.display()
        )),
    }
}

/// An exclusive claim on publishing `<parent>/<name>`: a lock file created with `create_new`
/// beside the destination and removed when the claim is dropped.
struct DestinationLock {
    path: PathBuf,
}

impl DestinationLock {
    fn acquire(parent: &Path, name: &str) -> Result<Self, Failure> {
        let path = parent.join(format!(".{name}.lock"));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                if error.kind() == io::ErrorKind::AlreadyExists {
                    Failure::Usage(format!(
                        "another generation is publishing {}: {} exists; remove it if no generation is running",
                        parent.join(name).display(),
                        path.display()
                    ))
                } else {
                    Failure::Usage(format!("cannot create lock {}: {error}", path.display()))
                }
            })?;
        let _ = io::Write::write_all(&mut file, format!("{}\n", std::process::id()).as_bytes());
        Ok(Self { path })
    }
}

impl Drop for DestinationLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Rename `from` to `to`, failing with [`io::ErrorKind::AlreadyExists`] instead of replacing
/// whatever is at `to`.
fn rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        use rustix::{
            fs::{renameat_with, RenameFlags, CWD},
            io::Errno,
        };
        match renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE) {
            Ok(()) => return Ok(()),
            // A filesystem or kernel without RENAME_NOREPLACE: fall back to checking first.
            Err(errno) if errno == Errno::INVAL || errno == Errno::NOSYS => {}
            Err(errno) => return Err(io::Error::from(errno)),
        }
    }
    if fs::symlink_metadata(to).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} already exists", to.display()),
        ));
    }
    fs::rename(from, to)
}

/// The marker a retained Rust CLI crate carries: the runtime version that generated it and, once
/// it has installed one, the binary it installed — the only file `--force` may replace at `--out`.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliMarker {
    format: String,
    runtime: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    installed: Option<Installed>,
}

/// A binary the generator installed: where, and a fingerprint of its bytes.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub(crate) struct Installed {
    path: String,
    length: u64,
    fnv1a64: String,
}

/// The marker text for this runtime version, recording `installed` when there is one.
pub(crate) fn cli_marker_text(installed: Option<Installed>) -> String {
    let marker = CliMarker {
        format: "entity-runtime-cli/1".to_owned(),
        runtime: env!("CARGO_PKG_VERSION").to_owned(),
        installed,
    };
    serde_json::to_string(&marker).expect("the marker serializes") + "\n"
}

fn read_cli_marker(build_dir: &Path) -> Option<CliMarker> {
    let text = fs::read_to_string(build_dir.join(GENERATED_CLI_MARKER)).ok()?;
    serde_json::from_str(&text).ok()
}

/// The binary the generator last installed from `build_dir`, as its marker records it.
pub(crate) fn previous_installation(build_dir: &Path) -> Option<Installed> {
    read_cli_marker(build_dir).and_then(|marker| marker.installed)
}

/// Length and FNV-1a 64 of the bytes at `path`. Not a defence against a forger — whoever can write
/// the build directory can write its marker — but it tells the binary the generator installed from
/// a file that was put at `--out` some other way.
fn fingerprint(path: &Path) -> io::Result<(u64, String)> {
    let mut file = fs::File::open(path)?;
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut length = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = io::Read::read(&mut file, &mut buffer)?;
        if read == 0 {
            break;
        }
        length += read as u64;
        for byte in &buffer[..read] {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    Ok((length, format!("{hash:016x}")))
}

fn record(destination: &Path, contents: &Path) -> io::Result<Installed> {
    let (length, fnv1a64) = fingerprint(contents)?;
    Ok(Installed {
        path: destination.to_string_lossy().into_owned(),
        length,
        fnv1a64,
    })
}

/// Refuse unless the file whose bytes are at `contents` is the binary `previous` records as
/// installed at `destination`.
fn owned(destination: &Path, contents: &Path, previous: Option<&Installed>) -> Result<(), Failure> {
    let refusal = || {
        Failure::Usage(format!(
            "{} exists and is not the binary this generator installed from the build directory; \
             remove it or choose another --out",
            destination.display()
        ))
    };
    let previous = previous.ok_or_else(refusal)?;
    let found = record(destination, contents).map_err(|error| {
        Failure::Usage(format!("cannot inspect {}: {error}", destination.display()))
    })?;
    if &found == previous {
        Ok(())
    } else {
        Err(refusal())
    }
}

/// Refuse to replace an existing `destination` (an absolute path) that the build directory's
/// marker does not record as installed by this generator. Checked before the build so a refusal
/// costs nothing; [`install_binary`] checks again on the file it moves aside.
pub(crate) fn check_output_owned(
    destination: &Path,
    previous: Option<&Installed>,
) -> Result<(), Failure> {
    owned(destination, destination, previous)
}

/// Install `built` at `destination` (an absolute path) and record it in `build_dir`'s marker.
///
/// An existing `destination` is replaced only when it is the binary `previous` records; it is
/// moved aside first and its bytes are checked there, so a file put in its place after
/// [`check_output_owned`] is moved back, not removed.
pub(crate) fn install_binary(
    built: &Path,
    destination: &Path,
    build_dir: &Path,
    previous: Option<&Installed>,
) -> Result<(), Failure> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let name = destination
        .file_name()
        .ok_or_else(|| Failure::Usage("--out must name a file".into()))?
        .to_string_lossy()
        .into_owned();
    fs::create_dir_all(parent)
        .map_err(|error| Failure::Usage(format!("cannot create {}: {error}", parent.display())))?;
    let temporary = parent.join(format!(".{name}.installing.{}", std::process::id()));
    let result = stage_binary(built, &temporary)
        .and_then(|()| place_binary(&temporary, destination, parent, &name, previous))
        .and_then(|()| {
            record(destination, destination)
                .map_err(|error| error.to_string())
                .and_then(|installed| {
                    fs::write(
                        build_dir.join(GENERATED_CLI_MARKER),
                        cli_marker_text(Some(installed)),
                    )
                    .map_err(|error| error.to_string())
                })
                .map_err(|error| {
                    Failure::Usage(format!(
                        "installed {}, but cannot record it in {}: {error}",
                        destination.display(),
                        build_dir.join(GENERATED_CLI_MARKER).display()
                    ))
                })
        });
    if result.is_err() {
        // Reserved with `create_new` by this process: removing it removes nobody else's file.
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn stage_binary(built: &Path, temporary: &Path) -> Result<(), Failure> {
    let mut source = fs::File::open(built).map_err(|error| {
        Failure::Usage(format!(
            "cannot open built binary {}: {error}",
            built.display()
        ))
    })?;
    let mut staged = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)
        .map_err(|error| {
            Failure::Usage(format!(
                "cannot reserve installation path {}: {error}",
                temporary.display()
            ))
        })?;
    io::copy(&mut source, &mut staged)
        .map_err(|error| Failure::Usage(format!("cannot stage {}: {error}", built.display())))?;
    drop(staged);
    let permissions = fs::metadata(built)
        .map_err(|error| Failure::Usage(format!("cannot inspect {}: {error}", built.display())))?
        .permissions();
    fs::set_permissions(temporary, permissions)
        .map_err(|error| Failure::Usage(format!("cannot preserve binary permissions: {error}")))
}

fn place_binary(
    temporary: &Path,
    destination: &Path,
    parent: &Path,
    name: &str,
    previous: Option<&Installed>,
) -> Result<(), Failure> {
    if fs::symlink_metadata(destination).is_err() {
        return rename_noreplace(temporary, destination).map_err(|error| {
            Failure::Usage(format!("cannot install {}: {error}", destination.display()))
        });
    }
    let backup = parent.join(format!(".{name}.replaced.{}", std::process::id()));
    rename_noreplace(destination, &backup).map_err(|error| {
        Failure::Usage(format!(
            "cannot stage replacement of {}: {error}",
            destination.display()
        ))
    })?;
    if let Err(refusal) = owned(destination, &backup, previous) {
        let reason = match refusal {
            Failure::Usage(reason) => reason,
            _ => format!("{} will not be replaced", destination.display()),
        };
        return Err(restore(&backup, destination, reason));
    }
    if let Err(error) = rename_noreplace(temporary, destination) {
        return Err(restore(
            &backup,
            destination,
            format!("cannot install {}: {error}", destination.display()),
        ));
    }
    fs::remove_file(&backup).map_err(|error| {
        Failure::Usage(format!(
            "installed {}, but cannot remove backup {}: {error}",
            destination.display(),
            backup.display()
        ))
    })
}

/// The file [`cli_marker_text`] is written to.
pub(crate) const GENERATED_CLI_MARKER: &str = ".entity-runtime-cli.json";

/// How the retained crate gets its `Cargo.lock`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Lockfile {
    /// A lockfile the same runtime version wrote is kept; only the workspace's own entries are
    /// refreshed, so the dependency resolution and the compiled artifacts stay reusable.
    Reuse,
    /// There is none to reuse: resolve from scratch.
    Generate,
}

impl Lockfile {
    /// The offline Cargo invocation that produces the lockfile the build then runs `--locked` on.
    pub(crate) fn cargo_arguments(&self) -> &'static [&'static str] {
        match self {
            Lockfile::Reuse => &["update", "--offline", "--workspace"],
            Lockfile::Generate => &["generate-lockfile", "--offline"],
        }
    }
}

/// Make `build_dir` ready to receive freshly generated sources.
///
/// An existing directory is touched only with `force` and when it carries
/// [`GENERATED_CLI_MARKER`]. Its generated sources are removed; `target/` — Cargo's cache — is
/// kept, and so is `Cargo.lock` when the marker names this runtime version. The marker itself is
/// left for the generator to overwrite, so an interrupted run leaves a directory the next forced
/// run may still replace.
pub(crate) fn prepare_build_dir(build_dir: &Path, force: bool) -> Result<Lockfile, Failure> {
    if !build_dir.exists() {
        return Ok(Lockfile::Generate);
    }
    let marker = build_dir.join(GENERATED_CLI_MARKER);
    if !force || !marker.is_file() {
        return Err(Failure::Usage(format!(
            "{} already exists and is not authorised for replacement",
            build_dir.display()
        )));
    }
    let same_runtime = read_cli_marker(build_dir)
        .is_some_and(|marker| marker.runtime == env!("CARGO_PKG_VERSION"));
    let replace = |error: io::Error| {
        Failure::Usage(format!("cannot replace {}: {error}", build_dir.display()))
    };
    for entry in fs::read_dir(build_dir).map_err(replace)? {
        let entry = entry.map_err(replace)?;
        let name = entry.file_name();
        if name == "target"
            || name == GENERATED_CLI_MARKER
            || (same_runtime && name == "Cargo.lock")
        {
            continue;
        }
        let path = entry.path();
        if entry.file_type().map_err(replace)?.is_dir() {
            fs::remove_dir_all(&path).map_err(replace)?;
        } else {
            fs::remove_file(&path).map_err(replace)?;
        }
    }
    Ok(if same_runtime && build_dir.join("Cargo.lock").is_file() {
        Lockfile::Reuse
    } else {
        Lockfile::Generate
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// A fresh directory under the test binary's target directory, never the system temp dir.
    fn scratch(test: &str) -> PathBuf {
        let executable = std::env::current_exe().expect("test executable path");
        let root = executable
            .parent()
            .expect("deps directory")
            .join("generated-output-tests")
            .join(test);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("scratch root");
        root
    }

    fn bundle() -> entity_surface::DocumentationBundle {
        BTreeMap::from([
            (
                entity_surface::DOCS_MARKER.to_owned(),
                "{\"format\":\"entity-runtime-docs/1\"}\n".to_owned(),
            ),
            ("index.md".to_owned(), "# generated\n".to_owned()),
        ])
    }

    fn succeeds<T>(result: Result<T, Failure>, what: &str) -> T {
        match result {
            Ok(value) => value,
            Err(Failure::Usage(message)) => panic!("{what}: {message}"),
            Err(_) => panic!("{what}: unexpected failure"),
        }
    }

    fn usage(result: Result<(), Failure>) -> String {
        match result {
            Err(Failure::Usage(message)) => message,
            Err(_) => panic!("expected a usage failure"),
            Ok(()) => panic!("expected a refusal"),
        }
    }

    fn leftovers(root: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(root)
            .expect("scratch root")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|name| name.starts_with('.'))
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_directory_created_while_documentation_is_staged_survives_ordinary_generation() {
        let root = scratch("docs-race-ordinary");
        let destination = root.join("reference");
        let message = usage(install_documentation_with(
            &destination,
            &bundle(),
            false,
            || {
                fs::create_dir(&destination).expect("the other writer's directory");
                fs::write(destination.join("keep"), "operator data")
                    .expect("the other writer's file");
            },
        ));
        assert!(message.contains("already exists"), "{message}");
        assert_eq!(
            fs::read_to_string(destination.join("keep")).expect("the other writer's file survives"),
            "operator data"
        );
        assert!(!destination.join(entity_surface::DOCS_MARKER).exists());
        assert_eq!(
            leftovers(&root),
            Vec::<String>::new(),
            "stage, backup and lock are gone"
        );
    }

    #[test]
    fn an_empty_directory_created_while_documentation_is_staged_is_not_replaced() {
        let root = scratch("docs-race-empty");
        let destination = root.join("reference");
        let message = usage(install_documentation_with(
            &destination,
            &bundle(),
            false,
            || {
                fs::create_dir(&destination).expect("the other writer's directory");
            },
        ));
        assert!(message.contains("already exists"), "{message}");
        assert_eq!(fs::read_dir(&destination).expect("still there").count(), 0);
    }

    #[test]
    fn an_unmarked_directory_created_while_staging_survives_forced_generation() {
        let root = scratch("docs-race-forced");
        let destination = root.join("reference");
        let message = usage(install_documentation_with(
            &destination,
            &bundle(),
            true,
            || {
                fs::create_dir(&destination).expect("the other writer's directory");
                fs::write(destination.join("keep"), "operator data")
                    .expect("the other writer's file");
            },
        ));
        assert!(message.contains("is not marked"), "{message}");
        assert_eq!(
            fs::read_to_string(destination.join("keep")).expect("the other writer's file survives"),
            "operator data"
        );
        assert_eq!(
            leftovers(&root),
            Vec::<String>::new(),
            "stage, backup and lock are gone"
        );
    }

    #[test]
    fn a_marked_directory_whose_marker_is_removed_while_staging_is_not_replaced() {
        let root = scratch("docs-race-unmarked");
        let destination = root.join("reference");
        succeeds(
            install_documentation(&destination, &bundle(), false),
            "first generation",
        );
        let message = usage(install_documentation_with(
            &destination,
            &bundle(),
            true,
            || {
                fs::remove_file(destination.join(entity_surface::DOCS_MARKER)).expect("unmark");
                fs::write(destination.join("keep"), "operator data").expect("operator file");
            },
        ));
        assert!(message.contains("is not marked"), "{message}");
        assert_eq!(
            fs::read_to_string(destination.join("keep")).expect("operator file survives"),
            "operator data"
        );
    }

    #[test]
    fn forced_generation_replaces_a_marked_directory_and_leaves_nothing_beside_it() {
        let root = scratch("docs-forced-replace");
        let destination = root.join("reference");
        succeeds(
            install_documentation(&destination, &bundle(), false),
            "first generation",
        );
        fs::write(destination.join("stale.md"), "old").expect("stale page");
        succeeds(
            install_documentation(&destination, &bundle(), true),
            "forced generation",
        );
        assert!(
            !destination.join("stale.md").exists(),
            "the old directory was replaced"
        );
        assert!(destination.join("index.md").is_file());
        assert_eq!(
            leftovers(&root),
            Vec::<String>::new(),
            "stage, backup and lock are gone"
        );
    }

    #[test]
    fn a_publication_in_progress_holds_off_a_second_forced_generation() {
        let root = scratch("docs-locked");
        let destination = root.join("reference");
        succeeds(
            install_documentation(&destination, &bundle(), false),
            "first generation",
        );
        let held = succeeds(DestinationLock::acquire(&root, "reference"), "first claim");
        let message = usage(install_documentation(&destination, &bundle(), true));
        assert!(
            message.contains("another generation is publishing"),
            "{message}"
        );
        assert!(destination.join(entity_surface::DOCS_MARKER).is_file());
        drop(held);
        succeeds(
            install_documentation(&destination, &bundle(), true),
            "after the claim is released",
        );
    }

    fn generated_build_dir(test: &str, marker: &str) -> PathBuf {
        let build_dir = scratch(test).join("refundctl");
        fs::create_dir_all(build_dir.join("src")).expect("src");
        fs::create_dir_all(build_dir.join("definitions")).expect("definitions");
        fs::create_dir_all(build_dir.join("target/release/deps")).expect("target");
        fs::write(build_dir.join(GENERATED_CLI_MARKER), marker).expect("marker");
        fs::write(build_dir.join("src/main.rs"), "fn main() {}\n").expect("source");
        fs::write(build_dir.join("definitions/0.yaml"), "entity: refund\n").expect("definition");
        fs::write(build_dir.join("Cargo.toml"), "[package]\n").expect("manifest");
        fs::write(build_dir.join("Cargo.lock"), "version = 4\n").expect("lockfile");
        fs::write(
            build_dir.join("target/release/deps/libserde.rlib"),
            "cached",
        )
        .expect("cache");
        build_dir
    }

    #[test]
    fn forced_regeneration_keeps_the_cargo_cache_and_the_lockfile_of_the_same_runtime() {
        let build_dir = generated_build_dir("rust-cli-same-runtime", &cli_marker_text(None));
        let lockfile = succeeds(
            prepare_build_dir(&build_dir, true),
            "authorised replacement",
        );
        assert_eq!(lockfile, Lockfile::Reuse);
        assert_eq!(
            fs::read_to_string(build_dir.join("target/release/deps/libserde.rlib"))
                .expect("compiled artifacts survive"),
            "cached"
        );
        assert_eq!(
            fs::read_to_string(build_dir.join("Cargo.lock")).expect("lockfile survives"),
            "version = 4\n"
        );
        for generated in ["src", "definitions", "Cargo.toml"] {
            assert!(
                !build_dir.join(generated).exists(),
                "{generated} is generated again, not kept"
            );
        }
        assert_eq!(
            lockfile.cargo_arguments(),
            ["update", "--offline", "--workspace"],
            "a kept lockfile is refreshed for the workspace only, not resolved from scratch"
        );
    }

    #[test]
    fn a_lockfile_written_for_another_runtime_version_is_resolved_again() {
        let build_dir = generated_build_dir(
            "rust-cli-other-runtime",
            "{\"format\":\"entity-runtime-cli/1\",\"runtime\":\"0.0.1\"}\n",
        );
        let lockfile = succeeds(
            prepare_build_dir(&build_dir, true),
            "authorised replacement",
        );
        assert_eq!(lockfile, Lockfile::Generate);
        assert!(!build_dir.join("Cargo.lock").exists());
        assert!(build_dir
            .join("target/release/deps/libserde.rlib")
            .is_file());
        assert_eq!(
            lockfile.cargo_arguments(),
            ["generate-lockfile", "--offline"]
        );
    }

    #[test]
    fn a_build_directory_without_the_marker_is_not_touched_even_with_force() {
        let build_dir = scratch("rust-cli-unmarked").join("refundctl");
        fs::create_dir_all(&build_dir).expect("build dir");
        fs::write(build_dir.join("keep"), "operator data").expect("operator file");
        let message = match prepare_build_dir(&build_dir, true) {
            Err(Failure::Usage(message)) => message,
            _ => panic!("expected a refusal"),
        };
        assert!(
            message.contains("not authorised for replacement"),
            "{message}"
        );
        assert_eq!(
            fs::read_to_string(build_dir.join("keep")).expect("untouched"),
            "operator data"
        );
    }

    /// A marked build directory, a built binary with `contents`, and an output path, all under one
    /// scratch root; no Cargo build.
    fn installation(test: &str, contents: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = scratch(test);
        let build_dir = root.join("build");
        fs::create_dir_all(&build_dir).expect("build dir");
        fs::write(build_dir.join(GENERATED_CLI_MARKER), cli_marker_text(None)).expect("marker");
        let built = root.join("built-binary");
        fs::write(&built, contents).expect("built binary");
        (build_dir, built, root.join("bin").join("refundctl"))
    }

    #[test]
    fn an_unrelated_file_at_the_output_is_not_replaced_even_with_force() {
        let (build_dir, built, destination) = installation("rust-cli-unrelated-output", "binary 1");
        fs::create_dir_all(destination.parent().expect("bin")).expect("bin dir");
        fs::write(&destination, "operator data").expect("unrelated file");
        let previous = previous_installation(&build_dir);
        let early = usage(check_output_owned(&destination, previous.as_ref()));
        assert!(
            early.contains("is not the binary this generator installed"),
            "{early}"
        );
        let late = usage(install_binary(
            &built,
            &destination,
            &build_dir,
            previous.as_ref(),
        ));
        assert!(
            late.contains("is not the binary this generator installed"),
            "{late}"
        );
        assert_eq!(
            fs::read_to_string(&destination).expect("unrelated file survives"),
            "operator data"
        );
        assert_eq!(
            leftovers(destination.parent().expect("bin")),
            Vec::<String>::new(),
            "no staged binary or backup is left beside the output"
        );
    }

    #[test]
    fn a_binary_this_generator_installed_is_replaced_and_the_new_one_recorded() {
        let (build_dir, built, destination) = installation("rust-cli-owned-output", "binary 1");
        succeeds(
            install_binary(&built, &destination, &build_dir, None),
            "first installation",
        );
        let previous = previous_installation(&build_dir);
        assert!(previous.is_some(), "the installation is recorded");
        succeeds(
            check_output_owned(&destination, previous.as_ref()),
            "the recorded binary is owned",
        );
        fs::write(&built, "binary 2").expect("rebuilt binary");
        succeeds(
            install_binary(&built, &destination, &build_dir, previous.as_ref()),
            "forced reinstallation",
        );
        assert_eq!(
            fs::read_to_string(&destination).expect("installed"),
            "binary 2"
        );
        assert_ne!(
            previous_installation(&build_dir),
            previous,
            "the record follows the binary"
        );
    }

    #[test]
    fn a_binary_changed_since_it_was_installed_is_not_replaced() {
        let (build_dir, built, destination) = installation("rust-cli-changed-output", "binary 1");
        succeeds(
            install_binary(&built, &destination, &build_dir, None),
            "first installation",
        );
        let previous = previous_installation(&build_dir);
        fs::write(&destination, "operator data").expect("someone else's file");
        let message = usage(install_binary(
            &built,
            &destination,
            &build_dir,
            previous.as_ref(),
        ));
        assert!(
            message.contains("is not the binary this generator installed"),
            "{message}"
        );
        assert_eq!(
            fs::read_to_string(&destination).expect("their file survives"),
            "operator data"
        );
    }

    #[test]
    fn an_installation_recorded_for_another_output_does_not_authorise_this_one() {
        let (build_dir, built, destination) = installation("rust-cli-other-output", "binary 1");
        succeeds(
            install_binary(&built, &destination, &build_dir, None),
            "first installation",
        );
        let previous = previous_installation(&build_dir);
        let elsewhere = destination.with_file_name("other");
        fs::copy(&destination, &elsewhere).expect("same bytes, another path");
        let message = usage(check_output_owned(&elsewhere, previous.as_ref()));
        assert!(
            message.contains("is not the binary this generator installed"),
            "{message}"
        );
    }
}
