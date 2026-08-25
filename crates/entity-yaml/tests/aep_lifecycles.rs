//! Phase 1 of [`docs/design/engineering-protocols-adoption-v0.1.md`]: the eight AEP lifecycle
//! documents from `engineering-protocols`, expressed as entity definitions under `examples/aep/`.
//!
//! The claim these tests make is narrow and is the whole of phase 1: **the definitions declare
//! exactly the edges the upstream transitions map declares — no more, no fewer.** No rules, no
//! preconditions, no evidence; those arrive with phase 3, once a rule can say `unknown`.
//!
//! The upstream documents are read from a committed fixture
//! (`tests/fixtures/aep-lifecycles/`, pinned at `79b641c` — see its `PIN.md`) rather than from a
//! sibling checkout, so this says the same thing on a machine that has only this repository.

use entity_core::{CoreError, Registry, Runtime};
use serde_json::json;
use serde_yaml_ng::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// One edge of a ladder: the state it starts in, the state it ends in.
type Edge = (String, String);

fn definitions_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/aep")
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/aep-lifecycles")
}

/// The `*.yaml` stems in a directory. `PIN.md` is prose and is not one of them.
fn kinds_in(dir: &Path) -> BTreeSet<String> {
    fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("{} is readable: {error}", dir.display()))
        .map(|entry| entry.expect("a readable directory entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "yaml")
        })
        .map(|path| {
            path.file_stem()
                .expect("a yaml file has a stem")
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

/// The kinds both directories agree on — every test below iterates this, so a kind that is missing
/// on either side is caught once, by name, rather than silently skipped everywhere.
fn pinned_kinds() -> BTreeSet<String> {
    let pinned = kinds_in(&fixtures_dir());
    assert!(!pinned.is_empty(), "the fixture directory is not empty");
    pinned
}

fn definition(kind: &str) -> entity_core::EntityDefinition {
    let path = definitions_dir().join(format!("{kind}.yaml"));
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} is readable: {error}", path.display()));
    entity_yaml::from_str(&text)
        .unwrap_or_else(|error| panic!("{} parses: {error}", path.display()))
}

/// The upstream document, reduced to the three things the equivalence is about.
struct Upstream {
    kind: String,
    initial: String,
    transitions: BTreeMap<String, Vec<String>>,
}

fn upstream(kind: &str) -> Upstream {
    let path = fixtures_dir().join(format!("{kind}.yaml"));
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} is readable: {error}", path.display()));
    let value: Value = serde_yaml_ng::from_str(&text)
        .unwrap_or_else(|error| panic!("{} parses: {error}", path.display()));

    let string = |key: &str| -> String {
        value
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("{} declares `{key}`", path.display()))
            .to_owned()
    };

    let transitions = value
        .get("transitions")
        .and_then(Value::as_mapping)
        .unwrap_or_else(|| panic!("{} declares `transitions`", path.display()))
        .iter()
        .map(|(from, to)| {
            let from = from.as_str().expect("a state is a string").to_owned();
            let to = to
                .as_sequence()
                .expect("a transitions entry is a list")
                .iter()
                .map(|state| state.as_str().expect("a state is a string").to_owned())
                .collect();
            (from, to)
        })
        .collect();

    Upstream {
        kind: string("kind"),
        initial: string("initial"),
        transitions,
    }
}

/// Every `(from, to)` pair the upstream map declares.
fn upstream_edges(upstream: &Upstream) -> BTreeSet<Edge> {
    upstream
        .transitions
        .iter()
        .flat_map(|(from, targets)| targets.iter().map(move |to| (from.clone(), to.clone())))
        .collect()
}

/// Every `(from, to)` pair the definition's operations yield, with the operation that yields it.
fn definition_edges(definition: &entity_core::EntityDefinition) -> BTreeMap<Edge, String> {
    let mut edges = BTreeMap::new();
    for (name, operation) in &definition.operations {
        for transition in &operation.transitions {
            for from in transition.from.as_slice() {
                let edge = (from.clone(), transition.to.clone());
                let previous = edges.insert(edge.clone(), name.clone());
                assert_eq!(
                    previous, None,
                    "{}: {edge:?} is declared by two operations, {previous:?} and {name}",
                    definition.entity
                );
            }
        }
    }
    edges
}

fn registry_for(kind: &str) -> (Registry, entity_core::EntityDefinition) {
    let definition = definition(kind);
    let mut registry = Registry::new();
    registry
        .register(definition.clone())
        .unwrap_or_else(|error| panic!("{kind} is a valid definition: {error}"));
    (registry, definition)
}

#[test]
fn every_pinned_lifecycle_has_a_definition_and_every_definition_a_pinned_lifecycle() {
    assert_eq!(
        pinned_kinds(),
        kinds_in(&definitions_dir()),
        "examples/aep/ and the pinned fixture must cover the same kinds — a kind on one side only \
         is a ladder nothing checks"
    );
}

#[test]
fn each_definition_is_named_for_the_kind_it_carries() {
    for kind in pinned_kinds() {
        assert_eq!(definition(&kind).entity, upstream(&kind).kind);
    }
}

#[test]
fn each_definition_starts_where_the_pinned_ladder_starts() {
    for kind in pinned_kinds() {
        assert_eq!(
            definition(&kind).lifecycle.initial,
            upstream(&kind).initial,
            "{kind}: initial state"
        );
    }
}

#[test]
fn each_definition_declares_exactly_the_states_the_pinned_ladder_declares() {
    for kind in pinned_kinds() {
        let upstream = upstream(&kind);
        let declared: BTreeSet<&str> = upstream.transitions.keys().map(String::as_str).collect();
        let definition = definition(&kind);
        let states: BTreeSet<&str> = definition
            .lifecycle
            .states
            .iter()
            .map(String::as_str)
            .collect();
        assert_eq!(states, declared, "{kind}: states");
    }
}

/// The claim phase 1 exists to make.
#[test]
fn each_definition_yields_exactly_the_edges_the_pinned_transitions_map_yields() {
    for kind in pinned_kinds() {
        let expected = upstream_edges(&upstream(&kind));
        let actual = definition_edges(&definition(&kind));
        let yielded: BTreeSet<Edge> = actual.keys().cloned().collect();

        let missing: Vec<&Edge> = expected.difference(&yielded).collect();
        let invented: Vec<&Edge> = yielded.difference(&expected).collect();
        assert!(
            missing.is_empty() && invented.is_empty(),
            "{kind}: the definition does not say what the pinned ladder says.\n  \
             not expressed: {missing:?}\n  not in the ladder: {invented:?}"
        );
    }
}

/// Upstream's terminal rungs — `archived`, and `rejected`/`superseded` where the ladder ends there —
/// carry an empty list. Nothing may leave them, which is the structural form of *nothing is deleted*.
#[test]
fn no_operation_leaves_a_state_the_pinned_ladder_ends_at() {
    let mut checked = 0;
    for kind in pinned_kinds() {
        let upstream = upstream(&kind);
        let edges = definition_edges(&definition(&kind));
        for (state, targets) in &upstream.transitions {
            if !targets.is_empty() {
                continue;
            }
            checked += 1;
            let leaving: Vec<(&Edge, &String)> = edges
                .iter()
                .filter(|((from, _), _)| from == state)
                .collect();
            assert!(
                leaving.is_empty(),
                "{kind}: `{state}` is terminal upstream, but {leaving:?} leaves it"
            );
        }
    }
    assert!(checked >= 8, "every kind has at least one terminal rung");
}

#[test]
fn every_definition_registers_so_no_ladder_is_ambiguous_or_unreachable() {
    let mut registry = Registry::new();
    for kind in pinned_kinds() {
        registry
            .register(definition(&kind))
            .unwrap_or_else(|error| panic!("{kind}: {error}"));
    }
}

#[test]
fn a_story_walks_the_ladder_and_every_move_is_a_fact() {
    let (registry, _) = registry_for("story");
    let runtime = Runtime::new(&registry);

    let created = runtime
        .create(
            "story",
            1,
            "story:three-valued-conditions",
            json!({ "title": "Three-valued rule evaluation", "owner": "kernel" }),
        )
        .expect("create");
    assert_eq!(created.instance.lifecycle_state, "draft");
    assert_eq!(created.events[0].event_type, "ArtifactCreated");
    assert_eq!(created.instance.fields["tags"], json!([]));

    let mut instance = created.instance;
    for (operation, state) in [
        ("propose", "proposed"),
        ("activate", "active"),
        ("implement", "implemented"),
    ] {
        let decision = runtime
            .execute(&instance, operation, json!({ "actor": "timo" }))
            .unwrap_or_else(|error| panic!("{operation}: {error}"));
        assert_eq!(decision.instance.lifecycle_state, state);
        assert_eq!(decision.events.len(), 1);
        assert_eq!(decision.events[0].event_type, "ArtifactMoved");
        assert_eq!(decision.events[0].payload["operation"], json!(operation));
        assert_eq!(decision.events[0].payload["to"], json!(state));
        assert_eq!(decision.events[0].payload["actor"], json!("timo"));
        instance = decision.instance;
    }
}

#[test]
fn a_story_cannot_reach_implemented_without_passing_through_the_ladder() {
    let (registry, _) = registry_for("story");
    let runtime = Runtime::new(&registry);
    let created = runtime
        .create("story", 1, "story:x", json!({ "title": "x" }))
        .expect("create");

    let refused = runtime
        .execute(&created.instance, "implement", json!({ "actor": "timo" }))
        .expect_err("draft cannot implement");
    assert_eq!(
        refused,
        CoreError::InvalidTransition {
            operation: "implement".to_owned(),
            state: "draft".to_owned(),
        }
    );
}

#[test]
fn an_archived_artifact_has_no_move_left_to_make() {
    let (registry, _) = registry_for("story");
    let runtime = Runtime::new(&registry);
    let created = runtime
        .create("story", 1, "story:x", json!({ "title": "x" }))
        .expect("create");
    let archived = runtime
        .execute(&created.instance, "archive", json!({ "actor": "timo" }))
        .expect("archive");
    assert_eq!(archived.instance.lifecycle_state, "archived");

    for operation in ["propose", "activate", "implement", "archive"] {
        let refused = runtime
            .execute(&archived.instance, operation, json!({ "actor": "timo" }))
            .expect_err("archived is terminal");
        assert!(
            matches!(refused, CoreError::InvalidTransition { .. }),
            "{operation} from archived: {refused}"
        );
    }
}

/// The status is the kernel's, not a field. An adopter cannot set it by writing one.
#[test]
fn status_is_not_a_field_so_nobody_can_move_an_artifact_by_editing_it() {
    for kind in pinned_kinds() {
        let definition = definition(&kind);
        for reserved in ["status", "state", "kind", "id"] {
            assert!(
                !definition.schema.fields.contains_key(reserved),
                "{kind}: `{reserved}` is declared as a field, so a status becomes something \
                 anybody can write"
            );
        }
        assert!(
            !definition.schema.additional_fields,
            "{kind}: additional fields would let `status` back in through the side"
        );
    }
}
