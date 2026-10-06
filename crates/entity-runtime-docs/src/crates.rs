//! The crate list, from `cargo metadata --no-deps`: every workspace package, its directory, the
//! name Rust code uses for it, its Cargo features, its minimum Rust and its `description`.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

use crate::{cell, front_matter, Result, TREE};

/// Where the crate list lands, relative to the repository root.
pub const PAGE: &str = "website/docs/reference/crates.md";

/// One workspace package as the page lists it.
#[derive(Debug, PartialEq, Eq)]
struct Package {
    name: String,
    directory: String,
    /// The library name (`use <lib>`), when the package has a library.
    lib: Option<String>,
    /// The binaries it builds.
    bins: Vec<String>,
    /// Its Cargo features, without `default`.
    features: Vec<String>,
    /// Its declared minimum Rust version.
    rust: String,
    /// Whether the package may be published (`publish = false` marks repository tooling).
    publishable: bool,
    description: String,
}

/// The packages of a `cargo metadata --format-version 1` document, sorted by name, with their
/// directories relative to `workspace_root`. A package without a description is refused: the page
/// would have nothing to say about it.
fn packages(metadata: &Value) -> Result<Vec<Package>> {
    let root = metadata["workspace_root"]
        .as_str()
        .ok_or("cargo metadata names no workspace_root")?;
    let mut found = Vec::new();
    for package in metadata["packages"]
        .as_array()
        .ok_or("cargo metadata lists no packages")?
    {
        let name = package["name"]
            .as_str()
            .ok_or("a package has no name")?
            .to_owned();
        let manifest = package["manifest_path"]
            .as_str()
            .ok_or_else(|| format!("{name} has no manifest_path"))?;
        let directory = Path::new(manifest)
            .parent()
            .and_then(|dir| dir.strip_prefix(root).ok())
            .ok_or_else(|| format!("{name} lies outside the workspace"))?
            .to_string_lossy()
            .replace('\\', "/");
        let description = package["description"]
            .as_str()
            .ok_or_else(|| format!("{name} has no description in its Cargo.toml"))?;
        let mut lib = None;
        let mut bins = Vec::new();
        for target in package["targets"].as_array().into_iter().flatten() {
            let kinds: Vec<&str> = target["kind"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            let target_name = target["name"].as_str().unwrap_or_default().to_owned();
            if kinds.contains(&"lib") {
                lib = Some(target_name);
            } else if kinds.contains(&"bin") {
                bins.push(target_name);
            }
        }
        bins.sort();
        let mut features: Vec<String> = package["features"]
            .as_object()
            .into_iter()
            .flat_map(|features| features.keys().cloned())
            .filter(|feature| feature != "default")
            .collect();
        features.sort();
        // `publish = false` is an empty registry list; an absent key is `null`.
        let publishable = !package["publish"]
            .as_array()
            .is_some_and(|registries| registries.is_empty());
        found.push(Package {
            name,
            directory,
            lib,
            bins,
            features,
            rust: package["rust_version"].as_str().unwrap_or("any").to_owned(),
            publishable,
            description: description.trim().to_owned(),
        });
    }
    if found.is_empty() {
        return Err("cargo metadata lists no packages".to_owned());
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

fn table(packages: &[&Package]) -> String {
    let mut out = String::from(
        "| Package | In Rust | Features | Rust | What it is |\n|---|---|---|---|---|\n",
    );
    for package in packages {
        let mut uses = Vec::new();
        if let Some(lib) = &package.lib {
            uses.push(format!("`use {lib}`"));
        }
        for bin in &package.bins {
            uses.push(format!("binary `{bin}`"));
        }
        let features = if package.features.is_empty() {
            "none".to_owned()
        } else {
            package
                .features
                .iter()
                .map(|feature| format!("`{feature}`"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        out.push_str(&format!(
            "| [`{}`]({TREE}/{}) | {} | {features} | {} | {} |\n",
            package.name,
            package.directory,
            uses.join(", "),
            package.rust,
            cell(&package.description),
        ));
    }
    out
}

/// The crate list page for the packages of `metadata`.
fn page_for(metadata: &Value) -> Result<String> {
    let all = packages(metadata)?;
    let (components, tooling): (Vec<&Package>, Vec<&Package>) =
        all.iter().partition(|package| package.publishable);
    let mut out = front_matter(
        "Crates",
        "Crates",
        "Every package of the Entity Runtime workspace, generated from cargo metadata.",
        2,
    );
    out.push_str(&format!(
        "# Crates\n\nGenerated by `entity-runtime-docs` from `cargo metadata --no-deps`: every \
         package of the workspace, with the `description` its `Cargo.toml` gives. There are {} \
         packages, all at the workspace version. None is published to a registry; depend on them \
         by Git tag, as [the library guide](../guides/embed-the-kernel.md) shows. \
         **Rust** is the package's declared minimum.\n\n## Libraries and the command\n\n{}\n\
         ## Repository tooling\n\nGenerators and checks the repository's own gate runs \
         (`publish = false`). Nothing outside the repository depends on them.\n\n{}",
        all.len(),
        table(&components),
        table(&tooling),
    ));
    Ok(out)
}

/// The crate list page for the workspace at `root`, read with `cargo metadata`.
pub fn page(root: &Path) -> Result<String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(&cargo)
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(root.join("Cargo.toml"))
        .output()
        .map_err(|error| format!("running cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let metadata: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("parsing cargo metadata: {error}"))?;
    page_for(&metadata)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(description: Option<&str>) -> Value {
        serde_json::json!({
            "workspace_root": "/w",
            "packages": [
                {
                    "name": "entity-runtime-docs",
                    "manifest_path": "/w/crates/entity-runtime-docs/Cargo.toml",
                    "description": "Generates pages.",
                    "publish": [],
                    "rust_version": "1.85",
                    "features": {},
                    "targets": [{"kind": ["bin"], "name": "entity-runtime-docs"}]
                },
                {
                    "name": "entity-eventlog",
                    "manifest_path": "/w/crates/entity-eventlog/Cargo.toml",
                    "description": description,
                    "publish": null,
                    "rust_version": "1.91",
                    "features": {"default": [], "tree": [], "file": []},
                    "targets": [
                        {"kind": ["lib"], "name": "entity_eventlog"},
                        {"kind": ["test"], "name": "providers"}
                    ]
                }
            ]
        })
    }

    #[test]
    fn packages_are_sorted_split_and_linked() {
        let page = page_for(&metadata(Some("Recorded | storage."))).unwrap();
        assert!(page.contains("# generated by entity-runtime-docs, do not edit"));
        assert!(page.contains("There are 2 packages"));
        let libraries = page.find("## Libraries and the command").unwrap();
        let tooling = page.find("## Repository tooling").unwrap();
        let eventlog = page.find("| [`entity-eventlog`]").unwrap();
        let docs = page.find("| [`entity-runtime-docs`]").unwrap();
        assert!(libraries < eventlog && eventlog < tooling && tooling < docs);
        assert!(page.contains(
            "| [`entity-eventlog`](https://github.com/beyond10x/entity-runtime/tree/main/crates/entity-eventlog) | `use entity_eventlog` | `file`, `tree` | 1.91 | Recorded \\| storage. |"
        ));
        assert!(page.contains("| binary `entity-runtime-docs` | none | 1.85 |"));
    }

    #[test]
    fn a_package_without_a_description_is_refused() {
        let error = page_for(&metadata(None)).unwrap_err();
        assert!(
            error.contains("entity-eventlog has no description"),
            "{error}"
        );
    }
}
