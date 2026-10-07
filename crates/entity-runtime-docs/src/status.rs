//! The status data, `website/data/status.json` (`b10x-status/1`), and the status page,
//! `website/docs/status.md`, both from [`CAPABILITIES`].
//!
//! A shipped capability names what holds it: a test as `path::function`, which generation checks
//! the file defines, or a gate step as `Taskfile.yml::task`, which generation checks the Taskfile
//! declares. A claim therefore cannot outlive its evidence. A capability that is not shipped names
//! none. The page is plain Markdown rather than MDX because the unified documentation site, which
//! still collects these pages, accepts no executable MDX.

use std::{fs, path::Path};

use serde_json::json;

use crate::{cell, front_matter, Result};

/// Where the status data lands, relative to the repository root.
pub const DATA: &str = "website/data/status.json";
/// Where the status page lands, relative to the repository root.
pub const PAGE: &str = "website/docs/status.md";

/// The date the list below was last checked against `main`.
const AS_OF: &str = "2026-10-07";

/// One capability on the status page.
pub struct Capability {
    /// The group the status table lists it under.
    pub area: &'static str,
    /// What it is, in a few words.
    pub label: &'static str,
    /// One sentence a reader can check.
    pub detail: &'static str,
    /// `shipped`, `decided` or `planned`.
    pub status: &'static str,
    /// For a shipped capability, what holds it: `path::function` or `Taskfile.yml::task`.
    pub evidence: Option<&'static str>,
    /// The documentation route that explains it, `/docs/...`.
    pub href: &'static str,
}

const fn shipped(
    area: &'static str,
    label: &'static str,
    detail: &'static str,
    evidence: &'static str,
    href: &'static str,
) -> Capability {
    Capability {
        area,
        label,
        detail,
        status: "shipped",
        evidence: Some(evidence),
        href,
    }
}

const fn planned(
    area: &'static str,
    label: &'static str,
    detail: &'static str,
    href: &'static str,
) -> Capability {
    Capability {
        area,
        label,
        detail,
        status: "planned",
        evidence: None,
        href,
    }
}

/// Every capability the site claims, in the order the status table lists them.
pub const CAPABILITIES: &[Capability] = &[
    shipped(
        "Kernel",
        "Deterministic decisions",
        "The same definition, instance, operation and arguments give the same Decision and the same serialized bytes.",
        "crates/entity-core/tests/requirements.rs::the_same_inputs_produce_the_same_decision_byte_for_byte",
        "/docs/concepts/guarantees",
    ),
    shipped(
        "Kernel",
        "No IO in the kernel",
        "entity-core reads no clock, filesystem, network, environment or random source and depends only on serde and serde_json.",
        "crates/entity-core/tests/purity.rs::the_kernel_reaches_no_clock_filesystem_network_or_random_source",
        "/docs/concepts/guarantees",
    ),
    shipped(
        "Kernel",
        "A refusal changes nothing",
        "A refused operation leaves the caller's instance untouched and produces no events.",
        "crates/entity-core/tests/requirements.rs::a_refusal_leaves_the_caller_owned_instance_untouched",
        "/docs/reference/refusals",
    ),
    shipped(
        "Kernel",
        "Closed definition documents",
        "A misspelled key, an unknown condition operator or a condition with two operators is refused, not ignored.",
        "crates/entity-core/tests/requirements.rs::a_misspelled_definition_key_is_refused_rather_than_ignored",
        "/docs/reference/definitions",
    ),
    shipped(
        "Kernel",
        "Three-valued rules",
        "A comparison over a value nobody observed answers unknown, and the refusal names every missing path.",
        "crates/entity-core/tests/requirements.rs::a_value_question_over_a_missing_reference_is_unobservable_and_exists_stays_two_valued",
        "/docs/reference/definitions",
    ),
    shipped(
        "Kernel",
        "Accumulated validation",
        "An object with several broken values reports every error, each with its path.",
        "crates/entity-core/tests/requirements.rs::validation_accumulates_every_field_error",
        "/docs/reference/definitions",
    ),
    shipped(
        "Kernel",
        "Prefix and suffix conditions",
        "starts_with and ends_with compare bytes, case-sensitively, under every semantics (0.24.0).",
        "crates/entity-core/tests/requirements.rs::starts_with_and_ends_with_test_a_string_prefix_and_suffix_byte_for_byte",
        "/docs/reference/definitions",
    ),
    shipped(
        "Kernel",
        "Typed references",
        "A ref field names another entity type; validate_all refuses a reference to a type nobody registered.",
        "crates/entity-core/tests/requirements.rs::validate_all_names_every_reference_whose_type_nobody_registered",
        "/docs/reference/definitions",
    ),
    shipped(
        "Kernel",
        "Named outcomes (service/1)",
        "Operations and creations carry ordered branches with guards, effects, responses and declared refusals; the kernel selects the branch.",
        "crates/entity-core/tests/service_semantics.rs::a_service_1_operation_runs_the_sixteen_steps_in_the_numbered_order",
        "/docs/concepts/service-semantics",
    ),
    shipped(
        "Kernel",
        "Optional values keep their absence (service/2)",
        "A copied optional argument stays absent, present or null through replay and retry.",
        "crates/entity-executor/tests/service_2_retry.rs::service_2_retry_distinguishes_absent_and_present_optional_arguments",
        "/docs/concepts/service-semantics",
    ),
    shipped(
        "Kernel",
        "Typed update actions (service/3)",
        "Set, Preserve and Remove actions apply after the outcome is selected and are kept in the record.",
        "crates/entity-store/tests/service_3_framing.rs::service_3_uses_record_4_and_request_4_with_exact_ordered_actions",
        "/docs/concepts/service-semantics",
    ),
    shipped(
        "Kernel",
        "Logical identity and derived addresses",
        "A service definition may declare an identity field; derived creation computes the storage address from it.",
        "crates/entity-core/tests/selected_creation_identity.rs::derived_and_supplied_creation_are_one_decision_and_replay_contract",
        "/docs/concepts/service-semantics",
    ),
    shipped(
        "Kernel",
        "Text length in service rules",
        "A service rule reads the length of a declared string as path.count, counted in Unicode scalar values with no normalization (0.27.0).",
        "crates/entity-core/tests/service_values.rs::a_text_count_is_its_number_of_unicode_scalar_values_under_service_1",
        "/docs/concepts/service-semantics#text-length-and-alphabets",
    ),
    shipped(
        "Kernel",
        "Text alphabets",
        "A service string field or argument may declare an alphabet; a value outside it is a validation error naming the first offending character, and generated contracts carry it as x-alphabet (0.27.0).",
        "crates/entity-core/tests/text_alphabet.rs::a_text_with_two_characters_outside_its_alphabet_is_one_error_naming_the_path_and_the_first",
        "/docs/concepts/service-semantics#text-length-and-alphabets",
    ),
    shipped(
        "Kernel",
        "Verified decision replay",
        "replay re-executes a complete decision record and refuses a record whose result was altered.",
        "crates/entity-core/tests/replay.rs::changing_any_recorded_result_is_refused_instead_of_becoming_state",
        "/docs/concepts/storage",
    ),
    shipped(
        "Kernel",
        "Legacy event folding",
        "rehydrate folds an event-only history and refuses an event no operation emits on its transition.",
        "crates/entity-core/tests/replay.rs::an_event_whose_type_no_operation_emits_on_its_transition_is_refused",
        "/docs/concepts/storage",
    ),
    shipped(
        "Storage",
        "Provider conformance suites",
        "One black-box suite holds Memory and File stores to the same write contract.",
        "crates/entity-store/tests/conformance.rs::the_memory_provider_conforms",
        "/docs/concepts/storage",
    ),
    shipped(
        "Storage",
        "File Store v2 and its migration",
        "entity store migrate-file copies a pre-0.15 store out of place; --dry-run writes nothing.",
        "crates/entity-cli/tests/cli.rs::file_store_migration_is_out_of_place_and_dry_run_writes_nothing",
        "/docs/guides/migrate-a-file-store",
    ),
    shipped(
        "Storage",
        "SQLite provider",
        "State, history and events commit in one SQLite transaction.",
        "crates/entity-sqlite/tests/conformance.rs::the_sqlite_provider_conforms",
        "/docs/concepts/storage",
    ),
    shipped(
        "Storage",
        "PostgreSQL provider",
        "The same contract against a server the caller connects to; the gate runs it only when ENTITY_POSTGRES_URL is set.",
        "crates/entity-postgres/tests/conformance.rs::the_postgres_provider_conforms",
        "/docs/concepts/storage",
    ),
    shipped(
        "Storage",
        "Remote protocol",
        "A store behind a caller-provided transport; a server that did not answer is Unreachable, never Absent.",
        "crates/entity-remote/tests/remote.rs::a_remote_that_did_not_answer_is_unreachable_and_never_absent",
        "/docs/concepts/storage",
    ),
    shipped(
        "Storage",
        "Hybrid authority and catch-up",
        "Authority, read path, offline and divergence behaviour are declared; divergences replay later with catch_up.",
        "crates/entity-remote/tests/hybrid.rs::a_laptop_that_wrote_while_the_replica_was_down_catches_up_when_it_returns",
        "/docs/concepts/storage",
    ),
    shipped(
        "Storage",
        "Document queries",
        "entity-query pages one entity type by JSON containment; a cursor is bound to the query that issued it.",
        "crates/entity-query/src/lib.rs::a_cursor_is_bound_to_the_query_that_emitted_it",
        "/docs/guides/embed-the-kernel",
    ),
    shipped(
        "Recorded execution",
        "Asynchronous executor with exact retries",
        "entity-executor runs complete recorded commands over async storage ports; a retry returns the original result.",
        "crates/entity-executor/tests/async_recorded_contract.rs::retry_uses_saved_definition_and_verified_prefix_before_current_authority",
        "/docs/concepts/storage",
    ),
    shipped(
        "Recorded execution",
        "Explicit definition versions",
        "execute_versioned and batch_versioned decide on a version the caller names (0.26.0).",
        "crates/entity-executor/tests/version_binding_review.rs::divergent_registered_versions_select_only_the_requested_guard",
        "/docs/concepts/storage",
    ),
    shipped(
        "Recorded execution",
        "Eventlog providers",
        "entity-eventlog keeps complete records on Eventlog file, sqlite, postgres and tree providers, provisioned explicitly before an open.",
        "crates/entity-eventlog/tests/providers.rs::tree_provider_provisions_replays_opens_and_rebuilds",
        "/docs/concepts/storage",
    ),
    shipped(
        "Recorded execution",
        "Forked subjects and merge",
        "On a tree store two branches that both decided for one subject fork it; BatchAction::Merge joins the heads.",
        "crates/entity-eventlog/tests/tree_branches.rs::a_merge_decision_joins_a_forked_subject_and_it_serves_again_after_reopening",
        "/docs/concepts/storage",
    ),
    shipped(
        "Recorded execution",
        "Recorded refusals",
        "An executor built with recording_refusals records each refusal once and moves no subject.",
        "crates/entity-eventlog/tests/tree_branches.rs::a_refused_command_is_recorded_once_and_changes_no_subject",
        "/docs/concepts/storage",
    ),
    shipped(
        "Recorded execution",
        "Batch import of legacy histories",
        "import_anchors establishes a batch of imported boundaries in one append group, and a retry of it settles.",
        "crates/entity-eventlog/tests/import_batch_retry_admission.rs::a_batch_bearing_retry_is_admitted_over_the_commit_it_already_made",
        "/docs/concepts/storage",
    ),
    shipped(
        "Recorded execution",
        "Tree stores keep each text once",
        "A new tree store is eventlog-tree/2; an eventlog-tree/1 store reads the same after migration (0.25.0).",
        "crates/entity-eventlog/tests/tree_text_blobs.rs::a_first_layout_store_reads_the_same_after_its_texts_are_kept_once_and_keeps_recording",
        "/docs/concepts/storage",
    ),
    shipped(
        "Recorded execution",
        "Provider-tracked reads on SQLite",
        "CapturePolicy::ProviderTracked reuses a verified observation SQLite proves unchanged (0.26.0); held to its own executable contract.",
        "Taskfile.yml::provider-ess-check",
        "/docs/concepts/storage",
    ),
    shipped(
        "Recorded execution",
        "Open from a persisted checkpoint on SQLite",
        "With durable open checkpoints enabled, a ProviderTracked open verifies the persisted checkpoint and the appends since it, not the whole history.",
        "crates/entity-eventlog/tests/bounded_open.rs::an_open_after_appends_verifies_only_the_suffix",
        "/docs/concepts/storage",
    ),
    shipped(
        "Surfaces",
        "The entity command",
        "Validate, inspect, graph, create, execute, list and store migrations from one Rust binary with typed exit codes.",
        "crates/entity-cli/tests/cli.rs::create_then_execute_through_a_pipe_and_a_refusal_with_its_typed_reason",
        "/docs/reference/cli",
    ),
    shipped(
        "Surfaces",
        "Eventlog File through the command",
        "A build with the eventlog-providers feature provisions and uses an Eventlog File store; no gate step builds that feature.",
        "crates/entity-cli/tests/cli.rs::explicit_eventlog_file_selection_provisions_creates_executes_retries_and_lists",
        "/docs/reference/cli",
    ),
    shipped(
        "Surfaces",
        "Lifecycle and reference graphs",
        "Text, Mermaid, DOT, SVG and HTML, the same bytes on every run.",
        "crates/entity-graph/tests/drawing.rs::every_format_is_the_same_bytes_twice",
        "/docs/guides/render-graphs",
    ),
    shipped(
        "Surfaces",
        "Entity pages, OpenAPI and AsyncAPI",
        "entity generate docs writes a static bundle and replaces only a directory it generated.",
        "crates/entity-cli/tests/cli.rs::generated_documentation_is_complete_and_replaces_only_generator_owned_output",
        "/docs/guides/generate-entity-docs",
    ),
    shipped(
        "Surfaces",
        "MCP tools over stdio",
        "Schema-derived create, get, list, events and operation tools; a stale call is refused and changes nothing.",
        "crates/entity-mcp/src/lib.rs::a_stale_tool_call_is_actionable_and_changes_nothing",
        "/docs/guides/mount-mcp-tools",
    ),
    shipped(
        "Surfaces",
        "Definition-specific Rust CLI",
        "entity generate rust-cli compiles a command with one subcommand per entity and operation.",
        "crates/entity-cli/tests/cli.rs::generated_rust_cli_compiles_and_executes_definition_specific_commands",
        "/docs/guides/generate-a-rust-cli",
    ),
    shipped(
        "Surfaces",
        "Agent skill",
        "entity skill renders a version-stamped Agent Skills document; replacing a file needs --force.",
        "crates/entity-cli/tests/cli.rs::skill_stdout_and_file_are_identical_and_replacement_is_explicit",
        "/docs/guides/connect-an-agent",
    ),
    shipped(
        "Contracts",
        "Executable ESS contracts for five libraries",
        "entity-core, entity-store, entity-executor, entity-shell and entity-query are checked against their ESS scenarios by the gate.",
        "Taskfile.yml::ess-check",
        "/docs/concepts/system-model",
    ),
    planned(
        "Planned",
        "JSON Schema for the definition format",
        "A schema generated from the Rust definition types.",
        "/docs/reference/definitions",
    ),
    planned(
        "Planned",
        "Definition migrations",
        "Moving stored instances from one definition version to another.",
        "/docs/concepts/guarantees",
    ),
    planned(
        "Planned",
        "entity explain",
        "Why an operation is or is not permitted from the current state.",
        "/docs/reference/cli",
    ),
    planned(
        "Planned",
        "Named predicates and schema fragments",
        "Reusable named rules, reusable schema fragments and definition inheritance.",
        "/docs/reference/definitions",
    ),
    planned(
        "Planned",
        "Responses checked against their schema",
        "A service response checked against its declared fields, including alphabet and max_length.",
        "/docs/concepts/service-semantics#text-length-and-alphabets",
    ),
    planned(
        "Planned",
        "Moves outcomes in YAML",
        "A service/1 outcome with a moves effect loads through entity-yaml and the entity command.",
        "/docs/concepts/service-semantics",
    ),
    planned(
        "Planned",
        "Definitions served over HTTP and NATS",
        "Registered definitions served as an HTTP and a NATS interface; today OpenAPI and AsyncAPI are contracts only.",
        "/docs/concepts/system-model",
    ),
];

/// Whether the evidence `file` (with its `source`) defines `name`.
fn defines(file: &str, source: &str, name: &str) -> bool {
    if file.ends_with("Taskfile.yml") {
        source.contains(&format!("\n  {name}:\n"))
    } else {
        source.contains(&format!("fn {name}("))
    }
}

fn checked(root: &Path, capabilities: &[Capability]) -> Result<()> {
    let mut labels = std::collections::BTreeSet::new();
    for capability in capabilities {
        if !labels.insert(capability.label) {
            return Err(format!("{}: listed twice", capability.label));
        }
        if !matches!(capability.status, "shipped" | "decided" | "planned") {
            return Err(format!(
                "{}: status {} is not shipped, decided or planned",
                capability.label, capability.status
            ));
        }
        if !capability.href.starts_with("/docs/") {
            return Err(format!("{}: href is not a /docs/ route", capability.label));
        }
        match (capability.status, capability.evidence) {
            ("shipped", Some(evidence)) => {
                let (file, name) = evidence.split_once("::").ok_or_else(|| {
                    format!("{}: {evidence} is not `path::name`", capability.label)
                })?;
                let source = fs::read_to_string(root.join(file))
                    .map_err(|error| format!("{}: reading {file}: {error}", capability.label))?;
                if !defines(file, &source, name) {
                    return Err(format!("{}: {file} has no {name}", capability.label));
                }
            }
            ("shipped", None) => {
                return Err(format!("{}: shipped without evidence", capability.label))
            }
            (_, Some(_)) => {
                return Err(format!(
                    "{}: only a shipped item names evidence",
                    capability.label
                ))
            }
            (_, None) => {}
        }
    }
    Ok(())
}

fn data_for(root: &Path, capabilities: &[Capability]) -> Result<String> {
    checked(root, capabilities)?;
    let items: Vec<_> = capabilities
        .iter()
        .map(|capability| {
            json!({
                "area": capability.area,
                "label": capability.label,
                "detail": capability.detail,
                "status": capability.status,
                "href": capability.href,
            })
        })
        .collect();
    let document = json!({
        "format": "b10x-status/1",
        "asOf": AS_OF,
        "source": "entity-runtime-docs, from its capability list; every shipped item names the test or gate step that holds it",
        "items": items,
    });
    serde_json::to_string_pretty(&document)
        .map(|text| text + "\n")
        .map_err(|error| format!("serializing the status data: {error}"))
}

/// The `b10x-status/1` document for the repository at `root`.
pub fn data(root: &Path) -> Result<String> {
    data_for(root, CAPABILITIES)
}

/// `/docs/guides/x#y` as a link from `website/docs/status.md`: `./guides/x.md#y`.
fn link(href: &str) -> String {
    let route = href.trim_start_matches("/docs/");
    let (page, anchor) = route.split_once('#').unwrap_or((route, ""));
    let anchor = if anchor.is_empty() {
        String::new()
    } else {
        format!("#{anchor}")
    };
    format!("./{page}.md{anchor}")
}

/// A status as the table shows it: a glyph and the word.
fn shown(status: &str) -> &'static str {
    match status {
        "shipped" => "● shipped",
        "decided" => "◐ decided",
        _ => "○ planned",
    }
}

fn page_for(root: &Path, capabilities: &[Capability]) -> Result<String> {
    checked(root, capabilities)?;
    let count = |status: &str| {
        capabilities
            .iter()
            .filter(|capability| capability.status == status)
            .count()
    };
    let mut out = front_matter(
        "Status",
        "Status",
        "What Entity Runtime ships today and what is planned, capability by capability.",
        3,
    );
    out.push_str(&format!(
        "# Status\n\n{} capabilities are shipped and {} are planned, as of {AS_OF}. **Shipped** \
         means released in a tagged version and held by a named test or gate step in the \
         repository; `entity-runtime-docs` generates this page and `website/data/status.json` \
         from one list and fails when a test it names is gone. **Planned** means a draft on the \
         plan, not built. The [changelog](https://github.com/beyond10x/entity-runtime/blob/main/CHANGELOG.md) \
         records every change a user sees, release by release.\n",
        count("shipped"),
        count("planned") + count("decided"),
    ));
    let mut area = "";
    for capability in capabilities {
        if capability.area != area {
            area = capability.area;
            out.push_str(&format!(
                "\n## {area}\n\n| Capability | Status | What it means |\n|---|---|---|\n"
            ));
        }
        out.push_str(&format!(
            "| [{}]({}) | {} | {} |\n",
            cell(capability.label),
            link(capability.href),
            shown(capability.status),
            cell(capability.detail),
        ));
    }
    Ok(out)
}

/// The status page for the repository at `root`.
pub fn page(root: &Path) -> Result<String> {
    page_for(root, CAPABILITIES)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capability(status: &'static str, evidence: Option<&'static str>) -> Capability {
        Capability {
            area: "A",
            label: "L",
            detail: "D",
            status,
            evidence,
            href: "/docs/status",
        }
    }

    #[test]
    fn every_shipped_capability_rests_on_evidence_that_exists() {
        let data = data(&crate::repository_root()).unwrap();
        assert!(data.contains("\"format\": \"b10x-status/1\""));
        let page = page(&crate::repository_root()).unwrap();
        assert!(
            page.contains("| [Deterministic decisions](./concepts/guarantees.md) | ● shipped |")
        );
    }

    #[test]
    fn a_vanished_test_or_a_shipped_item_without_one_is_refused() {
        let root = crate::repository_root();
        let gone = capability(
            "shipped",
            Some("crates/entity-cli/tests/cli.rs::no_such_test"),
        );
        let error = data_for(&root, &[gone]).unwrap_err();
        assert!(error.contains("has no no_such_test"), "{error}");
        let task = capability("shipped", Some("Taskfile.yml::no-such-task"));
        assert!(data_for(&root, &[task]).is_err());
        let task = capability("shipped", Some("Taskfile.yml::ess-check"));
        assert!(data_for(&root, &[task]).is_ok());
        assert!(data_for(&root, &[capability("shipped", None)]).is_err());
        let claimed = capability(
            "planned",
            Some("crates/entity-cli/tests/cli.rs::validate_accepts_the_example_and_exits_zero"),
        );
        assert!(data_for(&root, &[claimed]).is_err());
        assert!(data_for(&root, &[capability("released", None)]).is_err());
        assert!(data_for(&root, &[capability("planned", None)]).is_ok());
    }

    #[test]
    fn routes_become_relative_markdown_links() {
        assert_eq!(link("/docs/concepts/storage"), "./concepts/storage.md");
        assert_eq!(
            link("/docs/reference/cli#entity-list"),
            "./reference/cli.md#entity-list"
        );
    }
}
