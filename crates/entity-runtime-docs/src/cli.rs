//! The CLI reference page, walked from the clap definition of `entity` (`entity_cli::cli::Cli`).
//!
//! The walk sees the default build, which is what the release archives and a plain
//! `cargo install` contain. The options an `eventlog-providers` build adds are read from the
//! `#[cfg(feature = "eventlog-providers")]` items of the same source file, so the page names
//! them without this crate having to build the Eventlog providers.

use std::path::Path;

use clap::CommandFactory as _;

use crate::{cell, front_matter, Result, SOURCE};

/// Where the CLI reference lands, relative to the repository root.
pub const PAGE: &str = "website/docs/reference/cli.md";
/// The clap definition, relative to the repository root.
const DEFINITION: &str = "crates/entity-cli/src/cli.rs";
/// The attribute that gates an item behind the Eventlog providers.
const GATE: &str = "#[cfg(feature = \"eventlog-providers\")]";

fn text(value: Option<&clap::builder::StyledStr>) -> String {
    value.map(ToString::to_string).unwrap_or_default()
}

/// Every visible subcommand below `command`, depth first, without clap's `help`.
fn commands(command: &clap::Command, out: &mut Vec<clap::Command>) {
    for sub in command.get_subcommands() {
        if sub.get_name() == "help" || sub.is_hide_set() {
            continue;
        }
        out.push(sub.clone());
        commands(sub, out);
    }
}

/// `entity store migrate-file`, as clap names the command.
fn written(command: &clap::Command) -> String {
    command
        .get_bin_name()
        .unwrap_or(command.get_name())
        .to_owned()
}

/// The heading anchor Docusaurus gives `` ## `entity store migrate-file` ``.
fn anchor(command: &clap::Command) -> String {
    written(command).replace(' ', "-")
}

/// One row per argument of `command`: how it is written, whether it is required, whether it
/// repeats, its default and its help.
fn arguments(command: &clap::Command) -> Vec<[String; 5]> {
    command
        .get_arguments()
        .filter(|arg| !matches!(arg.get_id().as_str(), "help" | "version") && !arg.is_hide_set())
        .map(|arg| {
            let value = arg
                .get_value_names()
                .and_then(|names| names.first().map(ToString::to_string))
                .unwrap_or_else(|| arg.get_id().as_str().to_uppercase());
            let takes_value = arg.get_num_args().is_some_and(|n| n.takes_values());
            let many = arg.get_num_args().is_some_and(|n| n.max_values() > 1);
            let repeats = matches!(arg.get_action(), clap::ArgAction::Append) || many;
            let written = match (arg.get_long(), takes_value) {
                (Some(long), true) => format!("--{long} <{value}>"),
                (Some(long), false) => format!("--{long}"),
                (None, _) if many => format!("<{value}>..."),
                (None, _) => format!("<{value}>"),
            };
            let defaults: Vec<String> = arg
                .get_default_values()
                .iter()
                .map(|default| default.to_string_lossy().into_owned())
                .filter(|default| takes_value || default != "false")
                .collect();
            let possible: Vec<String> = arg
                .get_possible_values()
                .iter()
                .filter(|_| takes_value)
                .map(|value| format!("`{}`", value.get_name()))
                .collect();
            let mut help = cell(&text(arg.get_long_help().or(arg.get_help())));
            if !help.is_empty() && !help.ends_with('.') {
                help.push('.');
            }
            if !possible.is_empty() {
                help.push_str(&format!(" One of {}.", possible.join(", ")));
            }
            [
                format!("`{written}`"),
                if arg.is_required_set() { "yes" } else { "no" }.to_owned(),
                if repeats { "yes" } else { "no" }.to_owned(),
                if defaults.is_empty() {
                    "none".to_owned()
                } else {
                    format!("`{}`", defaults.join(", "))
                },
                help,
            ]
        })
        .collect()
}

/// One item of the clap definition that only an `eventlog-providers` build has.
#[derive(Debug, PartialEq, Eq)]
enum Gated {
    /// An option of the subcommand the variant `variant` declares.
    Option {
        variant: String,
        field: String,
        help: String,
    },
    /// A subcommand of the enum `parent`.
    Command {
        parent: String,
        variant: String,
        help: String,
    },
}

/// `ProvisionEventlogFile` → `provision-eventlog-file`, as clap derives a name.
fn kebab(name: &str) -> String {
    let mut out = String::new();
    for (index, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if index > 0 {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else if c == '_' {
            out.push('-');
        } else {
            out.push(c);
        }
    }
    out
}

/// The identifier a line declares: `ProvisionEventlogFile {` or `eventlog_config: Option<…>,`.
fn declared(line: &str) -> Option<(bool, String)> {
    let line = line.trim().trim_start_matches("pub ");
    let name: String = line
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    let rest = line[name.len()..].trim_start();
    if name.is_empty() {
        None
    } else if rest.starts_with(':') {
        Some((false, name))
    } else if rest.starts_with('{') && name.starts_with(|c: char| c.is_ascii_uppercase()) {
        Some((true, name))
    } else {
        None
    }
}

/// The gated options and subcommands of the clap definition in `source`.
fn gated(source: &str) -> Result<Vec<Gated>> {
    let lines: Vec<&str> = source.lines().collect();
    let mut found = Vec::new();
    for (at, line) in lines.iter().enumerate() {
        if line.trim() != GATE {
            continue;
        }
        let item = lines[at + 1..]
            .iter()
            .find(|line| !line.trim().starts_with("#[") && !line.trim().starts_with("///"))
            .ok_or_else(|| format!("{DEFINITION}:{}: nothing follows the gate", at + 1))?;
        let help = lines[..at]
            .iter()
            .rev()
            .take_while(|line| line.trim().starts_with("///"))
            .map(|line| line.trim().trim_start_matches("///").trim())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(" ");
        // An `impl`, a `use` or the flattened argument struct is not a command-line item of its
        // own: its options are listed with the gated subcommand that flattens it.
        let Some((is_variant, name)) = declared(item) else {
            continue;
        };
        let enclosing = |pattern: fn(&str) -> Option<String>| {
            lines[..at].iter().rev().find_map(|line| pattern(line))
        };
        if is_variant {
            let parent = enclosing(|line| {
                line.strip_prefix("pub enum ")
                    .and_then(|rest| rest.strip_suffix(" {"))
                    .map(str::to_owned)
            })
            .ok_or_else(|| format!("{DEFINITION}:{}: {name} is in no enum", at + 1))?;
            found.push(Gated::Command {
                parent,
                variant: name,
                help,
            });
        } else {
            let variant = enclosing(|line| {
                line.strip_prefix("    ")
                    .filter(|rest| rest.starts_with(|c: char| c.is_ascii_uppercase()))
                    .and_then(|rest| rest.strip_suffix(" {"))
                    .map(str::to_owned)
            })
            .ok_or_else(|| format!("{DEFINITION}:{}: {name} is in no variant", at + 1))?;
            found.push(Gated::Option {
                variant,
                field: name,
                help,
            });
        }
    }
    Ok(found)
}

/// The section naming what an `eventlog-providers` build adds, checked against the walked tree:
/// every gated option belongs to a subcommand that exists in the default build.
fn gated_section(gated: &[Gated], all: &[clap::Command]) -> Result<String> {
    let mut rows = Vec::new();
    for item in gated {
        match item {
            Gated::Option {
                variant,
                field,
                help,
            } => {
                let command = all
                    .iter()
                    .find(|command| command.get_name() == kebab(variant))
                    .ok_or_else(|| format!("gated option {field} is in no known command"))?;
                rows.push(format!(
                    "| `{}` | `--{} <{}>` | {} |\n",
                    written(command),
                    kebab(field),
                    field.to_uppercase(),
                    cell(help)
                ));
            }
            Gated::Command {
                parent,
                variant,
                help,
            } => {
                let parent = match parent.strip_suffix("Command") {
                    Some("") | None => "entity".to_owned(),
                    Some(prefix) => format!("entity {}", kebab(prefix)),
                };
                rows.push(format!(
                    "| `{parent}` | subcommand `{} {}` | {} |\n",
                    parent,
                    kebab(variant),
                    cell(help)
                ));
            }
        }
    }
    if rows.is_empty() {
        return Ok(String::new());
    }
    Ok(format!(
        "\n## What an `eventlog-providers` build adds\n\nThe tables above describe the default \
         build, which is what the release archives and a plain `cargo install` contain. Building \
         `entity-cli` with the `eventlog-providers` Cargo feature (Rust 1.91) adds the items \
         below, read from the `{GATE}` items of the same definition. `--help` on such a build \
         prints their exact flags.\n\n| Command | Adds | Meaning |\n|---|---|---|\n{}",
        rows.concat()
    ))
}

/// The CLI reference page for the command line `root`, with the gated items of `source`.
fn page_for(mut root: clap::Command, source: &str) -> Result<String> {
    root.build();
    let name = root.get_name().to_owned();
    let mut all = Vec::new();
    commands(&root, &mut all);

    let mut out = front_matter(
        &format!("{name} CLI reference"),
        &format!("{name} CLI"),
        &format!(
            "Every command and option of the {name} command, generated from its clap definition."
        ),
        1,
    );
    out.push_str(&format!(
        "# `{name}`\n\nGenerated by `entity-runtime-docs` from the clap definition in \
         [`{DEFINITION}`]({SOURCE}/{DEFINITION}). It lists only commands that exist. \
         `{name} --version` prints the version, and `--help` after any command prints the text \
         below.\n\n```text\n{}\n```\n\n## Commands\n\n| Command | What it does |\n|---|---|\n",
        text(root.get_long_about().or(root.get_about())).trim_end()
    ));
    for command in &all {
        out.push_str(&format!(
            "| [`{}`](#{}) | {} |\n",
            written(command),
            anchor(command),
            cell(&text(command.get_about()))
        ));
    }
    for command in &mut all {
        let usage = command.render_usage().to_string();
        out.push_str(&format!(
            "\n## `{}`\n\n{}\n\n```text\n{}\n```\n",
            written(command),
            cell(&text(command.get_long_about().or(command.get_about()))),
            usage.trim_end()
        ));
        let rows = arguments(command);
        if !rows.is_empty() {
            out.push_str(
                "\n| Argument | Required | Repeats | Default | Meaning |\n|---|---|---|---|---|\n",
            );
            for row in rows {
                out.push_str(&format!("| {} |\n", row.join(" | ")));
            }
        }
    }
    out.push_str(&gated_section(&gated(source)?, &all)?);
    Ok(out)
}

/// The CLI reference page for `entity`, for the repository at `root`.
pub fn page(root: &Path) -> Result<String> {
    let source = std::fs::read_to_string(root.join(DEFINITION))
        .map_err(|error| format!("reading {DEFINITION}: {error}"))?;
    page_for(entity_cli::cli::Cli::command(), &source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_option_and_default_is_listed() {
        let page = page(&crate::repository_root()).unwrap();
        assert!(page.contains("# generated by entity-runtime-docs, do not edit"));
        assert!(page.contains("custom_edit_url: null"));
        assert!(page.contains("| [`entity validate`](#entity-validate) |"));
        assert!(page.contains("| [`entity store migrate-file`](#entity-store-migrate-file) |"));
        assert!(page.contains("## `entity execute`"));
        assert!(page.contains("Usage: entity validate <DEFINITIONS>..."));
        assert!(page.contains("| `<DEFINITIONS>...` | yes | yes | none |"));
        assert!(page.contains("| `--definition <DEFINITIONS>` | yes | yes | none |"));
        assert!(page.contains("| `--format <FORMAT>` | no | no | `json` |"));
        assert!(page.contains("One of `text`, `mermaid`, `dot`, `svg`, `html`."));
        assert!(page.contains("| `--no-actor` | no | no | none |"));
        assert!(page.contains("| `--fields <FIELDS>` | no | no | `{}` |"));
        assert!(!page.contains("entity help"));
        assert!(page.contains("Exit codes: 0 decided"));
    }

    #[test]
    fn the_eventlog_build_adds_exactly_what_the_definition_gates() {
        let page = page(&crate::repository_root()).unwrap();
        for create in ["create", "execute", "list"] {
            assert!(
                page.contains(&format!(
                    "| `entity {create}` | `--eventlog-config <EVENTLOG_CONFIG>` |"
                )),
                "{create}"
            );
        }
        assert!(
            page.contains("| `entity store` | subcommand `entity store provision-eventlog-file` |")
        );
        assert!(!page.contains("provision-eventlog-file`](#"));
    }

    #[test]
    fn gated_items_are_read_with_their_enclosing_variant_and_help() {
        let source = "pub enum StoreCommand {\n    /// Prepare it.\n    #[cfg(feature = \"eventlog-providers\")]\n    ProvisionThing {\n        /// Where.\n        #[cfg(feature = \"eventlog-providers\")]\n        #[arg(long)]\n        root_dir: PathBuf,\n    },\n}\n#[cfg(feature = \"eventlog-providers\")]\nimpl X for Y {}\n";
        assert_eq!(
            gated(source).unwrap(),
            [
                Gated::Command {
                    parent: "StoreCommand".to_owned(),
                    variant: "ProvisionThing".to_owned(),
                    help: "Prepare it.".to_owned(),
                },
                Gated::Option {
                    variant: "ProvisionThing".to_owned(),
                    field: "root_dir".to_owned(),
                    help: "Where.".to_owned(),
                },
            ]
        );
        assert_eq!(kebab("ProvisionEventlogFile"), "provision-eventlog-file");
        assert_eq!(kebab("eventlog_config"), "eventlog-config");
    }
}
