---
format: aep.planning-md/1
id: epic:generated-entity-surfaces
kind: epic
status: draft
title: Generated entity surfaces for people and agents
summary: One validated definition set renders graphs, public contracts, an MCP tool surface and a definition-specific Rust command.
relations:
- decomposes: initiative:entity-runtime
- serves: vision:O2
revision: 2
---
# Epic: Generated entity surfaces for people and agents

## Outcome

One validated entity-definition set is the source for every surface an adopter or agent consumes: lifecycle and reference diagrams, human documentation, HTTP and event contracts, MCP tools and a purpose-built Rust command.

## Constraints

The kernel remains IO-free. Renderers are deterministic. Stores keep accepted state and events atomically. Every mutating agent-facing call requires caller provenance and an observed revision. Public website examples are executable and are pinned to their source definitions.
