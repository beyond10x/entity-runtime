//! Two checks that tie hand-written pages to the source they describe:
//!
//! - every refusal `kind` the code can produce — `DefinitionError::kind` and `CoreError::kind` in
//!   `entity-core`, `ShellError::kind` in `entity-shell` — appears, in backticks, on the typed
//!   refusals page;
//! - every code block titled with a repository file (`title="crates/…"`) is that file, byte for
//!   byte.

use std::{collections::BTreeSet, fs, path::Path};

use crate::Result;

/// The page that must name every refusal kind, relative to the repository root.
const REFUSALS: &str = "website/docs/reference/refusals.md";
/// The source files whose `kind()` functions name the refusal kinds.
const KIND_SOURCES: &[&str] = &[
    "crates/entity-core/src/error.rs",
    "crates/entity-shell/src/lib.rs",
];

/// The string literals returned by every `fn kind(&self) -> &'static str` body in `source`.
fn kinds_in(source: &str) -> BTreeSet<String> {
    let mut kinds = BTreeSet::new();
    let mut inside = false;
    for line in source.lines() {
        if line.contains("fn kind(&self) -> &'static str {") {
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        if line == "    }" {
            inside = false;
            continue;
        }
        // A literal is a returned kind when it is the whole arm body or the whole line.
        let trimmed = line.trim().trim_end_matches(',');
        let literal = trimmed
            .rsplit_once("=> ")
            .map_or(trimmed, |(_, body)| body)
            .trim_end_matches(',');
        if let Some(kind) = literal
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
        {
            kinds.insert(kind.to_owned());
        }
    }
    kinds
}

/// Every refusal kind the code returns that the refusals page does not name.
pub fn missing_refusal_kinds(root: &Path) -> Result<Vec<String>> {
    let page = read(root, REFUSALS)?;
    let mut missing = Vec::new();
    for source in KIND_SOURCES {
        for kind in kinds_in(&read(root, source)?) {
            if !page.contains(&format!("`{kind}`")) {
                missing.push(format!(
                    "{REFUSALS}: refusal kind `{kind}` from {source} is not listed"
                ));
            }
        }
    }
    Ok(missing)
}

/// Every code block in the Markdown `page` titled with a repository file that differs from it.
pub fn stale_code_blocks(root: &Path, relative: &str, page: &str) -> Result<Vec<String>> {
    let mut problems = Vec::new();
    let mut lines = page.lines().enumerate();
    while let Some((number, line)) = lines.next() {
        let Some(title) = line
            .strip_prefix("```")
            .and_then(|info| info.split_once("title=\""))
            .and_then(|(_, rest)| rest.split_once('"'))
            .map(|(title, _)| title)
        else {
            continue;
        };
        let mut block = String::new();
        for (_, inner) in lines.by_ref() {
            if inner == "```" {
                break;
            }
            block.push_str(inner);
            block.push('\n');
        }
        if !title.starts_with("crates/") {
            continue;
        }
        let file = read(root, title)?;
        if file != block {
            problems.push(format!(
                "{relative}:{}: the block titled {title} differs from that file",
                number + 1
            ));
        }
    }
    Ok(problems)
}

fn read(root: &Path, relative: &str) -> Result<String> {
    fs::read_to_string(root.join(relative)).map_err(|error| format!("reading {relative}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_are_read_from_one_line_and_block_arms() {
        let source = "impl E {\n    pub fn kind(&self) -> &'static str {\n        match self {\n            Self::A => \"a_kind\",\n            Self::B { .. } | Self::C => {\n                \"b_kind\"\n            }\n            Self::D(error) => error.kind(),\n        }\n    }\n    fn other(&self) -> &'static str {\n        \"not_a_kind\"\n    }\n}\n";
        assert_eq!(
            kinds_in(source).into_iter().collect::<Vec<_>>(),
            ["a_kind", "b_kind"]
        );
    }

    #[test]
    fn every_refusal_kind_of_this_tree_is_on_the_refusals_page() {
        let root = crate::repository_root();
        let kinds = kinds_in(&read(&root, KIND_SOURCES[0]).unwrap());
        assert!(kinds.contains("precondition_failed") && kinds.contains("empty_entity_name"));
        assert_eq!(missing_refusal_kinds(&root).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn a_titled_block_must_equal_its_file() {
        let root = crate::repository_root();
        let file = "crates/entity-runtime-docs/Cargo.toml";
        let content = read(&root, file).unwrap();
        let good = format!("text\n\n```toml title=\"{file}\"\n{content}```\n");
        assert!(stale_code_blocks(&root, "p.md", &good).unwrap().is_empty());
        let bad = format!("```toml title=\"{file}\"\n[package]\n```\n");
        let problems = stale_code_blocks(&root, "p.md", &bad).unwrap();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].starts_with("p.md:1:"), "{problems:?}");
    }
}
