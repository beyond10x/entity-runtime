---
format: aep.planning-md/1
id: epic:kernel
kind: epic
status: draft
title: 'The kernel: definitions as data, an IO-free decider'
summary: 'Grow entity-core from the 0.1 kernel toward the generalized object runtime: richer rules, references, replay, migrations, an SPI — without ever admitting IO.'
relations:
- decomposes: initiative:entity-runtime
revision: 2
---
# Epic: The kernel — definitions as data, an IO-free decider

## Outcome

`entity-core` grows from the 0.1 kernel — schema, lifecycle, operations, two rule scopes, a
thirteen-operator condition AST, templates — toward the generalized object runtime the vision
describes: references between entities, replay from events, migrations, an SPI, and rules that can
say *unknown*.

## Context

0.1 pins fifty requirements to tests (`docs/requirements.md`) and is gated by `task check`. Every
story here adds without breaking any row of that register, except `story:three-valued-conditions`,
which revises R-54 deliberately and says so.

## Acceptance

Each story lands with its requirement rows added to the register, its tests cited there, and the
gate green.
