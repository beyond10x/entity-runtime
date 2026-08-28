//! The version the binary reports and the version the changelog announces are the same number.
//!
//! 0.2.0 shipped five archives whose `entity --version` printed `0.1.0`: the changelog was cut, the
//! tag was written and the workspace version was never bumped. Nothing in the gate compared the
//! two, so five platforms' binaries went out claiming to be the release before them.

use std::{fs, path::Path};

/// The newest released heading in `CHANGELOG.md` — `## [0.2.1] - 2026-08-25` — ignoring
/// `## [Unreleased]`, which is where work sits before it has a number.
fn newest_released_version(changelog: &str) -> String {
    for line in changelog.lines() {
        let Some(rest) = line.strip_prefix("## [") else {
            continue;
        };
        let Some(version) = rest.split(']').next() else {
            continue;
        };
        if version != "Unreleased" {
            return version.to_owned();
        }
    }
    panic!("CHANGELOG.md carries no released version heading");
}

#[test]
fn the_binary_reports_the_version_the_changelog_announces() {
    let changelog =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../CHANGELOG.md"))
            .expect("CHANGELOG.md is readable");

    assert_eq!(
        env!("CARGO_PKG_VERSION"),
        newest_released_version(&changelog),
        "the crate version and the newest CHANGELOG.md heading disagree — bump the workspace \
         version in the same commit that cuts the section"
    );
}

#[test]
fn the_heading_reader_ignores_unreleased_and_finds_the_first_number() {
    let changelog = "# Changelog\n\n## [Unreleased]\n\nNothing yet.\n\n## [9.9.9] - 2026-01-01\n";
    assert_eq!(newest_released_version(changelog), "9.9.9");
}

#[test]
fn every_path_dependency_names_the_workspace_version() {
    // 0.12.0 was cut with the workspace at 0.12.0 and every `path = "../entity-core", version =
    // "0.11.0"` left as it was: the gate passed on a working tree whose manifests were bumped and
    // uncommitted, and the tag could not be consumed as a git dependency — `entity-store 0.12.0`
    // required `entity-core ^0.11.0`, which the same tree no longer had. This reads what the tag
    // will carry, not what the tree resolves to.
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let workspace = env!("CARGO_PKG_VERSION");
    let mut stale = Vec::new();
    for entry in fs::read_dir(&crates).expect("crates/ is readable") {
        let manifest = entry.expect("an entry").path().join("Cargo.toml");
        let Ok(text) = fs::read_to_string(&manifest) else {
            continue;
        };
        for line in text.lines() {
            if !line.contains("path = \"../") {
                continue;
            }
            let Some(version) = line
                .split("version = \"")
                .nth(1)
                .and_then(|rest| rest.split('"').next())
            else {
                continue;
            };
            if version != workspace {
                stale.push(format!("{}: {}", manifest.display(), line.trim()));
            }
        }
    }
    assert!(
        stale.is_empty(),
        "path dependencies not at the workspace version {workspace}:\n  {}",
        stale.join("\n  ")
    );
}
