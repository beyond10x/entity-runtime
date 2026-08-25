---
format: aep.planning-md/1
id: architecture-decision-record:rules-as-data-ast
kind: architecture-decision-record
status: proposed
title: Rules are a data AST, not an expression language
summary: Conditions are thirteen YAML operators; no CEL, Rhai or Lua behind the rule slots in 0.1.
relations:
- decides: epic:kernel
revision: 2
---
# ADR: Rules are a data AST, not an expression language

## Status

Proposed 2026-08-25. Implemented as the 0.1 condition language.

## Context

Preconditions and invariants need a way to say *total is positive* and *a rejected order has a
reason*. The obvious tool is an expression language — CEL, Rhai, Lua, a string the kernel parses.

## Decision

A condition is a YAML/JSON AST with thirteen operators (`all`, `any`, `not`, `exists`, `eq`, `ne`,
`gt`, `gte`, `lt`, `lte`, `in`, `contains`, literal booleans) whose operands are values with the
same `$` references as templates. No function call, loop, arithmetic, clock or lookup.

## Alternatives

* **Embed CEL or Rhai** — expressive, but a definition then carries source code: it cannot be
  validated for references at registration without a second parser, it cannot be rendered by
  tooling that does not know the language, and sandboxing becomes the kernel's problem. Deferred,
  not refused: a richer engine can sit behind the same two rule slots later.
* **Rust closures registered by name** — the proof of concept's first shape. Rejected for the
  dynamic kernel: it puts the rule back into a compiled artifact.

## Consequences

Rules can be validated when the definition is registered (unknown fields, illegal scope references
are refused before they ever run), evaluated identically everywhere, and drawn. Operators are added
one at a time, each with a changelog line. Missing-versus-false is collapsed in 0.1 and the
three-valued revision is `story:three-valued-conditions`.
