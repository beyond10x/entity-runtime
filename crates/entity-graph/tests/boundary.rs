//! What this crate is allowed to reach, asserted rather than agreed.
//!
//! Both checks read this crate's own text. That is the same mechanism `entity-core`'s purity scan
//! uses (R-01) and it is here for the same reason: a boundary nothing enforces is a boundary the
//! next change crosses without noticing, and a renderer that could read a clock or a file could
//! draw something the definition does not say. A drawing has to be reproducible from the definition
//! alone or it is not evidence of anything.

use std::fs;
use std::path::{Path, PathBuf};

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn sources() -> Vec<(PathBuf, String)> {
    fn walk(directory: &Path, found: &mut Vec<(PathBuf, String)>) {
        for entry in fs::read_dir(directory).expect("a readable directory") {
            let path = entry.expect("a readable entry").path();
            if path.is_dir() {
                walk(&path, found);
            } else if path.extension().is_some_and(|kind| kind == "rs") {
                found.push((path.clone(), fs::read_to_string(&path).expect("readable")));
            }
        }
    }
    let mut found = Vec::new();
    walk(&crate_root().join("src"), &mut found);
    assert!(!found.is_empty(), "the scan found no sources to scan");
    found
}

/// One dependency, and it is the kernel. Read from the manifest, because that is the file that
/// decides — a test that asserted this by listing `use` statements would pass while a dependency
/// nobody imported yet sat in the build.
#[test]
fn the_renderer_depends_on_the_kernel_and_nothing_else() {
    let manifest = fs::read_to_string(crate_root().join("Cargo.toml")).expect("readable");
    let dependencies = manifest
        .split("[dependencies]")
        .nth(1)
        .expect("the manifest declares dependencies")
        .split("\n[")
        .next()
        .expect("a section ends");

    let named: Vec<&str> = dependencies
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split(['=', ' ', '.']).next())
        .filter(|name| !name.is_empty())
        .collect();

    assert_eq!(
        named,
        vec!["entity-core"],
        "this crate takes one dependency; anything else needs an argument in the manifest first"
    );
}

/// The same list `entity-core` bans, plus floating point.
///
/// Floats are here and not there because this crate does arithmetic and that one does not lay
/// anything out: two machines that round differently would produce two drawings of one definition,
/// which is the failure this whole crate is arranged to avoid. Every coordinate is a `usize`.
#[test]
fn the_renderer_reaches_no_clock_filesystem_network_random_source_or_float() {
    const BANNED: &[&str] = &[
        "std::fs",
        "std::net",
        "std::time",
        "SystemTime",
        "Instant",
        "std::env",
        "std::process",
        "rand::",
        "HashMap",
        "HashSet",
        "f32",
        "f64",
        "tokio",
        "async",
        "include_str!",
        "include_bytes!",
    ];

    let mut findings = Vec::new();
    for (path, text) in sources() {
        let stripped = strip(&text);
        for banned in BANNED {
            if stripped.contains(banned) {
                findings.push(format!("{}: {banned}", path.display()));
            }
        }
    }
    assert!(findings.is_empty(), "{findings:?}");
}

/// Comments and string literals removed, so a *word* in prose is not a finding and a word in code
/// is. Without this the module doc above — which names every banned token — would fail the scan it
/// describes.
fn strip(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '/' if characters.peek() == Some(&'/') => {
                for next in characters.by_ref() {
                    if next == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '"' => {
                let mut escaped = false;
                for next in characters.by_ref() {
                    if escaped {
                        escaped = false;
                    } else if next == '\\' {
                        escaped = true;
                    } else if next == '"' {
                        break;
                    }
                }
                out.push_str("\"\"");
            }
            other => out.push(other),
        }
    }
    out
}

/// The scan has to see a planted violation, or it is decoration. Two plantings and two lookalikes,
/// on the same reasoning as `entity-core`'s.
#[test]
fn the_scan_sees_a_planted_violation_and_not_a_lookalike() {
    let planted = "fn read() { let _ = std::fs::read_to_string(\"x\"); }";
    assert!(strip(planted).contains("std::fs"), "a real call is seen");

    let planted = "let ratio: f64 = 1.0;";
    assert!(strip(planted).contains("f64"), "a real float is seen");

    let prose = "// this crate never touches std::fs or f64\n";
    assert!(!strip(prose).contains("std::fs"), "a comment is not a call");
    assert!(!strip(prose).contains("f64"), "a comment is not a float");

    let literal = "let message = \"std::fs is banned here\";";
    assert!(
        !strip(literal).contains("std::fs"),
        "a string literal is not a call"
    );
}
