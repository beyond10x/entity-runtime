---
format: aep.planning-md/1
id: story:validated-definition-boundary
kind: story
status: implemented
title: Only validated definitions reach execution
summary: Registration accumulates every definition defect and produces the sole handle accepted by kernel entry points.
relations:
- decomposes: epic:kernel
- serves: vision:O2
revision: 5
---
## Context

Public free functions currently accept raw definitions, while default handling, duplicate YAML keys, nested paths and single-file CLI validation leave routes around registration.

## Acceptance

Every create, execute and replay entry point requires a `ValidatedDefinition`, and registration reports every independent defect across schemas, defaults, rules, templates and projections before storing nothing on refusal.

## Implementation evidence

Moved to implemented on 2026-09-09 after an audit of every non-implemented artifact against `CHANGELOG.md`, `docs/roadmap.md` and the crates.

Kernel entry points take only `ValidatedDefinition` (`crates/entity-core/src/registry.rs`); registration accumulates every independent defect and stores nothing on refusal (`DefinitionErrors`).
