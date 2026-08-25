---
format: aep.planning-md/1
id: story:aep-move-through-kernel
kind: story
status: draft
title: 'Phase 2: protocol artifact move evaluated by the kernel'
summary: Identical accept/refuse verdicts on the org's planning stores, behind the existing CLI.
relations:
- derived_from: epic:drive-engineering-protocols
- depends_on: story:three-valued-conditions
- depends_on: story:aep-lifecycles-as-definitions
revision: 4
---
# Story: Phase 2 — protocol artifact move evaluated by the kernel

## Outcome

`protocol artifact move` asks this kernel whether the move is permitted, behind the existing CLI,
refusing exactly what it refuses today.

## Context

Depends on `story:three-valued-conditions` (invariant 5 there) and `story:aep-lifecycles-as-definitions`.
How the kernel is reached — a dependency, a vendored copy, a process boundary — is an ADR in `atlas`
first, because `engineering-protocols` is public and this repository's visibility is undecided.

## Acceptance

On the planning stores of `engineering-protocols` and `agentic-principles`, every legal and illegal
move produces the same verdict through the kernel as through `LifecycleRegistry`; the comparison is
a committed test with the store snapshots as fixtures.
