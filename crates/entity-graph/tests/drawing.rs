//! What the drawings actually say, and that they say it the same way twice.

use entity_core::EntityDefinition;
use entity_graph::{render, Emphasis, Graph, Layout};
use serde_json::json;

fn definition(value: serde_json::Value) -> EntityDefinition {
    serde_json::from_value(value).expect("a well-formed definition")
}

/// The four-beat ladder, with a step back up it — which is the shape almost every real lifecycle
/// has and the reason the layout has to classify back edges at all.
fn story() -> EntityDefinition {
    definition(json!({
        "entity": "story",
        "schema": {},
        "lifecycle": { "initial": "draft", "states": ["draft", "proposed", "active", "done"] },
        "operations": {
            "propose":  { "transitions": [{ "from": "draft", "to": "proposed" }] },
            "return":   { "transitions": [{ "from": "proposed", "to": "draft" }] },
            "activate": { "transitions": [{ "from": "proposed", "to": "active" }] },
            "finish":   { "transitions": [{ "from": "active", "to": "done" }] }
        }
    }))
}

#[test]
fn a_lifecycle_draws_its_states_its_way_in_and_the_rungs_nothing_leaves() {
    let graph = Graph::lifecycle(&story());
    assert_eq!(graph.title, "story v1");
    assert_eq!(graph.nodes.len(), 4);
    assert_eq!(graph.edges.len(), 4);

    let emphasis = |id: &str| {
        graph
            .nodes
            .iter()
            .find(|node| node.id == id)
            .unwrap_or_else(|| panic!("{id} is a node"))
            .emphasis
    };
    assert_eq!(emphasis("draft"), Emphasis::Entry, "the initial state");
    assert_eq!(emphasis("proposed"), Emphasis::Plain);
    assert_eq!(
        emphasis("done"),
        Emphasis::Terminal,
        "nothing leaves it, which a list of edges hides"
    );
}

/// `proposed -> draft` runs back up the ladder. Exactly one of the four edges is a back edge, and
/// the layering ignores it — otherwise the longest-path relaxation would never settle.
#[test]
fn a_rung_that_goes_back_up_the_ladder_is_classified_and_does_not_break_the_layering() {
    let graph = Graph::lifecycle(&story());
    let layout = Layout::of(&graph);

    assert_eq!(layout.back_edges.len(), 1, "{:?}", layout.back_edges);
    let back = &graph.edges[*layout.back_edges.iter().next().expect("one")];
    assert_eq!(
        (back.from.as_str(), back.to.as_str()),
        ("proposed", "draft")
    );

    // draft | proposed | active | done — four layers, one node each.
    assert_eq!(layout.width(), 4);
    assert_eq!(layout.height(), 1);
}

/// The property the whole crate is arranged around: the same definition gives the same bytes.
#[test]
fn every_format_is_the_same_bytes_twice() {
    let graph = Graph::lifecycle(&story());
    let layout = Layout::of(&graph);
    for (name, first, second) in [
        ("text", render::text(&graph), render::text(&graph)),
        ("dot", render::dot(&graph), render::dot(&graph)),
        (
            "svg",
            render::svg(&graph, &layout),
            render::svg(&graph, &layout),
        ),
        (
            "html",
            render::html(&graph, &layout),
            render::html(&graph, &layout),
        ),
    ] {
        assert_eq!(first, second, "{name} is not deterministic");
        assert!(!first.is_empty(), "{name} drew nothing");
    }

    // And rebuilding from the definition gives the same answer as the first build did.
    let again = Graph::lifecycle(&story());
    assert_eq!(graph, again);
    assert_eq!(layout, Layout::of(&again));
}

/// The picture that could not be drawn before typed references existed.
#[test]
fn references_draw_the_types_as_boxes_and_the_ref_fields_as_edges() {
    let epic = definition(json!({
        "entity": "epic",
        "schema": { "fields": { "stories": {
            "type": "array", "items": { "type": "ref", "entity": "story" }
        }}},
        "lifecycle": { "initial": "draft", "states": ["draft"] },
        "operations": { "touch": { "transitions": [{ "from": "draft", "to": "draft" }] } }
    }));
    let story = definition(json!({
        "entity": "story",
        "schema": { "fields": { "epic": { "type": "ref", "entity": "epic" } } },
        "lifecycle": { "initial": "draft", "states": ["draft"] },
        "operations": { "adopt": {
            "arguments": { "fields": { "by": { "type": "ref", "entity": "person" } } },
            "transitions": [{ "from": "draft", "to": "draft" }]
        }}
    }));

    let graph = Graph::references([&epic, &story]);
    let edge = |from: &str, label: &str| {
        graph
            .edges
            .iter()
            .find(|edge| edge.from == from && edge.label == label)
            .map(|edge| edge.to.clone())
    };
    assert_eq!(
        edge("epic", "stories[]").as_deref(),
        Some("story"),
        "through an array's items, and the `[]` says so — a bare `stories` and a list of them are \
         two labels, not one that overwrites the other"
    );
    assert_eq!(edge("story", "epic").as_deref(), Some("epic"));
    assert_eq!(
        edge("story", "adopt: by").as_deref(),
        Some("person"),
        "an operation's arguments are references too, and the label says which operation"
    );

    // `person` is nobody's definition here. It is still drawn, marked as a type this set does not
    // declare — leaving it out would hide exactly what `Registry::validate_all` refuses.
    let person = graph
        .nodes
        .iter()
        .find(|node| node.id == "person")
        .expect("a target with no definition is still a box");
    assert_eq!(person.emphasis, Emphasis::Terminal);
    assert_eq!(
        graph
            .nodes
            .iter()
            .find(|node| node.id == "epic")
            .map(|node| node.emphasis),
        Some(Emphasis::Plain)
    );
}

/// R-95: a name carrying a quote or a backslash must not close the string it is written into.
#[test]
fn a_name_that_would_close_the_string_it_is_written_into_is_escaped_in_every_format() {
    let awkward = definition(json!({
        "entity": "quote\"test",
        "schema": {},
        "lifecycle": { "initial": "he said \"no\"", "states": ["he said \"no\"", "back\\slash"] },
        "operations": { "go": { "transitions": [
            { "from": "he said \"no\"", "to": "back\\slash" }
        ]}}
    }));
    let graph = Graph::lifecycle(&awkward);
    let layout = Layout::of(&graph);

    let dot = render::dot(&graph);
    assert!(dot.contains(r#"\"no\""#), "{dot}");
    assert!(dot.contains(r"back\\slash"), "{dot}");

    for drawing in [render::svg(&graph, &layout), render::html(&graph, &layout)] {
        assert!(drawing.contains("&quot;"), "the quote is escaped");
        assert!(
            !drawing.contains(r#">he said "no"<"#),
            "a raw quote reached the document"
        );
    }
}

/// A graph that is one closed loop has no entry and no node without an incoming edge. The search
/// still has to visit it and the relaxation still has to stop.
#[test]
fn a_lifecycle_that_is_one_closed_loop_still_lays_out() {
    let ring = definition(json!({
        "entity": "ring",
        "schema": {},
        "lifecycle": { "initial": "a", "states": ["a", "b", "c"] },
        "operations": {
            "one":   { "transitions": [{ "from": "a", "to": "b" }] },
            "two":   { "transitions": [{ "from": "b", "to": "c" }] },
            "three": { "transitions": [{ "from": "c", "to": "a" }] }
        }
    }));
    let graph = Graph::lifecycle(&ring);
    let layout = Layout::of(&graph);
    assert_eq!(layout.back_edges.len(), 1, "one edge closes the ring");
    assert_eq!(layout.width(), 3);
    assert_eq!(
        layout.layers.iter().map(Vec::len).sum::<usize>(),
        3,
        "every node is placed exactly once"
    );
}

/// A self-loop — `touch: draft -> draft` — is an edge whose ends are the same box.
#[test]
fn a_self_loop_is_drawn_rather_than_collapsed_to_nothing() {
    let idle = definition(json!({
        "entity": "idle",
        "schema": {},
        "lifecycle": { "initial": "here", "states": ["here"] },
        "operations": { "touch": { "transitions": [{ "from": "here", "to": "here" }] } }
    }));
    let graph = Graph::lifecycle(&idle);
    let layout = Layout::of(&graph);
    assert_eq!(layout.back_edges.len(), 1);
    let svg = render::svg(&graph, &layout);
    assert!(svg.contains(" C"), "a self-loop is drawn as a curve: {svg}");
}

/// Two references that used to collapse into one edge, dropping the other silently — and the
/// dropped one was the dangling reference `Registry::validate_all` refuses. Found by an
/// independent review after 0.4.0 shipped.
#[test]
fn two_references_that_read_the_same_are_two_edges() {
    let story = definition(json!({
        "entity": "story",
        "schema": { "fields": {
            "a": { "type": "object", "properties": { "b": { "type": "ref", "entity": "epic" } } },
            "a.b": { "type": "ref", "entity": "person" }
        }},
        "lifecycle": { "initial": "draft", "states": ["draft"] },
        "operations": { "touch": { "transitions": [{ "from": "draft", "to": "draft" }] } }
    }));
    let graph = Graph::references([&story]);
    let targets: Vec<&str> = graph.edges.iter().map(|edge| edge.to.as_str()).collect();
    assert!(targets.contains(&"epic"), "{targets:?}");
    assert!(targets.contains(&"person"), "{targets:?}");
    assert_eq!(graph.edges.len(), 2, "neither may overwrite the other");
}

/// XML 1.0 permits no escape for most control characters, so they are replaced rather than
/// escaped. Before this, a name carrying `U+0001` produced an SVG no parser accepts from a
/// definition `entity validate` had passed.
#[test]
fn a_control_character_cannot_reach_the_document() {
    let awkward = definition(json!({
        "entity": "ctrl",
        "schema": {},
        "lifecycle": { "initial": "a\u{1}b", "states": ["a\u{1}b"] },
        "operations": { "go": { "transitions": [{ "from": "a\u{1}b", "to": "a\u{1}b" }] } }
    }));
    let graph = Graph::lifecycle(&awkward);
    let layout = Layout::of(&graph);
    for drawing in [render::svg(&graph, &layout), render::html(&graph, &layout)] {
        assert!(
            !drawing.contains('\u{1}'),
            "a raw control character reached the document"
        );
        assert!(
            drawing.contains('\u{FFFD}'),
            "and it is visible rather than dropped"
        );
    }
}

/// `Graph`'s fields are public, so a caller can build a node list the constructors never would.
/// Layout and renderer must then agree about which node an edge names.
#[test]
fn a_duplicate_node_id_is_read_the_same_way_by_the_layout_and_the_renderer() {
    let graph = Graph {
        title: "dup".to_owned(),
        nodes: vec![
            entity_graph::Node {
                id: "a".into(),
                label: "a".into(),
                emphasis: Emphasis::Plain,
            },
            entity_graph::Node {
                id: "b".into(),
                label: "b".into(),
                emphasis: Emphasis::Plain,
            },
            entity_graph::Node {
                id: "a".into(),
                label: "a2".into(),
                emphasis: Emphasis::Plain,
            },
        ],
        edges: vec![entity_graph::Edge {
            from: "a".into(),
            to: "b".into(),
            label: "x".into(),
        }],
    };
    let layout = Layout::of(&graph);
    // The layout indexes `a` as the last of the two; the renderer must too, or it draws the arrow
    // out of a box the layout put somewhere else.
    assert_eq!(
        layout.layer_of(2),
        Some(0),
        "the last `a` is the one the layout placed"
    );
    assert!(
        render::svg(&graph, &layout).contains("a2"),
        "and the renderer drew it"
    );
}

#[test]
fn an_empty_graph_draws_nothing_and_does_not_panic() {
    let graph = Graph::default();
    let layout = Layout::of(&graph);
    assert!(graph.is_empty());
    assert_eq!(layout.width(), 0);
    assert!(render::svg(&graph, &layout).starts_with("<svg"));
}
