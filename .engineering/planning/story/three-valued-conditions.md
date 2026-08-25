---
format: aep.planning-md/1
id: story:three-valued-conditions
kind: story
status: draft
title: 'Three-valued rule evaluation: unknown is not false'
summary: A missing reference makes a rule Unknown, never False; a rule holds only when True; the refusal names which of the two it was.
relations:
- derived_from: epic:kernel
revision: 3
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

## Decided 2026-08-25

Three questions the first draft left open, decided by the operator before the type ships because each
is cheap now and expensive afterwards:

1. **A present `null` is `Unknown`, and `exists` sees it that way too.** A key that is present with
   nothing after it is the YAML spelling of *nobody filled this in*, so it reads as unobserved rather
   than as absent-and-therefore-false. `exists` consequently returns `True`/`False`/`Unknown`, which
   revises this story's earlier sentence that it stays two-valued. Schema validation still refuses a
   null against a declared type before any rule runs (`runtime.rs:236`); this decision is what covers
   a `kind: json` field, where null is a legal value and the AEP body is modelled as json in the
   first step of the mapping.
2. **An `Unobservable` refusal names every unresolved reference, not the first.** One message the
   operator can act on once, rather than three refusals in sequence. `all`/`any` therefore evaluate
   every operand when the result is `Unknown` — R-54's deterministic short-circuit clause is revised
   to say so, and the truth value is unchanged either way because Kleene is order-independent.
3. **The refusal carries the addresses as data, not prose.** `CoreError::PreconditionFailed` today
   carries `rule: Option<String>` and a `message` (`error.rs:318-326`) and has no field for what did
   not resolve. Its `Unobservable` counterpart carries the unresolved paths, because telling somebody
   to go and observe without naming what to observe is the prose-rule failure this whole programme
   exists to end.

## Acceptance

A `Truth { True, False, Unknown }` result with Kleene `all`/`any`/`not`; a present `null` and a
missing reference both yield `Unknown`, including under `exists`; `PreconditionFailed` and
`InvariantViolation` gain an `Unobservable` counterpart carrying every unresolved path;
`all`/`any` evaluate all operands when the outcome is `Unknown`; R-54 is revised in the register with
the old and new wording; every existing test still passes except the ones that assert the old
collapse, which are rewritten to assert the new distinction.

## Out of scope

Changing the condition operators; three-valued *fields* — a field holds a value or it does not, and
`Unknown` is a property of an evaluation, not of storage.
