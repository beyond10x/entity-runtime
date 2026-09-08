---
format: aep.planning-md/1
id: story:generated-entity-rust-cli
kind: story
status: implemented
title: Generate a definition-specific Rust CLI binary
summary: Validated definitions compile into a host-platform Clap command with stored reads, events and direct operations.
relations:
- derived_from: epic:generated-entity-surfaces
- depends_on: story:generated-entity-contracts-and-docs
- serves: vision:O2
revision: 6
---
# Story: Generate a definition-specific Rust CLI binary

## Acceptance

The entity generate rust-cli command writes a retained Clap-derived Rust crate, embeds the validated definitions, builds it against a matching local runtime checkout with Cargo locked and offline, and installs the host binary at the requested path. The generated command exposes each entity with create, get, list, events and direct operation subcommands. Stored writes require provenance and operations require expected_revision. Replacement is restricted to generator-owned targets, and the website walks through the refund binary end to end.

## Implementation evidence

Moved to implemented on 2026-09-09 after an audit of every non-implemented artifact against `CHANGELOG.md`, `docs/roadmap.md` and the crates.

`entity generate rust-cli` builds a definition-specific Clap command from a retained crate with embedded definitions, locked and offline, and installs the executable Cargo reports building — `CHANGELOG.md` `## [0.16.0]`, path handling corrected in the 2026-09-08 review; `website/docs/guide/generated-cli.md`.
