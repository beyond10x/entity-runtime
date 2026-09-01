//! Phase 1 of [`docs/design/aep-adoption-v0.1.md`]: every AEP lifecycle
//! document AEP ships, expressed as an entity definition under `examples/aep/`. The number is read
//! from the fixture, never written down here, because a count in a test is a second place for the
//! truth to live.
//!
//! The claim these tests make is narrow and is the whole of phase 1: **the definitions declare
//! exactly the edges the upstream transitions map declares — no more, no fewer.** No rules, no
//! preconditions, no evidence; those arrive with phase 3, once a rule can say `unknown`.
//!
//! The upstream documents are read from a committed fixture
//! (`tests/fixtures/aep-lifecycles/`, pinned at `4e6279b` — see its `PIN.md`) rather than from a
//! sibling checkout, so this says the same thing on a machine that has only this repository.

use entity_core::{CoreError, EntityInstance, Registry, Runtime};
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

/// Phase 3's half of the equivalence: a rung upstream says costs evidence must cost it here too.
///
/// The edge test above compares which moves exist. This compares what one of them *asks for* —
/// paired by `(target status, evidence kind)`, which is what both sides genuinely say. It does not
/// compare counts or wording: upstream declares `at_least` on a status, these definitions declare a
/// `gte` on a verb-named operation, and pinning the sentence would be pinning a translation rather
/// than the claim.
#[test]
fn a_rung_the_pinned_ladder_charges_for_is_charged_for_here_too() {
    let mut compared = 0;
    for kind in pinned_kinds() {
        let upstream = upstream_requires(&kind);
        let definition = definition(&kind);
        for (state, evidence) in &upstream {
            compared += 1;
            let reaching: Vec<&entity_core::OperationDefinition> = definition
                .operations
                .values()
                .filter(|operation| {
                    operation
                        .transitions
                        .iter()
                        .any(|transition| &transition.to == state)
                })
                .collect();
            assert!(
                !reaching.is_empty(),
                "{kind}: nothing reaches `{state}`, which the ladder charges for"
            );
            let charged = reaching.iter().any(|operation| {
                operation.preconditions.iter().any(|rule| {
                    serde_json::to_string(&rule.condition)
                        .unwrap_or_default()
                        .contains(evidence.as_str())
                })
            });
            assert!(
                charged,
                "{kind}: the pinned ladder charges `{evidence}` to reach `{state}`, and no \
                 operation reaching it declares a precondition that reads one"
            );
        }
    }
    assert!(
        compared >= 1,
        "no pinned ladder charges for anything, so this test compared nothing"
    );
}

/// The other half of the same claim: a rung the pinned ladder **dates** must be dated here too.
///
/// Paired by `(target status, frontmatter key)`. Without this, a definition could quietly drop the
/// `before`/`after` precondition and only the upstream document would still say the rung waits.
#[test]
fn a_rung_the_pinned_ladder_dates_is_dated_here_too() {
    let mut compared = 0;
    for kind in pinned_kinds() {
        let definition = definition(&kind);
        for (state, key) in upstream_when(&kind) {
            compared += 1;
            let dated = definition
                .operations
                .values()
                .filter(|operation| {
                    operation
                        .transitions
                        .iter()
                        .any(|transition| transition.to == state)
                })
                .any(|operation| {
                    operation.preconditions.iter().any(|rule| {
                        serde_json::to_string(&rule.condition)
                            .unwrap_or_default()
                            .contains(&key)
                    })
                });
            assert!(
                dated,
                "{kind}: the pinned ladder opens `{state}` on `{key}`, and no operation reaching \
                 it declares a precondition that reads it"
            );
        }
    }
    assert!(
        compared >= 1,
        "no pinned ladder dates a rung, so this compared nothing"
    );
}

/// `(status, frontmatter key)` for every date guard the pinned ladder declares.
fn upstream_when(kind: &str) -> Vec<(String, String)> {
    let path = fixtures_dir().join(format!("{kind}.yaml"));
    let text = fs::read_to_string(&path).expect("readable");
    let value: Value = serde_yaml_ng::from_str(&text).expect("parses");
    let Some(when) = value.get("when").and_then(Value::as_mapping) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for (status, guard) in when {
        let status = status.as_str().expect("a status is a string").to_owned();
        let Some(guard) = guard.as_mapping() else {
            continue;
        };
        for edge in ["after", "before"] {
            if let Some(key) = guard.get(edge).and_then(Value::as_str) {
                found.push((status.clone(), key.to_owned()));
            }
        }
    }
    found
}

/// `(status, evidence kind)` for every requirement the pinned ladder declares.
fn upstream_requires(kind: &str) -> Vec<(String, String)> {
    let path = fixtures_dir().join(format!("{kind}.yaml"));
    let text = fs::read_to_string(&path).expect("readable");
    let value: Value = serde_yaml_ng::from_str(&text).expect("parses");
    let Some(requires) = value.get("requires").and_then(Value::as_mapping) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for (status, requirements) in requires {
        let status = status.as_str().expect("a status is a string").to_owned();
        for requirement in requirements.as_sequence().expect("a list") {
            let evidence = requirement
                .get("evidence")
                .and_then(Value::as_str)
                .expect("a requirement names an evidence kind")
                .to_owned();
            found.push((status.clone(), evidence));
        }
    }
    found
}

/// Upstream's terminal rungs — `archived`, and `rejected`/`superseded` where the ladder ends there —
/// carry an empty list. Nothing may leave them, which is the structural form of *nothing is deleted*.
#[test]
fn no_operation_leaves_a_state_the_pinned_ladder_ends_at() {
    for kind in pinned_kinds() {
        let upstream = upstream(&kind);
        let edges = definition_edges(&definition(&kind));
        let mut terminal = 0;
        for (state, targets) in &upstream.transitions {
            if !targets.is_empty() {
                continue;
            }
            terminal += 1;
            let leaving: Vec<(&Edge, &String)> = edges
                .iter()
                .filter(|((from, _), _)| from == state)
                .collect();
            assert!(
                leaving.is_empty(),
                "{kind}: `{state}` is terminal upstream, but {leaving:?} leaves it"
            );
        }
        // Per kind, not summed across them: a total would let one ladder lose its terminal rung
        // and stay green on another ladder's count, which is a check that cannot see what it is
        // for.
        assert!(
            terminal >= 1,
            "{kind}: no terminal rung, so nothing was checked"
        );
    }
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
        // `implement` costs a test result now — phase 3 of the adoption design, and gap-register
        // `:39` on the other side. Every other rung on this ladder is free, and only `implement`
        // *declares* an `evidence` argument: passing one to `propose` is refused as an argument the
        // operation does not take, which is the schema doing its job rather than a special case.
        let arguments = if operation == "implement" {
            json!({ "actor": "timo", "evidence": { "test_result": 1 } })
        } else {
            json!({ "actor": "timo" })
        };
        let decision = runtime
            .execute(&instance, operation, arguments)
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
fn a_story_cannot_reach_implemented_without_the_evidence_its_ladder_asks_for() {
    let (registry, _) = registry_for("story");
    let runtime = Runtime::new(&registry);
    let active = EntityInstance {
        entity: "story".to_owned(),
        version: 1,
        id: "story:x".to_owned(),
        lifecycle_state: "active".to_owned(),
        revision: 3,
        fields: serde_json::from_value(json!({ "title": "x", "tags": [] })).expect("fields"),
    };

    // Nobody presented one: unobservable, naming the address, not "the requirement failed".
    let unobserved = runtime
        .execute(&active, "implement", json!({ "actor": "timo" }))
        .expect_err("no evidence was presented");
    assert!(
        matches!(
            unobserved,
            CoreError::PreconditionUnobservable { ref unresolved, .. }
                if unresolved == &["$args.evidence.test_result".to_owned()]
        ),
        "{unobserved}"
    );

    // Somebody presented a count and it is short: a different refusal, for a different reader.
    let insufficient = runtime
        .execute(
            &active,
            "implement",
            json!({ "actor": "timo", "evidence": { "test_result": 0 } }),
        )
        .expect_err("zero is an observation");
    assert!(
        matches!(insufficient, CoreError::PreconditionFailed { .. }),
        "{insufficient}"
    );
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
