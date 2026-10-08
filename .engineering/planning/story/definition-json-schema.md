---
format: aep.planning-md/3
id: story:definition-json-schema
kind: story
status: draft
title: A JSON Schema for the definition format, generated from the Rust types
summary: entity schema emits the schema; the gate checks the committed copy against the types so editors and adopters validate definitions before registering them.
relations:
- derived_from: epic:kernel
scope:
- confidence: inferred
  path: .github/workflows/gate.yml
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: Cargo.lock
- confidence: inferred
  path: Cargo.toml
- confidence: cited
  path: Taskfile.yml
- confidence: inferred
  path: crates/entity-cli/Cargo.toml
- confidence: cited
  path: crates/entity-cli/src/cli.rs
- confidence: inferred
  path: crates/entity-cli/src/main.rs
- confidence: cited
  path: crates/entity-core/src/definition.rs
- confidence: cited
  path: examples/order.yaml
- confidence: cited
  path: schemas/
- confidence: inferred
  path: website/docs/reference/cli.md
revision: 4
---
# Story: A JSON Schema for the definition format, generated from the Rust types

## Outcome

Editors and adopters validate a definition document before registering it, against a schema the
gate proves is current.

## Acceptance

`entity schema` prints the JSON Schema for `EntityDefinition`, derived from the Rust types
(`schemars` or equivalent; the one new dependency is justified in the manifest); the schema is
committed under `schemas/` and a gate step fails when it differs from what the types produce;
`examples/order.yaml` validates against it.
