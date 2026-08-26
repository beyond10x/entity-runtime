---
format: aep.planning-md/1
id: story:accumulating-definition-validation
kind: story
status: implemented
title: Definition validation accumulates every defect
summary: Registry::register reports every DefinitionError of a document at once instead of the first.
relations:
- derived_from: epic:kernel
revision: 6
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

**Built 2026-08-25.** `Registry::register`, `Registry::replace` and `EntityDefinition::validate`
return `DefinitionErrors`, a non-empty list; `CoreError::Definition` carries it; `entity validate`
prints one line per defect. `definition_validation_reports_every_defect_not_the_first` asserts
exactly three with their paths, and R-13 cites it.

One thing the acceptance did not ask for and the code needed: **a check whose prerequisite already
failed is skipped.** A ladder with a duplicate rung would otherwise report a second finding per
transition in the document — *state `open` is not declared* — burying the defect that caused them.
`a_broken_ladder_is_reported_once_and_does_not_cascade_through_every_transition` pins that. A
cascade is worse than a short list.

`DefinitionErrors` compares equal to a single `DefinitionError` only when it carries exactly that
one, so the thirty single-defect assertions already in the suite kept working *and* became
assertions that there were no other defects.
