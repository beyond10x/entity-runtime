---
format: aep.planning-md/3
id: story:explain-verb
kind: story
status: draft
title: 'entity explain: why an operation is or is not permitted from here'
summary: Per-rule verdicts for an operation against an instance, without executing it; the kernel exposes the verdicts, the CLI renders them.
relations:
- derived_from: epic:kernel
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: crates/entity-cli/src/cli.rs
- confidence: inferred
  path: crates/entity-cli/src/main.rs
- confidence: cited
  path: crates/entity-cli/tests/cli.rs
- confidence: inferred
  path: crates/entity-core/src/lib.rs
- confidence: inferred
  path: crates/entity-core/src/runtime.rs
- confidence: inferred
  path: crates/entity-runtime-docs/src/status.rs
- confidence: inferred
  path: docs/requirements.md
- confidence: inferred
  path: ess/domains/core.yaml
- confidence: inferred
  path: ess/scenarios/core
- confidence: cited
  path: examples/order.yaml
- confidence: inferred
  path: website/docs/reference/cli.md
revision: 4
---
# Story: entity explain — why an operation is or is not permitted from here

## Outcome

`entity explain --definition … --instance … --operation …` prints, without executing: whether a
transition exists from the current state, and each precondition's verdict, so a person or an agent
sees *what would unlock this* rather than one refusal at a time.

## Context

Mirrors `protocol explain` in `engineering-protocols`. Needs the kernel to expose per-rule verdicts;
with `story:three-valued-conditions` each verdict is `True`/`False`/`Unknown`.

## Acceptance

A kernel function returning the transition selection and every precondition's verdict with the
rule's name and message; the verb rendering it in text and JSON; a CLI test on the order example
showing `approve` from `submitted` with `total_cents: 0` as one failing rule named `positive_total`.
