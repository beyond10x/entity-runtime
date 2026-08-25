---
format: aep.planning-md/1
id: story:three-valued-conditions
kind: story
status: draft
title: 'Three-valued rule evaluation: unknown is not false'
summary: A missing reference makes a rule Unknown, never False; a rule holds only when True; the refusal names which of the two it was.
relations:
- derived_from: epic:kernel
revision: 5
---
# Story: Three-valued rule evaluation — unknown is not false

## Outcome

A rule that compares against a reference which does not resolve evaluates to `Unknown`, not
`False`; a rule holds only when `True`; and the refusal says which of the two it was, so *nobody
looked* is never reported as *it is wrong*. Asking whether the reference is there at all stays a
two-valued question, because the kernel can always answer it.

## Context

R-54 today: a missing reference makes a comparison `false`. Fine for a lifecycle ladder; wrong for
an evidence gate — `engineering-protocols` invariant 5 exists to keep the two apart. This is the one
story in the epic that changes kernel semantics rather than adding to them.

## Decided 2026-08-25

Three questions the first draft left open, decided by the operator before the type ships because each
is cheap now and expensive afterwards:

1. **A present `null` is not a value.** A key that is present with nothing after it is the YAML
   spelling of *nobody filled this in*, so it must not satisfy a gate. Schema validation still
   refuses a null against a declared type before any rule runs (`runtime.rs:236`); this decision is
   what covers a `kind: json` field, where null is a legal value and the AEP body is modelled as
   json in the first step of the mapping.

   **Amended when it was built (2026-08-25).** As first decided this read *"`Unknown`, and `exists`
   sees it that way too"* — making `exists` three-valued and never `False`, with a two-valued
   `absent` operator beside it to carry *this must not be set*. That pair was not each other's
   negation, which is a rule nobody can hold in their head, and it had the kernel claiming it could
   not tell whether a field was set — which is false, since it holds the instance. `Unknown`
   belongs to the **question**, not to the operator: `exists` asks about the store and stays
   two-valued, reporting `false` for a present null; every comparison asks about a value and is
   `Unknown` when there is none to read. `absent` was dropped. `engineering-protocols` had already
   made this split without naming it — six comparison operators, no presence operator, and
   `ValueAbsent` as the one candidate-shaped `Unknown`.
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

A `Truth { True, False, Unknown }` result with Kleene `all`/`any`/`not`; a comparison over a
reference that resolves to nothing — a missing key or a present `null` — yields `Unknown`, while
`exists` stays two-valued and reports `false` for both; `PreconditionFailed` and
`InvariantViolation` gain an `Unobservable` counterpart carrying every unresolved path;
`all`/`any` evaluate all operands; R-54 is revised in the register with the old and new wording;
every existing test still passes except the ones that assert the old collapse, which are rewritten
to assert the new distinction.

**Built 2026-08-25.** `crates/entity-core/src/truth.rs`; R-57 and R-58 in the register;
`kernel-v0.1.md` § 4.1, which records the rejected `exists`/`absent` draft so nobody re-proposes
it. `task check` exit 0, 101 tests. Both invariant tests that the rejected draft broke went back to
their original assertions unaltered, which is the evidence that `Unknown` was confined to the right
place.

## Out of scope

Changing the condition operators; three-valued *fields* — a field holds a value or it does not, and
`Unknown` is a property of an evaluation, not of storage.
