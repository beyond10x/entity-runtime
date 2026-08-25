---
format: aep.planning-md/1
id: architecture-decision-record:repository-name
kind: architecture-decision-record
status: proposed
title: The repository is called entity-runtime
summary: Chosen over object platform, entity engine, aggregate runtime, domain runtime, decider and lifecycle.
relations:
- decides: epic:kernel
revision: 2
---
# ADR: The repository is called entity-runtime

## Status

Proposed 2026-08-25. The name was chosen by the agent under the operator's instruction to decide
and report; a rename before the first push costs a directory move and one line in `atlas`.

## Context

`atlas/AGENTS.md`: repos are plain lowercase nouns naming their function. The naming conversation
that produced the proof of concept ended on *Entity Runtime* for a system whose objects have
schemas, lifecycles, operations, invariants and events.

## Decision

`entity-runtime`. It names the function (a runtime for entities), it is the term the proof of
concept settled on, and it does not collide with a type or crate in `engineering-protocols`, where
*entity* is the universal model this kernel would execute.

## Alternatives

* **`object-platform` / `object-runtime`** — the broadest names from the conversation; rejected as
  naming the ambition rather than the thing built.
* **`entity-engine`, `aggregate-runtime`, `domain-runtime`** — accurate; rejected for jargon
  (`aggregate`) or vagueness (`domain`).
* **`decider`** — the functional name for decide/evolve; rejected as obscure.
* **`lifecycle`** — a single noun naming the defining feature; rejected because the runtime is also
  the schema, the rules and the events, and because `artifacts/lifecycles/` exists in
  `engineering-protocols` with a narrower meaning.

## Consequences

Crate names follow: `entity-core`, `entity-yaml`, `entity-cli`; the binary is `entity`. The
`atlas` map carries the row.
