---
format: aep.planning-md/3
id: story:projections
kind: story
status: implemented
title: Projection definitions
summary: Declared folds over events for search and read models, executed by the shell.
relations:
- derived_from: epic:kernel
- decomposes: epic:the-shell
- depends_on: story:event-envelope
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-08-26T02:48:08Z", actor: "timo", revision: 5, imported: true}
- {from: "proposed", to: "active", at: "2026-08-26T02:48:08Z", actor: "timo", revision: 6, imported: true}
- {from: "active", to: "implemented", at: "2026-08-26T02:48:08Z", actor: "timo", revision: 7, decided_on: {"recorded":{"test_result":1}}, imported: true}
---
# Story: Projection definitions

## Outcome

A definition can declare folds over its events for read models and search — `by_status`,
`open_per_customer` — executed by the shell.

## Acceptance

A `projections:` section whose folds use the template language; a shell-side evaluator in the SPI
crate; a test that a sequence of decisions produces the declared read model.
