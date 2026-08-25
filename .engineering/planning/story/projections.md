---
format: aep.planning-md/1
id: story:projections
kind: story
status: draft
title: Projection definitions
summary: Declared folds over events for search and read models, executed by the shell.
relations:
- derived_from: epic:kernel
revision: 2
---
# Story: Projection definitions

## Outcome

A definition can declare folds over its events for read models and search — `by_status`,
`open_per_customer` — executed by the shell.

## Acceptance

A `projections:` section whose folds use the template language; a shell-side evaluator in the SPI
crate; a test that a sequence of decisions produces the declared read model.
