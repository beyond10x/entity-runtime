---
format: aep.planning-md/3
id: story:multi-format-entity-graphs
kind: story
status: implemented
title: Render entity graphs for terminals, Mermaid and Graphviz
summary: Lifecycle and reference graphs render deterministically as text, Mermaid, DOT, SVG and HTML.
relations:
- derived_from: epic:generated-entity-surfaces
- serves: vision:O2
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-09-08T23:39:12Z", actor: "human:timo", revision: 4, imported: true}
- {from: "proposed", to: "active", at: "2026-09-08T23:39:12Z", actor: "human:timo", revision: 5, imported: true}
- {from: "active", to: "implemented", at: "2026-09-08T23:40:27Z", actor: "human:timo", revision: 6, decided_on: {"recorded":{"test_result":1}}, imported: true}
---
# Story: Render entity graphs for terminals, Mermaid and Graphviz

## Acceptance

The graph model records whether it represents a lifecycle or references. The entity graph command renders deterministic text, Mermaid, DOT, SVG and HTML. Lifecycle Mermaid uses stateDiagram-v2; references use a flowchart. Names cannot inject syntax in any format, and the refund website quickstart renders the exact shipped Mermaid output.

## Implementation evidence

Moved to implemented on 2026-09-09 after an audit of every non-implemented artifact against `CHANGELOG.md`, `docs/roadmap.md` and the crates.

`GraphKind` in `crates/entity-graph/src/graph.rs` records lifecycle versus references; `render.rs` emits text, Mermaid, DOT, SVG and HTML with escaped labels — `CHANGELOG.md` `## [0.16.0]`.
