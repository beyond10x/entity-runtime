---
format: aep.planning-md/1
id: story:generated-entity-contracts-and-docs
kind: story
status: draft
title: Generate entity documentation and API contracts
summary: A definition set becomes a standalone human reference with OpenAPI and AsyncAPI in YAML and JSON.
relations:
- derived_from: epic:generated-entity-surfaces
- depends_on: story:multi-format-entity-graphs
revision: 2
---
# Story: Generate entity documentation and API contracts

## Acceptance

A command accepts a validated definition set and writes one standalone, generator-marked directory containing index.html, index.md, one HTML and Markdown page per entity, shared styling, OpenAPI 3.2 YAML and JSON, and AsyncAPI 3.1 YAML and JSON. Pages explain properties, versions, states, operations, rules, events, projections and references with diagrams. Existing non-generated directories are never replaced. The website publishes and tests a generated refund example.
