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

/// One dependency, and it is the kernel.
///
/// Read with `scan_support::dependencies`, which sees `[dependencies.foo]` and
/// `[target.'cfg(unix)'.dependencies]` as well as the literal heading. The hand-rolled version this
/// replaces split on the literal `[dependencies]` string and saw neither: a real `tokio` added as
/// its own table compiled, and this test passed.
#[test]
fn the_renderer_depends_on_the_kernel_and_nothing_else() {
    let manifest = fs::read_to_string(crate_root().join("Cargo.toml")).expect("readable");
    let declared: Vec<String> = scan_support::dependencies(&manifest).into_iter().collect();
    assert_eq!(
        declared,
        vec!["entity-core".to_owned()],
        "this crate takes one dependency; anything else needs an argument in the manifest first"
    );
}

/// The same list `entity-core` bans, plus floating point.
///
/// Floats are here and not there because this crate does arithmetic and that one lays nothing out:
/// two machines that round differently would draw one definition two ways, which is the failure
/// this crate is arranged to avoid. Every coordinate is a `usize`.
#[test]
fn the_renderer_reaches_no_clock_filesystem_network_random_source_or_float() {
    const BANNED: &[&str] = &[
        "fs",
        "net",
        "SystemTime",
        "Instant",
        "env",
        "process",
        "rand",
        "HashMap",
        "HashSet",
        "f32",
        "f64",
        "tokio",
        "include_str",
        "include_bytes",
    ];

    let mut findings = Vec::new();
    for (path, text) in sources() {
        let code = scan_support::code_only(&text);
        for (line, word) in scan_support::words(&code) {
            if BANNED.contains(&word) {
                findings.push(format!("{}:{line}: `{word}`", path.display()));
            }
        }
    }
    assert!(findings.is_empty(), "{findings:#?}");
}

/// The plantings an independent review used to show the previous scanner enforced nothing. They
/// live in `scan-support` too; they are repeated here because this crate's test is what a reader
/// of this crate will check.
#[test]
fn the_scan_sees_what_the_one_it_replaced_could_not() {
    let after_a_char_literal = "if c == '\"' { }\nlet _ = std::fs::read_to_string(\"x\");";
    assert!(
        scan_support::words(&scan_support::code_only(after_a_char_literal))
            .any(|(_, word)| word == "fs"),
        "a char literal holding a quote must not blank out the rest of the file"
    );

    let prose = "// this crate never touches std::fs or f64\n";
    assert!(
        !scan_support::words(&scan_support::code_only(prose)).any(|(_, w)| w == "fs" || w == "f64"),
        "a comment is not a call"
    );

    let manifest = "[dependencies]\nentity-core = { path = \"..\" }\n\n[dependencies.tokio]\nversion = \"1\"\n";
    assert!(
        scan_support::dependencies(manifest).contains("tokio"),
        "a dependency written as its own table is a dependency"
    );
}
