---
format: aep.planning-md/3
id: story:definition-migrations
kind: story
status: draft
title: Definition migrations between versions
summary: How an instance created under version n is carried to version n+1, and who advances it.
relations:
- derived_from: epic:kernel
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: crates/entity-core/src/definition.rs
- confidence: inferred
  path: crates/entity-core/src/error.rs
- confidence: inferred
  path: crates/entity-core/src/runtime.rs
- confidence: inferred
  path: crates/entity-core/src/validation.rs
- confidence: inferred
  path: crates/entity-core/tests/requirements.rs
- confidence: inferred
  path: crates/entity-store/src/asynchronous/encoding.rs
- confidence: inferred
  path: docs/design/kernel-v0.2.md
- confidence: cited
  path: docs/requirements.md
- confidence: inferred
  path: ess/domains/core.yaml
- confidence: inferred
  path: ess/scenarios/core
revision: 4
---
# Story: Definition migrations between versions

## Outcome

An instance created under `order` v1 can be carried to v2 deliberately, by a declared migration,
rather than by an instance silently reading a definition it was not created under (which R-45
refuses).

## Acceptance

A `migrations:` section or a migration document from `(entity, n)` to `(entity, n+1)` with field
mappings in the template language; a kernel function that applies it and returns a new instance at
the new version with revision +1 and a `Migrated` event; tests for a field rename and a default fill.
