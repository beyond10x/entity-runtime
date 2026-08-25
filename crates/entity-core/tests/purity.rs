//! The kernel is IO-free and deterministic, and this test is what makes that a fact rather than
//! a sentence in a README: it scans every source file of `entity-core` for a token that would let
//! it read a clock, the filesystem, the network, the environment or a random source, or spawn a
//! task. A new dependency is caught by `Cargo.toml` (two crates, both serialisation); a new
//! `std` capability is caught here.

use std::{fs, path::Path};

const BANNED: &[&str] = &[
    "SystemTime",
    "Instant::",
    "std::time",
    "std::fs",
    "std::net",
    "std::env",
    "std::process",
    "std::thread",
    "tokio",
    "async fn",
    ".await",
    "rand::",
    "getrandom",
    "uuid",
    "HashMap",
    "HashSet",
];

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

/// Whether `line` contains `token` as its own word. `Operand::` contains `rand::`, and a scan that
/// did not know that would ban the kernel's own operand type; so a token that starts with an
/// identifier character must not be preceded by one.
fn contains_token(line: &str, token: &str) -> bool {
    let is_identifier = |character: char| character.is_ascii_alphanumeric() || character == '_';
    let needs_boundary = token.chars().next().is_some_and(is_identifier);
    line.match_indices(token).any(|(start, _)| {
        !needs_boundary || !line[..start].chars().next_back().is_some_and(is_identifier)
    })
}

/// Doc comments and line comments may *mention* a banned token — this file's own doc comment does
/// — so only code lines count. Doc tests in `lib.rs` are also comments and are skipped the same way.
fn code_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| {
            let trimmed = line.trim_start();
            !(trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*'))
        })
        .map(|(index, line)| (index + 1, line))
}

fn offences(text: &str) -> Vec<(usize, &'static str)> {
    code_lines(text)
        .flat_map(|(number, line)| {
            BANNED
                .iter()
                .filter(move |token| contains_token(line, token))
                .map(move |token| (number, *token))
        })
        .collect()
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
        for (line_number, token) in offences(text) {
            report.push(format!("{path}:{line_number}: `{token}`"));
        }
    }
    assert!(
        report.is_empty(),
        "the kernel must stay IO-free:\n{}",
        report.join("\n")
    );
}

#[test]
fn the_scan_would_notice_an_offence_and_ignores_comments_and_lookalikes() {
    // A scan that has silently stopped seeing anything passes on everything, so check it against
    // lines it must catch, a comment it must not, and a lookalike it must not.
    let planted = concat!(
        "    let now = std::time::SystemTime::now();\n", // 1: two tokens
        "    // std::time is only mentioned here\n",     // 2: a comment
        "    Operand::Present(value) => resolved.push(value),\n", // 3: `rand::` inside `Operand::`
        "    let x = a.await;\n",                        // 4: `.await`
        "    let map: HashMap<u8, u8> = HashMap::new();\n", // 5: unordered map
    );
    let found = offences(planted);
    assert_eq!(
        found,
        [
            (1, "SystemTime"),
            (1, "std::time"),
            (4, ".await"),
            (5, "HashMap")
        ],
        "{found:?}"
    );
}

#[test]
fn the_kernel_depends_on_serialisation_and_nothing_else() {
    let manifest = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("manifest is readable");
    let dependencies: Vec<&str> = manifest
        .split("[dependencies]")
        .nth(1)
        .expect("a [dependencies] table")
        .split("\n[")
        .next()
        .expect("the table body")
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            (!line.is_empty() && !line.starts_with('#')).then(|| {
                line.split(['.', ' ', '='])
                    .next()
                    .expect("a dependency name")
            })
        })
        .collect();
    assert_eq!(dependencies, ["serde", "serde_json"], "{manifest}");
}
