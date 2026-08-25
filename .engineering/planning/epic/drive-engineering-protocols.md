---
format: aep.planning-md/1
id: epic:drive-engineering-protocols
kind: epic
status: draft
title: Drive engineering-protocols with the kernel
summary: Express the AEP artifact model — kinds, lifecycles, moves, events — as entity definitions and evaluate its status moves through entity-core, in phases, each accepted on a plan page there.
relations:
- decomposes: initiative:entity-runtime
revision: 2
---
# Epic: Drive engineering-protocols with the kernel

## Outcome

The AEP artifact model — kinds, lifecycles, legal moves, events — is expressed as entity
definitions and its status moves are evaluated by `entity-core`, so a new status or a new
precondition on a move is a change to a YAML document in `engineering-protocols`, not to a Rust
enum.

## Context

`docs/design/engineering-protocols-adoption-v0.1.md` carries the mapping, the phases and the
boundaries. It is **proposed**: nothing past phase 0 starts until a plan page or a story in
`engineering-protocols` accepts it. Phase 2 depends on `story:three-valued-conditions`, because
that repository's invariant 5 — *Unknown is not False* — cannot be honoured by a two-valued rule.

## Acceptance

Phase by phase, each with the evidence named in the design's § 3 table; the epic is done when
gap-register rows :39 and :70 there close with a mechanism.
