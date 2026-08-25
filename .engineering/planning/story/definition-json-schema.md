---
format: aep.planning-md/1
id: story:definition-json-schema
kind: story
status: draft
title: A JSON Schema for the definition format, generated from the Rust types
summary: entity schema emits the schema; the gate checks the committed copy against the types so editors and adopters validate definitions before registering them.
relations:
- derived_from: epic:kernel
revision: 2
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
