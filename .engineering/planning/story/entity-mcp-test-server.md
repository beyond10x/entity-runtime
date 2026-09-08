---
format: aep.planning-md/1
id: story:entity-mcp-test-server
kind: story
status: implemented
title: Mount entities as MCP tools for agent evaluation
summary: A local stdio MCP server exposes stored reads, creates and named operations with schemas and typed refusals.
relations:
- derived_from: epic:generated-entity-surfaces
- depends_on: story:generated-entity-contracts-and-docs
- serves: vision:O2
revision: 6
---
# Story: Mount entities as MCP tools for agent evaluation

## Acceptance

The entity mcp command serves newline-delimited JSON-RPC over stdio for MCP 2026-07-28 and the 2025-11-25 initialization era. It exposes entity.create, entity.get, entity.list, entity.events and every non-conflicting named operation. Writes use File Store v2, require provenance, and operations require expected_revision. Tool failures are structured and actionable; protocol failures remain JSON-RPC errors; stdout contains protocol messages only. The public site includes a complete refund setup and call transcript.

## Implementation evidence

Moved to implemented on 2026-09-09 after an audit of every non-implemented artifact against `CHANGELOG.md`, `docs/roadmap.md` and the crates.

`crates/entity-mcp` serves both protocol eras (`MODERN`, `LEGACY` in `src/lib.rs`), per-entity create/get/list/events plus named operations, requires provenance and `expected_revision`, keeps stdout protocol-only, and has its guide page `website/docs/guide/mcp.md`.
