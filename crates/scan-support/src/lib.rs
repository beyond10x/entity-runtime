//! Reading a Rust source file well enough to say what its **code** names, and a manifest well
//! enough to say what it depends on.
//!
//! # Why this is a crate and not a copy
//!
//! It was a copy, twice, and the second copy was strictly weaker than the first. `entity-graph`
//! shipped a boundary test whose scanner treated the `"` inside a char literal as opening a string,
//! so everything after `if character == '"'` was invisible to it — which happened to be the whole
//! escaping function the test existed to protect. A real `std::fs::read_to_string` planted there
//! passed. An independent review found it after the release.
//!
//! Writing it a third time was the obvious next move and the wrong one. A guard written twice
//! diverges, and the weaker copy is the one nobody notices. So there is one, both crates' tests use
//! it, and the plantings at the bottom of this file are the proof it still works.
//!
//! Test support: `publish = false`, and nothing depends on it outside `[dev-dependencies]`.

/// A source file with its comments and literals blanked out, so only *code* remains.
///
/// Blanked rather than deleted: every byte becomes a space and every newline is kept, so a line
/// number in the result is a line number in the file.
///
/// Four things are not code, and the third caught this out. Line comments, block comments (which
/// nest), string literals — and **char literals**, because `'"'` is a quote that opens nothing. A
/// scanner without that case reads the rest of the file as one long string and finds nothing in it,
/// which is how a planted `std::fs::read_to_string` passed a test written to catch it. Raw strings
/// are handled for the same reason: `r#"..."#` ends at `"#`, not at the first `"`.
///
/// A lifetime is not a char literal and survives: `'a` has no closing quote, and swallowing to the
/// next one would blank out real code.
#[must_use]
pub fn code_only(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0usize;

    fn blank(out: &mut String, chars: &[char], from: usize, to: usize) {
        for &character in &chars[from..to.min(chars.len())] {
            out.push(if character == '\n' { '\n' } else { ' ' });
        }
    }

    while at < chars.len() {
        let current = chars[at];
        let next = chars.get(at + 1).copied();

        if current == '/' && next == Some('/') {
            let end = chars[at..]
                .iter()
                .position(|c| *c == '\n')
                .map_or(chars.len(), |offset| at + offset);
            blank(&mut out, &chars, at, end);
            at = end;
        } else if current == '/' && next == Some('*') {
            let mut depth = 1usize;
            let mut end = at + 2;
            while end < chars.len() && depth > 0 {
                match (chars[end], chars.get(end + 1).copied()) {
                    ('/', Some('*')) => {
                        depth += 1;
                        end += 2;
                    }
                    ('*', Some('/')) => {
                        depth -= 1;
                        end += 2;
                    }
                    _ => end += 1,
                }
            }
            blank(&mut out, &chars, at, end);
            at = end;
        } else if current == 'r' && raw_string_end(&chars, at).is_some() {
            let end = raw_string_end(&chars, at).expect("just checked");
            blank(&mut out, &chars, at, end);
            at = end;
        } else if current == '\'' && char_literal_end(&chars, at).is_some() {
            let end = char_literal_end(&chars, at).expect("just checked");
            blank(&mut out, &chars, at, end);
            at = end;
        } else if current == '"' {
            let mut end = at + 1;
            while end < chars.len() {
                match chars[end] {
                    '\\' => end += 2,
                    '"' => {
                        end += 1;
                        break;
                    }
                    _ => end += 1,
                }
            }
            blank(&mut out, &chars, at, end);
            at = end;
        } else {
            out.push(current);
            at += 1;
        }
    }
    out
}

/// Where a raw string starting at `at` ends, or `None` when this `r` is an ordinary identifier.
fn raw_string_end(chars: &[char], at: usize) -> Option<usize> {
    let mut hashes = 0usize;
    while chars.get(at + 1 + hashes) == Some(&'#') {
        hashes += 1;
    }
    if chars.get(at + 1 + hashes) != Some(&'"') {
        return None;
    }
    let mut end = at + 2 + hashes;
    while end < chars.len() {
        if chars[end] == '"' && (1..=hashes).all(|offset| chars.get(end + offset) == Some(&'#')) {
            return Some(end + 1 + hashes);
        }
        end += 1;
    }
    Some(chars.len())
}

/// Where a char literal starting at `at` ends, or `None` when this is a lifetime.
fn char_literal_end(chars: &[char], at: usize) -> Option<usize> {
    let mut end = at + 1;
    if chars.get(end) == Some(&'\\') {
        end += 1;
        if chars.get(end) == Some(&'u') {
            while end < chars.len() && chars[end] != '}' {
                end += 1;
            }
        }
        end += 1;
    } else {
        end += 1;
    }
    (chars.get(end) == Some(&'\'')).then_some(end + 1)
}

/// Splits code into identifier-ish words, so `std::fs::read_to_string` yields `std`, `fs` and
/// `read_to_string` — and a substring match cannot fire on `Operand::` for `rand`.
pub fn words(code: &str) -> impl Iterator<Item = (usize, &str)> {
    code.lines().enumerate().flat_map(|(index, line)| {
        line.split(|character: char| !(character.is_alphanumeric() || character == '_'))
            .filter(|word| !word.is_empty())
            .map(move |word| (index + 1, word))
    })
}

/// Every crate named by a **dependency** table of a manifest.
///
/// Every table, not the literal `[dependencies]` alone: `[dependencies.foo]` and
/// `[target.'cfg(unix)'.dependencies]` are dependencies too, and a check that split on the literal
/// heading would not see either. `[dev-dependencies]` is deliberately *not* counted — it is not
/// linked into the library and cannot reach anything at run time. Build dependencies are counted:
/// build scripts execute during compilation and can generate code the source scan never sees.
#[must_use]
pub fn dependencies(manifest: &str) -> std::collections::BTreeSet<String> {
    let mut declared = std::collections::BTreeSet::new();
    let mut in_table = false;
    for line in manifest.lines() {
        let line = line.trim();
        if let Some(header) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            let header = header.trim();
            if let Some(name) = header.strip_prefix("dependencies.") {
                declared.insert(name.to_owned());
                in_table = false;
                continue;
            }
            in_table = header == "dependencies"
                || header == "build-dependencies"
                || (header.starts_with("target.")
                    && (header.ends_with(".dependencies")
                        || header.ends_with(".build-dependencies")));
            continue;
        }
        if in_table && !line.is_empty() && !line.starts_with('#') {
            if let Some(name) = line.split(['=', ' ', '.']).next() {
                if !name.is_empty() {
                    declared.insert(name.to_owned());
                }
            }
        }
    }
    declared
}

#[cfg(test)]
mod tests {
    use super::{code_only, dependencies, words};

    /// The two holes an independent review found in a hand-rewritten copy of this scanner. Each is
    /// a planting: if the scanner regresses, the code after it stops being scanned and these fail.
    #[test]
    fn the_scanner_sees_past_a_char_literal_and_a_block_comment() {
        let quote_char = "if c == '\"' { }\nlet _ = std::fs::read_to_string(\"x\");";
        assert!(
            code_only(quote_char).contains("std"),
            "a char literal holding a quote must not swallow the rest of the file"
        );

        let block = "/* std::fs is discussed here */\nlet _ = std::net::TcpStream::connect(\"x\");";
        let scanned = code_only(block);
        assert!(
            scanned.contains("net"),
            "code after a block comment is code"
        );
        assert_eq!(
            scanned.matches("std").count(),
            1,
            "and the mention inside the comment is not"
        );
    }

    #[test]
    fn a_string_literal_and_a_line_comment_are_not_code() {
        assert!(!code_only("let m = \"std::fs::read\";").contains("fs"));
        assert!(!code_only("// std::fs::read\n").contains("fs"));
    }

    #[test]
    fn words_splits_on_paths_so_a_substring_cannot_fire() {
        let found: Vec<&str> = words("std::fs::read_to_string").map(|(_, w)| w).collect();
        assert_eq!(found, vec!["std", "fs", "read_to_string"]);
        assert!(
            !words("Operand::Rand").any(|(_, w)| w == "rand"),
            "case matters, and a substring must not fire"
        );
    }

    /// The manifest hole: a dependency written as its own table.
    #[test]
    fn every_dependency_table_is_read_not_only_the_literal_heading() {
        let manifest = "[package]\nname = \"x\"\n\n[dependencies]\nserde = \"1\"\n\n\
                        [dependencies.tokio]\nversion = \"1\"\n\n\
                        [target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n\n\
                        [target.'cfg(unix)'.build-dependencies]\ncc = \"1\"\n\n\
                        [build-dependencies]\nquote = \"1\"\n\n\
                        [dev-dependencies]\nproptest = \"1\"\n";
        let found = dependencies(manifest);
        assert!(found.contains("serde"), "{found:?}");
        assert!(
            found.contains("tokio"),
            "a dependency table is a dependency: {found:?}"
        );
        assert!(
            found.contains("libc"),
            "so is a target-specific one: {found:?}"
        );
        assert!(
            found.contains("cc"),
            "target build dependencies run: {found:?}"
        );
        assert!(found.contains("quote"), "build dependencies run: {found:?}");
        assert!(
            !found.contains("proptest"),
            "a dev-dependency is not linked into the library: {found:?}"
        );
    }
}
