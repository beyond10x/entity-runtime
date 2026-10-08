---
format: aep.planning-md/3
id: story:named-predicates
kind: story
status: draft
title: Named reusable predicates
summary: A definition declares predicates once and rules reference them by name.
relations:
- derived_from: epic:kernel
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: cited
  path: crates/entity-core/src/definition.rs
- confidence: inferred
  path: crates/entity-core/src/error.rs
- confidence: inferred
  path: crates/entity-core/src/runtime.rs
- confidence: inferred
  path: crates/entity-core/src/validation.rs
- confidence: inferred
  path: crates/entity-core/tests/requirements.rs
- confidence: cited
  path: crates/entity-runtime-docs/src/status.rs
- confidence: cited
  path: docs/requirements.md
- confidence: inferred
  path: ess/coverage.json
- confidence: inferred
  path: ess/generated
- confidence: inferred
  path: ess/scenarios/core
- confidence: inferred
  path: website/docs/reference/definitions.md
- confidence: inferred
  path: website/docs/reference/refusals.md
revision: 4
---
# Story: Named reusable predicates

## Outcome

A definition declares `predicates:` once and rules reference them by name, so *is_estimated* is
written once and used by three operations.

## Acceptance

`predicates: { is_estimated: { gt: [$fields.points, 0] } }` and `assert: { use: is_estimated }`
parse, validate (unknown name refused at registration; scopes still enforced per use) and evaluate
identically to the inlined condition; a test proves the equivalence.
