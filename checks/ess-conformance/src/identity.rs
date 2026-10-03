//! Shared build-time and run-time identity of the actual library and adapter sources.
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn source_identity(root: &Path, mut watch: impl FnMut(&Path)) -> Result<String> {
    fn collect(root: &Path, path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        for entry in fs::read_dir(path)? {
            let path = entry?.path();
            if path.is_dir() {
                collect(root, &path, files)?;
            } else if matches!(
                path.extension().and_then(|s| s.to_str()),
                Some("rs" | "toml" | "lock")
            ) {
                files.push(path.strip_prefix(root)?.to_owned());
            }
        }
        Ok(())
    }
    let mut files = vec![
        PathBuf::from("checks/ess-conformance/build.rs"),
        PathBuf::from("Cargo.toml"),
        PathBuf::from("Cargo.lock"),
        PathBuf::from("checks/ess-conformance/Cargo.toml"),
        PathBuf::from("checks/ess-conformance/Cargo.lock"),
    ];
    for name in ["core", "store", "executor", "shell", "query", "eventlog"] {
        collect(
            root,
            &root.join(format!("crates/entity-{name}/src")),
            &mut files,
        )?;
        files.push(PathBuf::from(format!("crates/entity-{name}/Cargo.toml")));
    }
    collect(root, &root.join("checks/ess-conformance/src"), &mut files)?;
    files.sort();
    let mut hash = Sha256::new();
    for path in files {
        watch(&root.join(&path));
        let bytes = fs::read(root.join(&path))?;
        hash.update(path.to_string_lossy().as_bytes());
        hash.update([0]);
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
    }
    Ok(format!("source-sha256:{:x}", hash.finalize()))
}
