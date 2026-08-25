//! What is being drawn: nodes, labelled edges, and which nodes are ways in and out.
//!
//! Deliberately small and deliberately *not* about entities. A layout that knew what a lifecycle
//! state was would have to learn what an entity type was to draw the second picture, and then what
//! the third one is. The two builders below are the only place this crate knows either.

use std::collections::BTreeSet;

use entity_core::{EntityDefinition, FieldDefinition, FieldKind, ObjectSchema};

/// How a node is drawn, which is the only thing a renderer needs to know about its meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Emphasis {
    /// An ordinary node.
    #[default]
    Plain,
    /// Where a reader starts: a lifecycle's initial state.
    Entry,
    /// A node nothing leaves. In a lifecycle that is a terminal state, and it is worth showing —
    /// *nothing moves from here* is a fact about the ladder that a list of edges hides.
    Terminal,
}

/// One node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// Unique within the graph, and what edges name.
    pub id: String,
    /// What is written in the box. May differ from the id, and may be empty.
    pub label: String,
    /// How it is drawn.
    pub emphasis: Emphasis,
}

/// One directed, labelled edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    /// The node it leaves.
    pub from: String,
    /// The node it arrives at.
    pub to: String,
    /// What is written along it — an operation name, a field name.
    pub label: String,
}

/// A drawing, before anything has decided where to put it.
///
/// Nodes and edges are held in a stable order — the order the builders produce them in, which is
/// itself derived from `BTreeMap`/`BTreeSet` iteration — so two runs over the same definition
/// produce the same bytes. That is the whole reason there is no `HashMap` in this crate.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Graph {
    /// What the drawing is of.
    pub title: String,
    /// The nodes, in a stable order.
    pub nodes: Vec<Node>,
    /// The edges, in a stable order.
    pub edges: Vec<Edge>,
}

impl Graph {
    /// The lifecycle of one definition: states as nodes, operations as the edges between them.
    ///
    /// This is what `entity graph` has always drawn. An operation with several `from` states
    /// produces several edges carrying the same label, because that is what it is.
    #[must_use]
    pub fn lifecycle(definition: &EntityDefinition) -> Self {
        let mut edges = Vec::new();
        for (name, operation) in &definition.operations {
            for transition in &operation.transitions {
                for from in transition.from.as_slice() {
                    edges.push(Edge {
                        from: from.clone(),
                        to: transition.to.clone(),
                        label: name.clone(),
                    });
                }
            }
        }
        edges.sort_by(|left, right| {
            (&left.from, &left.label, &left.to).cmp(&(&right.from, &right.label, &right.to))
        });

        let leaves: BTreeSet<&str> = edges.iter().map(|edge| edge.from.as_str()).collect();
        let nodes = definition
            .lifecycle
            .states
            .iter()
            .map(|state| Node {
                id: state.clone(),
                label: state.clone(),
                emphasis: if state == &definition.lifecycle.initial {
                    Emphasis::Entry
                } else if leaves.contains(state.as_str()) {
                    Emphasis::Plain
                } else {
                    Emphasis::Terminal
                },
            })
            .collect();

        Self {
            title: format!("{} v{}", definition.entity, definition.version),
            nodes,
            edges,
        }
    }

    /// The references between a set of definitions: entity types as nodes, `ref` fields as edges.
    ///
    /// The picture nobody could draw before typed references existed, and the reason they were
    /// built first. An edge is labelled with the field that declares it, so `story --epic--> epic`
    /// reads as the definition does.
    ///
    /// A target type nothing in the set declares still gets a node, drawn [`Emphasis::Terminal`]:
    /// leaving it out would silently turn a dangling reference into a graph with one fewer box,
    /// and a drawing that hides the thing `Registry::validate_all` refuses is worse than no drawing.
    #[must_use]
    pub fn references<'a>(definitions: impl IntoIterator<Item = &'a EntityDefinition>) -> Self {
        let mut edges = Vec::new();
        let mut declared = BTreeSet::new();
        for definition in definitions {
            declared.insert(definition.entity.clone());
            let mut found = Vec::new();
            collect(&definition.schema, "", &mut found);
            for (name, operation) in &definition.operations {
                collect(&operation.arguments, &format!("{name}: "), &mut found);
            }
            for (label, target) in found {
                edges.push(Edge {
                    from: definition.entity.clone(),
                    to: target,
                    label,
                });
            }
        }
        edges.sort_by(|left, right| {
            (&left.from, &left.label, &left.to).cmp(&(&right.from, &right.label, &right.to))
        });
        // Two files declaring the same entity drew the same edge twice, and two overlaid labels.
        edges.dedup();

        let mut ids: BTreeSet<String> = declared.clone();
        ids.extend(edges.iter().map(|edge| edge.to.clone()));
        let nodes = ids
            .into_iter()
            .map(|id| Node {
                emphasis: if declared.contains(&id) {
                    Emphasis::Plain
                } else {
                    Emphasis::Terminal
                },
                label: id.clone(),
                id,
            })
            .collect();

        Self {
            title: "references".to_owned(),
            nodes,
            edges,
        }
    }

    /// Whether the graph has nothing to draw.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// Every `ref` a schema declares, as `label -> target`, at any depth.
///
/// A `Vec`, not a map keyed by label. A map dropped an edge whenever two references produced the
/// same display label — a nested `a` → `b` and a field literally named `a.b` both read as `a.b`,
/// and the second silently replaced the first. The drawing then hid a dangling reference that
/// `Registry::validate_all` refuses, which is the one thing this picture must never do. Found by an
/// independent review after 0.4.0 shipped.
///
/// Array items append `[]`, as `entity_core`'s own `relation_targets` does, so a list of refs and a
/// bare ref of the same name are two labels rather than one.
fn collect(schema: &ObjectSchema, prefix: &str, found: &mut Vec<(String, String)>) {
    for (name, field) in &schema.fields {
        collect_field(field, &format!("{prefix}{name}"), found);
    }
}

fn collect_field(field: &FieldDefinition, label: &str, found: &mut Vec<(String, String)>) {
    if field.kind == FieldKind::Ref {
        if let Some(target) = &field.entity {
            found.push((label.to_owned(), target.clone()));
        }
    }
    if let Some(items) = &field.items {
        collect_field(items, &format!("{label}[]"), found);
    }
    for (name, property) in &field.properties {
        collect_field(property, &format!("{label}.{name}"), found);
    }
}
