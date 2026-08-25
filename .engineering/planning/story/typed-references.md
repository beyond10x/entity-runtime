---
format: aep.planning-md/1
id: story:typed-references
kind: story
status: draft
title: Typed references between entities
summary: A field kind ref naming an entity type, with the checking boundary (kernel input vs shell) decided.
relations:
- derived_from: epic:kernel
revision: 4
---
# Story: Typed references between entities

## Outcome

A field can be declared `type: ref` with an `entity`, so a definition says that an order's
`customer` is a customer, and the runtime can check it.

## Context

The open design question is *where* the check runs. The kernel cannot load the referenced instance
(R-01), so either the reference is checked by the shell before `execute`, or the referenced
instance's existence enters the kernel as an input. Decide, record it in `kernel-v0.1.md` § 12's
successor, then build.

## Acceptance

`type: ref` parses and validates at registration (**amended when built**: the named entity type must be registered *in the set*, checked by `Registry::validate_all` rather than by `register` — two types that point at each other cannot both demand the other first);
the decided checking boundary is implemented and tested; `entity inspect` shows the reference.

## Built 2026-08-25

The open question this story existed to settle — *where does the check run* — is answered and
recorded in `docs/design/kernel-v0.1.md` § 3.5, which is § 12's successor as the story asked.

**The answer is the shell, and it is not a compromise.** The kernel checks the declaration
(`entity` is named) and the shape of a value (a non-empty identity), and `Registry::validate_all`
asks the one cross-definition question it can answer: is every target *type* registered? Whether an
instance carrying that identity exists is a question about another instance, which `execute` is
never handed (R-01) — and resolving one by lookup would let the same inputs give different decisions
at different moments, which is R-02. The purity constraint and the determinism guarantee point the
same way, so there was no trade to make.

**One deviation from the approved plan, and it made the feature smaller.** The plan had a
`relations:` block beside `schema` with its own `cardinality` key, plus a `ref` field kind — two
ways to declare a pointer to another entity. Cardinality is the array machinery that already
exists: `type: ref` is one, `type: array` of `items: {type: ref}` is many. A second spelling would
have been the same defect as a condition carrying two operators. `inverse` and `acyclic` moved onto
the field, where they sit beside `values` and `items` as constraints that govern one kind and are
refused on any other (R-26).

Acceptance, clause by clause: `type: ref` parses and validates at registration ✓; the checking
boundary is implemented and tested — R-27 for the declaration and the value, R-28 for the set ✓;
`entity inspect` shows the reference, from either the field or an array's `items` ✓.

Beyond it: `examples/references/` is a mutually-referencing pair, which is the case that forced
`validate_all` to be a set-level question rather than a registration-time one.

