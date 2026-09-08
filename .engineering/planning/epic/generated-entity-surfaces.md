---
format: aep.planning-md/1
id: epic:generated-entity-surfaces
kind: epic
status: implemented
title: Generated entity surfaces for people and agents
summary: One validated definition set renders graphs, public contracts, an MCP tool surface and a definition-specific Rust command.
relations:
- decomposes: initiative:entity-runtime
- serves: vision:O2
revision: 6
---
# Epic: Generated entity surfaces for people and agents

## Outcome

One validated entity-definition set is the source for every surface an adopter or agent consumes: lifecycle and reference diagrams, human documentation, HTTP and event contracts, MCP tools and a purpose-built Rust command.

## Constraints

The kernel remains IO-free. Renderers are deterministic. Stores keep accepted state and events atomically. Every mutating agent-facing call requires caller provenance and an observed revision. Public website examples are executable and are pinned to their source definitions.

## Implementation evidence

Moved to implemented on 2026-09-09 after an audit of every non-implemented artifact against `CHANGELOG.md`, `docs/roadmap.md` and the crates.

All four surfaces exist from one validated definition set: graphs in five formats, a documentation bundle with OpenAPI 3.2 and AsyncAPI 3.1, MCP tools, and a generated Rust CLI — `CHANGELOG.md` `## [0.16.0]` Added, hardened again in 0.17.7 and the 2026-09-08 review.
