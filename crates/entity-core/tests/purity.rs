//! The kernel is IO-free and deterministic, and this test is what makes that a fact rather than a
//! sentence in a README.
//!
//! It works on the crate's own sources with comments and string literals removed, so a token in a
//! doc comment (this file's included) is not a breach and a token in code cannot hide behind one.
//! Two scans run over what is left:
//!
//! 1. **imports** — every `use` path is expanded, so `use std::{fs, env};` is seen as `std::fs`
//!    and `std::env` rather than as a line containing neither; an alias (`use std::env::var as
//!    v;`) is caught at the import even though the call site says only `v(..)`;
//! 2. **identifiers** — banned words anywhere in the remaining code, so a fully-qualified
//!    `std::time::SystemTime::now()` is caught with no import at all.
//!
//! The dependency list is the other half of the guarantee and is pinned below: two serialisation
//! crates, and nothing that could reach anything.

use std::{collections::BTreeSet, fs, path::Path};

/// Words that may not appear in the kernel's code. Module segments are listed with their `::` so
/// `fs::read_to_string` is caught after any import form; bare names catch the rest.
const BANNED_WORDS: &[&str] = &[
    // clocks
    "SystemTime",
    "Instant",
    "time",
    "chrono",
    // filesystem, network, environment, processes, threads
    "fs",
    "io",
    "net",
    "os",
    "env",
    "process",
    "thread",
    "libc",
    "include_str",
    "include_bytes",
    "println",
    "print",
    "eprintln",
    "eprint",
    "dbg",
    // randomness
    "rand",
    "getrandom",
    "RandomState",
    "uuid",
    // asynchrony
    "tokio",
    "async",
    "await",
    // unordered iteration
    "HashMap",
    "HashSet",
];

/// Path segments no `use` in the kernel may name, whatever it renames them to.
const BANNED_SEGMENTS: &[&str] = &[
    "fs", "io", "net", "os", "env", "process", "thread", "time", "sync", "rand", "tokio", "chrono",
    "libc", "hash",
];

/// Replaces every comment and string literal with spaces, keeping line numbers intact.
///
/// This is what makes the scan honest in both directions: prose about `std::fs` is not a breach,
/// and `*slot = std::time::SystemTime::now();` is not skipped because a line-based filter mistook
use scan_support::code_only;

/// Splits code into identifier-ish words, so `std::fs::read_to_string` yields `std`, `fs`,
use scan_support::words;

/// Every path a `use` statement brings into scope, with `{}` groups expanded.
fn use_paths(code: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut rest = code;
    while let Some(start) = rest.find("use ") {
        let after = &rest[start + 4..];
        let Some(end) = after.find(';') else { break };
        expand(after[..end].trim(), String::new(), &mut paths);
        rest = &after[end..];
    }
    paths
}

fn expand(fragment: &str, prefix: String, out: &mut Vec<String>) {
    let fragment = fragment.trim();
    match fragment.find('{') {
        None => out.push(format!("{prefix}{fragment}")),
        Some(brace) => {
            let head = format!("{prefix}{}", &fragment[..brace]);
            let Some(close) = fragment.rfind('}') else {
                out.push(head);
                return;
            };
            let inner = &fragment[brace + 1..close];
            let mut depth = 0usize;
            let mut item = String::new();
            for character in inner.chars() {
                match character {
                    '{' => {
                        depth += 1;
                        item.push(character);
                    }
                    '}' => {
                        depth -= 1;
                        item.push(character);
                    }
                    ',' if depth == 0 => {
                        expand(&item, head.clone(), out);
                        item.clear();
                    }
                    _ => item.push(character),
                }
            }
            if !item.trim().is_empty() {
                expand(&item, head, out);
            }
        }
    }
}

fn sources(dir: &Path, out: &mut Vec<(String, String)>) {
    for entry in fs::read_dir(dir).expect("source directory is readable") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let text = fs::read_to_string(&path).expect("source file is readable");
            out.push((path.display().to_string(), text));
        }
    }
}

fn offences(text: &str) -> Vec<String> {
    let code = code_only(text);
    let mut found = Vec::new();

    for path in use_paths(&code) {
        for segment in path.split("::").map(|segment| {
            segment
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .next()
                .unwrap_or_default()
        }) {
            if BANNED_SEGMENTS.contains(&segment) {
                found.push(format!("import `{}` names `{segment}`", path.trim()));
            }
        }
    }

    for (line, word) in words(&code) {
        if BANNED_WORDS.contains(&word) {
            found.push(format!("line {line}: `{word}`"));
        }
    }
    found
}

#[test]
fn the_kernel_reaches_no_clock_filesystem_network_or_random_source() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&root, &mut files);
    assert!(
        files.len() >= 5,
        "expected the kernel's source files, found {}",
        files.len()
    );

    let mut report = Vec::new();
    for (path, text) in &files {
        for offence in offences(text) {
            report.push(format!("{path}: {offence}"));
        }
    }
    assert!(
        report.is_empty(),
        "the kernel must stay IO-free:\n{}",
        report.join("\n")
    );
}

#[test]
fn the_scan_sees_every_evasion_it_is_meant_to_see() {
    // A scan that has silently stopped seeing anything passes on everything. Each line here is one
    // way the previous line-and-substring scan could be walked past.
    for planted in [
        "let now = std::time::SystemTime::now();",
        "use std::{fs, env};",
        "use std::env::var as fetch;",
        "let s = fs::read_to_string(\"/etc/passwd\");",
        "let h = env::var(\"HOME\");",
        "std::io::stdin().read_line(&mut buffer);",
        "println!(\"{value}\");",
        "const DATA: &str = include_str!(\"/etc/hostname\");",
        "let seed = std::hash::RandomState::new();",
        "    *slot = std::time::SystemTime::now();",
        "let map: HashMap<u8, u8> = HashMap::new();",
        "let value = future.await;",
        "async fn reach() {}",
        "let socket = std::os::unix::net::UnixStream::connect(path);",
    ] {
        assert!(
            !offences(planted).is_empty(),
            "the scan would not have caught: {planted}"
        );
    }
}

#[test]
fn the_scan_does_not_fire_on_prose_or_lookalikes() {
    for innocent in [
        "// std::fs is only mentioned here",
        "/// A doc comment about SystemTime and HashMap.",
        "/* std::env::var\n   across lines */",
        "let message = \"cannot read std::fs\";",
        "Operand::Present(value) => resolved.push(value),", // contains `rand`
        "use std::collections::BTreeMap;",
        "use std::{fmt, slice};",
        "let formatted = format!(\"{path}.{name}\");",
    ] {
        assert!(
            offences(innocent).is_empty(),
            "the scan fired on: {innocent} -> {:?}",
            offences(innocent)
        );
    }
}

#[test]
fn the_kernel_depends_on_serialisation_and_nothing_else() {
    let manifest = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("manifest is readable");

    // Every dependency table, not just the literal `[dependencies]`: a
    // `[target.'cfg(unix)'.dependencies]` or a `[dependencies.foo]` section would otherwise be
    // invisible to this check.
    let mut declared = BTreeSet::new();
    let mut in_table = false;
    for line in manifest.lines() {
        let line = line.trim();
        if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let header = header.trim();
            if let Some(name) = header.strip_prefix("dependencies.") {
                declared.insert(name.to_owned());
                in_table = false;
                continue;
            }
            in_table = header == "dependencies"
                || header.ends_with(".dependencies")
                || header == "dev-dependencies"
                || header == "build-dependencies";
            continue;
        }
        if in_table && !line.is_empty() && !line.starts_with('#') {
            if let Some(name) = line.split(['.', ' ', '=']).next() {
                if !name.is_empty() {
                    declared.insert(name.to_owned());
                }
            }
        }
    }

    // Every table, dev-dependencies included, because a short list is worth more than a precise
    // one: three names a reader can hold beats a rule about which tables link.
    //
    // `scan-support` is this workspace's own, `publish = false`, and used by nothing but the source
    // scan above. It is here because that scanner was written twice and the second copy was weaker
    // — it read the `"` inside a char literal as opening a string, so everything after it was
    // invisible. One scanner, with the plantings beside it.
    let expected: BTreeSet<String> = ["scan-support", "serde", "serde_json"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    assert_eq!(declared, expected, "{manifest}");
}
