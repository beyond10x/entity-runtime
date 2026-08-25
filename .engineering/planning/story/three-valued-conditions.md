---
format: aep.planning-md/1
id: story:three-valued-conditions
kind: story
status: draft
title: 'Three-valued rule evaluation: unknown is not false'
summary: A missing reference makes a rule Unknown, never False; a rule holds only when True; the refusal names which of the two it was.
relations:
- derived_from: epic:kernel
revision: 2
---
# Story: Three-valued rule evaluation — unknown is not false

## Outcome

A rule whose reference does not resolve evaluates to `Unknown`, not `False`; a rule holds only when
`True`; and the refusal says which of the two it was, so *nobody looked* is never reported as *it is
wrong*.

## Context

R-54 today: a missing reference makes a comparison `false`. Fine for a lifecycle ladder; wrong for
an evidence gate — `engineering-protocols` invariant 5 exists to keep the two apart. This is the one
story in the epic that changes kernel semantics rather than adding to them.

## Acceptance

A `Truth { True, False, Unknown }` result with Kleene `all`/`any`/`not`; `exists` stays the presence
test and returns `True`/`False` only; `PreconditionFailed` and `InvariantViolation` gain an
`Unobservable` counterpart (or a field) distinguishing the cases; R-54 is revised in the register
with the old and new wording; every existing test still passes except the ones that assert the old
collapse, which are rewritten to assert the new distinction.

## Out of scope

Changing the condition operators; three-valued *fields*.
