---
format: aep.planning-md/1
id: story:multi-format-entity-graphs
kind: story
status: draft
title: Render entity graphs for terminals, Mermaid and Graphviz
summary: Lifecycle and reference graphs render deterministically as text, Mermaid, DOT, SVG and HTML.
relations:
- derived_from: epic:generated-entity-surfaces
revision: 2
---
# Story: Render entity graphs for terminals, Mermaid and Graphviz

## Acceptance

The graph model records whether it represents a lifecycle or references. The entity graph command renders deterministic text, Mermaid, DOT, SVG and HTML. Lifecycle Mermaid uses stateDiagram-v2; references use a flowchart. Names cannot inject syntax in any format, and the refund website quickstart renders the exact shipped Mermaid output.
