---
format: aep.planning-md/1
id: story:accumulating-definition-validation
kind: story
status: draft
title: Definition validation accumulates every defect
summary: Registry::register reports every DefinitionError of a document at once instead of the first.
relations:
- derived_from: epic:kernel
revision: 2
---
# Story: Definition validation accumulates every defect

## Outcome

`Registry::register` on a document with four defects reports four `DefinitionError`s, not one per
attempt.

## Acceptance

`register` returns `Result<(), Vec<DefinitionError>>` or an error type carrying the list; a test
registers a definition with an unknown initial state, an ambiguous transition and an invalid default
and asserts exactly three errors with their paths; `entity validate` prints them all; R-13's row
cites the new test.
