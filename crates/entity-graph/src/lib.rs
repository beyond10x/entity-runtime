//! Drawings of what a definition says.
//!
//! `entity-core` holds an entity type as data so it can be *validated when it is written and
//! evaluated identically everywhere*. The third thing the vision claims for it is that a definition
//! can be **rendered by tooling that never parses code** — and until this crate existed the only
//! rendering was a list of arrows.
//!
//! Two pictures, and the second is new:
//!
//! | [`Graph::lifecycle`] | states as boxes, operations as the edges between them |
//! | [`Graph::references`] | entity types as boxes, `ref` fields as the edges between them |
//!
//! The second was impossible before typed references, which is why they were built first.
//!
//! ```
//! use entity_graph::{render, Graph, Layout};
//!
//! let definition: entity_core::EntityDefinition = serde_json::from_value(serde_json::json!({
//!     "entity": "light",
//!     "schema": {},
//!     "lifecycle": { "initial": "off", "states": ["off", "on"] },
//!     "operations": {
//!         "switch_on":  { "transitions": [{ "from": "off", "to": "on" }] },
//!         "switch_off": { "transitions": [{ "from": "on", "to": "off" }] }
//!     }
//! }))?;
//!
//! let graph = Graph::lifecycle(&definition);
//! assert_eq!(graph.edges.len(), 2);
//!
//! // `on -> off` runs back up the ladder, so exactly one of the two is a back edge.
//! let layout = Layout::of(&graph);
//! assert_eq!(layout.back_edges.len(), 1);
//! assert_eq!(layout.width(), 2, "off, then on");
//!
//! assert!(render::text(&graph).starts_with("light v1: initial off"));
//! assert!(render::svg(&graph, &layout).starts_with("<svg"));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # What this crate refuses to do
//!
//! **No layout engine.** Calling graphviz would make the drawing depend on which `dot` is
//! installed, so a picture could change without the definition changing — and a picture nobody can
//! reproduce is not reviewable. The layering is integer arithmetic in [`Layout`].
//!
//! **No IO, no clock, no floating point**, for the same reason and checked the same way
//! `entity-core`'s purity is: `tests/boundary.rs` scans this crate's own sources. The one
//! dependency is `entity-core`, asserted by a test that reads the manifest — a renderer that could
//! reach a filesystem is one that could draw something the definition does not say.
//!
//! **No opinion about entities.** [`Graph`] knows about nodes and labelled edges. Everything this
//! crate knows about lifecycles and references lives in the two constructors, so a third picture is
//! a third constructor rather than a change to the layout.

mod graph;
mod layout;
pub mod render;

pub use graph::{Edge, Emphasis, Graph, GraphKind, Node};
pub use layout::Layout;
